//! Bounded native model hierarchy/vertex conversion. Native units and bits are retained.
//! Layout references: X-Hax/sa_tools SAModel ModelFile, NJS_OBJECT, VertexChunk.

use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_ITEMS: usize = 65_536;

/// Shared across an entire decoded file, including aliased source arrays.
/// Count bounds alone do not prevent many attachments reusing one large span.
pub(crate) struct DecodeBudget {
    remaining: usize,
}

impl DecodeBudget {
    pub fn new() -> Self {
        Self {
            remaining: 64 * 1024 * 1024,
        }
    }
    pub fn charge(&mut self, count: usize, item_bytes: usize) -> Result<()> {
        let cost = count
            .checked_mul(item_bytes)
            .ok_or_else(|| Error::invalid("native decoded allocation overflow"))?;
        self.remaining = self
            .remaining
            .checked_sub(cost)
            .ok_or_else(|| Error::invalid("native decoded allocation budget exceeded"))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataChunk {
    pub kind: u32,
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub format: String,
    pub version: u8,
    pub root_offset: u32,
    pub metadata: Vec<MetadataChunk>,
    pub labels: BTreeMap<u32, String>,
    pub nodes: Vec<Node>,
    pub attachments: Vec<ChunkAttachment>,
    pub unknowns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub offset: u32,
    pub parent_offset: Option<u32>,
    pub flags: u32,
    pub attach_offset: u32,
    pub position: [f32; 3],
    pub rotation_bams: [i32; 3],
    pub scale: [f32; 3],
    pub child_offset: u32,
    pub sibling_offset: u32,
    pub quaternion_scalar: Option<f32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkAttachment {
    pub offset: u32,
    pub vertex_offset: u32,
    pub polygon_offset: u32,
    pub bounds_center: [f32; 3],
    pub bounds_radius: f32,
    pub vertex_chunks: Vec<VertexChunk>,
    pub polygon_chunks: Vec<PolygonChunk>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexChunk {
    pub offset: u32,
    pub kind: u8,
    pub flags: u8,
    pub index_offset: u16,
    pub length: usize,
    pub vertices: Vec<NativeVertex>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVertex {
    pub position: [f32; 3],
    pub normal: Option<[f32; 3]>,
    /// Every source attribute word, including positions, colors and weights.
    pub raw_words: Vec<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonChunk {
    pub offset: u32,
    pub kind: u8,
    pub flags: u8,
    pub length: usize,
    /// Raw material/strip words retain alpha, texture IDs and unknown flags.
    pub raw_words: Vec<u16>,
    /// Native diffuse color; no opacity override or eye/body adaptation.
    pub diffuse_rgba: Option<[u8; 4]>,
    pub texture_id: Option<u16>,
}

pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn span(&self, at: usize, size: usize) -> Result<&'a [u8]> {
        at.checked_add(size)
            .and_then(|end| self.data.get(at..end))
            .ok_or_else(|| Error::invalid(format!("native span out of bounds: {at:#x}+{size:#x}")))
    }
    pub fn u16(&self, at: usize) -> Result<u16> {
        let b = self.span(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&self, at: usize) -> Result<u32> {
        let b = self.span(at, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn f32(&self, at: usize) -> Result<f32> {
        let f = f32::from_bits(self.u32(at)?);
        if !f.is_finite() {
            return Err(Error::invalid(format!(
                "non-finite native float at {at:#x}"
            )));
        }
        Ok(f)
    }
    pub fn vec3(&self, at: usize) -> Result<[f32; 3]> {
        Ok([self.f32(at)?, self.f32(at + 4)?, self.f32(at + 8)?])
    }
    pub fn string(&self, at: usize, end: usize) -> Result<String> {
        let available = end.min(self.data.len());
        let bytes = self.span(
            at,
            available
                .checked_sub(at)
                .ok_or_else(|| Error::invalid("string bounds"))?,
        )?;
        let len = bytes
            .iter()
            .take(4097)
            .position(|&x| x == 0)
            .ok_or_else(|| Error::invalid("unterminated or overlong native string"))?;
        String::from_utf8(bytes[..len].to_vec())
            .map_err(|_| Error::invalid("native string is not UTF-8"))
    }
}

/// Field reads shared by the node and Chunk decoders, so the same decoding
/// serves little-endian PC images and big-endian GameCube-format banks.
pub(crate) trait Words {
    fn span(&self, at: usize, size: usize) -> Result<&[u8]>;
    fn u16(&self, at: usize) -> Result<u16>;
    fn u32(&self, at: usize) -> Result<u32>;
    fn f32(&self, at: usize) -> Result<f32> {
        let f = f32::from_bits(self.u32(at)?);
        if !f.is_finite() {
            return Err(Error::invalid(format!(
                "non-finite native float at {at:#x}"
            )));
        }
        Ok(f)
    }
    fn vec3(&self, at: usize) -> Result<[f32; 3]> {
        Ok([self.f32(at)?, self.f32(at + 4)?, self.f32(at + 8)?])
    }
}
impl Words for Reader<'_> {
    fn span(&self, at: usize, size: usize) -> Result<&[u8]> {
        Reader::span(self, at, size)
    }
    fn u16(&self, at: usize) -> Result<u16> {
        Reader::u16(self, at)
    }
    fn u32(&self, at: usize) -> Result<u32> {
        Reader::u32(self, at)
    }
}

/// Big-endian fields: each 16- and 32-bit field is stored most significant
/// byte first, as in the GameCube-format character model banks.
pub(crate) struct BeReader<'a> {
    pub data: &'a [u8],
}
impl Words for BeReader<'_> {
    fn span(&self, at: usize, size: usize) -> Result<&[u8]> {
        Reader { data: self.data }.span(at, size)
    }
    fn u16(&self, at: usize) -> Result<u16> {
        let b = self.span(at, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32> {
        let b = self.span(at, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

pub(crate) fn metadata(
    data: &[u8],
    version: u8,
    relative_labels: bool,
) -> Result<(Vec<MetadataChunk>, BTreeMap<u32, String>)> {
    let r = Reader { data };
    let mut at = r.u32(12)? as usize;
    let mut chunks = Vec::new();
    let mut labels = BTreeMap::new();
    let mut label_budget = DecodeBudget {
        remaining: 8 * 1024 * 1024,
    };
    if at == 0 {
        return Ok((chunks, labels));
    }
    if version < 2 {
        // Old layouts carry a sentinel-terminated label table (v1 only).
        if version == 1 {
            for _ in 0..MAX_ITEMS {
                let target = r.u32(at)?;
                if target == u32::MAX {
                    return Ok((chunks, labels));
                }
                let ptr = r.u32(at + 4)? as usize;
                let name = r.string(ptr, data.len())?;
                label_budget.charge(1, name.len() + 64)?;
                labels.insert(target, name);
                at += 8;
            }
            return Err(Error::invalid("native label limit exceeded"));
        }
        return Ok((chunks, labels));
    }
    for _ in 0..MAX_ITEMS {
        let kind = r.u32(at)?;
        let length = r.u32(at + 4)? as usize;
        let start = at + 8;
        r.span(start, length)?;
        chunks.push(MetadataChunk {
            kind,
            offset: start,
            length,
        });
        if kind == u32::from_le_bytes(*b"END\0") || kind == u32::MAX {
            return Ok((chunks, labels));
        }
        if kind == u32::from_le_bytes(*b"LABL") {
            let mut entry = start;
            loop {
                if entry + 8 > start + length {
                    return Err(Error::invalid("missing native label sentinel"));
                }
                let target = r.u32(entry)?;
                let pointer = r.u32(entry + 4)?;
                if target == u32::MAX && pointer == u32::MAX {
                    break;
                }
                let pointer = if relative_labels {
                    start
                        .checked_add(pointer as usize)
                        .ok_or_else(|| Error::invalid("label pointer overflow"))?
                } else {
                    pointer as usize
                };
                let end = if relative_labels {
                    start + length
                } else {
                    data.len()
                };
                let name = r.string(pointer, end)?;
                label_budget.charge(1, name.len() + 64)?;
                if labels.insert(target, name).is_some() {
                    return Err(Error::invalid("duplicate native label offset"));
                }
                if labels.len() > MAX_ITEMS {
                    return Err(Error::invalid("native label limit exceeded"));
                }
                entry += 8;
            }
        }
        at = start + length;
    }
    Err(Error::invalid("native metadata chunk limit exceeded"))
}

pub fn read_nodes_at(data: &[u8], root: u32, limit: usize) -> Result<Vec<Node>> {
    read_nodes(data, root, limit, false, &mut DecodeBudget::new())
}

pub(crate) fn read_nodes(
    data: &[u8],
    root: u32,
    limit: usize,
    gc: bool,
    budget: &mut DecodeBudget,
) -> Result<Vec<Node>> {
    let r = Reader {
        data: data
            .get(..limit)
            .ok_or_else(|| Error::invalid("node region limit"))?,
    };
    read_nodes_in(&r, root, gc, budget)
}

pub(crate) fn read_nodes_in<R: Words>(
    r: &R,
    root: u32,
    gc: bool,
    budget: &mut DecodeBudget,
) -> Result<Vec<Node>> {
    if root == 0 {
        return Err(Error::invalid("null native root node"));
    }
    let mut pending = vec![(root, None)];
    let mut visited = BTreeSet::new();
    let mut nodes = Vec::new();
    while let Some((offset, parent_offset)) = pending.pop() {
        if !visited.insert(offset) {
            return Err(Error::invalid("native node cycle or shared tree node"));
        }
        if nodes.len() >= MAX_ITEMS {
            return Err(Error::invalid("native node limit exceeded"));
        }
        let at = offset as usize;
        r.span(at, if gc { 56 } else { 52 })?;
        let child = r.u32(at + 44)?;
        let sibling = r.u32(at + 48)?;
        budget.charge(1, std::mem::size_of::<Node>() * 2 + 64)?;
        nodes.push(Node {
            offset,
            parent_offset,
            flags: r.u32(at)?,
            attach_offset: r.u32(at + 4)?,
            position: r.vec3(at + 8)?,
            rotation_bams: [
                r.u32(at + 20)? as i32,
                r.u32(at + 24)? as i32,
                r.u32(at + 28)? as i32,
            ],
            scale: r.vec3(at + 32)?,
            child_offset: child,
            sibling_offset: sibling,
            quaternion_scalar: if gc { Some(r.f32(at + 52)?) } else { None },
        });
        if sibling != 0 {
            pending.push((sibling, parent_offset));
        }
        if child != 0 {
            pending.push((child, Some(offset)));
        }
    }
    Ok(nodes)
}

pub(crate) fn read_chunk_attachment(
    data: &[u8],
    offset: u32,
    budget: &mut DecodeBudget,
) -> Result<ChunkAttachment> {
    read_chunk_attachment_in(&Reader { data }, offset, budget)
}

pub(crate) fn read_chunk_attachment_in<R: Words>(
    r: &R,
    offset: u32,
    budget: &mut DecodeBudget,
) -> Result<ChunkAttachment> {
    let at = offset as usize;
    r.span(at, 24)?;
    read_chunk_streams_in(r, offset, r.u32(at)?, r.u32(at + 4)?, budget)
}

/// Stream addresses already resolved into offsets in the supplied byte image.
pub(crate) fn read_chunk_streams(
    data: &[u8],
    offset: u32,
    vertex_offset: u32,
    polygon_offset: u32,
    budget: &mut DecodeBudget,
) -> Result<ChunkAttachment> {
    read_chunk_streams_in(
        &Reader { data },
        offset,
        vertex_offset,
        polygon_offset,
        budget,
    )
}

pub(crate) fn read_chunk_streams_in<R: Words>(
    r: &R,
    offset: u32,
    vertex_offset: u32,
    polygon_offset: u32,
    budget: &mut DecodeBudget,
) -> Result<ChunkAttachment> {
    let at = offset as usize;
    budget.charge(1, std::mem::size_of::<ChunkAttachment>() * 2)?;
    let mut vertex_chunks = Vec::new();
    if vertex_offset != 0 {
        let mut p = vertex_offset as usize;
        loop {
            let h = r.u32(p)?;
            let kind = h as u8;
            if kind == 255 {
                break;
            }
            if vertex_chunks.len() >= MAX_ITEMS {
                return Err(Error::invalid("vertex chunk limit"));
            }
            let size = (h >> 16) as usize;
            let length = (size + 1) * 4;
            if length < 8 {
                return Err(Error::invalid("short vertex chunk"));
            }
            r.span(p, length)?;
            // One 32-bit word: index offset in the low half, count in the high half.
            let w = r.u32(p + 4)?;
            let index_offset = w as u16;
            let count = (w >> 16) as usize;
            let stride = match kind {
                32 => 16,
                33 => 32,
                34 => 12,
                35..=39 => 16,
                41 => 24,
                42..=46 => 28,
                _ => return Err(Error::invalid(format!("unsupported vertex chunk {kind}"))),
            };
            if count.checked_mul(stride).and_then(|n| n.checked_add(8)) != Some(length) {
                return Err(Error::invalid("vertex chunk count/size mismatch"));
            }
            budget.charge(1, std::mem::size_of::<VertexChunk>() * 2)?;
            budget.charge(count, std::mem::size_of::<NativeVertex>() * 2 + stride)?;
            let mut vertices = Vec::with_capacity(count);
            for i in 0..count {
                let v = p + 8 + i * stride;
                let position = r.vec3(v)?;
                let normal = if kind == 33 {
                    Some(r.vec3(v + 16)?)
                } else if kind >= 41 {
                    Some(r.vec3(v + 12)?)
                } else {
                    None
                };
                let raw_words = (0..stride / 4)
                    .map(|j| r.u32(v + j * 4))
                    .collect::<Result<Vec<_>>>()?;
                vertices.push(NativeVertex {
                    position,
                    normal,
                    raw_words,
                });
            }
            vertex_chunks.push(VertexChunk {
                offset: p as u32,
                kind,
                flags: (h >> 8) as u8,
                index_offset,
                length,
                vertices,
            });
            p += length;
        }
    }
    let mut polygon_chunks = Vec::new();
    if polygon_offset != 0 {
        let mut p = polygon_offset as usize;
        loop {
            let h = r.u16(p)?;
            let kind = h as u8;
            if kind == 255 {
                break;
            }
            if polygon_chunks.len() >= MAX_ITEMS {
                return Err(Error::invalid("polygon chunk limit"));
            }
            let length = match kind {
                0..=5 => 2,
                8..=9 => 4,
                16..=31 | 56..=58 | 64..=75 => 4 + r.u16(p + 2)? as usize * 2,
                _ => return Err(Error::invalid(format!("unsupported polygon chunk {kind}"))),
            };
            r.span(p, length)?;
            budget.charge(1, std::mem::size_of::<PolygonChunk>() * 2 + length)?;
            let raw_words = (0..length / 2)
                .map(|i| r.u16(p + i * 2))
                .collect::<Result<Vec<_>>>()?;
            let diffuse_rgba = if (16..=31).contains(&kind) && kind & 1 != 0 {
                if length < 8 {
                    return Err(Error::invalid("short diffuse material chunk"));
                }
                // Two 16-bit fields, low half first: the same value in both byte orders.
                let argb = u32::from(r.u16(p + 4)?) | u32::from(r.u16(p + 6)?) << 16;
                Some([
                    (argb >> 16) as u8,
                    (argb >> 8) as u8,
                    argb as u8,
                    (argb >> 24) as u8,
                ])
            } else {
                None
            };
            let texture_id = if (8..=9).contains(&kind) {
                Some(r.u16(p + 2)? & 0x1fff)
            } else {
                None
            };
            polygon_chunks.push(PolygonChunk {
                offset: p as u32,
                kind,
                flags: (h >> 8) as u8,
                length,
                raw_words,
                diffuse_rgba,
                texture_id,
            });
            p += length;
        }
    }
    Ok(ChunkAttachment {
        offset,
        vertex_offset,
        polygon_offset,
        bounds_center: r.vec3(at + 8)?,
        bounds_radius: r.f32(at + 20)?,
        vertex_chunks,
        polygon_chunks,
    })
}

pub fn read_model(data: &[u8]) -> Result<Model> {
    let r = Reader { data };
    let h = r.span(0, 16)?;
    let (format, gc) = match &h[..7] {
        b"SA2MDL\0" => ("sa2-chunk", false),
        b"SA2BMDL" => ("sa2-battle-gc", true),
        _ => return Err(Error::invalid("unsupported SA2 model magic")),
    };
    let version = h[7];
    if version > 3 {
        return Err(Error::invalid("unsupported SA2 model version"));
    }
    let root_offset = r.u32(8)?;
    let meta = r.u32(12)? as usize;
    let limit = if meta == 0 { data.len() } else { meta };
    let (metadata, labels) = metadata(data, version, version >= 3)?;
    let mut budget = DecodeBudget::new();
    let nodes = read_nodes(data, root_offset, limit, gc, &mut budget)?;
    let mut attachments = Vec::new();
    let mut offsets = BTreeSet::new();
    if !gc {
        for node in &nodes {
            if node.attach_offset != 0 && offsets.insert(node.attach_offset) {
                attachments.push(read_chunk_attachment(
                    &data[..limit],
                    node.attach_offset,
                    &mut budget,
                )?);
            }
        }
    }
    let unknowns = if gc {
        vec!["GC attachments remain opaque; model hierarchy and source offsets retained".into()]
    } else {
        vec!["Polygon chunks preserve raw materials/strips; strip triangulation, draw-cache evaluation, Chao short-weight interpretation and glTF skin/material export remain unimplemented".into()]
    };
    Ok(Model {
        format: format.into(),
        version,
        root_offset,
        metadata,
        labels,
        nodes,
        attachments,
        unknowns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Vec<u8> {
        let mut b = vec![0; 68];
        b[..8].copy_from_slice(b"SA2MDL\0\x03");
        b[8..12].copy_from_slice(&16u32.to_le_bytes());
        for i in [48, 52, 56] {
            b[i..i + 4].copy_from_slice(&1f32.to_le_bytes());
        }
        b
    }
    #[test]
    fn preserves_units_and_rotation() {
        let mut b = model();
        b[24..28].copy_from_slice(&2.5f32.to_le_bytes());
        b[36..40].copy_from_slice(&(-123i32).to_le_bytes());
        let m = read_model(&b).unwrap();
        assert_eq!(m.nodes[0].position[0], 2.5);
        assert_eq!(m.nodes[0].rotation_bams[0], -123);
        assert_eq!(m.nodes[0].scale, [1.; 3]);
    }
    #[test]
    fn rejects_cycle_and_truncation() {
        let mut b = model();
        b[60..64].copy_from_slice(&16u32.to_le_bytes());
        assert!(read_model(&b).is_err());
        assert!(read_model(&b[..40]).is_err());
    }
    #[test]
    fn rejects_non_finite_coordinate() {
        let mut b = model();
        b[24..28].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(read_model(&b).is_err());
    }
    #[test]
    fn rejects_bad_metadata_length() {
        let mut b = model();
        b[12..16].copy_from_slice(&68u32.to_le_bytes());
        b.extend_from_slice(b"LABL\xff\xff\xff\xff");
        assert!(read_model(&b).is_err());
    }
    #[test]
    fn preserves_original_material_alpha() {
        let mut b = model();
        b[20..24].copy_from_slice(&68u32.to_le_bytes());
        b.resize(92, 0);
        b[72..76].copy_from_slice(&92u32.to_le_bytes());
        b.extend_from_slice(&0x2511u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&0xccff_ffffu32.to_le_bytes());
        b.extend_from_slice(&255u16.to_le_bytes());
        let parsed = read_model(&b).unwrap();
        let polygon = &parsed.attachments[0].polygon_chunks[0];
        assert_eq!(polygon.diffuse_rgba, Some([255, 255, 255, 204]));
        assert_eq!(polygon.raw_words, [0x2511, 2, 0xffff, 0xccff]);
    }
    #[test]
    fn rejects_unterminated_polygon_stream_and_attach_pointer() {
        let mut b = model();
        b[20..24].copy_from_slice(&68u32.to_le_bytes());
        b.resize(92, 0);
        b[72..76].copy_from_slice(&92u32.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        assert!(read_model(&b).is_err());
        b[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_model(&b).is_err());
    }
    #[test]
    fn rejects_shared_stream_allocation_amplification() {
        let mut b = model();
        b.resize(92, 0);
        let mut budget = DecodeBudget {
            remaining: 2 * std::mem::size_of::<ChunkAttachment>(),
        };
        read_chunk_attachment(&b, 68, &mut budget).unwrap();
        assert!(read_chunk_attachment(&b, 68, &mut budget).is_err());
        let mut budget = DecodeBudget::new();
        assert!(budget.charge(usize::MAX, 2).is_err());
    }
}
