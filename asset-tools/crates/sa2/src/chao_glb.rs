//! A Chao model as an animated glTF 2.0 binary: the game's own node tree in native
//! space (one glTF node per native node, named `NNN_object_<address>` by tree order),
//! each attachment's strips as that node's mesh, and motions baked one key per native
//! frame. Nothing is retargeted, recoloured or rescaled; hidden nodes stay undrawn.
use crate::chao_dll::ChaoMotion;
use crate::model::Model;
use crate::pose::{evaluate_model_pose, Matrix4};
use crate::visual::{read_strips, Material};
use crate::visual_glb::{pack, validate_png, wrap, Buffers};
use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Node flag: the node's own attachment is not drawn.
const HIDE: u32 = 8;
/// Seconds per native frame in the written clips (the game steps motions at 60 Hz).
const FRAME_SECONDS: f32 = 1. / 60.;

/// Translation, rotation (x, y, z, w) and scale of a native local matrix.
fn trs(m: &Matrix4) -> Result<([f32; 3], [f32; 4], [f32; 3])> {
    let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
    let scale = [0, 1, 2].map(|c| column(c).iter().map(|v| v * v).sum::<f64>().sqrt());
    if scale.iter().any(|s| *s < 1e-12) {
        return Err(Error::invalid("degenerate native node scale"));
    }
    let r = |row: usize, c: usize| m[row][c] / scale[c];
    let det = r(0, 0) * (r(1, 1) * r(2, 2) - r(1, 2) * r(2, 1))
        - r(0, 1) * (r(1, 0) * r(2, 2) - r(1, 2) * r(2, 0))
        + r(0, 2) * (r(1, 0) * r(2, 1) - r(1, 1) * r(2, 0));
    if det <= 0. {
        return Err(Error::invalid("mirrored native node scale"));
    }
    let trace = r(0, 0) + r(1, 1) + r(2, 2);
    let q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            (r(2, 1) - r(1, 2)) / s,
            (r(0, 2) - r(2, 0)) / s,
            (r(1, 0) - r(0, 1)) / s,
            s / 4.,
        ]
    } else if r(0, 0) > r(1, 1) && r(0, 0) > r(2, 2) {
        let s = (1. + r(0, 0) - r(1, 1) - r(2, 2)).sqrt() * 2.;
        [
            s / 4.,
            (r(0, 1) + r(1, 0)) / s,
            (r(0, 2) + r(2, 0)) / s,
            (r(2, 1) - r(1, 2)) / s,
        ]
    } else if r(1, 1) > r(2, 2) {
        let s = (1. + r(1, 1) - r(0, 0) - r(2, 2)).sqrt() * 2.;
        [
            (r(0, 1) + r(1, 0)) / s,
            s / 4.,
            (r(1, 2) + r(2, 1)) / s,
            (r(0, 2) - r(2, 0)) / s,
        ]
    } else {
        let s = (1. + r(2, 2) - r(0, 0) - r(1, 1)).sqrt() * 2.;
        [
            (r(0, 2) + r(2, 0)) / s,
            (r(1, 2) + r(2, 1)) / s,
            s / 4.,
            (r(1, 0) - r(0, 1)) / s,
        ]
    };
    Ok((
        [m[0][3] as f32, m[1][3] as f32, m[2][3] as f32],
        q.map(|v| v as f32),
        scale.map(|v| v as f32),
    ))
}

