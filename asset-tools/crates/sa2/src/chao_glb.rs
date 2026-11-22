//! A Chao model as an animated glTF 2.0 binary: the game's own node tree in native
//! space (one glTF node per native node, named `NNN_object_<address>` by tree order),
//! each attachment's strips as that node's mesh, and motions baked one key per native
//! frame. Nothing is retargeted, recoloured or rescaled; hidden nodes stay undrawn.
use crate::chao_dll::ChaoMotion;
use crate::chao_palette::{Colors, Ratios};
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
/// index the model's materials name; an index without a PNG is an error. Nodes listed in
/// `undrawn` (tree order numbers) keep their place in the tree but get no mesh.
pub fn export(
    model: &Model,
    pngs: &BTreeMap<u16, Vec<u8>>,
    clips: &[(&str, &ChaoMotion)],
    undrawn: &[usize],
) -> Result<Vec<u8>> {
    export_family(model, pngs, clips, undrawn, &Family::default())
}

/// The other models of a body family, written as morph targets of the base model.
#[derive(Default)]
pub struct Family<'a> {
    /// Family name for the record, e.g. `al_ncz`.
    pub name: &'a str,
    /// Target name and model; every model shares the base model's tree and vertex counts.
    pub targets: &'a [(&'a str, &'a Model)],
    /// Texture names by index, written as image names (the page tells face textures by name).
    pub texture_names: BTreeMap<u16, String>,
}

/// One node of a model as the family blend needs it.
struct Part {
    trs: ([f32; 3], [f32; 4], [f32; 3]),
    positions: BTreeMap<usize, [f32; 3]>,
    /// Diffuse per strip chunk, RGBA 0..1.
    diffuse: Vec<[f64; 4]>,
}

fn parts(model: &Model) -> Result<Vec<Part>> {
    let rest = evaluate_model_pose(model, None, 0.)?;
    let attachments: BTreeMap<_, _> = model.attachments.iter().map(|a| (a.offset, a)).collect();
    let mut out: Vec<Option<Part>> = model.nodes.iter().map(|_| None).collect();
    for pose in &rest.nodes {
        let i = model
            .nodes
            .iter()
            .position(|n| n.offset == pose.offset)
            .ok_or_else(|| Error::invalid("posed node outside the model"))?;
        let (mut positions, mut diffuse) = (BTreeMap::new(), Vec::new());
        if let Some(attachment) = attachments.get(&pose.attach_offset) {
            for chunk in &attachment.vertex_chunks {
                for (n, v) in chunk.vertices.iter().enumerate() {
                    positions.insert(chunk.index_offset as usize + n, v.position);
                }
            }
            let mut material = Material::default();
            for chunk in &attachment.polygon_chunks {
                match chunk.kind {
                    8 | 17..=23 => material.apply(chunk)?,
                    64..=66 => diffuse.push(
                        material
                            .diffuse_rgba
                            .unwrap_or([255; 4])
                            .map(|x| f64::from(x) / 255.),
                    ),
                    _ => {}
                }
            }
        }
        out[i] = Some(Part {
            trs: trs(&pose.local_matrix)?,
            positions,
            diffuse,
        });
    }
    out.into_iter()
        .map(|p| p.ok_or_else(|| Error::invalid("model node without a pose")))
        .collect()
}

/// q = inverse(a) * b for unit quaternions [x, y, z, w].
fn quat_between(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (ax, ay, az, aw) = (-a[0], -a[1], -a[2], a[3]);
    [
        aw * b[0] + ax * b[3] + ay * b[2] - az * b[1],
        aw * b[1] - ax * b[2] + ay * b[3] + az * b[0],
        aw * b[2] + ax * b[1] - ay * b[0] + az * b[3],
        aw * b[3] - ax * b[0] - ay * b[1] - az * b[2],
    ]
}

