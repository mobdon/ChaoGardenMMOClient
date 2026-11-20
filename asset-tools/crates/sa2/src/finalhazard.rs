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
    BossClip::load(exe, clip)?.pose(frame)
}

/// Retains the decoded rig and keys; sampling no longer needs the game image.
pub struct BossClip {
    model: crate::model::Model,
    motion: crate::motion::Motion,
}
impl BossClip {
    pub fn load(exe: &[u8], clip: usize) -> Result<Self> {
        if clip >= 4 {
            return Err(Error::invalid("Finalhazard clip index outside 0..4"));
        }
        let [boss, _] = read(exe)?;
        let image = crate::pe::read_pe(exe)?;
        let at = image.rva_to_offset(0x4ac678 + 28 * clip as u32, 28)?;
        let va = Reader { data: exe }.u32(at)?;
        image.va_to_offset(va, 12)?;
        Ok(Self {
            model: rig_model(&boss),
            motion: crate::board_motion::pc_transform_motion(exe, va - image.image_base, 46)?,
        })
    }
    pub fn frame_count(&self) -> u32 {
        self.motion.frame_count
    }
    pub fn pose(&self, frame: f64) -> Result<crate::pose::Pose> {
        if !(0.0..f64::from(self.frame_count())).contains(&frame) {
            return Err(Error::invalid("frame outside Finalhazard clip"));
        }
        crate::pose::evaluate_model_pose(&self.model, Some(&self.motion), frame)
    }
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
    textured_glb(exe, prs, &scene)
}
pub fn boss_glb(exe: &[u8], prs: &[u8], clip: usize, frame: f64) -> Result<Vec<u8>> {
    textured_glb(
        exe,
        prs,
        &crate::finalhazard_visual::sample(exe, clip, frame)?,
    )
}
fn textured_glb(exe: &[u8], prs: &[u8], scene: &crate::visual::StaticVisual) -> Result<Vec<u8>> {
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

/// Expand the boss's cached polygon list at its draw site. Material and strip
/// commands keep their source order; source and executing joints are distinct.
pub fn polygon_schedule(exe: &[u8]) -> Result<serde_json::Value> {
    use crate::model::PolygonChunk;
    let [boss, _] = read(exe)?;
    let attachments = geometry(exe)?;
    let mut cache = std::collections::BTreeMap::<u8, (u32, &[PolygonChunk])>::new();
    let mut ops = Vec::new();
    let mut saves = Vec::new();
    let mut draws = Vec::new();
    let mut emit = |source: u32, at: u32, c: &PolygonChunk| -> Result<()> {
        match c.kind {
            0=>(),
            8|17..=23|64..=66=>ops.push(serde_json::json!({"sourceJoint":source,"drawJoint":at,"fileOffset":c.offset,"kind":c.kind})),
            _=>return Err(Error::invalid("unsupported Finalhazard polygon operation")),
        }
        Ok(())
    };
    for joint in boss {
        let Some(a) = joint.attachment_rva else {
            continue;
        };
        let chunks = &attachments[&a].polygon_chunks;
        for (i, c) in chunks.iter().enumerate() {
            match c.kind {
                4 => {
                    cache.insert(c.flags, (joint.rva, &chunks[i + 1..]));
                    saves.push(serde_json::json!({"slot":c.flags,"joint":joint.rva}));
                    break;
                }
                5 => {
                    let &(source, list) = cache
                        .get(&c.flags)
                        .ok_or_else(|| Error::invalid("unbound Finalhazard polygon list"))?;
                    draws.push(serde_json::json!({"slot":c.flags,"joint":joint.rva}));
                    for cmd in list {
                        emit(source, joint.rva, cmd)?;
                    }
                }
                _ => emit(joint.rva, joint.rva, c)?,
            }
        }
    }
    Ok(serde_json::json!({"cacheWrites":saves,"cacheDraws":draws,"operations":ops}))
}
/// Local transform records used by the installed weakpoint display callback.
/// Matrix selection and encounter state remain external to this reader.
pub fn weakpoint_placements(bytes: &[u8]) -> Result<serde_json::Value> {
    placement_report(bytes, None)
}
/// Evaluate all candidate origins in boss space. Selection and collision are external.
pub fn weakpoint_origins(
    bytes: &[u8],
    exe: &[u8],
    clip: usize,
    frame: f64,
) -> Result<serde_json::Value> {
    let pose = pose(exe, clip, frame)?;
    let mut report = placement_report(bytes, Some(&pose))?;
    report["clip"] = serde_json::json!(clip);
    report["frame"] = serde_json::json!(frame);
    Ok(report)
}
fn placement_report(bytes: &[u8], pose: Option<&crate::pose::Pose>) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let pair = |at: u32, section: u8| -> Result<u32> {
        let hi = rel
            .reference(1, at)
            .ok_or_else(|| Error::invalid("missing Finalhazard reference"))?;
        let lo = rel
            .reference(1, at + 4)
            .ok_or_else(|| Error::invalid("missing Finalhazard reference"))?;
        if (hi.module, hi.kind, hi.section) != (88, 6, section)
            || lo != (crate::city_rel::Reference { kind: 4, ..hi })
        {
            return Err(Error::invalid("unexpected Finalhazard reference pair"));
        }
        Ok(hi.addend)
    };
    // Constructor passes one CCL_INFO and list ID 3. Its bank byte is
    // copied from the caller; entry remains zero in calloc'd work at creation.
    let code = rel.data(1)?;
    let instruction = |at: usize| u32::from_be_bytes(code[at..at + 4].try_into().unwrap());
    if instruction(0x85c8) != 0x9ba30000
        || instruction(0x8610) != 0x38a00001
        || instruction(0x8614) != 0x38c00003
        || pair(0x857e, 1)? != 0x83dc
    {
        return Err(Error::invalid("unexpected weakpoint constructor"));
    }
    let desc = pair(0x860a, 4)? as usize;
    let constants = rel.data(4)?;
    let c = constants
        .get(desc..desc + 48)
        .ok_or_else(|| Error::invalid("short weakpoint collision descriptor"))?;
    let float = |i: usize| f32::from_be_bytes(c[i..i + 4].try_into().unwrap());
    if c[1] != 0 {
        return Err(Error::invalid("expected weakpoint collision sphere"));
    }
    let init = serde_json::json!({"bankSource":"constructor argument low byte","initialEntry":0,
        "collision":{"descriptorSection4":desc,"count":1,"listId":3,"kind":c[0],"form":"sphere",
        "push":c[2],"damageByte":c[3],"attributes":u32::from_be_bytes(c[4..8].try_into().unwrap()),
        "localCenter":[float(8),float(12),float(16)],"radius":float(20)}});
    let base = pair(0x844e, 5)?;
    let matrices = pair(0x847e, 6)?;
    let lookup = pair(0x41a2, 5)?;
    if pair(0x41be, 6)? != matrices {
        return Err(Error::invalid("different weakpoint matrix buffer"));
    }
    let mut pending = vec![0x18614u32];
    let mut hierarchy = Vec::new();
    while let Some(n) = pending.pop() {
        if hierarchy.contains(&n) || hierarchy.len() >= 46 {
            return Err(Error::invalid("unexpected boss hierarchy"));
        }
        hierarchy.push(n);
        for field in [48, 44] {
            if let Some((section, next)) = rel.local_pointer(5, n + field)? {
                if section != 5 {
                    return Err(Error::invalid("foreign boss joint"));
                }
                pending.push(next);
            }
        }
    }
    // The collision update indexes a position buffer, not the hierarchy.
    // The draw callback fills 27 positions from 20-byte joint lookup records.
    let mut body_origins = Vec::new();
    let mut body_centers = Vec::new();
    if let Some(pose) = pose {
        if pair(0x41ae, 6)? != pair(0x956, 6)? {
            return Err(Error::invalid(
                "body collision uses a different capture buffer",
            ));
        }
        let data = rel.data(5)?;
        let descriptors = pair(0x566, 5)? as usize;
        let source_origin = pair(0x4172, 4)? as usize;
        if rel.data(4)?.get(source_origin..source_origin + 12) != Some(&[0; 12])
            || rel
                .reference(1, 0x984)
                .map(|r| (r.module, r.kind, r.addend))
                != Some((0, 10, 0x80027ab4))
        {
            return Err(Error::invalid(
                "unsupported body capture origin or center operation",
            ));
        }
        let bindings = pair(0x942, 5)? as usize;
        let count = (instruction(0x56c) & 0xffff) as usize;
        let capture_count = instruction(0x42e4) & 0xffff;
        let vec3 = |at: usize| {
            [0, 4, 8]
                .map(|n| f32::from_be_bytes(data[at + n..at + n + 4].try_into().unwrap()) as f64)
        };
        for slot in 0..count {
            let b = bindings + 32 * slot;
            let index = data[b] as i8;
            let descriptor = descriptors + slot * 48;
            if index <= 0 {
                body_centers.push(serde_json::json!({"slot":slot,"form":data[descriptor+1],"center":vec3(descriptor+8),"animated":false}));
                continue;
            }
            if index as u32 >= capture_count {
                return Err(Error::invalid("body capture index out of bounds"));
            }
            let record = lookup + 20 * index as u32;
            let target = rel.local_pointer(5, record + 16)?;
            let joint = hierarchy
                .iter()
                .position(|&n| target == Some((5, n)))
                .ok_or_else(|| Error::invalid("unmapped collision joint"))?;
            let node = pose
                .nodes
                .get(joint)
                .ok_or_else(|| Error::invalid("missing collision pose joint"))?;
            let local = vec3(record as usize + 4);
            let origin = crate::pose::transform_point(&node.world_matrix, local);
            let offset = vec3(b + 4);
            // The collision helper takes the captured position and the slot
            // offset separately: only the capture offset rotates with the joint.
            let center: [f32; 3] = std::array::from_fn(|i| origin[i] as f32 + offset[i] as f32);
            body_centers.push(serde_json::json!({"slot":slot,"form":data[descriptor+1],"center":center,"animated":true}));
            body_origins.push(
                serde_json::json!({"slot":slot,"captureIndex":index,"joint":joint,
                "jointOffset":local,"bossLocalCapture":origin,"collisionOffset":offset}),
            );
        }
    }
    let mut captures = Vec::new();
    let mut joints = [0usize; 5];
    for (slot, call) in [0x4278, 0x4290, 0x42a8, 0x42bc, 0x42d0]
        .into_iter()
        .enumerate()
    {
        let callee = rel
            .reference(1, call)
            .ok_or_else(|| Error::invalid("missing matrix capture"))?;
        if (callee.module, callee.kind, callee.addend) != (0, 10, 0x80114c24) {
            return Err(Error::invalid("unexpected matrix capture"));
        }
        let target = rel
            .local_pointer(5, lookup + 16 + 20 * slot as u32)?
            .ok_or_else(|| Error::invalid("null capture joint"))?;
        let joint = hierarchy
            .iter()
            .position(|&n| target == (5, n))
            .ok_or_else(|| Error::invalid("unmapped capture joint"))?;
        joints[slot] = joint;
        captures.push(serde_json::json!({"slot":slot,"bossJointIndex":joint,"jointSection5":target.1,"captureCallSection1":call}));
    }
    let model = pair(0x84c2, 5)?;
    let data = rel.data(5)?;
    let table = data
        .get(base as usize..)
        .and_then(|tail| tail.get(..2 * 0xe0))
        .ok_or_else(|| Error::invalid("short Finalhazard placement table"))?;
    let mut rows = Vec::new();
    for (i, r) in table.chunks_exact(0x1c).enumerate() {
        let xyz = |at: usize| {
            [at, at + 4, at + 8].map(|x| u32::from_be_bytes(r[x..x + 4].try_into().unwrap()))
        };
        let slot = r[0] as usize;
        let joint = *joints
            .get(slot)
            .ok_or_else(|| Error::invalid("invalid weakpoint matrix slot"))?;
        let translation = xyz(16).map(f32::from_bits);
        let mut row = serde_json::json!({"bank":i/8,"entry":i%8,"matrixSlot":slot,"angles":xyz(4).map(|v|v as i32),"translation":translation});
        if let Some(pose) = pose {
            let node = pose
                .nodes
                .get(joint)
                .ok_or_else(|| Error::invalid("missing weakpoint pose joint"))?;
            row["bossJointIndex"] = serde_json::json!(joint);
            row["bossLocalOrigin"] = serde_json::json!(crate::pose::transform_point(
                &node.world_matrix,
                translation.map(f64::from)
            ));
        }
        rows.push(row);
    }
    Ok(
        serde_json::json!({"modelSection5":model,"tableSection5":base,"matrixArraySection6":matrices,
        "constructor":init,"matrixStride":48,"matrixCaptures":captures,"bossLocalCollisionCenters":body_centers,"bodyCollisionPoseBindings":body_origins,"placements":rows,"scope":"Display records only; matrix contents, selection timing and hit detection are external."}),
    )
}
/// Callback dispatch only. The selected handler runs before the contact check;
/// its state changes must be visible to that check.
pub fn weakpoint_update_plan(bytes: &[u8], gate_byte: i8, mode: i8) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let code = rel.data(1)?;
    let branch = |at: usize| -> Result<i64> {
        let w = u32::from_be_bytes(code[at..at + 4].try_into().unwrap());
        if w & 0xfc00_0003 != 0x4800_0001 {
            return Err(Error::invalid("unexpected weakpoint update call"));
        }
        Ok(at as i64 + i64::from(((w & 0x03ff_fffc) << 6) as i32 >> 6))
    };
    let entry = rel
        .reference(1, 0x83c4)
        .ok_or_else(|| Error::invalid("missing collision entry"))?;
    if (entry.module, entry.kind, entry.addend) != (0, 10, 0x8000f4ec) {
        return Err(Error::invalid("unexpected collision entry"));
    }
    let mut calls = Vec::new();
    if gate_byte == 0 {
        let handler = match mode {
            1 => Some(0x8390),
            2 => Some(0x839c),
            3 => Some(0x83a8),
            5 => Some(0x83b4),
            _ => None,
        };
        if let Some(site) = handler {
            calls.push(serde_json::json!({"role":"modeHandler","section1":branch(site)?}));
        }
        calls.push(serde_json::json!({"role":"contactCheck","section1":branch(0x83bc)?}));
        calls.push(serde_json::json!({"role":"CCL_Entry","engineAddress":entry.addend}));
    }
    Ok(
        serde_json::json!({"mode":mode,"gateByte":gate_byte,"calls":calls,"scope":"Ordered calls only; collision registration is not a hit."}),
    )
}
/// Task-work writes from section 1, 0x7684. Contact-query results are supplied
/// by the collision engine; intermediate effect/physics calls remain external.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeakpointContactState {
    pub flags: u16,
    pub timer: i16,
    pub mode: i8,
}

