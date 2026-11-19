//! Static land collision from the Steam-delivered City Escape REL or Win32 DLL.
//! Rebase REL references or PE virtual addresses into a private little-endian view so that
//! the same validated Basic polygon and hierarchy reader serves both file formats.
use crate::{city_rel::CityRel, stage, Error, Result};
use std::collections::BTreeSet;
const SEC: u8 = 5;
const HEADER: usize = 0x162d1c - 0x5dd40;
const BASE: usize = 16;
enum Source<'a, 'b> {
    Rel(&'a CityRel<'b>),
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
            Source::Rel(_) => u16::from_be_bytes(b.try_into().unwrap()),
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
            Source::Rel(_) => u32::from_be_bytes(b.try_into().unwrap()),
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
            Source::Rel(rel) => match rel.local_pointer(SEC, p as u32)? {
                Some((SEC, n)) if (n as usize) < self.src.len() => Some(n as usize),
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
        source: Source::Rel(&rel),
        src,
        out: vec![0; src.len() + BASE],
    };
    decode(v, HEADER)
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
    )
}
fn decode(mut v: View<'_, '_>, h: usize) -> Result<stage::Stage> {
    v.out[..8].copy_from_slice(b"SA2BLVL\x03");
    v.out[8..12].copy_from_slice(&((h + BASE) as u32).to_le_bytes());

    let count = v.half(h)? as usize;
    let visual = v.half(h + 2)? as usize;
    if count != 2912 || visual != 2376 {
        return Err(Error::invalid(
            "City land entry counts differ from installed profile",
        ));
    }
    v.words(h + 4, 3)?;
    let table = v
        .ptr(h + 16)?
        .ok_or_else(|| Error::invalid("City land table missing"))?;
    // Texture loading and dynamic land animation are outside the static collision view.
    let mut seen_nodes = BTreeSet::new();
    let mut seen_basic = BTreeSet::new();
    for i in 0..count {
        let e = table + i * 32;
        v.words(e, 4)?;
        let root = v.ptr(e + 16)?;
        v.words(e + 20, 3)?;
        let mut stack: Vec<_> = root.into_iter().collect();
        while let Some(n) = stack.pop() {
            if !seen_nodes.insert((i >= visual, n)) {
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
            if i < visual {
                v.word(n + 52)?;
            } else if let Some(a) = a {
                if seen_basic.insert(a) {
                    v.attach(a)?
                }
            }
        }
    }
    stage::read_stage(&v.out)
}
