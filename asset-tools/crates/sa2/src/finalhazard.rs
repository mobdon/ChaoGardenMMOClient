//! Finalhazard actor models. Addresses in the report are RVAs of the pinned PC image.
use crate::{model::Reader, object_table, Error, Result};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub struct Joint {
    rva: u32,
    parent: Option<usize>,
    flags: u32,
    position: [f32; 3],
    rotation: [i32; 3],
    scale: [f32; 3],
    attachment_rva: Option<u32>,
}

pub fn read(exe: &[u8]) -> Result<[Vec<Joint>; 2]> {
    let image = object_table::verify_image(exe)?;
    let r = Reader { data: exe };
    let tree = |root| -> Result<Vec<Joint>> {
        let mut nodes = Vec::new();
        let mut seen = BTreeSet::new();
        let mut todo = vec![(root, None)];
        while let Some((rva, parent)) = todo.pop() {
            if !seen.insert(rva) {
                return Err(Error::invalid("repeated Finalhazard model joint"));
            }
            let at = image.rva_to_offset(rva, 52)?;
            let ptr = |field| -> Result<Option<u32>> {
                let va = r.u32(at + field)?;
                if va == 0 {
                    return Ok(None);
                }
                image.va_to_offset(va, 4)?;
                Ok(Some(va - image.image_base))
            };
            let vector = |field| -> Result<[u32; 3]> {
                Ok([
                    r.u32(at + field)?,
                    r.u32(at + field + 4)?,
                    r.u32(at + field + 8)?,
                ])
            };
            let index = nodes.len();
            if let Some(sibling) = ptr(48)? {
                todo.push((sibling, parent));
            }
            if let Some(child) = ptr(44)? {
                todo.push((child, Some(index)));
            }
            nodes.push(Joint {
                rva,
                parent,
                flags: r.u32(at)?,
                position: vector(8)?.map(f32::from_bits),
                rotation: vector(20)?.map(|v| v as i32),
                scale: vector(32)?.map(f32::from_bits),
                attachment_rva: ptr(4)?,
            });
        }
        Ok(nodes)
    };
    Ok([tree(0x13048dc)?, tree(0x12d36bc)?])
}

/// Decode shared geometry once. Nested stream offsets are executable file
/// offsets; the attachment map keys are RVAs matching the hierarchy report.
pub fn geometry(
    exe: &[u8],
) -> Result<std::collections::BTreeMap<u32, crate::model::ChunkAttachment>> {
    let rigs = read(exe)?;
    let image = crate::pe::read_pe(exe)?;
    let r = Reader { data: exe };
    let mut result = std::collections::BTreeMap::new();
    let mut budget = crate::model::DecodeBudget::new();
    for joint in rigs.into_iter().flatten() {
        let Some(rva) = joint.attachment_rva else {
            continue;
        };
        if result.contains_key(&rva) {
            continue;
        }
        let at = image.rva_to_offset(rva, 24)?;
        let stream = |field| -> Result<u32> {
            let va = r.u32(at + field)?;
            if va == 0 {
                Ok(0)
            } else {
                Ok(image.va_to_offset(va, 4)? as u32)
            }
        };
        result.insert(
            rva,
            crate::model::read_chunk_streams(exe, at as u32, stream(0)?, stream(4)?, &mut budget)?,
        );
    }
    Ok(result)
}

/// Resolve the original model texlist by name; archive order is not a binding.
pub fn textures(exe: &[u8], prs: &[u8]) -> Result<serde_json::Value> {
    use crate::{archive, manifest::sha256, texture};
    let image = object_table::verify_image(exe)?;
    if sha256(prs) != "d4b805a4b0cd5de1ea696c9e031955d5e803e09f4252fa871563144b88bd0f48" {
        return Err(Error::invalid(
            "unrecognized Finalhazard model texture archive",
        ));
    }
    let r = Reader { data: exe };
    let list = image.rva_to_offset(0x13049d8, 8)?;
    let count = r.u32(list + 4)? as usize;
    let names = image.va_to_offset(r.u32(list)?, count * 12)?;
    let raw = archive::decompress_prs(prs, 32 * 1024 * 1024)?;
    let entries = archive::read_gvm(&raw)?;
    let mut bindings = Vec::new();
    for slot in 0..count {
        let at = image.va_to_offset(r.u32(names + slot * 12)?, 32)?;
        let name = r.string(at, at + 32)?;
        let mut matches = entries
            .iter()
            .filter(|e| e.name.eq_ignore_ascii_case(&name));
        let e = matches
            .next()
            .ok_or_else(|| Error::invalid(format!("missing Finalhazard texture {name}")))?;
        if matches.next().is_some() {
            return Err(Error::invalid("ambiguous Finalhazard texture"));
        }
        let tex = texture::decode_gvr(e.data, None)?;
        let mut alpha = [0usize; 3];
        for pixel in tex.rgba.chunks_exact(4) {
            alpha[match pixel[3] {
                0 => 0,
                255 => 2,
                _ => 1,
            }] += 1;
        }
        bindings.push(serde_json::json!({"slot":slot,"name":name,"archiveIndex":e.index,"width":tex.width,"height":tex.height,
            "sourceSha256":sha256(e.data),"rgbaSha256":sha256(&tex.rgba),"alphaPixels":{"transparent":alpha[0],"partial":alpha[1],"opaque":alpha[2]}}));
    }
    Ok(serde_json::json!({"texlistRva":0x13049d8,"bindings":bindings}))
}
