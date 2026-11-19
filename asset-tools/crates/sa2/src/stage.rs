//! SA2/SA2B landtable descriptors and native Basic collision polygons.
//! Coordinates remain in each original node's local space; no render-heightfield substitution.
use crate::model::{
    metadata, read_chunk_attachment, read_nodes, ChunkAttachment, DecodeBudget, MetadataChunk,
    Node, Reader,
};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub format: String,
    pub version: u8,
    pub offset: u32,
    pub visual_count: i16,
    pub far_clip: f32,
    pub texture_name: Option<String>,
    pub texture_list_offset: u32,
    pub raw_header_words: Vec<u32>,
    pub labels: BTreeMap<u32, String>,
    pub metadata: Vec<MetadataChunk>,
    pub entries: Vec<LandEntry>,
    pub basic_attachments: Vec<BasicAttachment>,
    pub chunk_attachments: Vec<ChunkAttachment>,
    pub unknowns: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandEntry {
    pub index: usize,
    pub offset: u32,
    pub bounds_center: [f32; 3],
    pub bounds_radius: f32,
    pub root_offset: u32,
    pub field_14_bits: u32,
    pub block_bits: u32,
    pub surface_flags: u32,
    pub geometry_format: String,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicAttachment {
    pub offset: u32,
    pub vertex_offset: u32,
    pub normal_offset: u32,
    pub mesh_offset: u32,
    pub material_offset: u32,
    pub bounds_center: [f32; 3],
    pub bounds_radius: f32,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub meshsets: Vec<BasicMeshset>,
    pub material_words: Vec<[u32; 5]>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicMeshset {
    pub offset: u32,
    pub material_id: u16,
    pub polygon_type: u8,
    pub attributes: u32,
    pub polygon_offset: u32,
    pub polygon_normals_offset: u32,
    pub vertex_colors_offset: u32,
    pub uv_offset: u32,
    pub polygons: Vec<BasicPolygon>,
    pub colors_argb: Vec<u32>,
    pub uv_signed: Vec<[i16; 2]>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicPolygon {
    pub offset: u32,
    pub reversed: bool,
    pub indices: Vec<u16>,
}

fn count_u16(r: &Reader<'_>, at: usize) -> Result<usize> {
    let n = r.u16(at)? as i16;
    if n < 0 {
        return Err(Error::invalid("negative native count"));
    }
    Ok(n as usize)
}

pub(crate) fn basic(
    data: &[u8],
    offset: u32,
    budget: &mut DecodeBudget,
) -> Result<BasicAttachment> {
    let r = Reader { data };
    let at = offset as usize;
    r.span(at, 40)?;
    budget.charge(1, std::mem::size_of::<BasicAttachment>() * 2)?;
    let vertex_offset = r.u32(at)?;
    let normal_offset = r.u32(at + 4)?;
    let vertex_count = r.u32(at + 8)? as usize;
    if vertex_count > 262_144 || (vertex_count != 0 && vertex_offset == 0) {
        return Err(Error::invalid("Basic vertex count/pointer outside bounds"));
    }
    r.span(vertex_offset as usize, vertex_count * 12)?;
    budget.charge(vertex_count, 12)?;
    let vertices = (0..vertex_count)
        .map(|i| r.vec3(vertex_offset as usize + i * 12))
        .collect::<Result<Vec<_>>>()?;
    let normals = if normal_offset == 0 {
        None
    } else {
        r.span(normal_offset as usize, vertex_count * 12)?;
        budget.charge(vertex_count, 12)?;
        Some(
            (0..vertex_count)
                .map(|i| r.vec3(normal_offset as usize + i * 12))
                .collect::<Result<Vec<_>>>()?,
        )
    };
    let mesh_offset = r.u32(at + 12)?;
    let material_offset = r.u32(at + 16)?;
    let mesh_count = count_u16(&r, at + 20)?;
    let material_count = count_u16(&r, at + 22)?;
    if (mesh_count != 0 && mesh_offset == 0) || (material_count != 0 && material_offset == 0) {
        return Err(Error::invalid("null Basic mesh/material pointer"));
    }
    r.span(mesh_offset as usize, mesh_count * 24)?;
    budget.charge(mesh_count, std::mem::size_of::<BasicMeshset>() * 2)?;
    let mut meshsets = Vec::with_capacity(mesh_count);
    let mut total_indices = 0usize;
    for i in 0..mesh_count {
        let p = mesh_offset as usize + i * 24;
        let type_material = r.u16(p)?;
        let polygon_type = (type_material >> 14) as u8;
        let polygon_count = count_u16(&r, p + 2)?;
        let polygon_offset = r.u32(p + 4)?;
        if polygon_count != 0 && polygon_offset == 0 {
            return Err(Error::invalid("null Basic polygon pointer"));
        }
        let mut cursor = polygon_offset as usize;
        budget.charge(polygon_count, std::mem::size_of::<BasicPolygon>())?;
        let mut polygons = Vec::with_capacity(polygon_count);
        let mut corners = 0usize;
        for _ in 0..polygon_count {
            let poly_at = cursor;
            let (count, reversed) = match polygon_type {
                0 => (3, false),
                1 => (4, false),
                2 | 3 => {
                    let n = r.u16(cursor)?;
                    cursor += 2;
                    ((n & 0x7fff) as usize, n & 0x8000 != 0)
                }
                _ => unreachable!(),
            };
            if count < 3 {
                return Err(Error::invalid("Basic polygon has fewer than three corners"));
            }
            total_indices = total_indices
                .checked_add(count)
                .ok_or_else(|| Error::invalid("Basic polygon total overflow"))?;
            if total_indices > 2_000_000 {
                return Err(Error::invalid("Basic polygon corner limit exceeded"));
            }
            r.span(cursor, count * 2)?;
            budget.charge(count, 2)?;
            let indices = (0..count)
                .map(|j| r.u16(cursor + j * 2))
                .collect::<Result<Vec<_>>>()?;
            if indices.iter().any(|&n| n as usize >= vertex_count) {
                return Err(Error::invalid("Basic polygon vertex index out of bounds"));
            }
            cursor += count * 2;
            corners += count;
            polygons.push(BasicPolygon {
                offset: poly_at as u32,
                reversed,
                indices,
            });
        }
        let polygon_normals_offset = r.u32(p + 12)?;
        if polygon_normals_offset != 0 {
            r.span(polygon_normals_offset as usize, polygon_count * 12)?;
        }
        let vertex_colors_offset = r.u32(p + 16)?;
        let colors_argb = if vertex_colors_offset == 0 {
            Vec::new()
        } else {
            r.span(vertex_colors_offset as usize, corners * 4)?;
            budget.charge(corners, 4)?;
            (0..corners)
                .map(|j| r.u32(vertex_colors_offset as usize + j * 4))
                .collect::<Result<Vec<_>>>()?
        };
        let uv_offset = r.u32(p + 20)?;
        let uv_signed = if uv_offset == 0 {
            Vec::new()
        } else {
            r.span(uv_offset as usize, corners * 4)?;
            budget.charge(corners, 4)?;
            (0..corners)
                .map(|j| {
                    Ok([
                        r.u16(uv_offset as usize + j * 4)? as i16,
                        r.u16(uv_offset as usize + j * 4 + 2)? as i16,
                    ])
                })
                .collect::<Result<Vec<_>>>()?
        };
        meshsets.push(BasicMeshset {
            offset: p as u32,
            material_id: type_material & 0x3fff,
            polygon_type,
            attributes: r.u32(p + 8)?,
            polygon_offset,
            polygon_normals_offset,
            vertex_colors_offset,
            uv_offset,
            polygons,
            colors_argb,
            uv_signed,
        });
    }
    let effective_material_count = meshsets
        .iter()
        .map(|m| m.material_id as usize + 1)
        .max()
        .unwrap_or(0)
        .max(material_count);
    let material_words = if material_offset == 0 {
        Vec::new()
    } else {
        r.span(material_offset as usize, effective_material_count * 20)?;
        budget.charge(effective_material_count, 20)?;
        (0..effective_material_count)
            .map(|i| {
                let m = material_offset as usize + i * 20;
                Ok([
                    r.u32(m)?,
                    r.u32(m + 4)?,
                    r.u32(m + 8)?,
                    r.u32(m + 12)?,
                    r.u32(m + 16)?,
                ])
            })
            .collect::<Result<Vec<_>>>()?
    };
    Ok(BasicAttachment {
        offset,
        vertex_offset,
        normal_offset,
        mesh_offset,
        material_offset,
        bounds_center: r.vec3(at + 24)?,
        bounds_radius: r.f32(at + 36)?,
        vertices,
        normals,
        meshsets,
        material_words,
    })
}

pub fn read_stage(data: &[u8]) -> Result<Stage> {
    let all = Reader { data };
    let h = all.span(0, 16)?;
    let battle = match &h[..7] {
        b"SA2LVL\0" => false,
        b"SA2BLVL" => true,
        _ => return Err(Error::invalid("unsupported SA2 stage magic")),
    };
    let version = h[7];
    if version > 3 {
        return Err(Error::invalid("unsupported SA2 stage version"));
    }
    let offset = all.u32(8)?;
    let metadata_offset = all.u32(12)? as usize;
    let limit = if metadata_offset == 0 {
        data.len()
    } else {
        metadata_offset
    };
    let (metadata, labels) = metadata(data, version, version >= 3)?;
    let r = Reader {
        data: data
            .get(..limit)
            .ok_or_else(|| Error::invalid("stage metadata offset outside bounds"))?,
    };
    let at = offset as usize;
    r.span(at, 32)?;
    let entry_count = count_u16(&r, at)?;
    let visual_count = r.u16(at + 2)? as i16;
    if visual_count < -1 || visual_count as i32 > entry_count as i32 {
        return Err(Error::invalid("stage visual count outside entry table"));
    }
    let table = r.u32(at + 16)? as usize;
    if table == 0 && entry_count != 0 {
        return Err(Error::invalid("null stage entry table"));
    }
    r.span(table, entry_count * 32)?;
    let mut budget = DecodeBudget::new();
    budget.charge(entry_count, std::mem::size_of::<LandEntry>())?;
    let mut entries = Vec::with_capacity(entry_count);
    let mut basic_attachments = Vec::new();
    let mut chunk_attachments = Vec::new();
    let mut seen_basic = BTreeSet::new();
    let mut seen_chunk = BTreeSet::new();
    let mut total_nodes = 0usize;
    for i in 0..entry_count {
        let p = table + i * 32;
        let surface_flags = r.u32(p + 28)?;
        let is_basic = if visual_count < 0 {
            surface_flags & 0x8000_0000 == 0
        } else {
            i >= visual_count as usize
        };
        let geometry_format = if is_basic {
            "basic"
        } else if battle {
            "gc-opaque"
        } else {
            "chunk"
        };
        let root_offset = r.u32(p + 16)?;
        let nodes = read_nodes(r.data, root_offset, limit, !is_basic && battle, &mut budget)?;
        total_nodes += nodes.len();
        if total_nodes > 262_144 {
            return Err(Error::invalid("stage total node limit exceeded"));
        }
        for node in &nodes {
            if node.attach_offset != 0 {
                if is_basic && seen_basic.insert(node.attach_offset) {
                    basic_attachments.push(basic(r.data, node.attach_offset, &mut budget)?);
                } else if !battle && !is_basic && seen_chunk.insert(node.attach_offset) {
                    chunk_attachments.push(read_chunk_attachment(
                        r.data,
                        node.attach_offset,
                        &mut budget,
                    )?);
                } else if !is_basic && battle {
                    r.span(node.attach_offset as usize, 4)?;
                }
            }
        }
        entries.push(LandEntry {
            index: i,
            offset: p as u32,
            bounds_center: r.vec3(p)?,
            bounds_radius: r.f32(p + 12)?,
            root_offset,
            field_14_bits: r.u32(p + 20)?,
            block_bits: r.u32(p + 24)?,
            surface_flags,
            geometry_format: geometry_format.into(),
            nodes,
        });
    }
    let tex = r.u32(at + 24)? as usize;
    let texture_name = if tex == 0 {
        None
    } else {
        Some(r.string(tex, limit)?)
    };
    let raw_header_words = (0..8)
        .map(|i| r.u32(at + i * 4))
        .collect::<Result<Vec<_>>>()?;
    Ok(Stage { format: if battle { "sa2-battle" } else { "sa2" }.into(), version, offset, visual_count, far_clip: r.f32(at + 12)?, texture_name, texture_list_offset: r.u32(at + 28)?, raw_header_words, labels, metadata, entries, basic_attachments, chunk_attachments, unknowns: vec!["Battle GC render attachments remain opaque; Basic collision polygons retain original winding, surface flags, coordinates and node transforms".into(), "No gameplay collision solver, transformed world triangulation, scene exclusions or navigation heightfield parity is claimed".into()] })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> Vec<u8> {
        let mut b = vec![0; 48];
        b[..8].copy_from_slice(b"SA2BLVL\x03");
        b[8..12].copy_from_slice(&16u32.to_le_bytes());
        b
    }
    #[test]
    fn empty_stage_and_unknown_words_retained() {
        let mut b = empty();
        b[20..24].copy_from_slice(&0x1234u32.to_le_bytes());
        let s = read_stage(&b).unwrap();
        assert_eq!(s.raw_header_words[1], 0x1234);
        assert!(s.entries.is_empty());
    }
    #[test]
    fn rejects_bad_counts_and_pointer() {
        let mut b = empty();
        b[16..18].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(read_stage(&b).is_err());
        let mut b = empty();
        b[16..18].copy_from_slice(&1u16.to_le_bytes());
        b[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_stage(&b).is_err());
    }
    #[test]
    fn rejects_basic_index_outside_vertices() {
        let mut b = vec![0; 106];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[8..12].copy_from_slice(&3u32.to_le_bytes());
        b[12..16].copy_from_slice(&76u32.to_le_bytes());
        b[20..22].copy_from_slice(&1u16.to_le_bytes());
        b[78..80].copy_from_slice(&1u16.to_le_bytes());
        b[80..84].copy_from_slice(&100u32.to_le_bytes());
        b[100..102].copy_from_slice(&0u16.to_le_bytes());
        b[102..104].copy_from_slice(&1u16.to_le_bytes());
        b[104..106].copy_from_slice(&2u16.to_le_bytes());
        let parsed = basic(&b, 0, &mut DecodeBudget::new()).unwrap();
        assert_eq!(parsed.meshsets[0].polygons[0].indices, [0, 1, 2]);
        b[104..106].copy_from_slice(&3u16.to_le_bytes());
        assert!(basic(&b, 0, &mut DecodeBudget::new()).is_err());
    }
}