impl WeakpointContactState {
    /// Pair contains the other collider's query result followed by this
    /// weakpoint's result for collider zero. PPC cmpw compares signed words.
    pub fn contact(&mut self, hit: Option<(i32, i32)>) -> bool {
        if self.flags & 4 != 0 || !hit.is_some_and(|(other, own)| other > own) {
            return false;
        }
        self.flags |= 4;
        self.timer = 90;
        self.mode = 5;
        true
    }
}
#[cfg(test)]
mod contact_tests {
    use super::*;
    #[test]
    fn contact_requires_strict_signed_advantage_and_latches() {
        let initial = WeakpointContactState {
            flags: 0x20,
            timer: -2,
            mode: 2,
        };
        let mut s = initial;
        for hit in [None, Some((2, 2)), Some((-1, 0))] {
            assert!(!s.contact(hit));
            assert_eq!(s, initial);
        }
        assert!(s.contact(Some((0, -1))));
        assert_eq!(
            s,
            WeakpointContactState {
                flags: 0x24,
                timer: 90,
                mode: 5
            }
        );
        s.timer = 89;
        assert!(!s.contact(Some((99, 0))));
        assert_eq!(s.timer, 89);
    }
}
/// Tail of mode 5 (section 1 + 0x82b0). Earlier animation/effect calls
/// supply work[0x24] anew each tick, so it must not be treated as a persistent fade.
pub fn weakpoint_mode5_tail(
    bytes: &[u8],
    timer: u16,
    submode: u8,
    work24: f32,
) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
        || !work24.is_finite()
    {
        return Err(Error::invalid("invalid Finalhazard mode-five observation"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let constant = |at: u32, section: u8, delta: usize| -> Result<f32> {
        let hi = rel
            .reference(1, at)
            .ok_or_else(|| Error::invalid("missing mode-five constant"))?;
        let lo = rel
            .reference(1, at + 4)
            .ok_or_else(|| Error::invalid("missing mode-five constant"))?;
        if (hi.module, hi.kind, hi.section) != (88, 6, section)
            || lo != (crate::city_rel::Reference { kind: 4, ..hi })
        {
            return Err(Error::invalid("invalid mode-five relocation"));
        }
        let i = hi.addend as usize + delta;
        Ok(f32::from_be_bytes(
            rel.data(section)?[i..i + 4].try_into().unwrap(),
        ))
    };
    let mut mode = 5;
    let mut sub = submode;
    let mut value = work24;
    let next = if timer != 0 {
        timer - 1
    } else {
        value -= constant(0x82ce, 5, 8)?;
        if value < constant(0x82e6, 4, 0)? {
            mode = 3;
            sub = 0;
        }
        0
    };
    Ok(serde_json::json!({"mode":mode,"submode":sub,"timer":next,"work24":value}))
}
/// Mode-three state before the draw/placement helper. The helper receives
/// the current placement before a pending one can replace it.
#[derive(serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetractState {
    pub current: [i8; 2],
    pub pending: [i8; 2],
    pub work24: f32,
    pub scale: [f32; 3],
    pub collider_attributes: u32,
    pub submode: u8,
    pub work10: u32,
    pub work14: f32,
}
pub fn weakpoint_retract(bytes: &[u8], mut state: RetractState) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let ptr = rel
        .reference(1, 0x7d9a)
        .ok_or_else(|| Error::invalid("missing retraction speed"))?;
    let lo = rel
        .reference(1, 0x7d9e)
        .ok_or_else(|| Error::invalid("missing retraction speed"))?;
    if (ptr.module, ptr.section, ptr.kind) != (88, 5, 6)
        || lo != (crate::city_rel::Reference { kind: 4, ..ptr })
    {
        return Err(Error::invalid("unsupported retraction speed reference"));
    }
    let at = ptr.addend as usize + 8;
    let speed = f32::from_be_bytes(rel.data(5)?[at..at + 4].try_into().unwrap());
    let old = state.current;
    state.work24 -= speed;
    state.scale[1] = (state.scale[1] - speed).max(0.);
    let mut mode = 3;
    let mut calls = vec![0x7548u32];
    if state.work24 <= -1. {
        state.work24 = -1.;
        state.scale = [1., 0., 1.];
        state.work10 = 0;
        state.work14 = 0.;
        state.submode = 0;
        if state.pending[1] >= 0 {
            state.collider_attributes &= !0x10;
            state.current = state.pending;
            state.pending = [-1, -1];
            calls.push(0x1f94);
            mode = 1;
        } else {
            state.collider_attributes |= 0x10;
            mode = 4;
        }
        calls.push(0x8818);
    }
    Ok(
        serde_json::json!({"state":state,"mode":mode,"placementUsedBeforeSwitch":old,
        "stageHelperCalls":calls,"scope":"Caller supplies queued bytes. External level/volume effects and helper internals are not simulated."}),
    )
}
/// Placement request after the engine converts its random sample into a bucket.
/// Section 1 + 0x743c maps that bucket to an entry; 0x86b8 applies it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRequest {
    pub state: RetractState,
    pub mode: i8,
    pub requested_bank: i8,
    pub bucket: u8,
}
pub fn request_placement(bytes: &[u8], req: PlacementRequest) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let mut s = req.state;
    if !(0..2).contains(&req.requested_bank)
        || !(0..2).contains(&s.current[0])
        || !(0..8).contains(&s.current[1])
    {
        return Err(Error::invalid("invalid weakpoint bank or entry"));
    }
    let same = req.requested_bank == s.current[0];
    if req.bucket >= if same { 7 } else { 8 } {
        return Err(Error::invalid("weakpoint selector bucket out of range"));
    }
    let entry = req.bucket as i8 + i8::from(same && req.bucket as i8 >= s.current[1]);
    let selected = [req.requested_bank, entry];
    let mut mode = req.mode;
    match mode {
        2 => {
            s.pending = selected;
            mode = 3;
            s.submode = 0;
        }
        3 => {
            s.current = selected;
        }
        _ => {
            s.current = selected;
            s.scale[0] = 1.;
            s.scale[2] = 1.;
            s.collider_attributes &= !0x10;
            mode = 1;
            s.submode = 0;
        }
    }
    Ok(
        serde_json::json!({"mode":mode,"state":s,"selected":selected,"scope":"Placement request writes, given the engine-selected bucket. Engine callbacks and RNG state are external."}),
    )
}
/// Two separate caller paths into the placement-request routine. A periodic
/// tick is valid only when the encounter dispatch reaches section 1 + 0x17f0.
#[derive(serde::Deserialize)]
#[serde(tag = "path", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlacementClock {
    Entry {
        submode: i8,
        timer: u16,
    },
    Periodic {
        timer: u16,
    },
    InitialHealth,
    /// Section 1 + 0xad8c: one update of a super-form player object.
    SuperPlayer(Box<SuperPlayerFrame>),
    /// Section 1 + 0xe370: super-form flight movement for one frame.
    SuperFlight(Box<SuperFlightFrame>),
    /// Section 1 + 0xc598: per-frame collision placement of a super form.
    SuperCollision(Box<SuperCollisionFrame>),
    /// Section 1 + 0xe9f0: super-form hover (no stick steering).
    SuperHover(Box<SuperHoverFrame>),
    /// Section 1 + 0xeb38: the voice-channel gate checked before stage cues.
    /// `channels` is None when the engine channel table pointer is null.
    VoiceGate {
        channels: Option<[VoiceChannel; 2]>,
    },
    /// Section 1 + 0xf070: the timed boss voice lines, called every frame
    /// from the shared tail with the stage task and its work block.
    TimedVoice {
        /// Work byte 0 (active player), short 0xe (countdown), byte 6 (next line).
        player: i8,
        countdown: i16,
        line: i8,
        /// Engine global 0x801cbd88: word 0x7c (frame count), bytes 0x42/0x43.
        global_7c: i32,
        global_42: i8,
        global_43: i8,
        /// Word 0xc of the stage task.
        task_0c: u32,
        channels: Option<[VoiceChannel; 2]>,
    },
    /// Section 1 + 0xee8c: per-frame arena bounds for both players.
    ArenaClamp {
        switch_state: i8,
        /// Non-null entries of the engine table at 0x801e7388.
        present: [bool; 2],
        /// Words 0x14..0x1c of each 0x801e7368 entry.
        positions: [[f32; 3]; 2],
        /// Results of the engine distance call (0x8011b314) for each slot.
        distances: [f32; 2],
    },
    SwitchCamera {
        entered_state: u8,
        player: u8,
        character: i8,
        yaw: i32,
        position_pointer: u32,
        table_words: [u32; 4],
        camera_mode: i32,
        second_lookup: Option<i32>,
    },
    /// Frames of the stage player task from the slot setup at 0xf458 through
    /// the shared tail's timer increment: handoff states 1..3, and state 0
    /// (ring drain and handoff trigger) for frames that carry `stage`.
    SwitchReplay {
        player: u8,
        switch_state: i8,
        timer: i16,
        countdown: i16,
        visits: [i8; 2],
        /// Engine pointer tables at 0x801e7368, 0x801e7328 and 0x801e7348.
        player1: [u32; 2],
        player2: [u32; 2],
        player_tasks: [u32; 2],
        table_words: [u32; 4],
        /// Work byte 6, the next timed voice line (used by frames with `stage`).
        #[serde(default)]
        line: i8,
        /// Work byte 4 (switch reason), byte 5 (switch count), short 0xa
        /// (ring clock) and short 0xc (ring pool).
        #[serde(default)]
        reason: i8,
        #[serde(default)]
        switch_count: i8,
        #[serde(default)]
        ring_clock: i16,
        #[serde(default)]
        pool: i16,
        /// Ring counts behind the engine's 0x80062a24 (get) and 0x80063584
        /// (add). The replay assumes the add call adds its delta exactly.
        #[serde(default)]
        rings: [i32; 2],
        frames: Vec<HandoffFrame>,
    },
    SwitchTick {
        player: u8,
        switch_state: u8,
        timer: i16,
        poll: i32,
    },

    SwitchStart {
        player: u8,
        visits: [i8; 2],
        reason: i8,
        switch_count: i8,
        target_count: i16,
        player_counts: [i32; 2],
        character: i8,
        cue_gate: i32,
        global42: i8,
        global43: i8,
    },

    HitReaction {
        submode: i8,
        flags: u16,
        cursor: i8,
        child14: bool,
        child18: bool,
        animation_flags: u16,
        frame: f32,
        completion_poll: i32,
        gate_after_clear: Option<PhaseGateTask>,
    },

    BossDrawTransform {
        position: [f32; 3],
        rotation_bams: [i32; 3],
        scale: [f32; 3],
    },

    BossConstructor {
        task_allocated: bool,
        work_allocated: bool,
        aux_allocated: bool,
        collision_flags: u16,
    },

    BodyContactGeometry {
        player: i8,
        center: [f32; 3],
        player_position: [f32; 3],
        own_query: i32,
        other_query: i32,
    },

    /// Input to 0x71e0 from the body contact branch.
    BodyImpulse {
        player: i8,
        player_y: f32,
        contact_vector: Option<[f32; 3]>,
    },

    /// Continuation at 0x7f0, using the signed collision-query returns.
    BodyContact {
        slot: Option<u8>,
        own_query: i32,
        other_query: i32,
        gate_task: Option<PhaseGateTask>,
        wait_work_present: bool,
        wait_word: i32,
        timer: u16,
    },
    BodyCollision,
    /// Inputs at 0x1d74. Supply the final player byte separately because the
    /// stage reads it again after the damage and collision calls.
    CommonTail {
        weakpoint_position: Option<[f32; 3]>,
        previous_target: [f32; 3],
        flags: u16,
        player: i8,
        player_after_updates: i8,
    },
    PlayerSwitch {
        mode: i8,
        submode: i8,
        cursor: i8,
        previous_player: i8,
        active_player: i8,
        global37: u8,
        global39: u8,
        switch_byte: u8,
        child14: bool,
        child18: bool,
        gate_task: Option<PhaseGateTask>,
    },

    /// Dispatch after the player-switch branch, at section 1 + 0x1cc4.
    Dispatch {
        mode: i8,
        wait_word: i32,
        timer: u16,
    },

    /// Imported return values at section 1 + 0x1b3c and 0x1b4c.
    DefeatHandoff {
        completion_poll: i32,
        player_byte: u8,
        lookup_result: u32,
    },

    Defeat {
        submode: i8,
        flags: u16,
        child14: bool,
        child18: bool,
        frame: f32,
        global_counter: u32,
        completion_poll: i32,
    },
    DamageCheck {
        health: f32,
        mode: i8,
        submode: i8,
        flags: u16,
        weakpoint_flags: Option<u16>,
        gate_task: Option<PhaseGateTask>,
    },
    /// Arguments and task state at the paired selector/transition calls.
    NextPhase {
        work28: f32,
        cursor: i8,
        mode: i8,
        submode: i8,
        gate_task: Option<PhaseGateTask>,
    },
    /// Mode 11 submode 2; null means the child slot is empty.
    AttackChild {
        child_mode: Option<i8>,
    },
    /// Mode 11 submode 3. Null group means no task; null slot means the
    /// child or one of its two required work pointers is absent.
    AttackGroup {
        slots: Option<Vec<Option<u16>>>,
    },
    /// State at the submode dispatch, after the common 0xaf0 update.
    Attack {
        mode: i8,
        submode: i8,
        task_timer: u16,
        placement_timer: u16,
    },
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseGateTask {
    pub mode: i8,
    pub flags: u16,
}
fn phase_constant<'a>(
    rel: &crate::city_rel::CityRel<'a>,
    site: usize,
    section: u8,
) -> Result<&'a [u8]> {
    let p = rel
        .reference(1, site as u32)
        .ok_or_else(|| Error::invalid("missing encounter constant"))?;
    if (p.module, p.section, p.kind) != (88, section, 6)
        || rel.reference(1, site as u32 + 4) != Some(crate::city_rel::Reference { kind: 4, ..p })
    {
        return Err(Error::invalid("unexpected encounter constant reference"));
    }
    rel.data(section)?
        .get(p.addend as usize..p.addend as usize + 4)
        .ok_or_else(|| Error::invalid("truncated encounter constant"))
}
fn phase_write_allowed(task: Option<PhaseGateTask>) -> bool {
    task.is_some_and(|task| task.mode != 13 && task.flags & 4 == 0)
}
/// One entry (stride 0x1c) of the engine voice-channel table at 0x803ad3d0.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceChannel {
    /// Word 0x30.
    pub state: i32,
    /// Byte 0x25.
    pub category: i8,
}
/// Section 1 + 0x40cc: frsqrte, three double-precision Newton steps, x*y,
/// frsp. Nonpositive and NaN inputs return unchanged. The estimate here is
/// the exact reciprocal root (as Unicorn/QEMU computes frsqrte); hardware
/// estimates converge to the same double except in rare final roundings.
fn stage_sqrt(x: f32) -> f32 {
    if x.is_nan() || x <= 0.0 {
        return x;
    }
    let x = x as f64;
    let mut y = 1.0 / x.sqrt();
    for _ in 0..3 {
        y = 0.5 * y * (3.0 - x * (y * y));
    }
    (x * y) as f32
}
fn stage_f32(
    rel: &crate::city_rel::CityRel<'_>,
    site: usize,
    section: u8,
    offset: usize,
) -> Result<f32> {
    let p = rel
        .reference(1, site as u32)
        .ok_or_else(|| Error::invalid("missing arena constant"))?;
    phase_constant(rel, site, section)?;
    let at = p.addend as usize + offset;
    let bytes = rel
        .data(section)?
        .get(at..at + 4)
        .ok_or_else(|| Error::invalid("truncated arena constant"))?;
    Ok(f32::from_be_bytes(bytes.try_into().unwrap()))
}
struct SwitchStartResult {
    cue: u16,
    /// (slot, ring delta) in native call order: incoming, then outgoing.
    adjustments: Vec<(usize, i32)>,
    motion: Option<u16>,
    switch_count: i8,
    target_count: i16,
    visits: [i8; 2],
}
/// Section 1 + 0xec04: the handoff start (state 1) decisions.
#[allow(clippy::too_many_arguments)]
fn switch_start(
    code: &[u8],
    player: u8,
    mut visits: [i8; 2],
    reason: i8,
    mut switch_count: i8,
    mut target_count: i16,
    player_counts: [i32; 2],
    character: i8,
) -> SwitchStartResult {
    let word = |offset: usize| u32::from_be_bytes(code[offset..offset + 4].try_into().unwrap());
    let from = player as usize;
    let to = 1 - from;
    let cue_site = match (visits[from] == 0, reason == 1, player) {
        (true, _, 0) => 0xec78,
        (true, _, _) => 0xec80,
        (false, true, 0) => 0xeca0,
        (false, true, _) => 0xeca8,
        (false, false, 0) => 0xecb8,
        _ => 0xecc0,
    };
    let cue = word(cue_site) as u16;
    let mut adjustments = Vec::new();
    let mut motion = None;
    if reason == 1 {
        switch_count = switch_count.wrapping_add(1);
        target_count = word(if (0..4).contains(&switch_count) {
            0xed64
        } else {
            0xed7c
        }) as i16;
        adjustments.push((to, i32::from(target_count).wrapping_sub(player_counts[to])));
        adjustments.push((from, player_counts[from].wrapping_neg()));
        motion = match character {
            9 => Some(word(0xee20) as u16),
            10 => Some(word(0xee2c) as u16),
            _ => None,
        };
    }
    visits[from] = visits[from].saturating_add(1);
    SwitchStartResult {
        cue,
        adjustments,
        motion,
        switch_count,
        target_count,
        visits,
    }
}
/// Section 1 + 0xeb38. Returns -1 when a channel holds a voice that is not
/// category 3; busy category-3 channels are stopped through the engine.
fn voice_gate(
    rel: &crate::city_rel::CityRel<'_>,
    channels: Option<&[VoiceChannel; 2]>,
) -> Result<(i32, Vec<serde_json::Value>)> {
    let mut events = Vec::new();
    for (i, ch) in channels.into_iter().flatten().enumerate() {
        // States 5, 0 and -1 are idle; anything else holds a voice.
        if matches!(ch.state, 5 | 0 | -1) {
            continue;
        }
        if ch.category != 3 {
            return Ok((-1, events));
        }
        events
            .push(serde_json::json!({"site":0xebc0,"call":engine_import(rel, 0xebc0)?,"args":[i]}));
    }
    Ok((0, events))
}
/// Section 1 + 0xee8c: arena bounds for both players unless switch state 4.
fn arena_clamp(
    rel: &crate::city_rel::CityRel<'_>,
    switch_state: i8,
    present: [bool; 2],
    positions: [[f32; 3]; 2],
    distances: [f32; 2],
) -> Result<Vec<serde_json::Value>> {
    let mut events = Vec::new();
    if switch_state == 4 {
        return Ok(events);
    }
    let center = [
        stage_f32(rel, 0xeede, 5, 0)?,
        stage_f32(rel, 0xeede, 5, 4)?,
        stage_f32(rel, 0xeede, 5, 8)?,
    ];
    let plane_y = stage_f32(rel, 0xeee6, 4, 0)?;
    let radius = stage_f32(rel, 0xeeee, 5, 0)?;
    let one = stage_f32(rel, 0xeef6, 4, 0)?;
    let x_max = stage_f32(rel, 0xeefe, 5, 0)?;
    let y_max = stage_f32(rel, 0xef06, 5, 0)?;
    let y_min = stage_f32(rel, 0xef0e, 5, 0)?;
    for slot in 0..2 {
        if !present[slot] {
            continue;
        }
        let [px, py, pz] = positions[slot];
        events.push(
            serde_json::json!({"site":0xef74,"call":engine_import(rel, 0xef74)?,
            "vectors":[[px, plane_y, pz], [center[0], plane_y, center[2]]]}),
        );
        let (mut x, mut z) = (px, pz);
        let clamped = distances[slot] > radius;
        if clamped {
            let dx = px - center[0];
            let dz = pz - center[2];
            let scale = one / stage_sqrt(dx * dx + dz * dz);
            let reach = (radius - one) * scale;
            x = center[0] + dx * reach;
            z = center[2] + dz * reach;
        }
        if x > x_max {
            x = x_max;
        }
        let mut y = py;
        if y > y_max {
            y = y_max;
        }
        if y < y_min {
            y = y_min;
        }
        events.push(
            serde_json::json!({"site":0xf038,"call":engine_import(rel, 0xf038)?,
            "args":[slot],"floats":[x, y, z],"radial_clamp":clamped}),
        );
    }
    Ok(events)
}
struct TimedVoiceResult {
    line: i8,
    voice: Option<i16>,
    spoke: bool,
    events: Vec<serde_json::Value>,
}
/// Section 1 + 0xf070: the timed boss voice lines.
#[allow(clippy::too_many_arguments)]
fn timed_voice(
    rel: &crate::city_rel::CityRel<'_>,
    code: &[u8],
    player: i8,
    countdown: i16,
    mut line: i8,
    global_7c: i32,
    global_42_43: [i8; 2],
    task_0c: u32,
    channels: Option<&[VoiceChannel; 2]>,
) -> Result<TimedVoiceResult> {
    let word = |offset: usize| u32::from_be_bytes(code[offset..offset + 4].try_into().unwrap());
    let mut events = Vec::new();
    let release = |events: &mut Vec<serde_json::Value>, site: u32| -> Result<bool> {
        if task_0c != 0 {
            events.push(
                serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":[task_0c]}),
            );
        }
        Ok(task_0c != 0)
    };
    let mut voice = None;
    if global_42_43.contains(&1) {
        release(&mut events, 0xf0e8)?;
    } else if countdown <= 0 && global_7c & 3 == 0 && (0..=9).contains(&line) {
        // Jump table (section 5 + 0x40008) of ten threshold cases:
        // cmpwi frame,T at +0, li r31,a at +0x18, li r31,b at +0x20.
        let case = rel
            .reference(5, 0x40008 + 4 * line as u32)
            .filter(|r| r.module == 88 && r.section == 1 && r.kind == 1)
            .ok_or_else(|| Error::invalid("missing voice line jump target"))?
            .addend as usize;
        if global_7c >= word(case) as i16 as i32 {
            voice = Some(word(case + if player == 0 { 0x18 } else { 0x20 }) as i16);
            line += 1;
        }
    }
    let mut spoke = false;
    if let Some(v) = voice {
        // Inline channel scan: busy category-3 voices are stopped and defer
        // the line; other busy channels are ignored.
        let mut clear = true;
        for (i, ch) in channels.into_iter().flatten().enumerate() {
            if !matches!(ch.state, 5 | 0 | -1) && ch.category == 3 {
                events.push(serde_json::json!({"site":0xf380,"call":engine_import(rel, 0xf380)?,"args":[i]}));
                clear = false;
            }
        }
        if release(&mut events, 0xf3a4)? {
            clear = false;
        }
        if clear {
            events.push(serde_json::json!({"site":0xf3dc,"local":0x18ed0,
                "args":[0,0,v,1],"vector":[stage_f32(rel, 0xf08e, 4, 0)?,
                stage_f32(rel, 0xf08e, 4, 4)?, stage_f32(rel, 0xf08e, 4, 8)?]}));
            spoke = true;
        } else {
            line = line.wrapping_sub(1);
        }
    }
    Ok(TimedVoiceResult {
        line,
        voice,
        spoke,
        events,
    })
}
/// Stage state the three local calls of a handoff frame read. When a frame
/// carries it, the replay runs 0xeb38, 0xee8c and 0xf070 instead of only
/// reporting their call sites.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageFrame {
    pub channels: Option<[VoiceChannel; 2]>,
    pub present: [bool; 2],
    pub positions: [[f32; 3]; 2],
    pub distances: [f32; 2],
    #[serde(default)]
    pub global_42: i8,
    #[serde(default)]
    pub global_43: i8,
    #[serde(default)]
    pub task_0c: u32,
    /// State 0 only: module word section 6 + 0x664, the result chain of the
    /// local 0x9f4 call (None when it or its word 0x34 is null; else the
    /// halfword at +4 of that object), words 0x176/0x178 of both
    /// 0x801e7328 entries, and the two 0x80022bc4 query results.
    #[serde(default)]
    pub global_664: u32,
    #[serde(default)]
    pub boss_flags: Option<i16>,
    #[serde(default)]
    pub actions: [[i16; 2]; 2],
    #[serde(default)]
    pub sound_queries: [i32; 2],
}
/// Engine results a state 4 frame observes when it spawns debris (every
/// fourth frame): the seven 0x8010d940 results, the five trigonometric
/// results in call order (0x80112050(b), 0x80112068(a), 0x80112050(a),
/// 0x80112068(b), 0x80112068(a)) and words 0x230..0x238 of the active
/// 0x801e7328 entry.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebrisFrame {
    pub randoms: [i32; 7],
    pub trig: [f32; 5],
    pub origin: [f32; 3],
}
/// Engine results one handoff frame observes.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffFrame {
    #[serde(default)]
    pub stage: Option<StageFrame>,
    #[serde(default)]
    pub debris: Option<DebrisFrame>,
    /// Result of the engine call 0x8001ccbc at the top of the task; 7 resets
    /// the work block.
    #[serde(default)]
    pub game_state: i32,
    pub poll: i32,
    /// Byte 1 of each 0x801e7328 entry and word 0xc of each 0x801e7368 entry.
    pub characters: [i8; 2],
    pub yaws: [i32; 2],
    pub camera_mode: i32,
    #[serde(default)]
    pub second_lookup: Option<i32>,
    /// Result of the local section 1 + 0xeb38 call after the state 3 camera.
    #[serde(default)]
    pub cue_result: i32,
    /// Word 0x7c of the engine global at 0x801cbd88.
    #[serde(default)]
    pub global_7c: u32,
}
fn engine_import(rel: &crate::city_rel::CityRel<'_>, site: u32) -> Result<u32> {
    rel.reference(1, site)
        .filter(|r| r.kind == 10 && r.module == 0)
        .map(|r| r.addend)
        .ok_or_else(|| Error::invalid("missing handoff engine import"))
}
struct HandoffCamera {
    packet: [u32; 6],
    table_offset: u32,
    /// Second camera-mode result and the engine call it is passed to.
    reset: Option<(i32, u32)>,
    submit: u32,
    /// Engine call sites in native order: mode read, [second read, reset], submit.
    sites: Vec<u32>,
}
/// Camera request submitted when the handoff enters state 2 (section 1 +
/// 0xf5cc..0xf6a8) or state 3 (0xf724..0xf7fc).
#[allow(clippy::too_many_arguments)]
fn handoff_camera(
    rel: &crate::city_rel::CityRel<'_>,
    code: &[u8],
    entered_state: u8,
    character: i8,
    yaw: i32,
    position_pointer: u32,
    table_words: &[u32; 4],
    camera_mode: i32,
    second_lookup: Option<i32>,
) -> Result<HandoffCamera> {
    // Table input order follows ascending offsets: 354,364,37c,39c.
    let (index, load, scalar, submit) = match (entered_state, character == 9) {
        (2, true) => (1, 0xf650, 0xf66e, 0xf6a8),
        (2, false) => (3, 0xf664, 0xf66e, 0xf6a8),
        (3, true) => (0, 0xf7a4, 0xf7c2, 0xf7fc),
        (3, false) => (2, 0xf7b8, 0xf7c2, 0xf7fc),
        _ => {
            return Err(Error::invalid(
                "handoff camera requires entry to state two or three",
            ))
        }
    };
    let scalar_bits = u32::from_be_bytes(phase_constant(rel, scalar, 4)?.try_into().unwrap());
    // Above mode 13 the stage reads the mode again (submit - 0x18) and
    // passes that result to a second engine call (submit - 0xc).
    let mut sites = vec![submit - 0x28];
    let reset = if camera_mode > 13 {
        sites.extend([submit - 0x18, submit - 0xc]);
        Some((
            second_lookup.ok_or_else(|| Error::invalid("missing second camera lookup result"))?,
            engine_import(rel, submit - 0xc)?,
        ))
    } else {
        None
    };
    sites.push(submit);
    let load_word = u32::from_be_bytes(code[load..load + 4].try_into().unwrap());
    Ok(HandoffCamera {
        packet: [
            scalar_bits,
            0x8000i32.wrapping_sub(yaw) as u32,
            table_words[index],
            position_pointer,
            0x51,
            0,
        ],
        table_offset: load_word & 0xffff,
        reset,
        submit: engine_import(rel, submit)?,
        sites,
    })
}
fn body_impulse(
    rel: &crate::city_rel::CityRel<'_>,
    player: i8,
    player_y: f32,
    contact_vector: Option<[f32; 3]>,
) -> Result<serde_json::Value> {
    let constant = |site| -> Result<f32> {
        Ok(f32::from_be_bytes(
            phase_constant(rel, site, 4)?.try_into().unwrap(),
        ))
    };
    let (site, velocity) = if let Some(v) = contact_vector {
        // The normalize helper sees all three axes. The caller then
        // restores the incoming Y when it passes the vector onward.
        let scale = constant(0x85a)?;
        let len = (v[0] as f64).hypot(v[1] as f64).hypot(v[2] as f64);
        let axis = |x: f32| {
            if len == 0.0 || scale == 0.0 {
                x
            } else {
                ((x as f64 / len) as f32) * scale
            }
        };
        (0x72c8, [axis(v[0]), v[1], axis(v[2])])
    } else {
        let z = constant(0x896)?;
        let y = constant(if player_y > constant(0x723e)? {
            0x7252
        } else {
            0x7262
        })?;
        (
            0x7278,
            [constant(0x726e)?, y, if player == 0 { z } else { -z }],
        )
    };
    let target = rel
        .reference(1, site)
        .ok_or_else(|| Error::invalid("missing body impulse import"))?;
    return Ok(
        serde_json::json!({"player":player,"velocity_argument":velocity,"import":target.addend,
                "scope":"Arguments to the player motion import; vector normalization uses Rust arithmetic, not emulated engine rounding."}),
    );
}
pub fn placement_clock(bytes: &[u8], input: PlacementClock) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let code = rel.data(1)?;
    let word = |offset: usize| u32::from_be_bytes(code[offset..offset + 4].try_into().unwrap());
    let mut calls = Vec::new();
    let (timer, submode) = match input {
        PlacementClock::Entry {
            mut submode,
            mut timer,
        } => {
            if submode == 0 {
                timer = word(0x1038) as u16;
                calls.push(0x86b8);
                submode = 1;
            }
            if submode == 1 {
                timer = timer.wrapping_sub(1);
                if timer == 0 {
                    calls.extend([0xd40, 0xdc4]);
                }
            }
            (timer, Some(submode))
        }
        PlacementClock::Defeat {
            mut submode,
            mut flags,
            mut child14,
            mut child18,
            frame,
            global_counter,
            completion_poll,
        } => {
            // Ordered call sites, including imports whose side effects belong to
            // the engine. The caller supplies the animation frame and poll result.
            let mut sites = Vec::new();
            if submode == 0 {
                if child14 {
                    sites.push(0x18ac);
                    child14 = false;
                }
                if child18 {
                    sites.push(0x18c4);
                    child18 = false;
                }
                sites.extend([0x18d8, 0x18f0, 0x18f8, 0x190c]);
                flags &= !4;
                submode = 1;
            }
            if submode == 1 {
                let threshold = |site| -> Result<f32> {
                    Ok(f32::from_be_bytes(
                        phase_constant(&rel, site, 4)?.try_into().unwrap(),
                    ))
                };
                if frame < threshold(0x1922)? && global_counter % 45 == 0 {
                    sites.extend([0x197c, 0x198c]);
                }
                for (lo, hi, group) in [
                    (0x1992, 0x19a6, &[0x19cc, 0x19dc][..]),
                    (0x19e2, 0x19f6, &[0x1a20, 0x1a30, 0x1a40][..]),
                    (0x1a46, 0x1a5a, &[0x1a80, 0x1a90][..]),
                ] {
                    if frame > threshold(lo)? && frame < threshold(hi)? {
                        sites.extend_from_slice(group);
                    }
                }
                if frame < threshold(0x1aca)? {
                    if global_counter % 30 == 0 {
                        sites.push(0x1ae0);
                    }
                    if global_counter % 10 == 0 {
                        sites.extend([0x1b30, 0x1b34]);
                    }
                }
                sites.push(0x1b3c);
                if completion_poll != 0 {
                    sites.extend([0x1b4c, 0x1b58, 0x1b70]);
                    submode = 2;
                }
            }
            return Ok(
                serde_json::json!({"submode":submode,"flags":flags,"child14":child14,"child18":child18,
                "call_sites":sites,"global_halfword_0a":0}),
            );
        }
        PlacementClock::DefeatHandoff {
            completion_poll,
            player_byte,
            lookup_result,
        } => {
            let mut imports = Vec::new();
            if completion_poll != 0 {
                for (site, args) in [
                    (0x1b4c, vec![0]),
                    (0x1b58, vec![0, lookup_result]),
                    (0x1b70, vec![player_byte as i8 as i32 as u32]),
                ] {
                    let target = rel
                        .reference(1, site)
                        .ok_or_else(|| Error::invalid("missing defeat handoff target"))?;
                    if target.module != 0 || target.kind != 10 {
                        return Err(Error::invalid("unexpected defeat handoff relocation"));
                    }
                    imports.push(serde_json::json!({"address":target.addend,"arguments":args}));
                }
            }
            return Ok(
                serde_json::json!({"imports":imports,"submode":if completion_poll==0 {1} else {2},"global_halfword_0a":0}),
            );
        }
        PlacementClock::Dispatch {
            mode,
            wait_word,
            mut timer,
        } => {
            let mut handler = None;
            if wait_word == 1 {
                timer = timer.wrapping_sub(1);
                if timer == 0 {
                    calls.push(0xd04);
                }
            } else if (0..=13).contains(&mode) {
                let invalid = || Error::invalid("invalid Finalhazard mode dispatch");
                let base = rel.reference(1, 0x1d02).ok_or_else(invalid)?;
                if (base.module, base.kind, base.section) != (88, 6, 5)
                    || rel.reference(1, 0x1d06)
                        != Some(crate::city_rel::Reference { kind: 4, ..base })
                {
                    return Err(invalid());
                }
                let case = rel
                    .reference(5, base.addend + 4 * mode as u32)
                    .ok_or_else(invalid)?;
                if (case.module, case.kind, case.section) != (88, 1, 1) {
                    return Err(invalid());
                }
                if case.addend != 0x1d74 {
                    let pc = case.addend + 4;
                    let instruction = word(pc as usize);
                    if instruction & 0xfc00_0003 != 0x4800_0001 {
                        return Err(invalid());
                    }
                    let displacement = ((instruction << 6) as i32 >> 6) & !3;
                    handler = Some(pc.wrapping_add_signed(displacement));
                }
            }
            return Ok(
                serde_json::json!({"handler":handler,"timer":timer,"stageCalls":calls,"continue_at":0x1d74}),
            );
        }
        PlacementClock::PlayerSwitch {
            mut mode,
            mut submode,
            mut cursor,
            previous_player,
            active_player,
            global37,
            global39,
            switch_byte,
            mut child14,
            mut child18,
            gate_task,
        } => {
            let entry_mode = mode;
            let changed = previous_player != active_player;
            let mut sites = Vec::new();
            let next = if global37 != 0 {
                0x1dec
            } else if global39 != 0 {
                sites.push(0x1c10);
                0x1dec
            } else if switch_byte != 0 {
                if changed {
                    if child14 {
                        sites.push(0x1c58);
                        child14 = false;
                    }
                    if child18 {
                        sites.push(0x1c70);
                        child18 = false;
                    }
                }
                sites.push(0x1c80);
                0x1dec
            } else {
                if changed {
                    sites.extend([0x1ca0, 0x1cac, 0x1cc0]);
                    cursor = -1;
                    if phase_write_allowed(gate_task) {
                        mode = 2;
                        submode = 0;
                    }
                }
                0x1cc4
            };
            // Previous-player byte is written only in the later common tail.
            return Ok(
                serde_json::json!({"entry_mode_copy":entry_mode,"mode":mode,"submode":submode,
                "cursor":cursor,"child14":child14,"child18":child18,"call_sites":sites,"continue_at":next}),
            );
        }
        PlacementClock::CommonTail {
            weakpoint_position,
            previous_target,
            mut flags,
            player,
            player_after_updates,
        } => {
            let mut target = previous_target;
            let mut sites = vec![0x1d78];
            if let Some([x, y, z]) = weakpoint_position {
                let threshold =
                    f32::from_be_bytes(phase_constant(&rel, 0x2012, 4)?.try_into().unwrap());
                let opposite =
                    f32::from_be_bytes(phase_constant(&rel, 0x2032, 4)?.try_into().unwrap());
                let depth = if z.abs() < threshold {
                    if player == 0 {
                        opposite
                    } else {
                        threshold
                    }
                } else {
                    -z
                };
                target = [x, y, depth];
                sites.push(0x1da4);
                if flags & 0x100 != 0 {
                    sites.push(0x1db8);
                    flags &= !0x100;
                }
            }
            sites.extend([0x1dcc, 0x1dd4]);
            return Ok(
                serde_json::json!({"target":target,"flags_before_damage":flags,
                "previous_player_after_tail":player_after_updates,"call_sites":sites}),
            );
        }
        PlacementClock::BodyCollision => {
            let table = |site: u32| -> Result<&[u8]> {
                let p = rel
                    .reference(1, site)
                    .ok_or_else(|| Error::invalid("missing body collision table"))?;
                if (p.module, p.section, p.kind) != (88, 5, 6)
                    || rel.reference(1, site + 4)
                        != Some(crate::city_rel::Reference { kind: 4, ..p })
                {
                    return Err(Error::invalid("body collision table reference mismatch"));
                }
                Ok(&rel.data(5)?[p.addend as usize..])
            };
            let desc = table(0x566)?;
            let bindings = table(0x942)?;
            let count = (word(0x56c) & 0xffff) as usize;
            let mut slots = Vec::with_capacity(count);
            for i in 0..count {
                let d = &desc[i * 48..][..48];
                let b = &bindings[i * 32..][..32];
                let raw = |off| u32::from_be_bytes(d[off..off + 4].try_into().unwrap());
                let f = |off| f32::from_bits(raw(off));
                let local = |off| f32::from_be_bytes(b[off..off + 4].try_into().unwrap());
                let joint = b[0] as i8;
                slots.push(serde_json::json!({"slot":i,"kind":d[0],"form":d[1],"push":d[2],"damage":d[3],
                    "attributes":raw(4),"center":[f(8),f(12),f(16)],"parameters":[f(20),f(24),f(28),f(32)],
                    "rotation":[raw(36) as i32,raw(40) as i32,raw(44) as i32],"binding_index":joint,
                    "updates_center":joint>0,"binding_offset":[local(4),local(8),local(12)]}));
            }
            return Ok(
                serde_json::json!({"list_id":word(0x570)&0xffff,"slots":slots,
                "scope":"Initial descriptors and body center-update bindings; collision response and live transform buffers are external."}),
            );
        }
        PlacementClock::BodyContact {
            slot,
            own_query,
            other_query,
            gate_task,
            wait_work_present,
            mut wait_word,
            mut timer,
        } => {
            let mut sites = vec![0x7f4];
            let mut response = None;
            if let Some(slot) = slot {
                if u32::from(slot) >= (word(0x56c) & 0xffff) {
                    return Err(Error::invalid(
                        "body contact slot outside boss collision table",
                    ));
                }
                sites.extend([0x868, 0x878, 0x884]);
                let site = if own_query > other_query {
                    sites.extend([0x8a0, 0x8a4]);
                    if phase_write_allowed(gate_task) && wait_work_present {
                        timer = word(0xce4) as u16;
                        wait_word = 1;
                    }
                    0x896
                } else {
                    sites.extend([0x8b4, 0x8c8]);
                    0x85a
                };
                response = Some(f32::from_be_bytes(
                    phase_constant(&rel, site, 4)?.try_into().unwrap(),
                ));
                sites.extend([0x8d4, 0x8ec, 0x8fc]);
            }
            return Ok(
                serde_json::json!({"slot":slot,"response_scalar":response,"timer":timer,
                "wait_word":wait_word,"call_sites":sites}),
            );
        }
        PlacementClock::BodyImpulse {
            player,
            player_y,
            contact_vector,
        } => {
            return body_impulse(&rel, player, player_y, contact_vector);
        }
        PlacementClock::BodyContactGeometry {
            player,
            center,
            player_position,
            own_query,
            other_query,
        } => {
            let delta: [f32; 3] = std::array::from_fn(|i| player_position[i] - center[i]);
            let len = (delta[0] as f64)
                .hypot(delta[1] as f64)
                .hypot(delta[2] as f64);
            if len == 0.0 || !(len as f32).is_finite() {
                return Err(Error::invalid(
                    "unsupported coincident or nonfinite contact direction",
                ));
            }
            let margin = f32::from_be_bytes(phase_constant(&rel, 0x85a, 4)?.try_into().unwrap());
            let magnitude = len as f32 + margin;
            let point: [f32; 3] =
                std::array::from_fn(|i| center[i] + ((delta[i] as f64 / len) as f32) * magnitude);
            let vector: [f32; 3] = std::array::from_fn(|i| point[i] - center[i]);
            if !point.iter().chain(&vector).all(|x| x.is_finite()) {
                return Err(Error::invalid("unrepresentable body contact point"));
            }
            let impulse = body_impulse(
                &rel,
                player,
                player_position[1],
                (own_query <= other_query).then_some(vector),
            )?;
            return Ok(
                serde_json::json!({"contact_point":point,"contact_vector":vector,"impulse":impulse}),
            );
        }
        PlacementClock::BossConstructor {
            task_allocated,
            work_allocated,
            aux_allocated,
            collision_flags,
        } => {
            let mut sites = vec![0x448];
            if task_allocated {
                sites.extend([0x488, 0x498]);
                if !work_allocated {
                    sites.push(0x4ac);
                } else {
                    sites.push(0x4c0);
                    if !aux_allocated {
                        sites.push(0x4d0);
                    }
                }
            }
            if !(task_allocated && work_allocated && aux_allocated) {
                return Ok(serde_json::json!({"created":false,"call_sites":sites}));
            }
            let position_ptr = rel
                .reference(1, 0x4e2)
                .ok_or_else(|| Error::invalid("missing constructor position"))?;
            phase_constant(&rel, 0x4e2, 4)?;
            let at = position_ptr.addend as usize;
            let d = rel.data(4)?;
            let position =
                [0, 4, 8].map(|n| f32::from_be_bytes(d[at + n..at + n + 4].try_into().unwrap()));
            let yaw = i32::from_be_bytes(phase_constant(&rel, 0x502, 5)?.try_into().unwrap());
            let scale = f32::from_be_bytes(phase_constant(&rel, 0x512, 4)?.try_into().unwrap());
            let health = phase_constant(&rel, 0x52a, 5)?[0] as i8 as f32;
            sites.extend([0x574, 0x59c, 0x5ac, 0x5c4, 0x5f4, 0x60c]);
            return Ok(
                serde_json::json!({"created":true,"initial_position":position,
                "initial_yaw_bams":yaw,"initial_scale":vec![scale;3],"health":health,
                "collision_flags":collision_flags & !0x40,"call_sites":sites}),
            );
        }
        PlacementClock::BossDrawTransform {
            position,
            rotation_bams,
            scale,
        } => {
            // 0x1e30..0x1e6c: translate, Z, X, Y, fixed Y, scale.
            // Column vectors therefore encounter the scale first.
            let fixed = word(0x1e60) as i16 as i32;
            let [x, y, z] = rotation_bams;
            let trig =
                |angle: i32| ((angle as i16 as f64) * std::f64::consts::TAU / 65536.).sin_cos();
            let (sx, cx) = trig(x);
            let (sy, cy) = trig(y.wrapping_add(fixed));
            let (sz, cz) = trig(z);
            let mut matrix = crate::pose::identity_matrix();
            for axis in 0..3 {
                let mut v = [0.; 3];
                v[axis] = f64::from(scale[axis]);
                let [a, b, c] = v;
                let (a, c) = (cy * a + sy * c, -sy * a + cy * c);
                let (b, c) = (cx * b - sx * c, sx * b + cx * c);
                let (a, b) = (cz * a - sz * b, sz * a + cz * b);
                for (r, value) in [a, b, c].into_iter().enumerate() {
                    matrix[r][axis] = value;
                }
            }
            for r in 0..3 {
                matrix[r][3] = position[r] as f64;
            }
            return Ok(
                serde_json::json!({"model_to_stage":matrix,"fixed_yaw_bams":fixed,
                "scope":"Boss draw transform before camera projection; native trigonometric rounding is not emulated."}),
            );
        }
        PlacementClock::HitReaction {
            mut submode,
            mut flags,
            mut cursor,
            mut child14,
            mut child18,
            animation_flags,
            frame,
            completion_poll,
            gate_after_clear,
        } => {
            let mut sites = Vec::new();
            let mut mode = 3;
            let mut start_switch = false;
            match submode {
                0 => {
                    if child14 {
                        sites.push(0x1114);
                        child14 = false;
                    }
                    if child18 {
                        sites.push(0x112c);
                        child18 = false;
                    }
                    sites.extend([0x1144, 0x1158]);
                    submode = 1;
                }
                1 if animation_flags & 2 == 0 => {
                    sites.extend([
                        0x1190, 0x1198, 0x11ac, 0x11b4, 0x11d0, 0x11d8, 0x11e4, 0x11ec,
                    ]);
                    submode = 2;
                }
                2 => {
                    let limit = |site| -> Result<f32> {
                        Ok(f32::from_be_bytes(
                            phase_constant(&rel, site, 4)?.try_into().unwrap(),
                        ))
                    };
                    if frame > limit(0x1206)? && frame < limit(0x121a)? {
                        sites.extend([0x1240, 0x1250]);
                    }
                    sites.push(0x1258);
                    if completion_poll != 0 {
                        flags &= !4;
                        cursor = -1;
                        sites.extend([0x1274, 0x128c, 0x1298, 0x12b0]);
                        if phase_write_allowed(gate_after_clear) {
                            mode = 1;
                            submode = 0;
                        }
                        start_switch = true;
                    }
                }
                _ => {}
            }
            return Ok(
                serde_json::json!({"mode":mode,"submode":submode,"flags":flags,"cursor":cursor,
                "child14":child14,"child18":child18,"start_player_switch":start_switch,
                "call_sites":sites,"global_halfword_0a":0}),
            );
        }
        PlacementClock::SwitchStart {
            player,
            mut visits,
            reason,
            mut switch_count,
            mut target_count,
            player_counts,
            character,
            cue_gate,
            global42,
            global43,
        } => {
            if player > 1 {
                return Err(Error::invalid("invalid switch player slot"));
            }
            let s = switch_start(
                code,
                player,
                visits,
                reason,
                switch_count,
                target_count,
                player_counts,
                character,
            );
            let (cue, adjustments, motion) = (s.cue, s.adjustments, s.motion);
            visits = s.visits;
            switch_count = s.switch_count;
            target_count = s.target_count;
            return Ok(
                serde_json::json!({"active_player":player,"switch_state":1,"timer":0,"reason":0,"global3a":0,
                "visits":visits,"switch_count":switch_count,"target_count":target_count,"count_adjustments":adjustments,
                "outgoing_motion_write":motion,"selected_cue":cue,"play_cue":cue_gate==0 && global42!=1 && global43!=1}),
            );
        }
        PlacementClock::SwitchTick {
            mut player,
            mut switch_state,
            mut timer,
            poll,
        } => {
            if player > 1 || !(1..=3).contains(&switch_state) {
                return Err(Error::invalid(
                    "switch tick requires player 0/1 and handoff state 1..3",
                ));
            }
            let mut poll_site = None;
            let mut restored = false;
            match switch_state {
                1 if timer > word(0xf514) as i16 => {
                    poll_site = Some(0xf52c);
                    if poll != 0 {
                        player ^= 1;
                        switch_state = 2;
                        timer = 0;
                    }
                }
                2 => {
                    poll_site = Some(0xf6c0);
                    if poll != 0 {
                        switch_state = 3;
                        timer = 0;
                    }
                }
                3 if timer > word(0xf890) as i16 => {
                    switch_state = 0;
                    timer = 0;
                    restored = true;
                }
                _ => {}
            }
            // Shared tail after the stage's player and cue updates.
            timer = timer.wrapping_add(1);
            return Ok(
                serde_json::json!({"active_player":player,"switch_state":switch_state,
                "timer":timer,"poll_site":poll_site,"restore_global3a":restored,
                "scope":"Slot and timing transitions only; player/camera calls remain external."}),
            );
        }
        PlacementClock::SwitchCamera {
            entered_state,
            player,
            character,
            yaw,
            position_pointer,
            table_words,
            camera_mode,
            second_lookup,
        } => {
            if player > 1 {
                return Err(Error::invalid("invalid camera target slot"));
            }
            let c = handoff_camera(
                &rel,
                code,
                entered_state,
                character,
                yaw,
                position_pointer,
                &table_words,
                camera_mode,
                second_lookup,
            )?;
            return Ok(
                serde_json::json!({"camera_slot":0,"player":player,"packet_words":c.packet,
                "resource_table_offset":c.table_offset,"reset_argument":c.reset.map(|r| r.0),
                "reset_address":c.reset.map(|r| r.1),"submit_address":c.submit}),
            );
        }
        PlacementClock::SwitchReplay {
            mut player,
            mut switch_state,
            mut timer,
            mut countdown,
            mut visits,
            player1,
            player2,
            player_tasks,
            table_words,
            mut line,
            mut reason,
            mut switch_count,
            mut ring_clock,
            mut pool,
            mut rings,
            frames,
        } => {
            if player > 1 {
                return Err(Error::invalid("handoff replay requires player 0/1"));
            }
            let call = |site: u32, args: &[u32]| -> Result<serde_json::Value> {
                Ok(serde_json::json!({"site":site,"call":engine_import(&rel, site)?,"args":args}))
            };
            let store =
                |site: u32, region: &str, slot: Option<u8>, offset: u32, size: u32, value: i64| {
                    let mask = (1i64 << (8 * size)) - 1;
                    serde_json::json!({"site":site,"store":region,"slot":slot,"offset":offset,
                    "size":size,"value":value & mask})
                };
            let local = |site: u32, target: u32| serde_json::json!({"site":site,"local":target});
            // Section 1 + 0xfec8: two engine queries, then a pair of calls
            // ordered by the active player.
            let sound_tail =
                |ev: &mut Vec<serde_json::Value>, s: &StageFrame, player: u8| -> Result<()> {
                    ev.push(call(0xfecc, &[0])?);
                    if s.sound_queries[0] != 0 {
                        ev.push(call(0xfedc, &[1])?);
                        if s.sound_queries[1] != 0 {
                            if player == 0 {
                                ev.extend([call(0xfefc, &[0])?, call(0xff04, &[1])?]);
                            } else {
                                ev.extend([call(0xff10, &[1])?, call(0xff18, &[0])?]);
                            }
                        }
                    }
                    Ok(())
                };
            let mut out = Vec::new();
            for (n, f) in frames.iter().enumerate() {
                let mut ev = vec![call(0xf41c, &[])?];
                // Engine state 7 (0x8001ccbc) resets the task work.
                if f.game_state == 7 {
                    player = 0;
                    switch_state = 0;
                    timer = 0;
                    pool = 50;
                    switch_count = 0;
                    reason = 0;
                    visits = [0, 0];
                    line = 0;
                    for (site, offset, size, value) in [
                        (0xf42c, 0, 1, 0),
                        (0xf430, 1, 1, 0),
                        (0xf434, 8, 2, 0),
                        (0xf43c, 0xc, 2, 50),
                        (0xf440, 5, 1, 0),
                        (0xf444, 4, 1, 0),
                        (0xf448, 2, 1, 0),
                        (0xf44c, 3, 1, 0),
                        (0xf450, 6, 1, 0),
                    ] {
                        ev.push(store(site, "task", None, offset, size, value));
                    }
                }
                ev.push(call(0xf454, &[])?);
                if switch_state == 0 && f.stage.is_none() {
                    return Err(Error::invalid(format!(
                        "frame {n} is in state 0 and needs a stage block"
                    )));
                }
                if switch_state == 4 && f.global_7c & 3 == 0 && f.debris.is_none() {
                    return Err(Error::invalid(format!(
                        "frame {n} spawns state 4 debris and needs a debris block"
                    )));
                }
                // r31 keeps the slot that was active when the frame began.
                let r31 = player;
                ev.push(store(
                    if r31 == 0 { 0xf47c } else { 0xf498 },
                    "global",
                    None,
                    0x4b,
                    1,
                    r31.into(),
                ));
                if f.global_7c == 14400 {
                    ev.push(call(0xf4c4, &[0xe, 0, 0, 0])?);
                }
                if countdown > 0 {
                    countdown -= 1;
                    ev.push(store(0xf4d8, "task", None, 0xe, 2, countdown.into()));
                }
                let camera = |ev: &mut Vec<serde_json::Value>, state: u8, slot: u8| -> Result<()> {
                    let slot = slot as usize;
                    let c = handoff_camera(
                        &rel,
                        code,
                        state,
                        f.characters[slot],
                        f.yaws[slot],
                        player1[slot].wrapping_add(0x14),
                        &table_words,
                        f.camera_mode,
                        f.second_lookup,
                    )?;
                    let (&submit, reads) = c.sites.split_last().unwrap();
                    ev.push(call(reads[0], &[0])?);
                    if let Some((second, _)) = c.reset {
                        ev.push(call(reads[1], &[0])?);
                        ev.push(call(reads[2], &[0, second as u32])?);
                    }
                    ev.push(serde_json::json!({"site":submit,"call":c.submit,"args":[0],
                        "packet":c.packet,"resource_table_offset":c.table_offset}));
                    Ok(())
                };
                match switch_state {
                    1 if timer > word(0xf514) as i16 => {
                        ev.push(call(0xf52c, &[0xff, 0, 0, 0])?);
                        if f.poll != 0 {
                            switch_state = 2;
                            timer = 0;
                            player ^= 1;
                            ev.extend([
                                store(0xf53c, "task", None, 1, 1, 2),
                                store(0xf544, "task", None, 8, 2, 0),
                                store(
                                    if r31 == 0 { 0xf55c } else { 0xf564 },
                                    "task",
                                    None,
                                    0,
                                    1,
                                    player.into(),
                                ),
                                call(0xf580, &[player_tasks[0], player2[0]])?,
                                call(0xf588, &[0])?,
                                call(0xf594, &[player_tasks[1], player2[1]])?,
                                call(0xf59c, &[1])?,
                                // These two writes target the outgoing slot.
                                store(0xf5b8, "player1", Some(r31), 0, 1, 0x42),
                                store(0xf5c8, "player2", Some(r31), 0x176, 2, 0x6b),
                            ]);
                            camera(&mut ev, 2, player)?;
                        }
                    }
                    2 => {
                        ev.push(call(0xf6c0, &[0, 0, 0, 0])?);
                        if f.poll != 0 {
                            switch_state = 3;
                            timer = 0;
                            ev.extend([
                                store(0xf6d0, "task", None, 1, 1, 3),
                                store(0xf6d8, "task", None, 8, 2, 0),
                                store(0xf6f4, "player1", Some(r31), 0, 1, 0x42),
                                store(0xf70c, "player2", Some(r31), 0x176, 2, 0x64),
                                store(0xf718, "player2", Some(r31), 0x17c, 2, 0x57),
                                call(0xf720, &[0])?,
                            ]);
                            camera(&mut ev, 3, player)?;
                            let cue_result = match &f.stage {
                                Some(s) => {
                                    let (result, inner) = voice_gate(&rel, s.channels.as_ref())?;
                                    ev.push(serde_json::json!({"site":0xf800,"local":0xeb38,
                                        "result":result,"events":inner}));
                                    result
                                }
                                None => {
                                    ev.push(local(0xf800, 0xeb38));
                                    f.cue_result
                                }
                            };
                            if cue_result != -1 {
                                let v = visits[r31 as usize];
                                let (site, cue) = match (player, v) {
                                    (0, v) if v > 1 => (0xf848, 0x872),
                                    (0, _) => (0xf838, 0x871),
                                    (_, v) if v > 0 => (0xf87c, 0x874),
                                    _ => (0xf86c, 0x873),
                                };
                                countdown = 20;
                                ev.push(call(site, &[cue, 3])?);
                                ev.push(store(0xf884, "task", None, 0xe, 2, 20));
                            }
                        }
                    }
                    3 if timer > word(0xf890) as i16 => {
                        switch_state = 0;
                        timer = 0;
                        ev.extend([
                            store(0xf89c, "task", None, 1, 1, 0),
                            store(0xf8a0, "task", None, 8, 2, 0),
                            call(0xf8a4, &[])?,
                            call(0xf8ac, &[3])?,
                            store(0xf8bc, "global", None, 0x3a, 1, 1),
                            call(0xf8c4, &[0])?,
                        ]);
                    }
                    0 => {
                        let s = f.stage.as_ref().unwrap();
                        let me = r31 as usize;
                        let other = 1 - me;
                        // Engine ring services, modeled as a plain counter.
                        let get = |ev: &mut Vec<serde_json::Value>,
                                   rings: &[i32; 2],
                                   site: u32,
                                   p: usize|
                         -> Result<i32> {
                            ev.push(call(site, &[p as u32])?);
                            Ok(rings[p])
                        };
                        let add = |ev: &mut Vec<serde_json::Value>,
                                   rings: &mut [i32; 2],
                                   site: u32,
                                   p: usize,
                                   d: i32|
                         -> Result<()> {
                            ev.push(call(site, &[p as u32, d as u32])?);
                            rings[p] = rings[p].wrapping_add(d);
                            Ok(())
                        };
                        // Ring drain: once every 61 frames unless a hold is active.
                        if s.global_664 == 0 && s.global_42 != 1 && s.global_43 != 1 {
                            ring_clock = ring_clock.wrapping_add(1);
                            ev.push(store(0xfc28, "task", None, 0xa, 2, ring_clock.into()));
                            if ring_clock > 60 {
                                ring_clock = 0;
                                ev.push(store(0xfc54, "task", None, 0xa, 2, 0));
                                if f.global_7c >= 18000 {
                                    for (p, (g, a)) in
                                        [(0xfc68, 0xfc74), (0xfc7c, 0xfc88)].into_iter().enumerate()
                                    {
                                        let r = get(&mut ev, &rings, g, p)?;
                                        add(&mut ev, &mut rings, a, p, r.wrapping_neg())?;
                                    }
                                    pool = 0;
                                    ev.push(store(0xfc8c, "task", None, 0xc, 2, 0));
                                } else {
                                    add(&mut ev, &mut rings, 0xfc9c, me, -1)?;
                                    let held = i32::from(pool)
                                        .wrapping_sub(get(&mut ev, &rings, 0xfca4, me)?);
                                    let r = get(&mut ev, &rings, 0xfcb4, other)?;
                                    add(&mut ev, &mut rings, 0xfcc0, other, held.wrapping_sub(r))?;
                                }
                                // Warnings at 15 and 4 rings; at 0 the active
                                // player is defeated and the task enters state 4.
                                let warn = |ev: &mut Vec<serde_json::Value>,
                                            countdown: &mut i16,
                                            sites: (u32, u32, u32),
                                            cue: u32|
                                 -> Result<()> {
                                    let (result, inner) = voice_gate(&rel, s.channels.as_ref())?;
                                    ev.push(serde_json::json!({"site":sites.0,"local":0xeb38,
                                        "result":result,"events":inner}));
                                    if result == 0 {
                                        ev.push(call(sites.1, &[cue, 3])?);
                                        *countdown = 20;
                                        ev.push(store(sites.2, "task", None, 0xe, 2, 20));
                                    }
                                    Ok(())
                                };
                                if get(&mut ev, &rings, 0xfcc8, me)? as i16 == 15 {
                                    let cue = if player == 0 { 0x869 } else { 0x867 };
                                    warn(&mut ev, &mut countdown, (0xfd04, 0xfd18, 0xfd20), cue)?;
                                } else if get(&mut ev, &rings, 0xfd2c, me)? as i16 == 4 {
                                    let cue = if player == 0 { 0x86a } else { 0x868 };
                                    warn(&mut ev, &mut countdown, (0xfd68, 0xfd7c, 0xfd84), cue)?;
                                } else if get(&mut ev, &rings, 0xfd90, me)? as i16 == 0 {
                                    switch_state = 4;
                                    ev.extend([
                                        call(0xfda4, &[me as u32])?,
                                        store(0xfdac, "task", None, 1, 1, 4),
                                        call(0xfdb4, &[3])?,
                                        call(0xfdc0, &[0, 0xb])?,
                                        call(0xfdc8, &[0])?,
                                        call(0xfdd8, &[0, f.camera_mode as u32, 0])?,
                                    ]);
                                }
                            }
                        }
                        // Handoff trigger: the active player crosses its Z limit.
                        ev.push(local(0xfddc, 0x9f4));
                        let held = s.boss_flags.is_some_and(|b| b & 4 != 0);
                        let posing = s.actions[0].contains(&0x6d) || s.actions[1].contains(&0x74);
                        if !held && reason != 1 && !posing && player1[me] != 0 {
                            let z = s.positions[me][2];
                            let crossed = if me == 0 {
                                z < stage_f32(&rel, 0xfe7e, 5, 0)?
                            } else {
                                z > stage_f32(&rel, 0xfeb2, 5, 0)?
                            };
                            if crossed {
                                let r = switch_start(
                                    code,
                                    player,
                                    visits,
                                    reason,
                                    switch_count,
                                    pool,
                                    rings,
                                    f.characters[me],
                                );
                                let mut inner = vec![call(0xec38, &[])?, call(0xec40, &[3])?];
                                let (gate, gate_events) = voice_gate(&rel, s.channels.as_ref())?;
                                inner.push(serde_json::json!({"site":0xeccc,"local":0xeb38,
                                    "result":gate,"events":gate_events}));
                                let played = gate == 0 && s.global_42 != 1 && s.global_43 != 1;
                                if played {
                                    inner.push(call(0xed08, &[r.cue.into(), 3])?);
                                }
                                for (&(p, d), (g, a)) in r
                                    .adjustments
                                    .iter()
                                    .zip([(0xedb8, 0xedd4), (0xeddc, 0xede8)])
                                {
                                    get(&mut inner, &rings, g, p)?;
                                    add(&mut inner, &mut rings, a, p, d)?;
                                }
                                switch_state = 1;
                                timer = 0;
                                reason = 0;
                                visits = r.visits;
                                switch_count = r.switch_count;
                                pool = r.target_count;
                                ev.push(
                                    serde_json::json!({"site":if me == 0 {0xfe90} else {0xfec4},
                                    "local":0xec04,"cue":r.cue,"played":played,
                                    "motion_write":r.motion,"events":inner}),
                                );
                            }
                        }
                        sound_tail(&mut ev, s, player)?;
                    }
                    4 => {
                        // Section 1 + 0xf8cc: after a ring-out, debris every
                        // fourth frame around the defeated player.
                        let me = r31 as usize;
                        let k = |site: usize| stage_f32(&rel, site, 4, 0);
                        if f.global_7c & 3 == 0 {
                            let d = f.debris.as_ref().unwrap();
                            let scale = k(0xf912)?;
                            let u = |i: usize| scale * (d.randoms[i] as f64 as f32);
                            for site in [0xf97c, 0xf9bc, 0xf9fc, 0xfa3c, 0xfa78, 0xfac0] {
                                ev.push(call(site, &[])?);
                            }
                            let pos = [
                                k(0xf922)? - k(0xf91a)? * u(0),
                                k(0xf932)? - k(0xf92a)? * u(1),
                                k(0xf93a)? + k(0xf91a)? * u(2),
                            ];
                            let speed = k(0xf94a)? + k(0xf942)? * u(3);
                            let a = (k(0xf95a)? - k(0xf952)? * u(4)) as f64 as i32;
                            let b = (k(0xf962)? * u(5)) as f64 as i32;
                            for (site, angle) in [
                                (0xfb04, b),
                                (0xfb10, a),
                                (0xfb24, a),
                                (0xfb34, b),
                                (0xfb40, a),
                            ] {
                                ev.push(call(site, &[angle as u32])?);
                            }
                            let t = d.trig;
                            let dir = [speed * t[1] * t[0], speed * t[2], speed * t[4] * t[3]];
                            ev.push(serde_json::json!({"site":0xfb58,
                                "call":engine_import(&rel, 0xfb58)?,"vectors":[dir, d.origin]}));
                            ev.push(call(0xfb5c, &[])?);
                            let size = k(0xf972)? + k(0xf96a)? * u(6);
                            ev.push(serde_json::json!({"site":0xfba0,"local":0x1425c,
                                "vectors":[dir, pos],"floats":[size]}));
                        }
                        for (site, offset, constant) in
                            [(0xfbc4, 0xc, 0xfba6), (0xfbd8, 0x10, 0xfbca)]
                        {
                            let bits = k(constant)?.to_bits();
                            ev.push(store(
                                site,
                                "player_task",
                                Some(me as u8),
                                offset,
                                4,
                                bits.into(),
                            ));
                        }
                    }
                    s if !(0..=4).contains(&s) => {
                        let st = f.stage.as_ref().ok_or_else(|| {
                            Error::invalid(format!("frame {n} needs a stage block"))
                        })?;
                        sound_tail(&mut ev, st, player)?;
                    }
                    _ => {}
                }
                // Shared tail: two local updates, then the handoff timer.
                match &f.stage {
                    Some(s) => {
                        let inner =
                            arena_clamp(&rel, switch_state, s.present, s.positions, s.distances)?;
                        ev.push(serde_json::json!({"site":0xff1c,"local":0xee8c,"events":inner}));
                        let v = timed_voice(
                            &rel,
                            code,
                            player as i8,
                            countdown,
                            line,
                            f.global_7c as i32,
                            [s.global_42, s.global_43],
                            s.task_0c,
                            s.channels.as_ref(),
                        )?;
                        line = v.line;
                        ev.push(serde_json::json!({"site":0xff28,"local":0xf070,
                            "voice":v.voice,"spoke":v.spoke,"events":v.events}));
                    }
                    None => ev.extend([local(0xff1c, 0xee8c), local(0xff28, 0xf070)]),
                }
                timer = timer.wrapping_add(1);
                ev.push(store(0xff34, "task", None, 8, 2, timer.into()));
                out.push(serde_json::json!({"frame":n,"events":ev,"player":player,
                    "switch_state":switch_state,"timer":timer,"countdown":countdown,"line":line,
                    "reason":reason,"switch_count":switch_count,"ring_clock":ring_clock,
                    "pool":pool,"rings":rings,"visits":visits}));
            }
            return Ok(
                serde_json::json!({"frames":out,"player":player,"switch_state":switch_state,
                "timer":timer,"countdown":countdown,"returned_to_normal_dispatch":switch_state==0,
                "scope":"Handoff frames 0xf458..0xff34 with engine and local calls observed; their effects are external."}),
            );
        }
        PlacementClock::TimedVoice {
            player,
            countdown,
            line,
            global_7c,
            global_42,
            global_43,
            task_0c,
            channels,
        } => {
            let v = timed_voice(
                &rel,
                code,
                player,
                countdown,
                line,
                global_7c,
                [global_42, global_43],
                task_0c,
                channels.as_ref(),
            )?;
            return Ok(
                serde_json::json!({"line":v.line,"voice":v.voice,"spoke":v.spoke,"events":v.events,
                "scope":"Line selection and gating; the 0x18ed0 voice task is created by engine code."}),
            );
        }
        PlacementClock::VoiceGate { channels } => {
            let (result, events) = voice_gate(&rel, channels.as_ref())?;
            return Ok(serde_json::json!({"result":result,"events":events}));
        }
        PlacementClock::ArenaClamp {
            switch_state,
            present,
            positions,
            distances,
        } => {
            let events = arena_clamp(&rel, switch_state, present, positions, distances)?;
            return Ok(
                serde_json::json!({"skipped":switch_state == 4,"events":events,
                "scope":"Stage-side bounds only; the distance and position-set calls are engine code."}),
            );
        }
        PlacementClock::SuperPlayer(frame) => return super_player(&rel, &frame),
        PlacementClock::SuperFlight(frame) => return super_flight(&rel, &frame),
        PlacementClock::SuperCollision(frame) => return super_collision(&rel, &frame),
        PlacementClock::SuperHover(frame) => return super_hover(&rel, &frame),
        PlacementClock::InitialHealth => {
            let data = phase_constant(&rel, 0x52a, 5)?;
            return Ok(serde_json::json!({"health": data[0] as i8 as f32}));
        }
        PlacementClock::DamageCheck {
            mut health,
            mut mode,
            mut submode,
            mut flags,
            weakpoint_flags,
            gate_task,
        } => {
            let hit = mode != 3 && weakpoint_flags.is_some_and(|f| f & 4 != 0);
            let mut requested = None;
            if hit {
                let constant = |site| -> Result<f32> {
                    Ok(f32::from_be_bytes(
                        phase_constant(&rel, site, 4)?[..4].try_into().unwrap(),
                    ))
                };
                health -= constant(0x79a)?;
                let next = if health > constant(0x7b2)? { 3 } else { 13 };
                requested = Some(next);
                if phase_write_allowed(gate_task) {
                    mode = next;
                    submode = 0;
                }
                flags |= 4;
            }
            return Ok(
                serde_json::json!({"health":health,"mode":mode,"submode":submode,"flags":flags,
                "requested_mode":requested,"continue_at":if hit {0x900} else {0x7f0}}),
            );
        }
        PlacementClock::NextPhase {
            work28,
            cursor,
            mode,
            submode,
            gate_task,
        } => {
            let row = work28.trunc();
            if !(1.0..=6.0).contains(&row) {
                return Err(Error::invalid("phase selector outside the six source rows"));
            }
            let ptr = rel
                .reference(1, 0xd76)
                .ok_or_else(|| Error::invalid("missing phase table relocation"))?;
            let lo = crate::city_rel::Reference { kind: 4, ..ptr };
            if (ptr.module, ptr.section, ptr.kind) != (88, 5, 6)
                || rel.reference(1, 0xd7a) != Some(lo)
                || rel.reference(1, 0xda6) != Some(ptr)
                || rel.reference(1, 0xdaa) != Some(lo)
            {
                return Err(Error::invalid("phase table relocation mismatch"));
            }
            let start = ptr.addend as usize + (row as usize - 1) * 4;
            let table = &rel.data(5)?[start..start + 4];
            let next = cursor.wrapping_add(1);
            if !(0..4).contains(&next) {
                return Err(Error::invalid("phase cursor outside source row"));
            }
            let next = if (table[next as usize] as i8) < 0 {
                0
            } else {
                next
            };
            let selected = table[next as usize] as i8;
            // 0xdc4 requires a zero gate result. Its nonzero path tests
            // whether the same value equals both 3 and 13, so cannot write.
            let apply = phase_write_allowed(gate_task);
            return Ok(
                serde_json::json!({"cursor":next,"selected":selected,"applied":apply,
                "mode":if apply {selected} else {mode},"submode":if apply {0} else {submode}}),
            );
        }
        PlacementClock::AttackChild { child_mode } => {
            let ready = matches!(child_mode, None | Some(4));
            calls.push(0x6410);
            if ready {
                calls.push(0x3fc0);
            }
            return Ok(serde_json::json!({"submode":if ready {3} else {2},
                "clear_child_slot":child_mode == Some(4),"stageCalls":calls}));
        }
        PlacementClock::AttackGroup { slots } => {
            if slots.as_ref().is_some_and(|s| s.len() > 2) {
                return Err(Error::invalid(
                    "attack group has only two addressable child slots",
                ));
            }
            // 0x403c overwrites its return byte for every slot, including
            // missing children; an empty existing group returns zero.
            let status: i8 = match slots.as_deref() {
                None => -1,
                Some([]) => 0,
                Some(group) => group
                    .last()
                    .copied()
                    .flatten()
                    .map_or(-1, |flags| i8::from(flags & 0x200 != 0)),
            };
            calls.push(0x403c);
            if status == 1 {
                calls.extend([0xd40, 0xdc4]);
            }
            return Ok(
                serde_json::json!({"group_status":status,"clear_group_slot":status == 1,
                "select_next_phase":status == 1,"stageCalls":calls}),
            );
        }
        PlacementClock::Attack {
            mode,
            mut submode,
            mut task_timer,
            mut placement_timer,
        } => {
            if submode == 0 {
                calls.extend([0x3e58, 0x62a0]);
                task_timer = word(0x16fc) as u16;
                submode = 1;
            }
            match submode {
                1 => {
                    task_timer = task_timer.wrapping_sub(1);
                    if task_timer == 0 {
                        calls.push(0x63e8);
                        if mode == 11 {
                            submode = 2;
                        } else {
                            calls.push(0x6460);
                            placement_timer = word(0x1758) as u16;
                            submode = 4;
                        }
                    }
                }
                4 => {
                    placement_timer = placement_timer.wrapping_sub(1);
                    if placement_timer == 0 {
                        calls.push(0x86b8);
                        placement_timer = word(0x1820) as u16;
                    }
                }
                _ => return Err(Error::invalid("attack clock requires submode 0, 1 or 4")),
            }
            return Ok(
                serde_json::json!({"submode":submode,"task_timer":task_timer,
                "placement_timer":placement_timer,"stageCalls":calls,"requestPlacement":calls.contains(&0x86b8)}),
            );
        }
        PlacementClock::Periodic { timer } => {
            let mut next = timer.wrapping_sub(1);
            if next == 0 {
                calls.push(0x86b8);
                next = word(0x1820) as u16;
            }
            (next, None)
        }
    };
    Ok(
        serde_json::json!({"timer":timer,"submode":submode,"stageCalls":calls,"requestPlacement":calls.contains(&0x86b8)}),
    )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelPlacement {
    pub state: RetractState,
    pub task_flags: u16,
}