/// [`export`] with `family.targets` as morph targets: vertex differences in each node's own
/// space, plus on the root node `extras.chaoMorph` with every target's node rest difference
/// (t = target - base, q = inverse(base) * target, s = target / base) and material diffuse.
/// With targets the face nodes are tagged for the page (`extras.chaoFace`): eyes 18 and 21,
/// eyelids 19 and 22, and each textured part of mouth node 27 as a node of its own.
pub fn export_family(
    model: &Model,
    pngs: &BTreeMap<u16, Vec<u8>>,
    clips: &[(&str, &ChaoMotion)],
    undrawn: &[usize],
    family: &Family,
) -> Result<Vec<u8>> {
    let morph = !family.targets.is_empty();
    let base_parts = parts(model)?;
    let target_parts = family
        .targets
        .iter()
        .map(|(_, m)| {
            let p = parts(m)?;
            if p.len() != base_parts.len() {
                return Err(Error::invalid("family model with another tree"));
            }
            Ok(p)
        })
        .collect::<Result<Vec<_>>>()?;
    let target_names: Vec<&str> = family.targets.iter().map(|t| t.0).collect();
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
        let mut image = json!({"mimeType":"image/png","bufferView":buffers.view(bytes,None)?,"extras":{"sa2TextureIndex":id}});
        if let Some(name) = family.texture_names.get(&id) {
            image["name"] = name.as_str().into();
        }
        images.push(image);
    }
    let (mut meshes, mut materials, mut textures, mut samplers) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut nodes = vec![Value::Null; model.nodes.len()];
    let mut mouth_nodes = Vec::new();
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
            .filter(|_| pose.flags & HIDE == 0 && !undrawn.contains(&i))
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
                        let mut deltas = vec![Vec::new(); target_parts.len()];
                        for strip in read_strips(chunk, &slots)? {
                            for corner in
                                strip.triangles.iter().flatten().map(|c| &strip.corners[*c])
                            {
                                let v = vertices[&corner.vertex];
                                positions.push(v.position);
                                for (delta, target) in deltas.iter_mut().zip(&target_parts) {
                                    let p = target[i].positions.get(&corner.vertex).ok_or_else(
                                        || Error::invalid("family model with other vertices"),
                                    )?;
                                    delta.push([0, 1, 2].map(|k| p[k] - v.position[k]));
                                }
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
                        let mut primitive =
                            json!({"attributes":attributes,"material":materials.len()-1,"mode":4});
                        if morph {
                            let targets = deltas
                                .iter()
                                .map(|d| Ok(json!({"POSITION":buffers.attribute(d,"VEC3",true)?})))
                                .collect::<Result<Vec<_>>>()?;
                            primitive["targets"] = targets.into();
                        }
                        primitives.push((primitive, material.texture_id));
                    }
                    kind => {
                        return Err(Error::invalid(format!(
                            "unsupported Chao polygon chunk {kind}"
                        )))
                    }
                }
            }
            let mut mesh = |name: String, primitives: Vec<Value>| {
                let mut mesh = json!({"name":name,"primitives":primitives});
                if morph {
                    mesh["weights"] = vec![0.; target_names.len()].into();
                    mesh["extras"] = json!({"targetNames":target_names});
                }
                meshes.push(mesh);
                meshes.len() - 1
            };
            let name = format!("attach_{:08X}", attachment.offset);
            if morph && i == 27 {
                // The mouth's two textured parts are shown and hidden apart.
                for (slot, (primitive, texture)) in primitives.into_iter().enumerate() {
                    let texture = texture.and_then(|id| family.texture_names.get(&id));
                    mouth_nodes.push((
                        i,
                        json!({"name":format!("{i:03}_mouth_{slot}"),
                        "mesh":mesh(format!("{name}_{slot}"), vec![primitive]),
                        "extras":{"chaoFace":"mouth","mouthTexture":texture}}),
                    ));
                }
            } else if !primitives.is_empty() {
                let primitives = primitives.into_iter().map(|p| p.0).collect();
                node["mesh"] = mesh(name, primitives).into();
            }
            if morph {
                match i {
                    18 | 21 => node["extras"]["chaoFace"] = "eye".into(),
                    19 | 22 => {
                        node["extras"]["chaoFace"] = "eyelid".into();
                        node["extras"]["lidSide"] = if i == 19 { "left" } else { "right" }.into();
                    }
                    _ => {}
                }
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
        let Some(number) = clip.number else {
            rows.push(json!({"nativeName":name}));
            continue;
        };
        rows.push(json!({"nativeName":name,"nativeState":number,"tableProperties":{"LoopProperty":clip.mode,"NextAnimation":clip.next,
            "TransitionSpeed":clip.link_step,"StartFrame":clip.start_frame,"EndFrame":clip.end_frame,"PlaySpeed":clip.play_speed}}));
    }
    for (parent, node) in mouth_nodes {
        let at = nodes.len();
        match nodes[parent]["children"].as_array_mut() {
            Some(children) => children.push(at.into()),
            None => nodes[parent]["children"] = json!([at]),
        }
        nodes.push(node);
    }
    let root = index_of[&model.root_offset];
    if morph {
        let (mut offsets, mut diffuse) = (serde_json::Map::new(), serde_json::Map::new());
        for (name, target) in target_names.iter().zip(&target_parts) {
            let mut of_target = serde_json::Map::new();
            for (i, (b, t)) in base_parts.iter().zip(target).enumerate() {
                if b.trs != t.trs {
                    let node = nodes[i]["name"].as_str().unwrap_or_default().to_owned();
                    let move_by = [0, 1, 2].map(|k| t.trs.0[k] - b.trs.0[k]);
                    let scale_by = [0, 1, 2].map(|k| t.trs.2[k] / b.trs.2[k]);
                    let turn_by = quat_between(b.trs.1, t.trs.1);
                    of_target.insert(node, json!({"t":move_by,"q":turn_by,"s":scale_by}));
                }
            }
            offsets.insert((*name).to_owned(), of_target.into());
        }
        for (i, b) in base_parts
            .iter()
            .enumerate()
            .filter(|p| !p.1.diffuse.is_empty())
        {
            let mut entry = json!({"base":b.diffuse});
            for (name, target) in target_names.iter().zip(&target_parts) {
                entry[*name] = json!(target[i].diffuse);
            }
            diffuse.insert(format!("{i:03}"), entry);
        }
        nodes[root]["extras"]["chaoMorph"] = json!({"family":family.name,"targets":target_names,
            "nodeOffsets":offsets,"space":"gltf-node-local",
            "materialDiffuse":{"nodes":diffuse,"order":"strip chunks in order, RGBA 0..1"}});
    }
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