/// `model` with `clips` (name, motion) as one GLB. `pngs` holds one PNG per texture
/// index the model's materials name; an index without a PNG is an error.
pub fn export(
    model: &Model,
    pngs: &BTreeMap<u16, Vec<u8>>,
    clips: &[(&str, &ChaoMotion)],
) -> Result<Vec<u8>> {
    let rest = evaluate_model_pose(model, None, 0.)?;
    let index_of: BTreeMap<u32, usize> = model
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.offset, i))
        .collect();
    let attachments: BTreeMap<_, _> = model.attachments.iter().map(|a| (a.offset, a)).collect();
    let mut buffers = Buffers::default();
    let (mut images, mut image_of) = (Vec::new(), BTreeMap::new());
    for (&id, bytes) in pngs {
        validate_png(bytes)?;
        image_of.insert(id, images.len());
        images.push(json!({"mimeType":"image/png","bufferView":buffers.view(bytes,None)?,"extras":{"sa2TextureIndex":id}}));
    }
    let (mut meshes, mut materials, mut textures, mut samplers) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut nodes = vec![Value::Null; model.nodes.len()];
    for pose in &rest.nodes {
        let i = index_of[&pose.offset];
        let (translation, rotation, scale) = trs(&pose.local_matrix)?;
        let children: Vec<usize> = (0..model.nodes.len())
            .filter(|c| model.nodes[*c].parent_offset == Some(pose.offset))
            .collect();
        let mut node = json!({"name":format!("{i:03}_object_{:08X}", pose.offset),
            "translation":translation,"rotation":rotation,"scale":scale,
            "extras":{"sa2NodeFlags":pose.flags}});
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        if let Some(attachment) = attachments
            .get(&pose.attach_offset)
            .filter(|_| pose.flags & HIDE == 0)
        {
            let mut vertices = BTreeMap::new();
            for chunk in &attachment.vertex_chunks {
                // Position + normal vertices with no weights: the only kind Chao bodies use.
                if chunk.kind != 41 || chunk.flags != 0 {
                    return Err(Error::invalid(format!(
                        "unsupported Chao vertex chunk {} flags {}",
                        chunk.kind, chunk.flags
                    )));
                }
                for (n, v) in chunk.vertices.iter().enumerate() {
                    vertices.insert(chunk.index_offset as usize + n, v);
                }
            }
            let slots: BTreeMap<u16, usize> = vertices.keys().map(|k| (*k as u16, *k)).collect();
            let mut material = Material::default();
            let mut primitives = Vec::new();
            for chunk in &attachment.polygon_chunks {
                match chunk.kind {
                    0 => {}
                    8 | 17..=23 => material.apply(chunk)?,
                    64..=66 => {
                        let (mut positions, mut normals, mut uvs) =
                            (Vec::new(), Vec::new(), Vec::new());
                        for strip in read_strips(chunk, &slots)? {
                            for corner in
                                strip.triangles.iter().flatten().map(|c| &strip.corners[*c])
                            {
                                let v = vertices[&corner.vertex];
                                positions.push(v.position);
                                normals.push(
                                    v.normal.ok_or_else(|| {
                                        Error::invalid("Chao vertex without normal")
                                    })?,
                                );
                                uvs.extend(corner.uv);
                            }
                        }
                        if positions.is_empty() {
                            continue;
                        }
                        let mut attributes = json!({"POSITION":buffers.attribute(&positions,"VEC3",true)?,
                            "NORMAL":buffers.attribute(&normals,"VEC3",false)?});
                        let diffuse = material
                            .diffuse_rgba
                            .unwrap_or([255; 4])
                            .map(|x| f64::from(x) / 255.);
                        let mut pbr = json!({"baseColorFactor":diffuse,"metallicFactor":0.0});
                        if uvs.len() == positions.len() {
                            attributes["TEXCOORD_0"] =
                                buffers.attribute(&uvs, "VEC2", false)?.into();
                            let id = material
                                .texture_id
                                .ok_or_else(|| Error::invalid("textured strip without texture"))?;
                            let image = *image_of.get(&id).ok_or_else(|| {
                                Error::invalid(format!("no PNG for texture index {id}"))
                            })?;
                            let flags = (material.texture_words.map_or(0, |w| w[0]) >> 8) as u8;
                            samplers.push(json!({"wrapS":wrap(flags,32,128),"wrapT":wrap(flags,16,64),"magFilter":9729,"minFilter":9729}));
                            textures.push(json!({"source":image,"sampler":samplers.len()-1}));
                            pbr["baseColorTexture"] = json!({"index":textures.len()-1});
                        }
                        materials.push(json!({"pbrMetallicRoughness":pbr,
                            "alphaMode":if chunk.flags & 8 != 0 {"BLEND"} else {"OPAQUE"},
                            "doubleSided":chunk.flags & 16 != 0,
                            "extras":{"sa2Chunk":{"material":material,"stripKind":chunk.kind,"stripFlags":chunk.flags}}}));
                        primitives.push(
                            json!({"attributes":attributes,"material":materials.len()-1,"mode":4}),
                        );
                    }
                    kind => {
                        return Err(Error::invalid(format!(
                            "unsupported Chao polygon chunk {kind}"
                        )))
                    }
                }
            }
            if !primitives.is_empty() {
                node["mesh"] = meshes.len().into();
                meshes.push(json!({"name":format!("attach_{:08X}", attachment.offset),"primitives":primitives}));
            }
        }
        nodes[i] = node;
    }
    let mut animations = Vec::new();
    let mut rows = Vec::new();
    for (name, clip) in clips {
        let motion = &clip.motion;
        let frames = motion.frame_count;
        let mut poses = Vec::with_capacity(frames as usize);
        for f in 0..frames {
            poses.push(evaluate_model_pose(model, Some(motion), f64::from(f))?);
        }
        let times: Vec<[f32; 1]> = (0..frames).map(|f| [f as f32 * FRAME_SECONDS]).collect();
        let input = buffers.attribute(&times, "SCALAR", true)?;
        let (mut gl_samplers, mut channels) = (Vec::new(), Vec::new());
        for (slot, pose) in poses[0].nodes.iter().enumerate() {
            let Some(part) = pose
                .motion_part_index
                .and_then(|p| motion.parts.get(p as usize))
            else {
                continue;
            };
            let has = |kind: &str| {
                part.channels
                    .iter()
                    .any(|c| c.kind == kind && c.key_count != 0)
            };
            let keys = poses
                .iter()
                .map(|p| trs(&p.nodes[slot].local_matrix))
                .collect::<Result<Vec<_>>>()?;
            let node = index_of[&pose.offset];
            if has("position") {
                let values: Vec<_> = keys.iter().map(|k| k.0).collect();
                gl_samplers.push(json!({"input":input,"output":buffers.attribute(&values,"VEC3",false)?,"interpolation":"LINEAR"}));
                channels.push(json!({"sampler":gl_samplers.len()-1,"target":{"node":node,"path":"translation"}}));
            }
            if has("rotationBams") {
                let mut values: Vec<[f32; 4]> = keys.iter().map(|k| k.1).collect();
                // q and -q are one rotation; keep neighbours on one side so linear keys don't spin.
                for k in 1..values.len() {
                    if (0..4).map(|c| values[k][c] * values[k - 1][c]).sum::<f32>() < 0. {
                        values[k] = values[k].map(|v| -v);
                    }
                }
                gl_samplers.push(json!({"input":input,"output":buffers.attribute(&values,"VEC4",false)?,"interpolation":"LINEAR"}));
                channels.push(
                    json!({"sampler":gl_samplers.len()-1,"target":{"node":node,"path":"rotation"}}),
                );
            }
        }
        if channels.is_empty() {
            return Err(Error::invalid("motion animates no node of this model"));
        }
        animations.push(json!({"name":name,"samplers":gl_samplers,"channels":channels}));
        rows.push(json!({"nativeName":name,"tableProperties":{"LoopProperty":clip.mode,"NextAnimation":clip.next,
            "TransitionSpeed":clip.link_step,"StartFrame":clip.start_frame,"EndFrame":clip.end_frame,"PlaySpeed":clip.play_speed}}));
    }
    let root = index_of[&model.root_offset];
    let mut doc = json!({"asset":{"version":"2.0","generator":"sa2-asset-tools","extras":{"chaoBehaviour":{"clips":rows}}},
        "scene":0,"scenes":[{"nodes":[root]}],"nodes":nodes,"meshes":meshes,"materials":materials,
        "buffers":[{"byteLength":buffers.bytes.len()}],"bufferViews":buffers.views.clone(),"accessors":buffers.accessors.clone()});
    for (key, list) in [
        ("textures", textures),
        ("samplers", samplers),
        ("images", images),
        ("animations", animations),
    ] {
        if !list.is_empty() {
            doc[key] = list.into();
        }
    }
    pack(&doc, buffers)
}

