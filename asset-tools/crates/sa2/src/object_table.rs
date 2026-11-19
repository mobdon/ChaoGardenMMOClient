//! PC level object tables. Resolve data pointers through the PE sections without loading the game.
use crate::{manifest, pe, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

pub const STEAM_EXE_SHA256: &str =
    "4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16";
pub const CITY_LIST_RVA: u32 = 0xC9EE90;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDefinition {
    pub load_flags: u8,
    pub list: u8,
    pub flags: u16,
    pub distance: f32,
    pub function_va: u32,
    pub name: String,
}

pub fn verify_image(bytes: &[u8]) -> Result<pe::PeImage> {
    if manifest::sha256(bytes) != STEAM_EXE_SHA256 {
        return Err(Error::invalid(
            "unsupported executable revision for PC object tables",
        ));
    }
    pe::read_pe(bytes)
}

pub fn read_table(
    bytes: &[u8],
    image: &pe::PeImage,
    head_rva: u32,
) -> Result<Vec<ObjectDefinition>> {
    let head = image.rva_to_offset(head_rva, 8)?;
    let head = bytes
        .get(head..head + 8)
        .ok_or_else(|| Error::invalid("object table header"))?;
    let count = u32::from_le_bytes(head[..4].try_into().unwrap()) as usize;
    if !(1..=4096).contains(&count) {
        return Err(Error::invalid("object table count"));
    }
    let start = image.va_to_offset(
        u32::from_le_bytes(head[4..].try_into().unwrap()),
        count * 16,
    )?;
    let records = bytes
        .get(start..start + count * 16)
        .ok_or_else(|| Error::invalid("object table contents"))?;
    records
        .as_chunks::<16>()
        .0
        .iter()
        .map(|record| {
            let distance = f32::from_le_bytes(record[4..8].try_into().unwrap());
            if !distance.is_finite() {
                return Err(Error::invalid("nonfinite object load distance"));
            }
            let function_va = u32::from_le_bytes(record[8..12].try_into().unwrap());
            let name_va = u32::from_le_bytes(record[12..16].try_into().unwrap());
            let name = image.c_string(
                bytes,
                name_va
                    .checked_sub(image.image_base)
                    .ok_or_else(|| Error::invalid("name VA below image base"))?,
            )?;
            Ok(ObjectDefinition {
                load_flags: record[0],
                list: record[1],
                flags: u16::from_le_bytes([record[2], record[3]]),
                distance,
                function_va,
                name,
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectList {
    pub head_rva: u32,
    pub entries: Vec<ObjectDefinition>,
}

/// Resolve each declared objlist in SA Tools' owner-supplied ObjectLists.ini.
/// Other mapping types and global options do not select an object table.
pub fn mapped_tables(bytes: &[u8], config: &str) -> Result<BTreeMap<String, ObjectList>> {
    let image = verify_image(bytes)?;
    let mut tables = BTreeMap::new();
    let mut total = 0usize;
    for (name, fields) in crate::stage_catalog::sections(config, &["type", "address"])? {
        if fields.get("type").map(String::as_str) != Some("objlist") {
            continue;
        }
        if tables.len() >= 512 {
            return Err(Error::invalid("object list mapping limit"));
        }
        let address = fields
            .get("address")
            .ok_or_else(|| Error::invalid("missing object list address"))?;
        let rva = u32::from_str_radix(address, 16)
            .map_err(|_| Error::invalid("invalid object list RVA"))?;
        let entries = read_table(bytes, &image, rva)?;
        total += entries.len();
        if total > 65536 {
            return Err(Error::invalid("object definition limit"));
        }
        tables.insert(
            name,
            ObjectList {
                head_rva: rva,
                entries,
            },
        );
    }
    if tables.is_empty() {
        return Err(Error::invalid("no objlist mappings in config"));
    }
    Ok(tables)
}