/// Face nodes the game does not draw on a Chao with its eyes open and mouth number 0:
/// eyelids 19 and 22 are skipped while CloseAng == -0x4000 (al_draw_B, ChaoMain 0x8CD4..0x8EA8),
/// and mouth 0 puts the fully transparent al_mouth01 on both mouth slots of node 27 (fn_60_17964).
pub const OPEN_EYES_NO_MOUTH: [usize; 3] = [19, 22, 27];

/// Largest decompressed texture archive accepted.
const MAX_ARCHIVE: usize = 16 << 20;

/// One PNG per texture index `model` names, from `al_body.prs` (the Chao texture list,
/// in archive order). Indexed body textures 0..17 take the bank TexBankList gives them
/// (texture id = row * 3 + bank) from the game's blend of `palettes` for this Chao; an
/// indexed texture the Chao's type has no palette for is an error, not a guessed palette.
pub fn body_textures(
    model: &Model,
    al_body_prs: &[u8],
    palettes: &[Colors],
    ratios: &Ratios,
) -> Result<BTreeMap<u16, Vec<u8>>> {
    let data = crate::archive::decompress_prs(al_body_prs, MAX_ARCHIVE)?;
    let entries = crate::archive::read_gvm(&data)?;
    let row = crate::chao_palette::rows(ratios.chao_type).map(|r| r.1);
    let palette = crate::chao_palette::blend(palettes, ratios)?;
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
        let id3 = id as usize;
        let colors = match &palette {
            Some(p) if row == Some(id3 / 3) => Some(&p[id3 % 3 * 16..][..16]),
            _ => None,
        };
        let mut png = Vec::new();
        crate::texture::decode_gvr(entry.data, colors)?.write_png(&mut png)?;
        pngs.insert(id, png);
    }
    Ok(pngs)
}

/// Texture names of `al_body.prs` by index.
pub fn body_texture_names(al_body_prs: &[u8]) -> Result<BTreeMap<u16, String>> {
    let data = crate::archive::decompress_prs(al_body_prs, MAX_ARCHIVE)?;
    let entries = crate::archive::read_gvm(&data)?;
    Ok((0..)
        .zip(entries.iter().map(|e| e.name.to_string()))
        .collect())
}

/// Target names and model numbers of the body family whose first model is `base`
/// (AL_RootObject holds 18 families of six: z, n, s, f, r, p). The child family 0 also takes
/// the hero and dark child sets, as the game's child blend moves toward them by alignment.
pub fn family_targets(base: usize) -> Result<Vec<(String, usize)>> {
    const SHAPES: [&str; 6] = ["z", "n", "s", "f", "r", "p"];
    if base % 6 != 0 || base > 102 || (6..18).contains(&base) {
        return Err(Error::invalid("not the first model of a body family"));
    }
    let mut out: Vec<_> = (1..6).map(|k| (SHAPES[k].to_owned(), base + k)).collect();
    if base == 0 {
        for (set, first) in [("h", 6), ("d", 12)] {
            out.extend((0..6).map(|k| (format!("{set}{}", SHAPES[k]), first + k)));
        }
    }
    Ok(out)
}
