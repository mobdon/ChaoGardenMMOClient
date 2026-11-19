//! PC death-zone tables from SA Tools' stage configurations. Read pointer targets
//! directly from mapped PE file spans, retaining original geometry and flags.
use crate::{model::Reader, object_table, pe::PeImage, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct DeathZone {
    pub flags: u32,
    pub object_va: u32,
    pub object_flags: u32,
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub scale: [f32; 3],
    pub model_va: u32,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub bounds_center: [f32; 3],
    pub bounds_radius: f32,
    pub material_words: [u32; 5],
    pub mesh_type: u16,
    pub mesh_material_index: u16,
    pub polygons: Vec<Polygon>,
}
#[derive(Debug, Serialize)]
pub struct Polygon {
    pub reversed: bool,
    pub indices: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct Table {
    pub rva: u32,
    pub zones: Vec<DeathZone>,
}

fn span<'a>(file: &'a [u8], image: &PeImage, va: u32, size: usize) -> Result<Reader<'a>> {
    let offset = image.va_to_offset(va, size)?;
    Ok(Reader {
        data: file
            .get(offset..offset + size)
            .ok_or_else(|| Error::invalid("truncated death zone"))?,
    })
}

fn zone(file: &[u8], image: &PeImage, object_va: u32, flags: u32) -> Result<DeathZone> {
    let object = span(file, image, object_va, 52)?;
    if object.u32(44)? != 0 || object.u32(48)? != 0 {
        return Err(Error::invalid("unsupported linked death-zone object"));
    }
    let model_va = object.u32(4)?;
    let model = span(file, image, model_va, 40)?;
    let n = model.u32(8)? as usize;
    if !(1..=4096).contains(&n) || model.u16(20)? != 1 || model.u16(22)? > 1 {
        return Err(Error::invalid("unsupported death-zone Basic model layout"));
    }
    let points = |at| -> Result<Vec<[f32; 3]>> {
        let r = span(file, image, model.u32(at)?, n * 12)?;
        (0..n).map(|i| r.vec3(i * 12)).collect()
    };
    let mat = span(file, image, model.u32(16)?, 20)?;
    let mesh = span(file, image, model.u32(12)?, 24)?;
    if [8, 12, 16, 20]
        .into_iter()
        .any(|off| mesh.u32(off).ok() != Some(0))
    {
        return Err(Error::invalid("unsupported death-zone mesh attributes"));
    }
    let type_material = mesh.u16(0)?;
    let mesh_type = type_material >> 14;
    if !matches!(mesh_type, 1 | 3) || type_material & 0x3fff != 0 {
        return Err(Error::invalid(
            "unsupported death-zone polygon or material type",
        ));
    }
    let count = usize::from(mesh.u16(2)?);
    if !(1..=4096).contains(&count) {
        return Err(Error::invalid("invalid death-zone polygon count"));
    }
    let mut cursor = mesh.u32(4)?;
    let mut polygons = Vec::with_capacity(count);
    let mut total = 0;
    for _ in 0..count {
        let (corners, reversed) = if mesh_type == 1 {
            (4, false)
        } else {
            let word = span(file, image, cursor, 2)?.u16(0)?;
            cursor = cursor
                .checked_add(2)
                .ok_or_else(|| Error::invalid("death zone address overflow"))?;
            (usize::from(word & 0x7fff), word & 0x8000 != 0)
        };
        total += corners;
        if corners < 3 || total > 8192 {
            return Err(Error::invalid("invalid death-zone corner count"));
        }
        let r = span(file, image, cursor, corners * 2)?;
        let indices = (0..corners)
            .map(|i| r.u16(i * 2))
            .collect::<Result<Vec<_>>>()?;
        if indices.iter().any(|&x| usize::from(x) >= n) {
            return Err(Error::invalid("death-zone index outside vertex array"));
        }
        polygons.push(Polygon { reversed, indices });
        cursor = cursor
            .checked_add((corners * 2) as u32)
            .ok_or_else(|| Error::invalid("death zone address overflow"))?;
    }
    Ok(DeathZone {
        flags,
        object_va,
        object_flags: object.u32(0)?,
        position: object.vec3(8)?,
        angles: [
            object.u32(20)? as i32,
            object.u32(24)? as i32,
            object.u32(28)? as i32,
        ],
        scale: object.vec3(32)?,
        model_va,
        vertices: points(0)?,
        normals: points(4)?,
        bounds_center: model.vec3(24)?,
        bounds_radius: model.f32(36)?,
        material_words: [
            mat.u32(0)?,
            mat.u32(4)?,
            mat.u32(8)?,
            mat.u32(12)?,
            mat.u32(16)?,
        ],
        mesh_type,
        mesh_material_index: type_material & 0x3fff,
        polygons,
    })
}

pub fn read(file: &[u8], config: &str) -> Result<BTreeMap<String, Table>> {
    if config.len() > 1024 * 1024 {
        return Err(Error::invalid("excessive death-zone configuration"));
    }
    let image = object_table::verify_image(file)?;
    let mut tables = BTreeMap::new();
    let mut total = 0usize;
    let mut name = None;
    let mut kind = None;
    let mut address = None;
    for raw in config.lines().chain(std::iter::once("[]")) {
        let line = raw.trim();
        if line.starts_with('[') {
            if let (Some(n), Some("deathzone")) = (name, kind) {
                let hex =
                    address.ok_or_else(|| Error::invalid("missing death-zone table address"))?;
                let rva = u32::from_str_radix(hex, 16)
                    .map_err(|_| Error::invalid("invalid death-zone address"))?;
                let mut zones = Vec::new();
                loop {
                    let entry = rva
                        .checked_add((zones.len() * 8) as u32)
                        .ok_or_else(|| Error::invalid("death-zone table overflow"))?;
                    let off = image.rva_to_offset(entry, 8)?;
                    let r = Reader { data: file };
                    let flags = r.u32(off)?;
                    let object = r.u32(off + 4)?;
                    if object == 0 {
                        break;
                    }
                    if zones.len() >= 256 || total >= 1024 {
                        return Err(Error::invalid("excessive death-zone count"));
                    }
                    zones.push(zone(file, &image, object, flags)?);
                    total += 1;
                }
                if tables.len() >= 128
                    || tables
                        .insert(String::from(n), Table { rva, zones })
                        .is_some()
                {
                    return Err(Error::invalid("duplicate or excessive death-zone tables"));
                }
            }
            name = Some(
                line.strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .ok_or_else(|| Error::invalid("invalid death-zone config section"))?,
            );
            kind = None;
            address = None;
        } else if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "type" => kind = Some(v.trim()),
                "address" => address = Some(v.trim()),
                _ => {}
            }
        }
    }
    if tables.is_empty() {
        return Err(Error::invalid("no deathzone sections"));
    }
    Ok(tables)
}