/// Section 1 + 0x866c cancels the queued placement before starting retraction.
/// Report the two imported calls in dispatch order; their effects remain external.
pub fn cancel_placement(bytes: &[u8], input: CancelPlacement) -> Result<serde_json::Value> {
    if crate::manifest::sha256(bytes)
        != "6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f"
    {
        return Err(Error::invalid("unsupported Finalhazard stage revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 88)?;
    let mut calls = Vec::new();
    for (site, arg5) in [(0x8848, 1), (0x8858, 0)] {
        let target = rel
            .reference(1, site)
            .ok_or_else(|| Error::invalid("missing weakpoint cancellation call"))?;
        calls.push(
            serde_json::json!({"module":target.module,"address":target.addend,"r4":0,"r5":arg5}),
        );
    }
    let mut state = input.state;
    state.pending = [-1, -1];
    state.submode = 0;
    Ok(
        serde_json::json!({"mode":3,"state":state,"task_flags":input.task_flags & !4,"external_calls":calls}),
    )
}

/// Addresses of the engine objects a super-form player update touches, so
/// pointer arguments can be reported exactly.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperAddresses {
    pub task: u32,
    /// Task words 0x34 (entity), 0x38 (second data block) and 0x40 (player).
    pub entity: u32,
    pub data2: u32,
    pub player: u32,
    /// Entity word 0x2c, its word 0xc, and player word 0x36c (0 when absent).
    pub collision: u32,
    pub inner: u32,
    pub fx: u32,
}
/// Player word 0x36c points at this effect block.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperFx {
    pub f04: f32,
    pub i0c: i32,
}
/// Inputs to section 1 + 0xad8c. Engine results are consumed in call order:
/// `lengths` for 0x80114eac, `randoms` for 0x8010d940, `trig` for
/// 0x80112050/0x80112068.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperPlayerFrame {
    pub addresses: SuperAddresses,
    /// Engine global 0x801cbd88 byte 0x37.
    pub enabled_mask: i8,
    /// Entity byte 0 and halfword 4.
    pub action: i8,
    pub entity_flags: i16,
    /// Collision halfword 4; inner byte 3, words 4 and 0x34.
    pub collision_flags: u16,
    pub inner_03: u8,
    pub inner_04: u32,
    pub inner_34: u32,
    /// Player bytes 0 (slot) and 1 (character), halfword 0x10, floats 0x68,
    /// 0xec, 0x134, 0x184, halfwords 0x176/0x178, byte 0x35c, halfword 0x35e,
    /// vectors 0x218, 0x230, 0x360.
    pub slot: i8,
    pub character: i8,
    pub flags_10: i16,
    pub f68: f32,
    pub fec: f32,
    pub f134: f32,
    pub f184: f32,
    pub anim_176: i16,
    pub anim_178: i16,
    pub counter_35c: i8,
    pub timer_35e: i16,
    pub v218: [f32; 3],
    pub v230: [f32; 3],
    pub v360: [f32; 3],
    pub fx: Option<SuperFx>,
    #[serde(default)]
    pub lengths: Vec<f32>,
    #[serde(default)]
    pub approach: f32,
    #[serde(default)]
    pub randoms: Vec<i32>,
    #[serde(default)]
    pub trig: Vec<f32>,
    #[serde(default)]
    pub defeat_query: i32,
}
/// Section 1 + 0xad8c: control flow of one super-form player update. Local
/// movement handlers are reported as call sites with their arguments.
fn super_player(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperPlayerFrame,
) -> Result<serde_json::Value> {
    let a = &f.addresses;
    if a.collision == 0 || a.inner == 0 {
        return Err(Error::invalid(
            "super player update needs its collision object",
        ));
    }
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let mut lengths = f.lengths.iter().copied();
    let mut randoms = f.randoms.iter().copied();
    let mut trig = f.trig.iter().copied();
    let missing = |what: &str| Error::invalid(format!("super player update needs more {what}"));
    let mut ev: Vec<serde_json::Value> = Vec::new();
    let call = |site: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?}))
    };
    let with = |mut v: serde_json::Value, key: &str, val: serde_json::Value| {
        v[key] = val;
        v
    };
    let store = |site: u32, region: &str, offset: u32, size: u32, value: u64| {
        let mask = if size == 4 {
            u32::MAX as u64
        } else {
            (1u64 << (8 * size)) - 1
        };
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value & mask})
    };
    let local = |site: u32, target: u32, args: &[u32]| serde_json::json!({"site":site,"local":target,"args":args});
    let trio = [a.entity, a.data2, a.player];
    let triple = |ev: &mut Vec<serde_json::Value>, sites: [u32; 3]| -> Result<()> {
        for site in sites {
            ev.push(with(call(site)?, "args", serde_json::json!(trio)));
        }
        Ok(())
    };
    let mut action = f.action;
    let mut entity_flags = f.entity_flags;
    let mut counter = f.counter_35c;
    let mut timer = f.timer_35e;
    let mut f68 = f.f68;
    let mut anim_176 = f.anim_176;
    let mut f18c = None;
    let mut v360 = f.v360;
    let mut fx = f.fx.as_ref().map(|x| (x.f04, x.i0c));
    let float = |r: i32| r as f64 as f32;

    ev.push(with(
        call(0xadd8)?,
        "args",
        serde_json::json!([a.task, a.entity, a.data2, a.player]),
    ));
    let enabled = f.enabled_mask as i32 & (1i32.wrapping_shl(f.slot as u32)) != 0;
    if !enabled {
        ev.push(local(
            0xae1c,
            0xbad0,
            &[a.entity, a.data2, a.player, a.player],
        ));
    }
    // Collision flags for this frame.
    let mut inner_04 = f.inner_04 & !0x4000;
    ev.push(store(0xae38, "inner", 4, 4, inner_04.into()));
    let (low, mid) = if f.flags_10 as u16 & 0x8000 != 0 {
        (3, 3)
    } else if entity_flags as u16 & 0x400 != 0 {
        inner_04 |= 0x4000;
        ev.push(store(0xae78, "inner", 4, 4, inner_04.into()));
        (2, 1)
    } else {
        (0, 0)
    };
    let mut inner_03 = (f.inner_03 & 0xfc) | (low & 3);
    ev.push(store(0xae98, "inner", 3, 1, inner_03.into()));
    inner_03 = (inner_03 & 0xf3) | ((mid << 2) & 0xc);
    ev.push(store(0xaeac, "inner", 3, 1, inner_03.into()));
    let boosting = action == 61 || (action as u8).wrapping_sub(62) <= 2 || action == 67;
    let inner_34 = if boosting {
        f.inner_34 & !0x10
    } else {
        f.inner_34 | 0x10
    };
    ev.push(store(
        if boosting { 0xaee0 } else { 0xaef0 },
        "inner",
        0x34,
        4,
        inner_34.into(),
    ));
    let mut collision_flags = f.collision_flags;
    // Unless the player is posing, the engine is given the player radius
    // plus the length of the player's 0x64 vector.
    let camera_follow = |ev: &mut Vec<serde_json::Value>,
                         lengths: &mut dyn Iterator<Item = f32>,
                         sites: [u32; 3]|
     -> Result<()> {
        ev.push(with(call(sites[0])?, "args", serde_json::json!([a.task])));
        ev.push(with(
            call(sites[1])?,
            "args",
            serde_json::json!([a.player + 0x64]),
        ));
        let len = lengths.next().ok_or_else(|| missing("lengths"))?;
        ev.push(with(
            with(call(sites[2])?, "args", serde_json::json!([a.task])),
            "floats",
            serde_json::json!([f.f134 + len]),
        ));
        Ok(())
    };
    let clear_follow =
        |ev: &mut Vec<serde_json::Value>, collision_flags: &mut u16, sites: [u32; 3]| {
            *collision_flags &= !0x1f;
            ev.push(store(sites[0], "collision", 0xa2, 2, 0xffff));
            ev.push(store(sites[1], "collision", 0xa4, 4, 0));
            ev.push(store(
                sites[2],
                "collision",
                4,
                2,
                (*collision_flags).into(),
            ));
        };
    if enabled {
        ev.push(local(0xaf24, 0xc598, &[a.entity, a.data2, a.player]));
        ev.push(with(
            call(0xaf30)?,
            "args",
            serde_json::json!([a.entity, a.player]),
        ));
        let anim = f.anim_178;
        let posing = anim == 54
            || (anim as u16).wrapping_sub(100) <= 2
            || (anim as u16).wrapping_sub(112) <= 1
            || anim == 85;
        if posing {
            clear_follow(&mut ev, &mut collision_flags, [0xaf94, 0xafa0, 0xafb0]);
        } else {
            camera_follow(&mut ev, &mut lengths, [0xaf6c, 0xaf74, 0xaf84])?;
        }
        ev.push(with(call(0xafc0)?, "args", serde_json::json!(trio)));
    } else {
        counter = counter.wrapping_add(1);
        ev.push(store(0xafd0, "player", 0x35c, 1, counter as u8 as u64));
        if counter >= 6 {
            counter = 0;
            ev.push(store(0xafe8, "player", 0x35c, 1, 0));
        }
        // Spark effect when the player moves faster than its threshold.
        if !matches!(action, 15 | 53 | 0 | 59 | 57) {
            ev.push(with(
                call(0xb020)?,
                "args",
                serde_json::json!([a.player + 0x64]),
            ));
            let len = lengths.next().ok_or_else(|| missing("lengths"))?;
            if len > f.fec {
                ev.push(call(0xb030)?);
                let r = randoms.next().ok_or_else(|| missing("randoms"))?;
                let n = (k(0xb06a)? * (k(0xb05a)? * float(r))) as f64 as i32;
                ev.push(with(
                    call(0xb094)?,
                    "args",
                    serde_json::json!([f.slot as i32 as u32, n as u32, a.player + 0x218, a.data2]),
                ));
            }
        }
        // Action dispatch (jump table at section 5 + 0x3fd50).
        let approach = |ev: &mut Vec<serde_json::Value>,
                        f68: &mut f32,
                        sites: (u32, usize, usize, u32)|
         -> Result<()> {
            ev.push(with(
                call(sites.0)?,
                "floats",
                serde_json::json!([*f68, k(sites.1)?, k(sites.2)?]),
            ));
            *f68 = f.approach;
            ev.push(store(
                sites.3,
                "player",
                0x68,
                4,
                f.approach.to_bits().into(),
            ));
            Ok(())
        };
        let move3 =
            |ev: &mut Vec<serde_json::Value>, site: u32| ev.push(local(site, 0xe370, &trio));
        let hold3 =
            |ev: &mut Vec<serde_json::Value>, site: u32| ev.push(local(site, 0xe9f0, &trio));
        match action {
            0 | 59 => {
                if action == 0 {
                    action = 59;
                    ev.push(store(0xb0c4, "entity", 0, 1, 59));
                }
                if f.anim_178 == 99 {
                    hold3(&mut ev, 0xb104);
                } else {
                    ev.push(with(call(0xb0e0)?, "args", serde_json::json!(trio)));
                    move3(&mut ev, 0xb0f0);
                }
                triple(&mut ev, [0xb114, 0xb124, 0xb134])?;
            }
            60 => {
                move3(&mut ev, 0xb148);
                triple(&mut ev, [0xb158, 0xb168, 0xb178])?;
                ev.push(local(0xb180, 0xe1ac, &[a.player]));
            }
            61..=64 => {
                let i = (action - 61) as u32;
                let sites = [
                    (0xb1a4, 0xb18e, 0xb19a, 0xb1a8),
                    (0xb20c, 0xb1f6, 0xb202, 0xb210),
                    (0xb274, 0xb25e, 0xb26a, 0xb278),
                    (0xb2dc, 0xb2c6, 0xb2d2, 0xb2e0),
                ][i as usize];
                approach(&mut ev, &mut f68, sites)?;
                move3(&mut ev, sites.0 + 0x14);
                triple(&mut ev, [sites.0 + 0x24, sites.0 + 0x34, sites.0 + 0x44])?;
            }
            67 => {
                triple(&mut ev, [0xb334, 0xb344, 0xb354])?;
                ev.push(local(0xb364, 0xc67c, &[a.entity, a.player, a.player]));
            }
            15 => triple(&mut ev, [0xb378, 0xb388, 0xb398])?,
            53 => {
                hold3(&mut ev, 0xb3ac);
                triple(&mut ev, [0xb3bc, 0xb3cc, 0xb3dc])?;
            }
            57 => {
                // Ring-out: after 40 frames a burst of 15 debris pieces.
                let t = timer;
                timer = timer.wrapping_add(1);
                ev.push(store(0xb3ec, "player", 0x35e, 2, timer as u16 as u64));
                if t > 40 && f.anim_178 != 104 {
                    f18c = Some(f.f184);
                    ev.push(store(0xb408, "player", 0x18c, 4, f.f184.to_bits().into()));
                    anim_176 = 104;
                    ev.push(store(0xb410, "player", 0x176, 2, 104));
                    let scale = k(0xb422)?;
                    for _ in 0..15 {
                        let mut u = || -> Result<f32> {
                            Ok(scale * float(randoms.next().ok_or_else(|| missing("randoms"))?))
                        };
                        let mut us = [0f32; 6];
                        for (i, site) in [0xb48c, 0xb4cc, 0xb50c, 0xb54c, 0xb588, 0xb5d0]
                            .into_iter()
                            .enumerate()
                        {
                            ev.push(call(site)?);
                            us[i] = u()?;
                        }
                        let pos = [
                            k(0xb432)? - k(0xb42a)? * us[0],
                            k(0xb442)? - k(0xb43a)? * us[1],
                            k(0xb432)? + k(0xb44a)? * us[2],
                        ];
                        let speed = k(0xb45a)? + k(0xb452)? * us[3];
                        let ang_a = (k(0xb46a)? - k(0xb462)? * us[4]) as f64 as i32;
                        let ang_b = (k(0xb472)? * us[5]) as f64 as i32;
                        let mut t5 = [0f32; 5];
                        for (i, (site, ang)) in [
                            (0xb614, ang_b),
                            (0xb620, ang_a),
                            (0xb634, ang_a),
                            (0xb644, ang_b),
                            (0xb650, ang_a),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            ev.push(with(call(site)?, "args", serde_json::json!([ang as u32])));
                            t5[i] = trig.next().ok_or_else(|| missing("trig"))?;
                        }
                        let dir = [speed * t5[1] * t5[0], speed * t5[2], speed * t5[4] * t5[3]];
                        ev.push(with(
                            call(0xb668)?,
                            "vectors",
                            serde_json::json!([dir, f.v230]),
                        ));
                        ev.push(call(0xb66c)?);
                        let size = k(0xb47a)? + u()?;
                        ev.push(serde_json::json!({"site":0xb6ac,"local":0x1425c,"vectors":[dir, pos],"floats":[size]}));
                    }
                }
                ev.push(with(call(0xb6c8)?, "args", serde_json::json!(trio)));
                hold3(&mut ev, 0xb6d8);
                triple(&mut ev, [0xb6e8, 0xb6f8, 0xb708])?;
            }
            58 => ev.push(local(
                0xb720,
                0xdc90,
                &[a.entity, a.data2, a.player, a.player],
            )),
            _ => {}
        }
        ev.push(with(
            call(0xb728)?,
            "args",
            serde_json::json!([a.player + 0x174]),
        ));
        ev.push(local(0xb738, 0xc598, &trio));
        if let Some((mut f04, mut i0c)) = fx {
            ev.push(call(0xb748)?);
            let r = randoms.next().ok_or_else(|| missing("randoms"))?;
            let jitter = k(0xb772)? * float(r);
            // Character 9 and 10 each scale the travel since last frame.
            for (ch, s) in [
                (
                    9,
                    [
                        0xb7b0u32, 0xb7b8, 0xb7be, 0xb7d4, 0xb808, 0xb816, 0xb85a, 0xb85a, 0xb87c,
                        0xb88e, 0xb8a8, 0xb8b0,
                    ],
                ),
                (
                    10,
                    [
                        0xb8e4, 0xb8ec, 0xb8f2, 0xb908, 0xb93c, 0xb94a, 0xb98e, 0xb99a, 0xb9bc,
                        0xb9ce, 0xb9e8, 0xb9f0,
                    ],
                ),
            ] {
                if f.character != ch {
                    continue;
                }
                ev.push(with(
                    call(s[0])?,
                    "vectors",
                    serde_json::json!([f.v218, v360]),
                ));
                ev.push(with(call(s[1])?, "vectors", serde_json::json!([f.v218])));
                let len = lengths.next().ok_or_else(|| missing("lengths"))?;
                let travel = k(s[2] as usize)? * len;
                v360 = f.v218;
                for (i, v) in v360.iter().enumerate() {
                    ev.push(store(
                        s[3] + [0, 4, 0xc][i],
                        "player",
                        0x360 + 4 * i as u32,
                        4,
                        v.to_bits().into(),
                    ));
                }
                if travel.abs() > f04.abs() {
                    f04 = travel;
                    ev.push(store(s[4], "fx", 4, 4, f04.to_bits().into()));
                }
                if travel.abs() >= k(s[5] as usize)? {
                    let step = if ch == 9 {
                        k(s[6] as usize)? + k(s[6] as usize)? * jitter
                    } else {
                        k(s[6] as usize)? + k(s[7] as usize)? * jitter
                    };
                    i0c = (float(i0c) + step) as f64 as i32;
                    ev.push(store(s[8], "fx", 0xc, 4, i0c as u32 as u64));
                } else if travel.abs() > k(s[9] as usize)? {
                    i0c = 0x4000;
                    ev.push(store(s[10], "fx", 0xc, 4, 0x4000));
                }
                ev.push(with(call(s[11])?, "args", serde_json::json!([a.fx])));
            }
            fx = Some((f04, i0c));
        }
        ev.push(with(
            call(0xb9fc)?,
            "args",
            serde_json::json!([f.slot as i32 as u32]),
        ));
        if f.defeat_query != 0 {
            action = 58;
            ev.push(store(0xba0c, "entity", 0, 1, 58));
            entity_flags &= -0x3501;
            ev.push(store(0xba1c, "entity", 4, 2, entity_flags as u16 as u64));
        }
        ev.push(with(
            call(0xba28)?,
            "args",
            serde_json::json!([a.entity, a.player]),
        ));
        if f.anim_178 == 54 || f.anim_178 == 85 || action == 54 {
            clear_follow(&mut ev, &mut collision_flags, [0xba7c, 0xba88, 0xba98]);
        } else {
            camera_follow(&mut ev, &mut lengths, [0xba54, 0xba5c, 0xba6c])?;
        }
        ev.push(with(call(0xbaa8)?, "args", serde_json::json!(trio)));
    }
    Ok(
        serde_json::json!({"events":ev,"action":action,"entity_flags":entity_flags,
        "collision_flags":collision_flags,"inner_03":inner_03,"inner_04":inner_04,"inner_34":inner_34,
        "counter_35c":counter,"timer_35e":timer,"f68":f68,"anim_176":anim_176,"f18c":f18c,
        "v360":v360,"fx":fx.map(|(a, b)| serde_json::json!({"f04":a,"i0c":b})),
        "scope":"Control flow of 0xad8c. Engine services and the local movement handlers 0xbad0, 0xc598, 0xe370, 0xe9f0, 0xe1ac, 0xc67c and 0xdc90 are reported, not run."}),
    )
}

