use crate::{
    finalhazard, pose,
    visual::{self, Material, StaticVisual, Surface, Vertex},
    Error, Result,
};
use std::collections::BTreeMap;

/// Bake the native vertex buffer at each draw; indices may be reused by
/// later joints, so strips retain indices into immutable output vertices.
pub fn sample(exe: &[u8], clip: usize, frame: f64) -> Result<StaticVisual> {
    BossMeshClip::load(exe, clip)?.sample(frame)
}
pub struct BossMeshClip {
    clip: finalhazard::BossClip,
    attachments: BTreeMap<u32, crate::model::ChunkAttachment>,
}
impl BossMeshClip {
    pub fn load(exe: &[u8], clip: usize) -> Result<Self> {
        Ok(Self {
            clip: finalhazard::BossClip::load(exe, clip)?,
            attachments: finalhazard::geometry(exe)?,
        })
    }
    pub fn frame_count(&self) -> u32 {
        self.clip.frame_count()
    }
    pub fn sample(&self, frame: f64) -> Result<StaticVisual> {
        let pose = self.clip.pose(frame)?;
        let attachments = &self.attachments;
        let mut scene = StaticVisual {
            vertices: vec![],
            surfaces: vec![],
        };
        let mut indices = BTreeMap::new();
        let mut pending = BTreeMap::<u16, ([f64; 3], [f64; 3])>::new();
        let mut cache = BTreeMap::new();
        let mut material = Material::default();
        for joint in pose.nodes {
            if joint.attach_offset == 0 {
                continue;
            }
            let a = &attachments[&joint.attach_offset];
            for c in &a.vertex_chunks {
                for (i, v) in c.vertices.iter().enumerate() {
                    let pos = pose::transform_point(&joint.world_matrix, v.position.map(f64::from));
                    let normal = visual::normal_covector(
                        &joint.world_matrix,
                        v.normal
                            .ok_or_else(|| Error::invalid("missing boss normal"))?,
                    )?;
                    let (index, p, n) = match c.kind {
                        41 => (
                            c.index_offset
                                .checked_add(i as u16)
                                .ok_or_else(|| Error::invalid("boss index overflow"))?,
                            pos,
                            normal,
                        ),
                        44 => {
                            let raw = v.raw_words[6];
                            let index = c
                                .index_offset
                                .checked_add(raw as u16)
                                .ok_or_else(|| Error::invalid("boss weight index overflow"))?;
                            let phase = c.flags & 3;
                            if phase == 0 && pending.insert(index, ([0.; 3], [0.; 3])).is_some() {
                                return Err(Error::invalid("duplicate weight start"));
                            }
                            let (p, n) = pending
                                .get_mut(&index)
                                .ok_or_else(|| Error::invalid("missing weight start"))?;
                            let w = f64::from(raw >> 16) / 255.;
                            for k in 0..3 {
                                p[k] += pos[k] * w;
                                n[k] += normal[k] * w;
                            }
                            if phase < 2 {
                                continue;
                            }
                            if phase != 2 {
                                return Err(Error::invalid("invalid boss weight phase"));
                            }
                            let (p, n) = pending.remove(&index).unwrap();
                            (index, p, n)
                        }
                        _ => return Err(Error::invalid("unsupported boss vertex format")),
                    };
                    indices.insert(index, scene.vertices.len());
                    scene.vertices.push(Vertex {
                        node_offset: joint.offset,
                        chunk_offset: c.offset,
                        source_index: index,
                        world_position: p,
                        local_normal: None,
                        world_normal: Some(visual::normalize_normal(n)?),
                        diffuse_rgba: None,
                    });
                }
            }
            let mut emit = |chunk: &crate::model::PolygonChunk, attachment: u32| -> Result<()> {
                match chunk.kind {
                    0 => (),
                    8 | 17..=23 => material.apply(chunk)?,
                    64..=66 => scene.surfaces.push(Surface {
                        node_offset: joint.offset,
                        attachment_offset: attachment,
                        polygon_offset: chunk.offset,
                        strip_kind: chunk.kind,
                        strip_flags: chunk.flags,
                        material: material.clone(),
                        strips: visual::read_strips(chunk, &indices)?,
                    }),
                    _ => return Err(Error::invalid("unsupported boss polygon command")),
                }
                Ok(())
            };
            for (i, c) in a.polygon_chunks.iter().enumerate() {
                match c.kind {
                    4 => {
                        cache.insert(c.flags, (joint.attach_offset, &a.polygon_chunks[i + 1..]));
                        break;
                    }
                    5 => {
                        let &(source, list) = cache
                            .get(&c.flags)
                            .ok_or_else(|| Error::invalid("missing boss polygon list"))?;
                        for op in list {
                            emit(op, source)?;
                        }
                    }
                    _ => emit(c, joint.attach_offset)?,
                }
            }
        }
        if !pending.is_empty() {
            return Err(Error::invalid("incomplete boss weights"));
        }
        Ok(scene)
    }
}
