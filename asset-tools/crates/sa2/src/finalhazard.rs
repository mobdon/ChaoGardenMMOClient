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
