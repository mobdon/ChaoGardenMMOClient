//! Static land collision from the Steam-delivered City Escape REL or Win32 DLL.
//! Rebase REL references or PE virtual addresses into a private little-endian view so that
//! the same validated Basic polygon and hierarchy reader serves both file formats.
use crate::{city_rel::CityRel, stage, Error, Result};
use std::collections::BTreeSet;
const SEC: u8 = 5;
const HEADER: usize = 0x162d1c - 0x5dd40;
const BASE: usize = 16;
enum Source<'a, 'b> {
    Rel(&'a CityRel<'b>, u8),
    Win32(&'a crate::pe::PeImage),
}
struct View<'a, 'b> {
    source: Source<'a, 'b>,
    src: &'b [u8],
    out: Vec<u8>,
}
impl View<'_, '_> {
    fn half(&mut self, p: usize) -> Result<u16> {
        let b = self
            .src
            .get(p..p + 2)
            .ok_or_else(|| Error::invalid("City land halfword outside section"))?;
        let n = match self.source {
            Source::Rel(_, _) => u16::from_be_bytes(b.try_into().unwrap()),
            Source::Win32(_) => u16::from_le_bytes(b.try_into().unwrap()),
        };
        self.out[BASE + p..BASE + p + 2].copy_from_slice(&n.to_le_bytes());
        Ok(n)
    }
    fn word(&mut self, p: usize) -> Result<u32> {
        let b = self
            .src
            .get(p..p + 4)
            .ok_or_else(|| Error::invalid("City land word outside section"))?;
        let n = match self.source {
            Source::Rel(_, _) => u32::from_be_bytes(b.try_into().unwrap()),
            Source::Win32(_) => u32::from_le_bytes(b.try_into().unwrap()),
        };
        self.out[BASE + p..BASE + p + 4].copy_from_slice(&n.to_le_bytes());
        Ok(n)
    }
    fn ptr(&mut self, p: usize) -> Result<Option<usize>> {
        let raw = self.word(p)?;
        let target = match self.source {
            Source::Win32(pe) => {
                if raw == 0 {
                    None
                } else {
                    Some(pe.va_to_offset(raw, 1)?)
                }
            }
            Source::Rel(rel, sec) => match rel.local_pointer(sec, p as u32)? {
                Some((dest, n)) if dest == sec && (n as usize) < self.src.len() => Some(n as usize),
                None => None,
                _ => return Err(Error::invalid("City land pointer outside data section")),
            },
        };
        let mapped = target.map_or(0, |n| (BASE + n) as u32);
        self.out[BASE + p..BASE + p + 4].copy_from_slice(&mapped.to_le_bytes());
        Ok(target)
    }
    fn words(&mut self, p: usize, n: usize) -> Result<()> {
        for i in 0..n {
            self.word(p + i * 4)?;
        }
        Ok(())
    }
    fn attach(&mut self, p: usize) -> Result<()> {
        let verts = self.ptr(p)?;
        let normals = self.ptr(p + 4)?;
        let count = self.word(p + 8)? as usize;
        if count > 262144 {
            return Err(Error::invalid("City vertex count"));
        }
        for a in [verts, normals].into_iter().flatten() {
            self.words(a, count * 3)?
        }
        let meshes = self.ptr(p + 12)?;
        let materials = self.ptr(p + 16)?;
        let nm = self.half(p + 20)? as usize;
        let mut nmat = self.half(p + 22)? as usize;
        self.words(p + 24, 4)?;
        if let Some(m) = meshes {
            for i in 0..nm {
                let s = m + i * 24;
                let ty = self.half(s)?;
                nmat = nmat.max((ty & 0x3fff) as usize + 1);
                let np = self.half(s + 2)? as usize;
                let mut pos = self.ptr(s + 4)?.unwrap_or(0);
                self.word(s + 8)?;
                let pn = self.ptr(s + 12)?;
                let col = self.ptr(s + 16)?;
                let uv = self.ptr(s + 20)?;
                let mut corners = 0;
                for _ in 0..np {
                    let n = match ty >> 14 {
                        0 => 3,
                        1 => 4,
                        _ => {
                            let n = self.half(pos)? & 0x7fff;
                            pos += 2;
                            n as usize
                        }
                    };
                    for _ in 0..n {
                        self.half(pos)?;
                        pos += 2
                    }
                    corners += n;
                }
                if let Some(a) = pn {
                    self.words(a, np * 3)?
                }
                if let Some(a) = col {
                    self.words(a, corners)?
                }
                if let Some(a) = uv {
                    for j in 0..corners * 2 {
                        self.half(a + j * 2)?;
                    }
                }
            }
        }
        if let Some(a) = materials {
            self.words(a, nmat * 5)?
        }
        Ok(())
    }
}
pub fn read_collision_stage(bytes: &[u8]) -> Result<stage::Stage> {
    let rel = CityRel::read(bytes)?;
    let src = rel.data(SEC)?;
    let v = View {
        source: Source::Rel(&rel, SEC),
        src,
        out: vec![0; src.len() + BASE],
    };
    decode(v, HEADER, true, 2912, 2376)
}
/// Reads the named export from the installed DLL without loading or executing it.
pub fn read_win32_collision_stage(bytes: &[u8]) -> Result<stage::Stage> {
    if bytes.len() != 31653888
        || crate::manifest::sha256(bytes)
            != "6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59"
    {
        return Err(Error::invalid("unsupported City Escape Win32 DLL revision"));
    }
    let pe = crate::pe::read_pe(bytes)?;
    let export = pe
        .exports
        .iter()
        .find(|e| e.name == "objLandTable0013" && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid("City Escape land export missing"))?;
    let h = pe.rva_to_offset(export.rva, 32)?;
    decode(
        View {
            source: Source::Win32(&pe),
            src: bytes,
            out: vec![0; bytes.len() + BASE],
        },
        h,
        true,
        2912,
        2376,
    )
}
pub(crate) fn read_rel_table(
    rel: &CityRel<'_>,
    sec: u8,
    h: usize,
    battle: bool,
    count: u16,
    visual: i16,
) -> Result<stage::Stage> {
    let src = rel.data(sec)?;
    decode(
        View {
            source: Source::Rel(rel, sec),
            src,
            out: vec![0; src.len() + BASE],
        },
        h,
        battle,
        count,
        visual,
    )
}
fn decode(
    mut v: View<'_, '_>,
    h: usize,
    battle: bool,
    expected_count: u16,
    expected_visual: i16,
) -> Result<stage::Stage> {
    // The shared stage reader treats Battle visual attachments as opaque. Use that
    // path for the static view, then restore the Chunk format labels for SA2.
    v.out[..8].copy_from_slice(b"SA2BLVL\x03");
    v.out[8..12].copy_from_slice(&((h + BASE) as u32).to_le_bytes());

    let count = v.half(h)? as usize;
    let visual = v.half(h + 2)? as i16;
    if count != expected_count as usize
        || visual != expected_visual
        || visual < -1
        || (visual as i32 > count as i32 && (battle || count != 514 || visual != 516))
    {
        return Err(Error::invalid(
            "land entry counts differ from installed profile",
        ));
    }
    v.words(h + 4, 3)?;
    let table = v
        .ptr(h + 16)?
        .ok_or_else(|| Error::invalid("City land table missing"))?;
    // Final Rush's Chunk UV table declares 516 visual entries but contains only
    // 514, all visible. SA Tools writes 514/514. Validate every native flag
    // before normalizing the derived view; catalog retains the source's 516.
    if visual as i32 > count as i32 {
        for i in 0..count {
            if v.word(table + i * 32 + 28)? != 0x8000_0000 {
                return Err(Error::invalid("unexpected UV table surface flags"));
            }
        }
        v.out[BASE + h + 2..BASE + h + 4].copy_from_slice(&(count as u16).to_le_bytes());
    }
    // Texture loading and dynamic land animation are outside the static collision view.
    let mut seen_nodes = BTreeSet::new();
    let mut seen_basic = BTreeSet::new();
    for i in 0..count {
        let e = table + i * 32;
        v.words(e, 4)?;
        let root = v.ptr(e + 16)?;
        v.words(e + 20, 3)?;
        let basic = if visual < 0 {
            v.word(e + 28)? & 0x8000_0000 == 0
        } else {
            i >= visual as usize
        };
        let mut stack: Vec<_> = root.into_iter().collect();
        while let Some(n) = stack.pop() {
            if !seen_nodes.insert((basic, n)) {
                continue;
            }
            v.word(n)?;
            let a = v.ptr(n + 4)?;
            v.words(n + 8, 9)?;
            for j in [44, 48] {
                if let Some(x) = v.ptr(n + j)? {
                    stack.push(x)
                }
            }
            if !basic {
                if battle {
                    v.word(n + 52)?;
                }
            } else if let Some(a) = a {
                if seen_basic.insert(a) {
                    v.attach(a)?
                }
            }
        }
    }
    let mut result = stage::read_stage(&v.out)?;
    if !battle {
        result.format = "sa2".into();
        for e in &mut result.entries {
            if e.geometry_format == "gc-opaque" {
                e.geometry_format = "chunk".into();
                for node in &mut e.nodes {
                    node.quaternion_scalar = None;
                }
            }
        }
        result.unknowns=vec!["This static collision view retains native Basic meshes; Chunk visual attachments are omitted.".into()];
    }
    Ok(result)
}

/// Rebase a single unlinked Win32 Basic object using the same pointer and
/// polygon layouts as installed land. The private offsets are file offsets + 16.
pub(crate) fn win32_basic_object(
    bytes: &[u8],
    image: &crate::pe::PeImage,
    rva: u32,
) -> Result<crate::duel_platform::Platform> {
    let root = image.rva_to_offset(rva, 52)?;
    let mut v = View {
        source: Source::Win32(image),
        src: bytes,
        out: vec![0; bytes.len() + BASE],
    };
    v.word(root)?;
    let base = v
        .ptr(root + 4)?
        .ok_or_else(|| Error::invalid("missing Basic model"))?;
    v.words(root + 8, 9)?;
    if v.ptr(root + 44)?.is_some() || v.ptr(root + 48)?.is_some() {
        return Err(Error::invalid("expected a single Basic platform object"));
    }
    v.attach(base)?;
    crate::duel_platform::decode(&v.out, (root + BASE) as u32)
}

/// One placed kart course piece: native object file offsets in the Win32 DLL
/// and the course-relative pose that places that occurrence.
pub(crate) struct KartOccurrence {
    pub display: Option<usize>,
    pub collision: Option<usize>,
    pub pose: crate::kart_course::CoursePose,
}
/// Synthetic wrapper node size: a 52-byte object plus the zeroed Battle word.
const KART_WRAPPER: usize = 56;
/// Assemble placed kart pieces into a private Battle-layout land view. Every
/// native object, attachment, polygon and material word is copied unchanged
/// (pointers rebased as for installed land). Each occurrence gets one
/// synthetic parent node whose position and Y rotation are the course pose;
/// the native root and its hierarchy stay below it untouched. Display objects
/// come first (GC attachments stay opaque), then the Basic collision objects.
/// Nothing is derived: entry bounds and surface flags stay zero.
pub(crate) fn win32_kart_course(
    bytes: &[u8],
    image: &crate::pe::PeImage,
    occurrences: &[KartOccurrence],
) -> Result<stage::Stage> {
    let visual: Vec<_> = occurrences
        .iter()
        .filter_map(|o| Some((o.display?, o, false)))
        .collect();
    let basic: Vec<_> = occurrences
        .iter()
        .filter_map(|o| Some((o.collision?, o, true)))
        .collect();
    let count = visual.len() + basic.len();
    if count > i16::MAX as usize {
        return Err(Error::invalid("kart course entry count exceeds land limit"));
    }
    let h = (BASE + bytes.len()).next_multiple_of(4);
    let table = h + 32;
    let wrappers = table + count * 32;
    let mut v = View {
        source: Source::Win32(image),
        src: bytes,
        out: vec![0; wrappers + count * KART_WRAPPER],
    };
    v.out[..8].copy_from_slice(b"SA2BLVL\x03");
    v.out[8..12].copy_from_slice(&(h as u32).to_le_bytes());
    v.out[h..h + 2].copy_from_slice(&(count as u16).to_le_bytes());
    v.out[h + 2..h + 4].copy_from_slice(&(visual.len() as u16).to_le_bytes());
    v.out[h + 16..h + 20].copy_from_slice(&(table as u32).to_le_bytes());
    let mut seen_nodes = BTreeSet::new();
    let mut seen_basic = BTreeSet::new();
    for (i, (root, o, is_basic)) in visual.into_iter().chain(basic).enumerate() {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if !seen_nodes.insert((is_basic, n)) {
                continue;
            }
            v.word(n)?;
            let a = v.ptr(n + 4)?;
            v.words(n + 8, 9)?;
            for j in [44, 48] {
                if let Some(x) = v.ptr(n + j)? {
                    stack.push(x)
                }
            }
            if is_basic {
                if let Some(a) = a {
                    if seen_basic.insert(a) {
                        v.attach(a)?
                    }
                }
            }
        }
        let e = table + i * 32;
        let w = wrappers + i * KART_WRAPPER;
        v.out[e + 16..e + 20].copy_from_slice(&(w as u32).to_le_bytes());
        let position = o.pose.position.map(|c| c as f32);
        if !position.iter().all(|c| c.is_finite()) {
            return Err(Error::invalid("nonfinite kart course pose"));
        }
        let mut words = [0u32; 13];
        for k in 0..3 {
            words[2 + k] = position[k].to_bits();
            words[8 + k] = 1f32.to_bits();
        }
        words[6] = u32::from(o.pose.heading);
        words[11] = (BASE + root) as u32;
        for (k, word) in words.into_iter().enumerate() {
            v.out[w + 4 * k..w + 4 * k + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    let mut result = stage::read_stage(&v.out)?;
    for e in &mut result.entries {
        // Win32 objects are 52 bytes; the trailing Battle word is not native data.
        for node in &mut e.nodes {
            node.quaternion_scalar = None;
        }
    }
    result.unknowns = vec![
        "Pieces are placed by the native course displacement and yaw table; one synthetic parent node per occurrence carries that pose, with its position rounded to binary32".into(),
        "The course rows carry no landtable surface flags or entry bounds; both stay zero and entries are classified only by the visual count".into(),
        "GC display attachments remain opaque; textures, draw order, scenery objects and the race scene are separate".into(),
    ];
    Ok(result)
}