/// Largest decompressed texture archive accepted.
const MAX_ARCHIVE: usize = 16 << 20;

/// One PNG per texture index `model` names, from `al_body.prs` (the Chao texture list,
/// in archive order). The three indexed child body textures take banks 0, 1 and 2 of
/// the neutral child palette `al_nc00.GVP` (mapping: SA Tools ChaoPalettes.cs); any
/// other indexed texture is an error rather than a guessed palette.
pub fn body_textures(
    model: &Model,
    al_body_prs: &[u8],
    child_gvp: &[u8],
) -> Result<BTreeMap<u16, Vec<u8>>> {
    let data = crate::archive::decompress_prs(al_body_prs, MAX_ARCHIVE)?;
    let entries = crate::archive::read_gvm(&data)?;
    let palette = crate::texture::read_gvp(child_gvp)?;
    let mut pngs = BTreeMap::new();
    let ids = model
        .attachments
        .iter()
        .flat_map(|a| &a.polygon_chunks)
        .filter_map(|c| c.texture_id);
    for id in ids {
        if pngs.contains_key(&id) {
            continue;
        }
        let entry = entries
            .get(id as usize)
            .ok_or_else(|| Error::invalid(format!("texture index {id} outside al_body")))?;
        let bank = ["al_child02", "al_child03", "al_child04"]
            .iter()
            .position(|n| entry.name.eq_ignore_ascii_case(n));
        let colors = bank.map(|b| palette.bank(b, 16)).transpose()?;
        let mut png = Vec::new();
        crate::texture::decode_gvr(entry.data, colors)?.write_png(&mut png)?;
        pngs.insert(id, png);
    }
    Ok(pngs)
}
