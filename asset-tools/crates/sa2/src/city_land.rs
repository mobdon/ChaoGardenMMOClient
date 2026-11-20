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

/// Rebase a single local REL Basic object (no child or sibling) through the
/// same validated relocation view used for REL land tables.
pub(crate) fn rel_basic_object(
    rel: &CityRel<'_>,
    sec: u8,
    root: usize,
) -> Result<crate::duel_platform::Platform> {
    let src = rel.data(sec)?;
    let mut v = View {
        source: Source::Rel(rel, sec),
        src,
        out: vec![0; src.len() + BASE],
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

/// One native object copy for `win32_posed_collision`: the source object's
/// file offset and the words the copy replaces.
pub(crate) struct PosedObject {
    pub root: usize,
    pub flags: u32,
    pub position: [f32; 3],
    pub rotation: [i32; 3],
    pub surface_flags: u32,
}
/// Bytes CartD copies per object (+0x660c allocates and copies 0x38).
const POSED_COPY: usize = 0x38;
/// A collision-only land view whose entries are copies of Win32 Basic
/// objects. Each copy keeps the source words except flags, position and
/// rotation; attachment, child and sibling pointers are rebased like
/// installed land, and every attachment is copied unchanged.
pub(crate) fn win32_posed_collision(
    bytes: &[u8],
    image: &crate::pe::PeImage,
    objects: &[PosedObject],
) -> Result<stage::Stage> {
    let count = objects.len();
    if count > i16::MAX as usize {
        return Err(Error::invalid("posed collision count exceeds land limit"));
    }
    let h = (BASE + bytes.len()).next_multiple_of(4);
    let table = h + 32;
    let copies = table + count * 32;
    let mut v = View {
        source: Source::Win32(image),
        src: bytes,
        out: vec![0; copies + count * POSED_COPY],
    };
    v.out[..8].copy_from_slice(b"SA2LVL\0\x03");
    v.out[8..12].copy_from_slice(&(h as u32).to_le_bytes());
    v.out[h..h + 2].copy_from_slice(&(count as u16).to_le_bytes());
    v.out[h + 16..h + 20].copy_from_slice(&(table as u32).to_le_bytes());
    let mut seen_nodes = BTreeSet::new();
    let mut seen_basic = BTreeSet::new();
    for (i, o) in objects.iter().enumerate() {
        // The source hierarchy below the copied root is shared, as in CartD.
        let mut stack = vec![];
        let mut words = [0u32; POSED_COPY / 4];
        for (k, word) in words.iter_mut().enumerate() {
            *word = v.word(o.root + 4 * k)?;
        }
        let mut link = |v: &mut View<'_, '_>, at: usize| -> Result<u32> {
            Ok(match v.ptr(at)? {
                Some(n) => {
                    stack.push(n);
                    (BASE + n) as u32
                }
                None => 0,
            })
        };
        let attach = v.ptr(o.root + 4)?;
        words[11] = link(&mut v, o.root + 44)?;
        words[12] = link(&mut v, o.root + 48)?;
        words[0] = o.flags;
        words[1] = attach.map_or(0, |a| (BASE + a) as u32);
        for k in 0..3 {
            words[2 + k] = o.position[k].to_bits();
            words[5 + k] = o.rotation[k] as u32;
        }
        if let Some(a) = attach {
            if seen_basic.insert(a) {
                v.attach(a)?;
            }
        }
        while let Some(n) = stack.pop() {
            if !seen_nodes.insert(n) {
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
            if let Some(a) = a {
                if seen_basic.insert(a) {
                    v.attach(a)?
                }
            }
        }
        let c = copies + i * POSED_COPY;
        for (k, word) in words.into_iter().enumerate() {
            v.out[c + 4 * k..c + 4 * k + 4].copy_from_slice(&word.to_le_bytes());
        }
        let e = table + i * 32;
        v.out[e + 16..e + 20].copy_from_slice(&(c as u32).to_le_bytes());
        v.out[e + 28..e + 32].copy_from_slice(&o.surface_flags.to_le_bytes());
    }
    let mut result = stage::read_stage(&v.out)?;
    result.unknowns = vec![
        "Entries are the kart piece tasks' native object copies with their dynamic land flags; the engine registers each only while a kart is near, and this view holds all of them".into(),
        "Entry bounds are zero: the dynamic land list carries none".into(),
    ];
    Ok(result)
}