/// Inputs to section 1 + 0xe370. Engine outputs are inputs here: the stick
/// query 0x8004e2f8 (`held`, its angle and magnitude outputs), the 0x70
/// vector length (0x80114eac), the rotated axis (0x801151a4 output), the
/// two dot products (0x80114c88), the six-float 0x8011b470 output, and the
/// two angle results (0x8002b55c, 0x8002b678).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperFlightFrame {
    pub entity: u32,
    pub data2: u32,
    pub player: u32,
    /// Entity words 8, 0xc and 0x10 (rotation).
    pub rotation: [i32; 3],
    /// Data2 floats 0xc..0x14 (acceleration in).
    pub acceleration: [f32; 3],
    /// Player floats: 0x34, 0x64, 0x68, 0x6c, 0xd0, 0xe4, 0xe8, 0xec, 0xfc,
    /// 0x104, 0x114, 0x118, 0x11c, and vectors 0x70 and 0x7c.
    pub f34: f32,
    pub speed: f32,
    pub f68: f32,
    pub f6c: f32,
    pub fd0: f32,
    pub fe4: f32,
    pub fe8: f32,
    pub fec: f32,
    pub ffc: f32,
    pub f104: f32,
    pub f114: f32,
    pub f118: f32,
    pub f11c: f32,
    pub v70: [f32; 3],
    pub v7c: [f32; 3],
    pub held: i32,
    pub stick_angle: i32,
    pub stick_magnitude: f32,
    #[serde(default)]
    pub v70_length: f32,
    #[serde(default)]
    pub rotated: [f32; 3],
    #[serde(default)]
    pub dots: [f32; 2],
    #[serde(default)]
    pub cross: [f32; 6],
    #[serde(default)]
    pub turn: i32,
    #[serde(default)]
    pub angle_delta: i32,
}
/// Section 1 + 0xe370: flight thrust, speed limits and turning, with the
/// engine's vector services taken from the request.
fn super_flight(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperFlightFrame,
) -> Result<serde_json::Value> {
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let call = |site: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?}))
    };
    let with = |mut v: serde_json::Value, key: &str, val: serde_json::Value| {
        v[key] = val;
        v
    };
    let store = |site: u32, region: &str, offset: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":4,"value":value});
    let zero = k(0xe3aa)?;
    let mut ev = Vec::new();
    let mut acc = f.acceleration;
    let mut out = [zero, zero, zero];
    ev.push(with(call(0xe3c8)?, "args", serde_json::json!([f.entity])));
    ev.push(with(
        call(0xe3dc)?,
        "args",
        serde_json::json!([f.entity, f.player]),
    ));
    let held = f.held != 0;
    let spd = f.speed;
    // Thrust along the forward axis from the current speed.
    if !held && spd > f.fe8 {
        acc[0] += f.f114 * spd;
    } else if held && spd > f.fd0 {
        acc[0] += k(0xe42e)? * f.f114 * (spd - f.fd0);
    } else if spd > f.fd0 {
        acc[0] += f.f114 * (spd - f.fd0);
    } else if spd < k(0xe482)? {
        acc[0] += f.f114 * spd;
    }
    acc[1] += f.f118 * f.f68;
    acc[2] += f.f11c * f.f6c;
    let mut angle = f.stick_angle;
    let f3c;
    if !held {
        if spd > k(0xe4e6)? {
            out[0] = f.f104;
        } else if spd < k(0xe4e6)? {
            out[0] = -f.f104;
        }
        f3c = Some((0xe520, k(0xe516)?));
    } else {
        // Stick magnitude shaped by the speed bands 0xd0/0xe4/0xe8/0xec.
        let mut mag = f.stick_magnitude;
        let fc = f.ffc;
        if spd < f.fd0 {
            if spd < f.fe4 {
                if mag > k(0xe54a)? {
                    mag *= fc;
                } else if spd < k(0xe56e)? * f.fe4 {
                    let floor = k(0xe586)?;
                    if mag < floor {
                        mag = floor;
                    }
                    mag *= fc;
                } else {
                    mag = k(0xe5b2)?;
                }
            } else if spd < f.fe8 {
                // Above 0.7, or slower than 0.5 x (2 x 0xe8): keep the magnitude.
                if mag > k(0xe5d6)? || spd < k(0xe5fa)? * (f.fe8 + f.fe8) {
                    mag *= fc;
                } else {
                    mag = k(0xe626)?;
                }
            } else if spd < f.fec {
                if mag > k(0xe64a)? {
                    mag *= fc;
                } else {
                    mag *= k(0xe66e)? * fc;
                }
            } else {
                mag *= fc;
            }
        } else if spd >= f.fd0 && f.f34 < k(0xe6ae)? {
            mag *= fc;
        } else {
            mag *= k(0xe6da)? * fc;
        }
        // Turn toward the stick, corrected by the travel direction.
        ev.push(with(
            call(0xe6f8)?,
            "args",
            serde_json::json!([f.player + 0x70]),
        ));
        if f.v70_length > k(0xe6fe)? {
            let one = k(0xe712)?;
            ev.push(call(0xe72c)?);
            ev.push(with(
                call(0xe738)?,
                "args",
                serde_json::json!([0, f.rotation[2] as u32]),
            ));
            ev.push(with(
                call(0xe744)?,
                "args",
                serde_json::json!([0, f.rotation[0] as u32]),
            ));
            ev.push(with(
                call(0xe754)?,
                "args",
                serde_json::json!([0, angle.wrapping_neg() as u32]),
            ));
            ev.push(with(
                with(call(0xe764)?, "args", serde_json::json!([0])),
                "vectors",
                serde_json::json!({"4":[one, zero, zero]}),
            ));
            ev.push(call(0xe768)?);
            ev.push(with(
                call(0xe774)?,
                "vectors",
                serde_json::json!({"3":f.rotated,"4":f.v70}),
            ));
            if f.dots[0] < zero {
                let mut b3 = [zero; 6];
                b3[3..].copy_from_slice(&f.v70);
                let mut b4 = [zero; 6];
                b4[3..].copy_from_slice(&f.v7c);
                ev.push(with(
                    call(0xe7ec)?,
                    "blocks",
                    serde_json::json!({"3":b3,"4":b4}),
                ));
                let mut axis = [f.cross[3], f.cross[4], f.cross[5]];
                ev.push(with(
                    call(0xe7fc)?,
                    "vectors",
                    serde_json::json!({"3":axis,"4":f.rotated}),
                ));
                if f.dots[1] < zero {
                    axis = axis.map(|x| -x);
                }
                ev.push(with(
                    call(0xe83c)?,
                    "vectors",
                    serde_json::json!({"3":f.rotated,"4":axis,"5":f.v7c}),
                ));
                angle = angle.wrapping_sub(f.turn);
            }
        }
        ev.push(with(
            call(0xe854)?,
            "args",
            serde_json::json!([f.rotation[1] as u32, angle as u32]),
        ));
        let d = f.angle_delta;
        let half = k(0xe8a2)?;
        let turn = |ev: &mut Vec<serde_json::Value>, store_site: u32, site: u32| -> Result<()> {
            ev.push(store(store_site, "data2", 0x1c, angle as u32));
            ev.push(with(
                call(site)?,
                "args",
                serde_json::json!([f.entity, f.data2, f.player, angle as u32]),
            ));
            Ok(())
        };
        if k(0xe85a)? == spd && d > 4096 {
            mag = k(0xe85a)?;
            turn(&mut ev, 0xe87c, 0xe898)?;
        } else if spd >= half * (f.fe4 + f.fe8) && d > 4096 {
            mag = f.f104;
            turn(&mut ev, 0xe8d4, 0xe8f0)?;
        } else if spd >= f.fe4 && d < 4096 {
            turn(&mut ev, 0xe910, 0xe924)?;
        } else if spd >= f.fe4 && spd <= f.fec && d > 8192 {
            mag *= k(0xe95e)?;
            turn(&mut ev, 0xe954, 0xe980)?;
        } else {
            turn(&mut ev, 0xe98c, 0xe9a0)?;
        }
        f3c = Some((0xe9a8, mag));
        out[0] = mag;
    }
    if let Some((site, v)) = f3c {
        ev.push(store(site, "player", 0x3c, v.to_bits()));
    }
    ev.push(with(
        call(0xe9bc)?,
        "vectors",
        serde_json::json!({"3":acc,"4":out}),
    ));
    for (i, site) in [0xe9c8u32, 0xe9cc, 0xe9d4].into_iter().enumerate() {
        ev.push(store(site, "player", 0x58 + 4 * i as u32, acc[i].to_bits()));
    }
    Ok(
        serde_json::json!({"events":ev,"acceleration":acc,"thrust":out,"angle":angle,
        "scope":"Stage-side flight math of 0xe370. The engine stick query, vector and matrix services and the turn routines are observed; their effects are not modeled."}),
    )
}

