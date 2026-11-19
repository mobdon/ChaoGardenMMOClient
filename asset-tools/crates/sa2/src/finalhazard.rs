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
    child_rva: Option<u32>,
    sibling_rva: Option<u32>,
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
                child_rva: ptr(44)?,
                sibling_rva: ptr(48)?,
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
    texture_rows(exe, prs, |_, _| Ok(()))
}
fn texture_rows(
    exe: &[u8],
    prs: &[u8],
    mut visit: impl FnMut(u16, &crate::texture::Image) -> Result<()>,
) -> Result<serde_json::Value> {
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
        visit(slot as u16, &tex)?;
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

/// Four native MOTION_TABLE records. Selection and mode interpretation belong
/// to the encounter controller; the table itself does not trigger transitions.
pub fn motions(exe: &[u8]) -> Result<serde_json::Value> {
    let image = object_table::verify_image(exe)?;
    let r = Reader { data: exe };
    let mut rows = Vec::new();
    for (i, label) in ["intro", "idle", "damaged", "defeated"]
        .into_iter()
        .enumerate()
    {
        let at = image.rva_to_offset(0x4ac678 + 28 * i as u32, 28)?;
        let va = r.u32(at)?;
        image.va_to_offset(va, 12)?;
        let motion = crate::board_motion::pc_transform_motion(exe, va - image.image_base, 46)?;
        rows.push(serde_json::json!({"index":i,"label":label,"mode":r.u16(at+4)? as i16,
            "posture":r.u16(at+6)? as i16,"next":r.u32(at+8)? as i32,"linkStep":r.u32(at+12)? as i32,
            "start":r.f32(at+16)?,"end":r.f32(at+20)?,"speed":r.f32(at+24)?,"motion":motion}));
    }
    Ok(serde_json::json!({"tableRva":0x4ac678,"entries":rows}))
}

/// Offline boss-local pose; no encounter state transitions or world placement.
pub fn pose(exe: &[u8], clip: usize, frame: f64) -> Result<crate::pose::Pose> {
    if clip >= 4 {
        return Err(Error::invalid("Finalhazard clip index outside 0..4"));
    }
    let [boss, _] = read(exe)?;
    let image = crate::pe::read_pe(exe)?;
    let at = image.rva_to_offset(0x4ac678 + 28 * clip as u32, 28)?;
    let va = Reader { data: exe }.u32(at)?;
    image.va_to_offset(va, 12)?;
    let motion = crate::board_motion::pc_transform_motion(exe, va - image.image_base, 46)?;
    if !(0.0..f64::from(motion.frame_count)).contains(&frame) {
        return Err(Error::invalid("frame outside Finalhazard clip"));
    }
    let model = rig_model(&boss);
    crate::pose::evaluate_model_pose(&model, Some(&motion), frame)
}
fn rig_model(boss: &[Joint]) -> crate::model::Model {
    let nodes = boss
        .iter()
        .map(|j| crate::model::Node {
            offset: j.rva,
            parent_offset: j.parent.map(|p| boss[p].rva),
            flags: j.flags,
            attach_offset: j.attachment_rva.unwrap_or(0),
            position: j.position,
            rotation_bams: j.rotation,
            scale: j.scale,
            child_offset: j.child_rva.unwrap_or(0),
            sibling_offset: j.sibling_rva.unwrap_or(0),
            quaternion_scalar: None,
        })
        .collect();
    crate::model::Model {
        format: "sa2-chunk".into(),
        version: 0,
        root_offset: boss[0].rva,
        metadata: vec![],
        labels: Default::default(),
        nodes,
        attachments: vec![],
        unknowns: vec![],
    }
}

/// Standalone weakpoint in its authored local coordinates.
pub fn weakpoint_glb(exe: &[u8], prs: &[u8]) -> Result<Vec<u8>> {
    let [_, weakpoint] = read(exe)?;
    let mut model = rig_model(&weakpoint);
    let attachments = geometry(exe)?;
    for (rva, mut attachment) in attachments {
        if model.nodes.iter().any(|n| n.attach_offset == rva) {
            attachment.offset = rva;
            model.attachments.push(attachment);
        }
    }
    let scene = crate::visual::evaluate_static_model(&model)?;
    let mut pngs = std::collections::BTreeMap::new();
    texture_rows(exe, prs, |id, image| {
        if scene
            .surfaces
            .iter()
            .any(|s| s.material.texture_id == Some(id))
        {
            let mut png = Vec::new();
            image.write_png(&mut png)?;
            pngs.insert(id, png);
        }
        Ok(())
    })?;
    crate::visual_glb::export(&scene, &pngs)
}

/// Replay Ninja weight start/middle/end records in native node traversal order.
/// Preserve individual bone-local inputs; rigid vertices use a separate path.
pub fn weights(exe: &[u8]) -> Result<serde_json::Value> {
    weight_report(exe, None)
}

/// Sample weighted positions in boss-local coordinates (no encounter placement).
pub fn weighted_pose(exe: &[u8], clip: usize, frame: f64) -> Result<serde_json::Value> {
    let pose = pose(exe, clip, frame)?;
    let mut report = weight_report(exe, Some(&pose))?;
    report["clip"] = serde_json::json!(clip);
    report["frame"] = serde_json::json!(frame);
    Ok(report)
}
fn weight_report(exe: &[u8], pose: Option<&crate::pose::Pose>) -> Result<serde_json::Value> {
    use std::collections::BTreeMap;
    let matrices: BTreeMap<_, _> = pose
        .into_iter()
        .flat_map(|p| &p.nodes)
        .map(|n| (n.offset, &n.world_matrix))
        .collect();
    let [boss, _] = read(exe)?;
    let attachments = geometry(exe)?;
    let mut pending = BTreeMap::<u32, (Vec<serde_json::Value>, [f64; 3], [f64; 3])>::new();
    let mut finished = Vec::new();
    for joint in boss {
        let Some(a) = joint.attachment_rva else {
            continue;
        };
        for chunk in &attachments[&a].vertex_chunks {
            if chunk.kind != 44 {
                continue;
            }
            let phase = chunk.flags & 3;
            for v in &chunk.vertices {
                let word = *v
                    .raw_words
                    .get(6)
                    .ok_or_else(|| Error::invalid("missing vertex weight"))?;
                let index = u32::from(chunk.index_offset) + (word & 0xffff);
                if phase == 0
                    && pending
                        .insert(index, (Vec::new(), [0.; 3], [0.; 3]))
                        .is_some()
                {
                    return Err(Error::invalid("unfinished weight at start"));
                }
                if phase > 2 {
                    return Err(Error::invalid("unexpected vertex weight phase"));
                }
                let chain = pending
                    .get_mut(&index)
                    .ok_or_else(|| Error::invalid("weight without start"))?;
                if pose.is_some() {
                    let matrix = matrices
                        .get(&joint.rva)
                        .ok_or_else(|| Error::invalid("missing posed joint"))?;
                    let point = crate::pose::transform_point(matrix, v.position.map(f64::from));
                    // SA3D ChunkVertex.Weight: the high halfword divided by 255.
                    let weight = f64::from(word >> 16) / 255.;
                    let normal = crate::visual::normal_covector(
                        matrix,
                        v.normal
                            .ok_or_else(|| Error::invalid("missing Finalhazard source normal"))?,
                    )?;
                    for (sum, x) in chain.2.iter_mut().zip(normal) {
                        *sum += x * weight;
                    }
                    for (sum, x) in chain.1.iter_mut().zip(point) {
                        *sum += x * weight;
                    }
                }
                chain.0.push(serde_json::json!({"jointRva":joint.rva,"weightByte":(word>>16)&255,"rawFlags":word,
                    "position":v.position,"normal":v.normal}));
                if phase == 2 {
                    let (influences, position, normal) = pending.remove(&index).unwrap();
                    let mut vertex = serde_json::json!({"index":index,"influences":influences});
                    if pose.is_some() {
                        vertex["bossLocalNormal"] =
                            serde_json::json!(crate::visual::normalize_normal(normal)?);
                        vertex["bossLocalPosition"] = serde_json::json!(position);
                    }
                    finished.push(vertex);
                }
            }
        }
    }
    if !pending.is_empty() {
        return Err(Error::invalid("unterminated weight chains"));
    }
    Ok(
        serde_json::json!({"vertices":finished,"space":"each contribution is in its source joint's local space"}),
    )
}
