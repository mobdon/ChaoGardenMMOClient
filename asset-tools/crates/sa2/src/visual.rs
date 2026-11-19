//! Static native-space surfaces for the verified SA2 Chunk profiles.
use crate::model::{Model, PolygonChunk};
use crate::pose::{evaluate_nodes, transform_point};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

/// Bound per-draw metadata independently of vertex/index payload sizes.
pub(crate) const MAX_SURFACES: usize = 4096;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticVisual {
    pub vertices: Vec<Vertex>,
    pub surfaces: Vec<Surface>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vertex {
    pub node_offset: u32,
    pub chunk_offset: u32,
    pub source_index: u16,
    pub world_position: [f64; 3],
    /// Retained in native attachment coordinates (not a world-space normal).
    pub local_normal: Option<[f32; 3]>,
    pub world_normal: Option<[f64; 3]>,
    pub diffuse_rgba: Option<[u8; 4]>,
}

#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub diffuse_rgba: Option<[u8; 4]>,
    pub ambient_rgba: Option<[u8; 4]>,
    pub specular_rgbe: Option<[u8; 4]>,
    pub blend_flags: Option<u8>,
    pub texture_id: Option<u16>,
    pub texture_words: Option<[u16; 2]>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    pub node_offset: u32,
    pub attachment_offset: u32,
    pub polygon_offset: u32,
    pub strip_kind: u8,
    pub strip_flags: u8,
    pub material: Material,
    pub strips: Vec<Strip>,
}