/// Inputs to section 1 + 0xc598.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperCollisionFrame {
    pub entity: u32,
    pub data2: u32,
    pub player: u32,
    pub action: i8,
    /// Entity floats 0x14..0x1c (position) and player float 0x140.
    pub position: [f32; 3],
    pub height: f32,
    /// The vector the engine call 0x8003e778 leaves in place of (0, height, 0).
    pub rotated: [f32; 3],
}
/// Section 1 + 0xc598: unless the action is 67, clears the entity's X and Z
/// rotation and two data2 words; then places the collision sphere at the
/// position plus the rotated height offset.
fn super_collision(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperCollisionFrame,
) -> Result<serde_json::Value> {
    let store = |site: u32, region: &str, offset: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":4,"value":value});
    let mut ev = Vec::new();
    if f.action != 67 {
        ev.extend([
            store(0xc5cc, "data2", 0x20, 0),
            store(0xc5d0, "data2", 0x18, 0),
            store(0xc5d4, "entity", 0x10, 0),
            store(0xc5d8, "entity", 0x8, 0),
        ]);
    }
    let zero = stage_f32(rel, 0xc5de, 4, 0)?;
    ev.push(
        serde_json::json!({"site":0xc600,"call":engine_import(rel, 0xc600)?,
        "args":[f.entity],"vectors":{"4":[zero, f.height, zero]}}),
    );
    let mut center = f.rotated;
    for (i, site) in [0xc60cu32, 0xc610, 0xc618].into_iter().enumerate() {
        ev.push(store(site, "inner", 8 + 4 * i as u32, center[i].to_bits()));
    }
    for (i, site) in [0xc628u32, 0xc638, 0xc648].into_iter().enumerate() {
        center[i] += f.position[i];
        ev.push(store(site, "inner", 8 + 4 * i as u32, center[i].to_bits()));
    }
    for (i, site) in [0xc654u32, 0xc658, 0xc660].into_iter().enumerate() {
        ev.push(store(
            site,
            "inner",
            0x38 + 4 * i as u32,
            center[i].to_bits(),
        ));
    }
    Ok(serde_json::json!({"events":ev,"center":center}))
}

