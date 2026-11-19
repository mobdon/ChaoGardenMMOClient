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