#[derive(Debug, Serialize)]
pub struct Strip {
    pub reversed: bool,
    pub corners: Vec<Corner>,
    /// Wound indices into this strip's corners, retaining source order and seams.
    pub triangles: Vec<[usize; 3]>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Corner {
    pub source_offset: u32,
    pub vertex: usize,
    pub uv_raw: Option<[i16; 2]>,
    pub uv: Option<[f32; 2]>,
}

fn rgba(word: u32) -> [u8; 4] {
    [
        (word >> 16) as u8,
        (word >> 8) as u8,
        word as u8,
        (word >> 24) as u8,
    ]
}

fn normal_transform(matrix: &crate::pose::Matrix4, normal: [f32; 3]) -> Result<[f64; 3]> {
    // Columns of the cofactor matrix; divide by the signed determinant
    // before normalizing so reflected hierarchies retain the authored direction.
    let column = |i: usize| [matrix[0][i], matrix[1][i], matrix[2][i]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let [a, b, c] = [column(0), column(1), column(2)];
    let cof = [cross(b, c), cross(c, a), cross(a, b)];
    let det = (0..3).map(|i| a[i] * cof[0][i]).sum::<f64>();
    if !det.is_finite() || det == 0. {
        return Err(Error::invalid("singular normal transform"));
    }
    let n: [f64; 3] = std::array::from_fn(|i| {
        (0..3)
            .map(|j| cof[j][i] * f64::from(normal[j]))
            .sum::<f64>()
            / det
    });
    let length = n[0].hypot(n[1]).hypot(n[2]);
    if length == 0. || !length.is_finite() {
        return Err(Error::invalid("invalid static normal"));
    }
    Ok(n.map(|x| x / length))
}

pub(crate) fn read_strips(chunk: &PolygonChunk, indices: &BTreeMap<u16, usize>) -> Result<Vec<Strip>> {
    let words = &chunk.raw_words;
    let header = *words
        .get(2)
        .ok_or_else(|| Error::invalid("missing strip count"))?;
    if header >> 14 != 0 {
        return Err(Error::invalid("unsupported per-triangle attributes"));
    }
    let mut cursor = 3;
    let mut next = || {
        let value = words
            .get(cursor)
            .copied()
            .ok_or_else(|| Error::invalid("truncated strip"))?;
        cursor += 1;
        Ok::<_, Error>(value)
    };
    let mut strips = Vec::new();
    let mut word_index = 3u32;
    for _ in 0..header {
        let signed_length = next()? as i16;
        word_index += 1;
        let count = signed_length.unsigned_abs() as usize;
        if count < 3 {
            return Err(Error::invalid("short triangle strip"));
        }
        let mut corners = Vec::new();
        let mut triangles = Vec::new();
        for i in 0..count {
            let source_offset = chunk
                .offset
                .checked_add(word_index * 2)
                .ok_or_else(|| Error::invalid("strip offset overflow"))?;
            let vertex = *indices
                .get(&next()?)
                .ok_or_else(|| Error::invalid("strip references unavailable static vertex"))?;
            let uv_raw = if chunk.kind == 64 {
                None
            } else {
                Some([next()? as i16, next()? as i16])
            };
            word_index += if uv_raw.is_some() { 3 } else { 1 };
            let divisor = if chunk.kind == 66 { 1024. } else { 256. };
            corners.push(Corner {
                source_offset,
                vertex,
                uv_raw,
                uv: uv_raw.map(|pair| pair.map(|x| x as f32 / divisor)),
            });
            if i >= 2 {
                let (a, b) = if (signed_length < 0) ^ (i % 2 == 1) {
                    (i - 1, i - 2)
                } else {
                    (i - 2, i - 1)
                };
                if corners[a].vertex != corners[b].vertex
                    && corners[a].vertex != vertex
                    && corners[b].vertex != vertex
                {
                    triangles.push([a, b, i]);
                }
            }
        }
        strips.push(Strip {
            reversed: signed_length < 0,
            corners,
            triangles,
        });
    }
    if cursor != words.len() && !(cursor % 2 == 1 && words[cursor..] == [0]) {
        return Err(Error::invalid("trailing strip words"));
    }
    Ok(strips)
}

pub fn evaluate_static_model(model: &Model) -> Result<StaticVisual> {
    if model.format != "sa2-chunk" {
        return Err(Error::invalid("static visual requires SA2 Chunk model"));
    }
    let attachments: BTreeMap<_, _> = model.attachments.iter().map(|a| (a.offset, a)).collect();
    let mut result = StaticVisual {
        vertices: Vec::new(),
        surfaces: Vec::new(),
    };
    let mut material = Material::default();
    let mut remaining_words = 2_000_000usize;
    if model
        .nodes
        .iter()
        .any(|n| n.flags & 0x10 != 0 && n.child_offset != 0)
    {
        return Err(Error::invalid("unsupported SkipChildren with children"));
    }
    for node in evaluate_nodes(&model.nodes)? {
        if node.attach_offset == 0 || node.flags & 8 != 0 {
            continue;
        }
        let attachment = attachments
            .get(&node.attach_offset)
            .ok_or_else(|| Error::invalid("missing visual attachment"))?;
        let mut indices = BTreeMap::new();
        for chunk in &attachment.vertex_chunks {
            if !matches!(chunk.kind, 34 | 35 | 41) || chunk.flags != 0 {
                return Err(Error::invalid(format!(
                    "unsupported static vertex chunk {} flags {}",
                    chunk.kind, chunk.flags
                )));
            }
            remaining_words = remaining_words
                .checked_sub(2 + chunk.vertices.len() * 4)
                .ok_or_else(|| Error::invalid("static visual output limit"))?;
            for (i, v) in chunk.vertices.iter().enumerate() {
                let index = u16::try_from(i + usize::from(chunk.index_offset))
                    .map_err(|_| Error::invalid("vertex index overflow"))?;
                if indices.insert(index, result.vertices.len()).is_some() {
                    return Err(Error::invalid("overlapping static vertex indices"));
                }
                let diffuse_rgba = if chunk.kind == 35 {
                    Some(rgba(
                        *v.raw_words
                            .get(3)
                            .ok_or_else(|| Error::invalid("missing vertex diffuse"))?,
                    ))
                } else {
                    None
                };
                let world_position = transform_point(&node.world_matrix, v.position.map(f64::from));
                if !world_position.iter().all(|x| x.is_finite()) {
                    return Err(Error::invalid("invalid world vertex"));
                }
                result.vertices.push(Vertex {
                    node_offset: node.offset,
                    chunk_offset: chunk.offset,
                    source_index: index,
                    world_position,
                    local_normal: v.normal,
                    world_normal: v
                        .normal
                        .map(|n| normal_transform(&node.world_matrix, n))
                        .transpose()?,
                    diffuse_rgba,
                });
            }
        }
        for chunk in &attachment.polygon_chunks {
            remaining_words = remaining_words
                .checked_sub(chunk.raw_words.len())
                .ok_or_else(|| Error::invalid("static visual output limit"))?;
            match chunk.kind {
                0 => {}
                8 => {
                    let words: [u16; 2] = chunk
                        .raw_words
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::invalid("invalid texture chunk"))?;
                    material.texture_words = Some(words);
                    material.texture_id = chunk.texture_id;
                }
                17..=23 => {
                    let (pairs, remainder) = chunk
                        .raw_words
                        .get(2..)
                        .unwrap_or_default()
                        .as_chunks::<2>();
                    let mut colors = pairs.iter();
                    for (bit, field) in [
                        (1, &mut material.diffuse_rgba),
                        (2, &mut material.ambient_rgba),
                        (4, &mut material.specular_rgbe),
                    ] {
                        if chunk.kind & bit != 0 {
                            let pair = colors
                                .next()
                                .ok_or_else(|| Error::invalid("truncated material"))?;
                            *field = Some(rgba(u32::from(pair[0]) | (u32::from(pair[1]) << 16)));
                        }
                    }
                    if colors.next().is_some() || !remainder.is_empty() {
                        return Err(Error::invalid("trailing material words"));
                    }
                    material.blend_flags = Some(chunk.flags);
                }
                64..=66 if result.surfaces.len() >= MAX_SURFACES => {
                    return Err(Error::invalid("static visual surface limit"))
                }
                64..=66 => result.surfaces.push(Surface {
                    node_offset: node.offset,
                    attachment_offset: attachment.offset,
                    polygon_offset: chunk.offset,
                    strip_kind: chunk.kind,
                    strip_flags: chunk.flags,
                    material: material.clone(),
                    strips: read_strips(chunk, &indices)?,
                }),
                kind => {
                    return Err(Error::invalid(format!(
                        "unsupported static polygon chunk {kind}"
                    )))
                }
            }
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn strip(words: Vec<u16>) -> PolygonChunk {
        PolygonChunk {
            offset: 100,
            kind: 66,
            flags: 0,
            length: words.len() * 2,
            raw_words: words,
            diffuse_rgba: None,
            texture_id: None,
        }
    }
    #[test]
    fn negative_hd_uv_reversed_strip_and_word_provenance() {
        let chunk = strip(vec![
            66,
            11,
            1,
            (-3i16) as u16,
            2,
            (-512i16) as u16,
            2048,
            1,
            0,
            0,
            0,
            0,
            0,
            0,
        ]);
        let result = read_strips(&chunk, &BTreeMap::from([(0, 4), (1, 5), (2, 6)])).unwrap();
        assert_eq!(result[0].triangles, [[1, 0, 2]]);
        assert_eq!(result[0].corners[0].uv, Some([-0.5, 2.0]));
        assert_eq!(result[0].corners[2].source_offset, 120);
        assert_eq!(result[0].corners[0].vertex, 6);
    }
    #[test]
    fn rejects_unavailable_indices_and_nonzero_padding() {
        let mut chunk = strip(vec![64, 0, 1, 3, 0, 1, 2, 42]);
        chunk.kind = 64;
        assert!(read_strips(&chunk, &BTreeMap::from([(0, 0), (1, 1), (2, 2)])).is_err());
        chunk.raw_words.pop();
        assert!(read_strips(&chunk, &BTreeMap::new()).is_err());
    }
    #[test]
    fn reflected_nonuniform_scale_uses_inverse_transpose() {
        let mut m = crate::pose::identity_matrix();
        m[0][0] = -2.;
        m[1][1] = 4.;
        let n = normal_transform(&m, [1., 1., 0.]).unwrap();
        assert!((n[0] + 2. / 5f64.sqrt()).abs() < 1e-12);
        assert!((n[1] - 1. / 5f64.sqrt()).abs() < 1e-12);
        m[2][2] = 0.;
        assert!(normal_transform(&m, [1., 1., 0.]).is_err());
    }
    #[test]
    fn bounds_draw_metadata_even_for_empty_strips() {
        use crate::model::{ChunkAttachment, Node};
        let node = Node {
            offset: 1,
            parent_offset: None,
            flags: 0,
            attach_offset: 2,
            position: [0.; 3],
            rotation_bams: [0; 3],
            scale: [1.; 3],
            child_offset: 0,
            sibling_offset: 0,
            quaternion_scalar: None,
        };
        let polygons = (0..=MAX_SURFACES)
            .map(|_| {
                let mut chunk = strip(vec![64, 1, 0]);
                chunk.kind = 64;
                chunk
            })
            .collect();
        let attachment = ChunkAttachment {
            offset: 2,
            vertex_offset: 0,
            polygon_offset: 4,
            bounds_center: [0.; 3],
            bounds_radius: 0.,
            vertex_chunks: vec![],
            polygon_chunks: polygons,
        };
        let model = Model {
            format: "sa2-chunk".into(),
            version: 3,
            root_offset: 1,
            metadata: vec![],
            labels: BTreeMap::new(),
            nodes: vec![node],
            attachments: vec![attachment],
            unknowns: vec![],
        };
        assert!(evaluate_static_model(&model)
            .unwrap_err()
            .to_string()
            .contains("surface limit"));
    }
}
