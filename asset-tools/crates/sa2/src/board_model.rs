//! Original City Escape Chunk models from the pinned PC executable.
use crate::{archive, manifest::sha256, model::Reader, pe, texture, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoardModel {
    Streetboard,
    ShadowBoard,
    AmyBoard,
    MetalBoard,
    SonicBoard,
    CastShadow,
    Modifier,
}
pub struct Extracted {
    pub sa2mdl: Vec<u8>,
    pub texture_names: Vec<String>,
}
pub const EXE_HASH: &str = "4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16";
pub const ARCHIVE_HASH: &str = "0f74e2d685043944ddc4a14d2552b26312d451d3b39508ab0e1198e8966321b3";
impl BoardModel {
    // Contiguous native spans from first model data through NJS_OBJECT root;
    // roots and texlists independently identified by SA Tools PC_SA2/STG13.ini.
    fn layout(self) -> (u32, u32, Option<u32>) {
        match self {
            Self::Streetboard => (0xc89488, 0xc89bbc, Some(0xc89418)),
            Self::ShadowBoard => (0xc8acf0, 0xc8b974, Some(0xc8942c)),
            Self::AmyBoard => (0xc8b9d0, 0xc8c674, Some(0xc89440)),
            Self::MetalBoard => (0xc8c6b8, 0xc8d36c, Some(0xc89454)),
            Self::SonicBoard => (0xc8d3c8, 0xc8e06c, Some(0xc89468)),
            Self::CastShadow => (0xc8a550, 0xc8a80c, None),
            Self::Modifier => (0xc8ab98, 0xc8acb8, None),
        }
    }
}
pub fn extract(exe: &[u8], id: BoardModel) -> Result<Extracted> {
    if exe.len() != 21858816 || sha256(exe) != EXE_HASH {
        return Err(Error::invalid("unrecognized City Escape PC executable"));
    }
    let pe = pe::read_pe(exe)?;
    let (start, root, list) = id.layout();
    let size = (root - start) as usize + 52;
    let off = pe.rva_to_offset(start, size)?;
    let mut data = vec![0; 16];
    data[..8].copy_from_slice(b"SA2MDL\0\x03");
    let root = 16 + root - start;
    data[8..12].copy_from_slice(&root.to_le_bytes());
    data.extend_from_slice(Reader { data: exe }.span(off, size)?);
    let base = pe.image_base + start;
    let rebase = |data: &mut [u8], at: usize| -> Result<usize> {
        let ptr = Reader { data }.u32(at)?;
        if ptr == 0 {
            return Ok(0);
        }
        let relative = ptr
            .checked_sub(base)
            .and_then(|p| p.checked_add(16))
            .filter(|p| (*p as usize) < data.len())
            .ok_or_else(|| Error::invalid("board pointer outside source model"))?;
        data[at..at + 4].copy_from_slice(&relative.to_le_bytes());
        Ok(relative as usize)
    };
    let mut pending = vec![root as usize];
    let mut visited = BTreeSet::new();
    let mut attaches = BTreeSet::new();
    while let Some(n) = pending.pop() {
        if !visited.insert(n) {
            return Err(Error::invalid("board node repeated"));
        }
        let a = rebase(&mut data, n + 4)?;
        if a != 0 && attaches.insert(a) {
            rebase(&mut data, a)?;
            rebase(&mut data, a + 4)?;
        }
        for field in [44, 48] {
            let child = rebase(&mut data, n + field)?;
            if child != 0 {
                pending.push(child);
            }
        }
    }
    let mut names = Vec::new();
    if let Some(list) = list {
        let r = Reader { data: exe };
        let l = pe.rva_to_offset(list, 8)?;
        let count = r.u32(l + 4)? as usize;
        if count > 3 {
            return Err(Error::invalid("invalid board texture count"));
        }
        let array = pe.va_to_offset(r.u32(l)?, count * 12)?;
        for i in 0..count {
            let at = pe.va_to_offset(r.u32(array + 12 * i)?, 32)?;
            names.push(r.string(at, at + 32)?);
        }
    }
    Ok(Extracted {
        sa2mdl: data,
        texture_names: names,
    })
}

pub fn textures(prs: &[u8], names: &[String]) -> Result<Vec<texture::Image>> {
    if prs.len() != 348461 || sha256(prs) != ARCHIVE_HASH {
        return Err(Error::invalid("unrecognized OBJTEX_STG13 archive"));
    }
    if names.len() > 3 {
        return Err(Error::invalid("invalid board texture list"));
    }
    let raw = archive::decompress_prs(prs, 16 * 1024 * 1024)?;
    let entries = archive::read_gvm(&raw)?;
    names
        .iter()
        .map(|name| {
            let mut matched = entries.iter().filter(|e| e.name.eq_ignore_ascii_case(name));
            let entry = matched
                .next()
                .ok_or_else(|| Error::invalid(format!("original texture missing: {name}")))?;
            if matched.next().is_some() {
                return Err(Error::invalid("ambiguous board texture"));
            }
            texture::decode_gvr(entry.data, None)
        })
        .collect()
}

/// Rest pose, original indexed texture-list bindings and native alpha state. The
/// shadow pass and modifier volume are not ordinary textured display models.
pub fn board_glb(exe: &[u8], archive: &[u8], id: BoardModel) -> Result<Vec<u8>> {
    if matches!(id, BoardModel::CastShadow | BoardModel::Modifier) {
        return Err(Error::invalid(
            "shadow and modifier require their native render passes",
        ));
    }
    let x = extract(exe, id)?;
    let scene = crate::visual::evaluate_static_model(&crate::model::read_model(&x.sa2mdl)?)?;
    let mut pngs = std::collections::BTreeMap::new();
    for (id, image) in textures(archive, &x.texture_names)?.into_iter().enumerate() {
        let mut png = vec![];
        image.write_png(&mut png)?;
        pngs.insert(id as u16, png);
    }
    crate::visual_glb::export(&scene, &pngs)
}