/// Inputs to section 1 + 0xe9f0.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperHoverFrame {
    pub entity: u32,
    /// Data2 floats 0xc..0x14 and player floats 0x64, 0x68, 0x6c, 0xe8,
    /// 0x104, 0x114, 0x118, 0x11c.
    pub acceleration: [f32; 3],
    pub speed: f32,
    pub f68: f32,
    pub f6c: f32,
    pub fe8: f32,
    pub f104: f32,
    pub f114: f32,
    pub f118: f32,
    pub f11c: f32,
}
/// Section 1 + 0xe9f0: the released-stick part of flight. Forward thrust
/// above speed 0xe8 and drag below 0, vertical and side thrust, a fixed
/// coast value, and zero magnitude at player float 0x3c.
fn super_hover(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperHoverFrame,
) -> Result<serde_json::Value> {
    let zero = stage_f32(rel, 0xea22, 4, 0)?;
    let store = |site: u32, offset: u32, value: u32| serde_json::json!({"site":site,"store":"player","offset":offset,"size":4,"value":value});
    let mut ev = vec![
        serde_json::json!({"site":0xea3c,"call":engine_import(rel, 0xea3c)?,
        "args":[f.entity]}),
    ];
    let mut acc = f.acceleration;
    let spd = f.speed;
    if spd > f.fe8 || spd < zero {
        acc[0] += f.f114 * spd;
    }
    acc[1] += f.f118 * f.f68;
    acc[2] += f.f11c * f.f6c;
    let mut out = [zero; 3];
    if spd > stage_f32(rel, 0xeabe, 4, 0)? {
        out[0] = f.f104;
    } else if spd < stage_f32(rel, 0xeabe, 4, 0)? {
        out[0] = -f.f104;
    }
    ev.push(store(0xeaf8, 0x3c, stage_f32(rel, 0xeaee, 4, 0)?.to_bits()));
    ev.push(
        serde_json::json!({"site":0xeb04,"call":engine_import(rel, 0xeb04)?,
        "vectors":{"3":acc,"4":out}}),
    );
    for (i, site) in [0xeb10u32, 0xeb14, 0xeb1c].into_iter().enumerate() {
        ev.push(store(site, 0x58 + 4 * i as u32, acc[i].to_bits()));
    }
    Ok(serde_json::json!({"events":ev,"acceleration":acc,"thrust":out}))
}
