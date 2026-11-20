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
    /// Section 1 + 0xdc90: action 58, stick-driven free movement.
    SuperFreeMove(Box<SuperFreeMoveFrame>),
    /// Section 1 + 0xbad0: super-form hit handling and action selection.
    SuperControl(Box<SuperControlFrame>),
    /// Section 1 + 0xc67c: the action-67 charge effect draw.
    SuperChargeDraw(Box<SuperChargeDrawFrame>),
    /// Section 1 + 0xd2dc: the super-form player draw routine.
    SuperDraw(Box<SuperDrawFrame>),
    /// Section 1 + 0xd60c: super-form effects draw (spin, aura, fade, trail).
    SuperEffects(Box<SuperEffectsFrame>),
    /// Section 1 + 0x19470: the stage's ring drop when a player is hit.
    RingDrop(Box<RingDropFrame>),
    /// Section 1 + 0x40cc on each value: the stage square root.
    /// Section 1 + 0xa078: Finalhazard stage setup.
    StageSetup(Box<StageSetupFrame>),
    /// Section 1 + 0xa87c (character 9) / 0xab04 (character 10): super-form
    /// player construction.
    SuperConstruct(Box<SuperConstructFrame>),
    /// Section 1 + 0xa2a0: stage teardown.
    StageTeardown(Box<StageTeardownFrame>),
    /// Section 1 + 0xa390: stage object set-up (handlers 0xa3f4, 0xa560,
    /// 0xa758).
    StageObject(Box<StageObjectFrame>),
    /// Section 1 + 0x414: boss object construction.
    BossConstruct(Box<BossConstructFrame>),
    /// Section 1 + 0x1bb8: boss task update (dispatch on the entity action).
    BossUpdate(Box<BossUpdateFrame>),
    /// Section 1 + 0x9d4..0xe2c: the boss state helpers.
    BossHelper(Box<BossHelperFrame>),
    /// Section 1 + 0xe2c: boss action 0.
    BossActionZero(Box<BossActionZeroFrame>),
    /// Section 1 + 0xff4: boss action 2 (wait, then the next pattern entry).
    BossActionTwo(Box<BossActionTwoFrame>),
    /// Section 1 + 0x10ac: boss action 3.
    BossActionThree(Box<BossActionThreeFrame>),
    /// Section 1 + 0x12e8: boss actions 5, 6 and 7 (attacks 0x62a0).
    BossAttack(Box<BossAttackFrame>),
    /// Section 1 + 0x1440: boss actions 8, 9 and 10 (beam 0x3e58).
    BossBeam(Box<BossBeamFrame>),
    /// Section 1 + 0x1640: boss actions 11 and 12 (beam with attack).
    BossCombo(Box<BossComboFrame>),
    /// Section 1 + 0x1844: boss action 13 (defeat).
    BossDefeat(Box<BossDefeatFrame>),
    /// Section 1 + 0x638: boss task deleter.
    BossDelete(Box<BossDeleteFrame>),
    /// Section 1 + 0x690: point pushed out along a direction.
    BossAim(Box<BossAimFrame>),
    /// Section 1 + 0x744: boss hit check.
    BossHit(Box<BossHitFrame>),
    /// Section 1 + 0x920: boss attachment points.
    BossPoints(Box<BossPointsFrame>),
    /// Section 1 + 0xaf0: boss voice roll.
    BossVoice(Box<BossVoiceFrame>),
    /// Section 1 + 0x1e04, 0x1ed8, 0x1f94, 0x1fc8, 0x2078 and 0x210c: the boss
    /// draw handler and its small engine wrappers.
    BossSupport(Box<BossSupportFrame>),
    /// Section 1 + 0x2138, 0x2218 and 0x2300: boss texture scroll draws.
    BossScroll(Box<BossScrollFrame>),
    /// Section 1 + 0x8328..0x8b0c: the boss core (child) object.
    BossCore(Box<BossCoreFrame>),
    /// Section 1 + 0x7154, 0x71e0 and 0x72e0: core hit compare, knockback and
    /// proximity voice.
    CoreContact(Box<CoreContactFrame>),
    /// Section 1 + 0x743c, 0x7548 and 0x7684: core slot roll, attachment
    /// placement and hit reaction.
    CoreHit(Box<CoreHitFrame>),
    /// Section 1 + 0x783c: core action 1 (grow in).
    CoreGrow(Box<CoreGrowFrame>),
    /// Section 1 + 0x79dc: core action 2 (pulse).
    CorePulse(Box<CorePulseFrame>),
    /// Section 1 + 0x7d6c: core action 3 (shrink and switch stage).
    CoreShrink(Box<CoreShrinkFrame>),
    /// Section 1 + 0x7f58: core action 5 (stunned).
    CoreStun(Box<CoreStunFrame>),
    /// Section 1 + 0x83dc, 0x88c0 and 0x89e8: core draw, drifting spark
    /// update and spark draw.
    CoreDraw(Box<CoreDrawFrame>),
    /// Section 1 + 0x62a0..0x6504: the boss attack object interface.
    AttackObject(Box<AttackFrame>),
    /// Section 1 + 0x5f34..0x629c: the attack object update, its actions
    /// and its deleter.
    AttackUpdate(Box<AttackUpdateFrame>),
    /// Section 1 + 0x5808..0x59ac: attack piece helpers.
    AttackPiece(Box<AttackPieceFrame>),
    /// Section 1 + 0x59b0: attack formation spawn.
    AttackFormation(Box<AttackFormationFrame>),
    /// Section 1 + 0x5c04: attack hold timer.
    AttackHold(Box<AttackHoldFrame>),
    /// Section 1 + 0x5c4c: attack volley release.
    AttackVolley(Box<AttackVolleyFrame>),
    /// Section 1 + 0x5264..0x5804: the attack piece object.
    Piece(Box<PieceFrame>),
    /// Section 1 + 0x4fa8, 0x50a4 and 0x5130: piece burst, explosion and
    /// contact.
    PieceAction(Box<PieceActionFrame>),
    /// Section 1 + 0x4524 and 0x46e0: piece actions 0 (fly out) and the
    /// wobble 0x46e0 runs in action 1.
    PieceMotion(Box<PieceMotionFrame>),
    /// Section 1 + 0x4c20: piece action 3 (homing shot).
    PieceHoming(Box<PieceHomingFrame>),
    /// Section 1 + 0x4888: piece action 2 (aim and launch).
    PieceLaunch(Box<PieceLaunchFrame>),
    /// Section 1 + 0x6d64..0x7154: spark spawn, texture load and release,
    /// vector scaling, reseed and aim angles.
    BossUtility(Box<BossUtilityFrame>),
    /// Section 1 + 0x6a54, 0x6b48 and 0x6c28: the glow spark object.
    Glow(Box<GlowFrame>),
    /// Section 1 + 0x6508..0x6a50: the trail object, the glow chain update
    /// and the glow draw.
    Trail(Box<TrailFrame>),
    StageSqrt {
        values: Vec<f32>,
    },
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
        line: i8,
        /// Work byte 4 (switch reason), byte 5 (switch count), short 0xa
        /// (ring clock) and short 0xc (ring pool).
        reason: i8,
        switch_count: i8,
        ring_clock: i16,
        pool: i16,
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
/// frsp. Nonpositive and NaN inputs return unchanged.
fn stage_sqrt(x: f32) -> f32 {
    if x.is_nan() || x <= 0.0 {
        return x;
    }
    let x = x as f64;
    // Gekko estimate from the shared local tables (msl_math), else exact.
    let mut y = crate::msl_math::gekko_frsqrte(x);
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
    pub global_42: i8,
    pub global_43: i8,
    pub task_0c: u32,
    /// State 0 only: module word section 6 + 0x664, the result chain of the
    /// local 0x9f4 call (None when it or its word 0x34 is null; else the
    /// halfword at +4 of that object), words 0x176/0x178 of both
    /// 0x801e7328 entries, and the two 0x80022bc4 query results.
    pub global_664: u32,
    #[serde(default)]
    pub boss_flags: Option<i16>,
    pub actions: [[i16; 2]; 2],
    pub sound_queries: [i32; 2],
    /// State 0: results of the engine ring-count reads (0x80062a24) in call
    /// order. The ring-add calls are reported; their effect is engine-side.
    #[serde(default)]
    pub ring_reads: Vec<i32>,
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
    pub game_state: i32,
    pub poll: i32,
    /// Byte 1 of each 0x801e7328 entry and word 0xc of each 0x801e7368 entry.
    pub characters: [i8; 2],
    pub yaws: [i32; 2],
    pub camera_mode: i32,
    #[serde(default)]
    pub second_lookup: Option<i32>,
    /// Result of the local section 1 + 0xeb38 call after the state 3 camera.
    pub cue_result: i32,
    /// Word 0x7c of the engine global at 0x801cbd88.
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
                        // Engine ring services: each count read (0x80062a24) is
                        // taken from the request in call order; the add call
                        // (0x80063584) is reported, its effect stays engine-side.
                        let mut reads = s.ring_reads.iter().copied();
                        let get = |ev: &mut Vec<serde_json::Value>,
                                   reads: &mut dyn Iterator<Item = i32>,
                                   site: u32,
                                   p: usize|
                         -> Result<i32> {
                            ev.push(call(site, &[p as u32])?);
                            reads.next().ok_or_else(|| {
                                Error::invalid(format!("frame {n} needs more ring_reads"))
                            })
                        };
                        let add = |ev: &mut Vec<serde_json::Value>,
                                   site: u32,
                                   p: usize,
                                   d: i32|
                         -> Result<()> {
                            ev.push(call(site, &[p as u32, d as u32])?);
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
                                        let r = get(&mut ev, &mut reads, g, p)?;
                                        add(&mut ev, a, p, r.wrapping_neg())?;
                                    }
                                    pool = 0;
                                    ev.push(store(0xfc8c, "task", None, 0xc, 2, 0));
                                } else {
                                    add(&mut ev, 0xfc9c, me, -1)?;
                                    let held = i32::from(pool)
                                        .wrapping_sub(get(&mut ev, &mut reads, 0xfca4, me)?);
                                    let r = get(&mut ev, &mut reads, 0xfcb4, other)?;
                                    add(&mut ev, 0xfcc0, other, held.wrapping_sub(r))?;
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
                                if get(&mut ev, &mut reads, 0xfcc8, me)? as i16 == 15 {
                                    let cue = if player == 0 { 0x869 } else { 0x867 };
                                    warn(&mut ev, &mut countdown, (0xfd04, 0xfd18, 0xfd20), cue)?;
                                } else if get(&mut ev, &mut reads, 0xfd2c, me)? as i16 == 4 {
                                    let cue = if player == 0 { 0x86a } else { 0x868 };
                                    warn(&mut ev, &mut countdown, (0xfd68, 0xfd7c, 0xfd84), cue)?;
                                } else if get(&mut ev, &mut reads, 0xfd90, me)? as i16 == 0 {
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
                                    // Ring counts are only read for switch reason 1,
                                    // which blocks this trigger.
                                    [0, 0],
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
                                    get(&mut inner, &mut reads, g, p)?;
                                    add(&mut inner, a, p, d)?;
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
                    "pool":pool,"visits":visits}));
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
        PlacementClock::SuperFreeMove(frame) => return super_free_move(&rel, &frame),
        PlacementClock::SuperControl(frame) => return super_control(&rel, &frame),
        PlacementClock::SuperChargeDraw(frame) => return super_charge_draw(&rel, &frame),
        PlacementClock::SuperDraw(frame) => return super_draw(&rel, &frame),
        PlacementClock::SuperEffects(frame) => return super_effects(&rel, &frame),
        PlacementClock::RingDrop(frame) => return ring_drop(&rel, &frame),
        PlacementClock::StageSetup(frame) => return stage_setup(&rel, &frame),
        PlacementClock::SuperConstruct(frame) => return super_construct(&rel, &frame),
        PlacementClock::StageTeardown(frame) => return stage_teardown(&rel, &frame),
        PlacementClock::StageObject(frame) => return stage_object(&rel, &frame),
        PlacementClock::BossConstruct(frame) => return boss_construct(&rel, &frame),
        PlacementClock::BossUpdate(frame) => return boss_update(&rel, &frame),
        PlacementClock::BossHelper(frame) => return boss_helper(&rel, &frame),
        PlacementClock::BossActionZero(frame) => return boss_action_zero(&rel, &frame),
        PlacementClock::BossActionTwo(frame) => return boss_action_two(&rel, &frame),
        PlacementClock::BossActionThree(frame) => return boss_action_three(&rel, &frame),
        PlacementClock::BossAttack(frame) => return boss_attack(&rel, &frame),
        PlacementClock::BossBeam(frame) => return boss_beam(&rel, &frame),
        PlacementClock::BossCombo(frame) => return boss_combo(&rel, &frame),
        PlacementClock::BossDefeat(frame) => return boss_defeat(&rel, &frame),
        PlacementClock::BossDelete(frame) => return boss_delete(&rel, &frame),
        PlacementClock::BossAim(frame) => return boss_aim(&rel, &frame),
        PlacementClock::BossHit(frame) => return boss_hit(&rel, &frame),
        PlacementClock::BossPoints(frame) => return boss_points(&rel, &frame),
        PlacementClock::BossVoice(frame) => return boss_voice(&rel, &frame),
        PlacementClock::BossSupport(frame) => return boss_support(&rel, &frame),
        PlacementClock::BossScroll(frame) => return boss_scroll(&rel, &frame),
        PlacementClock::BossCore(frame) => return boss_core(&rel, &frame),
        PlacementClock::CoreContact(frame) => return core_contact(&rel, &frame),
        PlacementClock::CoreHit(frame) => return core_hit(&rel, &frame),
        PlacementClock::CoreGrow(frame) => return core_grow(&rel, &frame),
        PlacementClock::CorePulse(frame) => return core_pulse(&rel, &frame),
        PlacementClock::CoreShrink(frame) => return core_shrink(&rel, &frame),
        PlacementClock::CoreStun(frame) => return core_stun(&rel, &frame),
        PlacementClock::CoreDraw(frame) => return core_draw(&rel, &frame),
        PlacementClock::AttackObject(frame) => return attack_object(&rel, &frame),
        PlacementClock::AttackUpdate(frame) => return attack_update(&rel, &frame),
        PlacementClock::AttackPiece(frame) => return attack_piece(&rel, &frame),
        PlacementClock::AttackFormation(frame) => return attack_formation(&rel, &frame),
        PlacementClock::AttackHold(frame) => return attack_hold(&rel, &frame),
        PlacementClock::AttackVolley(frame) => return attack_volley(&rel, &frame),
        PlacementClock::Piece(frame) => return piece(&rel, &frame),
        PlacementClock::PieceAction(frame) => return piece_action(&rel, &frame),
        PlacementClock::PieceMotion(frame) => return piece_motion(&rel, &frame),
        PlacementClock::PieceHoming(frame) => return piece_homing(&rel, &frame),
        PlacementClock::PieceLaunch(frame) => return piece_launch(&rel, &frame),
        PlacementClock::BossUtility(frame) => return boss_utility(&rel, &frame),
        PlacementClock::Glow(frame) => return glow(&rel, &frame),
        PlacementClock::Trail(frame) => return trail(&rel, &frame),
        PlacementClock::StageSqrt { values } => {
            let out: Vec<u32> = values.iter().map(|&v| stage_sqrt(v).to_bits()).collect();
            return Ok(serde_json::json!({"bits":out}));
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
    pub approach: f32,
    #[serde(default)]
    pub randoms: Vec<i32>,
    #[serde(default)]
    pub trig: Vec<f32>,
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
    pub v70_length: f32,
    pub rotated: [f32; 3],
    pub dots: [f32; 2],
    pub cross: [f32; 6],
    pub turn: i32,
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

/// Inputs to section 1 + 0xdc90. The controller block is the player's
/// 52-byte entry at 0x801e5038 (words 8 and 0x10, stick halfwords 0x1c and
/// 0x1e); `atan2` is the double the engine's 0x80110e20 returns, `camera_yaw`
/// is word 0x10 of the object at 0x80174f98, and `trig` holds the two
/// 0x80112068/0x80112050 results.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperFreeMoveFrame {
    pub buttons_08: u32,
    pub buttons_10: u32,
    pub stick: [i16; 2],
    pub position: [f32; 3],
    pub atan2: f64,
    pub camera_yaw: i32,
    pub trig: [f32; 2],
}
/// Section 1 + 0xdc90: outside the ±14 stick dead zone, move 5 units per
/// frame. With button bit 0x400 held the stick's Y sign moves the player
/// down or up; otherwise the stick direction, relative to the camera yaw,
/// moves it across X and Z.
fn super_free_move(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperFreeMoveFrame,
) -> Result<serde_json::Value> {
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let store = |site: u32, offset: u32, value: f32| serde_json::json!({"site":site,"store":"entity","offset":offset,"size":4,"value":value.to_bits()});
    let call = |site: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?}))
    };
    let mut ev = Vec::new();
    if f.buttons_10 & 2 != 0 {
        ev.push(call(0xdcdc)?);
    }
    let float = |v: i16| v as i32 as f64 as f32;
    let (x, y) = (float(f.stick[0]), float(f.stick[1]));
    let mut pos = f.position;
    let vertical = f.buttons_08 & 0x400 != 0;
    let (hi, lo) = if vertical {
        (k(0xdd72)?, k(0xdd8a)?)
    } else {
        (k(0xde6a)?, k(0xde82)?)
    };
    let idle = x <= hi && x >= lo && y <= hi && y >= lo;
    if !idle && vertical {
        let zero = k(0xddba)?;
        let sign = if zero == y {
            zero
        } else {
            y / stage_sqrt(y * y)
        };
        pos[1] -= k(0xdde6)? * sign;
        ev.push(store(0xddf8, 0x18, pos[1]));
    } else if !idle {
        ev.push({
            let mut v = call(0xdeb0)?;
            v["floats"] = serde_json::json!([y, x]);
            v
        });
        let scale = {
            let p = rel
                .reference(1, 0xdeb6)
                .ok_or_else(|| Error::invalid("missing angle scale"))?;
            let at = p.addend as usize;
            f64::from_be_bytes(rel.data(4)?[at..at + 8].try_into().unwrap())
        };
        let angle = ((scale * f.atan2) as i32).wrapping_sub(f.camera_yaw);
        let step = k(0xdeee)?;
        let mut v = call(0xdee8)?;
        v["args"] = serde_json::json!([angle as u32]);
        ev.push(v);
        pos[0] += step * f.trig[0];
        ev.push(store(0xdf04, 0x14, pos[0]));
        let mut v = call(0xdf0c)?;
        v["args"] = serde_json::json!([angle as u32]);
        ev.push(v);
        pos[2] += step * f.trig[1];
        ev.push(store(0xdf20, 0x1c, pos[2]));
    }
    Ok(serde_json::json!({"events":ev,"position":pos}))
}

/// Boss object the charge check (0xe200) reads through the local lookup
/// 0x9f4: halfword 4 of its word 0x34 object and that object's position.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperBoss {
    pub flags: i16,
    pub position: [f32; 3],
}
/// Inputs to section 1 + 0xbad0 and the local checks it calls (0xdf44,
/// 0xe120, 0xe200). `hit_timer_after` is entity halfword 6 after the engine
/// hit check 0x800554cc; `held` and `landed` are the results of 0x8004e2f8
/// and 0x8004e228; `buttons` are the player's bytes in the engine tables
/// 0x803ad498, 0x803ad494, 0x803ad488 and 0x803ad484. For the charge check,
/// `charge` holds the engine's in-place vector results (difference,
/// length, normalized, transformed) and the transform call's return value.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperControlFrame {
    pub entity: u32,
    pub data2: u32,
    pub player: u32,
    pub action: i8,
    /// Entity byte 1, halfwords 4 and 6, position 0x14..0x1c.
    pub action_previous: i8,
    pub entity_flags: i16,
    pub hit_timer: u16,
    pub position: [f32; 3],
    /// Player bytes 0 (slot) and 1 (character), halfwords 0x10, 0x176,
    /// 0x178, 0x180, 0x35e, word 0x28, floats 0x64, 0x68 and 0xe8.
    pub slot: i8,
    pub character: i8,
    pub flags_10: i16,
    pub anim_176: i16,
    pub anim_178: i16,
    pub flags_180: i16,
    pub timer_35e: i16,
    pub word_28: i32,
    pub speed: f32,
    pub f68: f32,
    pub fe8: f32,
    /// Data2 word 0x1c and controller word 0x10.
    pub data2_1c: u32,
    pub pad_10: u32,
    pub hit_timer_after: u16,
    pub held: i32,
    pub landed: i32,
    pub buttons: [i8; 4],
    #[serde(default)]
    pub boss: Option<SuperBoss>,
    #[serde(default)]
    pub charge: Option<SuperCharge>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperCharge {
    pub difference: [f32; 3],
    pub length: f32,
    pub normalized: [f32; 3],
    pub transformed: [f32; 3],
    pub result: i32,
}
struct SuperCtl<'a, 'r> {
    rel: &'a crate::city_rel::CityRel<'r>,
    f: &'a SuperControlFrame,
    ev: Vec<serde_json::Value>,
    action: i8,
    flags: i16,
    anim: i16,
    flags_10: i16,
    f68: f32,
    speed: f32,
    timer: i16,
    hit_timer: u16,
}
impl SuperCtl<'_, '_> {
    fn st(&mut self, site: u32, region: &str, offset: u32, size: u32, value: u64) {
        let mask = if size == 4 {
            u32::MAX as u64
        } else {
            (1u64 << (8 * size)) - 1
        };
        self.ev.push(
            serde_json::json!({"site":site,"store":region,"offset":offset,
            "size":size,"value":value & mask}),
        );
    }
    fn action(&mut self, site: u32, v: i8) {
        self.action = v;
        self.st(site, "entity", 0, 1, v as u8 as u64);
    }
    fn anim(&mut self, site: u32, v: i16) {
        self.anim = v;
        self.st(site, "player", 0x176, 2, v as u16 as u64);
    }
    fn flags(&mut self, site: u32, v: i16) {
        self.flags = v;
        self.st(site, "entity", 4, 2, v as u16 as u64);
    }
    fn call(&mut self, site: u32, args: serde_json::Value) -> Result<()> {
        let target = engine_import(self.rel, site)?;
        self.ev
            .push(serde_json::json!({"site":site,"call":target,"args":args}));
        Ok(())
    }
    fn local(&mut self, site: u32, target: u32, args: serde_json::Value) {
        self.ev
            .push(serde_json::json!({"site":site,"local":target,"args":args}));
    }
    fn four(&self) -> serde_json::Value {
        serde_json::json!([self.f.entity, self.f.data2, self.f.player, self.f.player])
    }
    /// Section 1 + 0xdf44: leave an action when the entity's 0x1000 flag is
    /// set, by the previous action (9, 15, 33, 36, 37).
    fn interrupted(&mut self, site: u32) -> Result<bool> {
        let args = self.four();
        self.local(site, 0xdf44, args);
        if self.flags & 0x1000 == 0 {
            return Ok(false);
        }
        let f = self.f;
        let pose = f.word_28 as i16;
        match f.action_previous {
            9 => {
                self.action(0xdfa8, 18);
                self.anim(0xdfb4, pose);
                self.call(0xdfc0, serde_json::json!([f.data2, f.player]))?;
                self.flags(0xdfd0, self.flags & -0x2501);
            }
            15 => {
                if f.word_28 != 0 {
                    self.action(0xdfe8, 59);
                    self.anim(0xdff4, pose);
                    self.st(0xdffc, "player", 0x18, 2, 0);
                } else {
                    self.action(0xe008, 59);
                    self.st(0xe010, "player", 0x18, 2, 0);
                    self.anim(0xe018, 87);
                }
                self.flags(0xe028, self.flags & -0x2501);
            }
            33 => {
                self.action(0xe034, 53);
                self.anim(0xe040, pose);
                self.flags(0xe050, self.flags & -0x2501);
            }
            36 => {
                self.flags(0xe060, self.flags & !0x3c);
                self.flags_10 &= 0x3ffc;
                self.st(0xe074, "player", 0x10, 2, self.flags_10 as u16 as u64);
                self.action(0xe07c, 59);
                self.st(0xe084, "player", 0x18, 2, 0);
                self.anim(0xe08c, 87);
                self.flags(0xe09c, self.flags & -0x2501);
            }
            37 => {
                self.action(0xe0a8, 57);
                self.anim(0xe0b0, 59);
                self.flags(0xe0c0, self.flags & -0x2501);
                match f.character {
                    9 => self.call(0xe0d8, serde_json::json!([0x61f]))?,
                    10 => self.call(0xe0ec, serde_json::json!([0x61c]))?,
                    _ => {}
                }
                self.timer = 0;
                self.st(0xe0f4, "player", 0x35e, 2, 0);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    /// Section 1 + 0xe120: with the stick released and zero speed, settle
    /// into flight with pose 87.
    fn settled(&mut self, site: u32) -> Result<bool> {
        let f = self.f;
        self.local(site, 0xe120, serde_json::json!([f.entity, f.player]));
        self.call(0xe14c, serde_json::json!([f.entity, f.player, 0, 0]))?;
        if f.held == 0 && stage_f32(self.rel, 0xe15a, 4, 0)? == self.speed {
            self.action(0xe174, 59);
            self.st(0xe17c, "player", 0x18, 2, 0);
            self.anim(0xe184, 87);
            return Ok(true);
        }
        Ok(false)
    }
    /// Section 1 + 0xe200: within range of a boss that is not shielded,
    /// launch toward it (action 67, pose 89). Returns the engine transform
    /// call's result, as the native code does.
    fn charged(&mut self, site: u32) -> Result<bool> {
        let f = self.f;
        let args = self.four();
        self.local(site, 0xe200, args);
        self.local(0xe224, 0x9f4, serde_json::json!([]));
        let Some(boss) = &f.boss else {
            return Ok(false);
        };
        if boss.flags & 4 != 0 {
            return Ok(false);
        }
        let c = f
            .charge
            .as_ref()
            .ok_or_else(|| Error::invalid("charge check needs the engine vector results"))?;
        self.ev.push(
            serde_json::json!({"site":0xe27c,"call":engine_import(self.rel, 0xe27c)?,
            "vectors":{"3":boss.position,"4":f.position}}),
        );
        self.ev.push(
            serde_json::json!({"site":0xe284,"call":engine_import(self.rel, 0xe284)?,
            "vectors":{"3":c.difference}}),
        );
        if c.length > stage_f32(self.rel, 0xe28a, 4, 0)? {
            return Ok(false);
        }
        self.ev.push(
            serde_json::json!({"site":0xe2a8,"call":engine_import(self.rel, 0xe2a8)?,
            "vectors":{"3":c.difference}}),
        );
        self.ev.push(
            serde_json::json!({"site":0xe2c0,"call":engine_import(self.rel, 0xe2c0)?,
            "args":[f.slot as i32 as u32],"floats":c.normalized}),
        );
        let scale = stage_f32(self.rel, 0xe2ca, 4, 0)?;
        let v = c.normalized.map(|x| x * scale);
        for (i, site) in [0xe304u32, 0xe308, 0xe310].into_iter().enumerate() {
            self.st(site, "data2", 4 * i as u32, 4, v[i].to_bits().into());
        }
        self.ev.push(
            serde_json::json!({"site":0xe31c,"call":engine_import(self.rel, 0xe31c)?,
            "args":[f.entity],"vectors":{"4":v}}),
        );
        let t = c.transformed;
        for (i, site) in [0xe328u32, 0xe32c, 0xe334].into_iter().enumerate() {
            self.st(
                site,
                "player",
                0x64 + 4 * i as u32,
                4,
                t[i].to_bits().into(),
            );
        }
        self.speed = t[0];
        self.f68 = t[1];
        self.action(0xe33c, 67);
        self.anim(0xe344, 89);
        self.flags(0xe350, self.flags | 0x400);
        Ok(c.result != 0)
    }
    fn boost_sound(&mut self, sites: (u32, u32), first: u32, pair: (u32, u32)) -> Result<()> {
        let e = self.f.entity;
        self.call(sites.0, serde_json::json!([4, 0, 0, 0x4c, e + 0x14]))?;
        self.call(sites.1, serde_json::json!([first, pair.0, pair.1, 0]))
    }
}
/// Section 1 + 0xbad0: hit reaction and the action transitions between
/// flight (59), steering (60), the four boosts (61-64), the boss charge
/// (67), the hit stun (15), and actions 12, 18, 53, 54, 58 and 66,
/// including the local checks 0xdf44, 0xe120 and 0xe200.
fn super_control(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperControlFrame,
) -> Result<serde_json::Value> {
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let mut c = SuperCtl {
        rel,
        f,
        ev: Vec::new(),
        action: f.action,
        flags: f.entity_flags,
        anim: f.anim_176,
        flags_10: f.flags_10,
        f68: f.f68,
        speed: f.speed,
        timer: f.timer_35e,
        hit_timer: f.hit_timer,
    };
    let (e, d2, p) = (f.entity, f.data2, f.player);
    let slot = f.slot as i32 as u32;
    // Hit handling.
    if p != 0 && c.flags_10 as u16 & 0x8000 != 0 {
        c.flags(0xbb1c, c.flags | -0x8000);
        c.hit_timer = 0;
        c.st(0xbb24, "entity", 6, 2, 0);
    } else if c.hit_timer == 0 {
        c.call(0xbb40, serde_json::json!([e, p]))?;
        c.hit_timer = f.hit_timer_after;
        if c.hit_timer != 0 {
            match f.character {
                9 => c.call(0xbb74, serde_json::json!([0x300b, 0, 0, 0x7f, e + 0x14]))?,
                10 => c.call(0xbb98, serde_json::json!([0x301d, 0, 0, 0x7f, e + 0x14]))?,
                _ => {}
            }
            c.flags(0xbba4, c.flags | -0x8000);
            c.flags(0xbbb4, c.flags & -0x1401);
            if c.flags_10 & 3 != 0 {
                c.flags_10 &= !3;
                c.st(0xbbcc, "player", 0x10, 2, c.flags_10 as u16 as u64);
            }
            c.action(0xbbd4, 15);
            c.anim(0xbbdc, 21);
            c.flags(0xbbe8, c.flags & !4);
            c.speed = -f.fe8;
            c.st(0xbbf4, "player", 0x64, 4, c.speed.to_bits().into());
        } else {
            c.flags(0xbc04, c.flags & 0x7fff);
        }
    } else {
        c.hit_timer -= 1;
        c.st(0xbc10, "entity", 6, 2, c.hit_timer.into());
        c.flags(0xbc1c, c.flags & !4);
    }
    let [b498, b494, b488, b484] = f.buttons;
    match c.action {
        0 | 59 => {
            if c.action == 0 {
                c.action(0xbc4c, 59);
            }
            if !c.interrupted(0xbc60)? {
                c.call(0xbc98, serde_json::json!([e, p, 0, 0]))?;
                if f.held != 0 {
                    c.action(0xbca8, 60);
                    c.anim(0xbcb0, 88);
                } else if b498 != 0 {
                    c.action(0xbce0, 61);
                    c.anim(0xbce8, 90);
                    c.flags(0xbcf4, c.flags | 0x400);
                    if c.f68 < k(0xbcfe)? {
                        c.f68 = k(0xbcfe)?;
                        c.st(0xbd10, "player", 0x68, 4, c.f68.to_bits().into());
                    }
                    c.boost_sound((0xbd28, 0xbd3c), 0, (4, 0xf))?;
                } else if b494 != 0 {
                    c.action(0xbd64, 62);
                    c.anim(0xbd6c, 92);
                    c.flags(0xbd78, c.flags | 0x400);
                    if c.f68 > k(0xbd82)? {
                        c.f68 = k(0xbd82)?;
                        c.st(0xbd94, "player", 0x68, 4, c.f68.to_bits().into());
                    }
                    c.boost_sound((0xbdac, 0xbdc0), 0, (4, 0xf))?;
                }
            }
        }
        60 => {
            if !c.interrupted(0xbdd8)? && !c.settled(0xbdec)? {
                if (b488 != 0 || b484 != 0) && c.charged(0xbe48)? {
                    c.boost_sound((0xbe68, 0xbe80), slot, (3, 7))?;
                    c.timer = 0;
                    c.st(0xbe88, "player", 0x35e, 2, 0);
                } else if b498 != 0 {
                    c.action(0xbeb8, 63);
                    c.anim(0xbec0, 94);
                    c.flags(0xbecc, c.flags | 0x400);
                    if c.f68 < k(0xbed6)? {
                        c.f68 = k(0xbed6)?;
                        c.st(0xbee8, "player", 0x68, 4, c.f68.to_bits().into());
                    }
                    c.boost_sound((0xbf00, 0xbf18), slot, (4, 0xf))?;
                } else if b494 != 0 {
                    c.action(0xbf40, 64);
                    c.anim(0xbf48, 96);
                    c.flags(0xbf54, c.flags | 0x400);
                    if c.f68 > k(0xbf5e)? {
                        c.f68 = k(0xbf5e)?;
                        c.st(0xbf70, "player", 0x68, 4, c.f68.to_bits().into());
                    }
                    c.boost_sound((0xbf88, 0xbfa0), slot, (4, 0xf))?;
                }
            }
        }
        61 | 62 => {
            let s = if c.action == 61 {
                [0xbfb8, 0xbfd4, 0xbfe4, 0xbfec, 0xc01c, 0xc024, 0xc030]
            } else {
                [0xc048, 0xc064, 0xc074, 0xc07c, 0xc0ac, 0xc0b4, 0xc0c0]
            };
            let boost = c.action;
            if !c.interrupted(s[0])? {
                c.call(s[1], serde_json::json!([e, p, 0, 0]))?;
                let button = if boost == 61 { b498 } else { b494 };
                if f.held != 0 {
                    let (a, n) = if boost == 61 { (63, 94) } else { (64, 96) };
                    c.action(s[2], a);
                    c.anim(s[3], n);
                } else if button == 0 {
                    c.action(s[4], 59);
                    c.anim(s[5], 87);
                    c.flags(s[6], c.flags & !0x400);
                }
            }
        }
        63 | 64 => {
            let s = if c.action == 63 {
                [0xc0d8, 0xc0ec, 0xc100, 0xc130, 0xc138, 0xc144]
            } else {
                [0xc15c, 0xc170, 0xc184, 0xc1b4, 0xc1bc, 0xc1c8]
            };
            let button = if c.action == 63 { b498 } else { b494 };
            if !c.interrupted(s[0])? {
                if c.settled(s[1])? {
                    c.flags(s[2], c.flags & !0x400);
                } else if button == 0 {
                    c.action(s[3], 60);
                    c.local(s[4], 0xe1ac, serde_json::json!([p]));
                    c.flags(s[5], c.flags & !0x400);
                }
            }
        }
        67 => {
            if !c.interrupted(0xc1e0)? {
                if c.settled(0xc1f4)? {
                    c.flags(0xc208, c.flags & !0x400);
                } else {
                    let mut leave = true;
                    if (b498 != 0 || b494 != 0) && c.charged(0xc260)? {
                        let t = c.timer;
                        c.timer = c.timer.wrapping_add(1);
                        c.st(0xc274, "player", 0x35e, 2, c.timer as u16 as u64);
                        leave = t > 120;
                    }
                    if leave {
                        c.action(0xc284, 60);
                        c.local(0xc28c, 0xe1ac, serde_json::json!([p]));
                        c.flags(0xc298, c.flags & !0x400);
                        c.timer = 0;
                        c.st(0xc2a0, "player", 0x35e, 2, 0);
                    }
                }
            }
        }
        12 => {
            if !c.interrupted(0xc2b8)? {
                let mut landed = false;
                if c.speed <= k(0xc2ca)? {
                    c.call(0xc2e8, serde_json::json!([e, p]))?;
                    landed = f.landed != 0;
                }
                if landed {
                    c.action(0xc2f8, 1);
                    c.anim(0xc300, 27);
                    c.call(0xc310, serde_json::json!([e, d2, p]))?;
                    c.st(0xc318, "entity", 0xc, 4, f.data2_1c.into());
                } else if c.anim != 18 {
                    c.action(0xc330, 59);
                    c.st(0xc338, "player", 0x18, 2, 0);
                    c.anim(0xc340, 87);
                }
            }
        }
        15 => {
            if !c.interrupted(0xc358)? {
                if c.anim == 21 || c.anim == 22 {
                    if c.hit_timer == 0 {
                        c.action(0xc3b0, 59);
                        c.anim(0xc3b8, 99);
                    }
                } else if c.settled(0xc380)? {
                    c.action(0xc390, 59);
                    c.anim(0xc398, 99);
                }
            }
        }
        18 => {
            c.interrupted(0xc3d0)?;
        }
        53 | 66 => {
            let s = if c.action == 53 {
                [0xc3e8, 0xc408, 0xc410, 0xc418]
            } else {
                [0xc430, 0xc450, 0xc458, 0xc460]
            };
            if !c.interrupted(s[0])? && f.flags_180 & 1 != 0 {
                c.action(s[1], 59);
                c.st(s[2], "player", 0x18, 2, 0);
                c.anim(s[3], 87);
            }
        }
        54 if f.action_previous == 15 => {
            c.action(0xc47c, 59);
            c.st(0xc484, "player", 0x18, 2, 0);
            c.anim(0xc48c, 87);
            c.flags(0xc49c, c.flags & -0x2501);
        }
        58 if f.pad_10 & 4 != 0 => {
            c.action(0xc4d0, 59);
            c.anim(0xc4d8, 87);
        }
        _ => {}
    }
    Ok(
        serde_json::json!({"events":c.ev,"action":c.action,"entity_flags":c.flags,
        "anim_176":c.anim,"f68":c.f68,"speed":c.speed,"timer_35e":c.timer,
        "hit_timer":c.hit_timer,"flags_10":c.flags_10,
        "scope":"0xbad0 with its local checks; engine queries and vector services are request inputs."}),
    )
}

/// Inputs to section 1 + 0xc67c. `pose` is the player's animation table
/// entry for halfword 0x178 (its halfwords 0 and 2) and `base_pose` is
/// halfword 0 of the entry that halfword 0 names. `motion` and `model` are
/// word 4 of the selected 8-byte entries of the engine tables 0x801e6288
/// and 0x801e5928.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperChargeDrawFrame {
    pub entity: u32,
    /// Engine global 0x801cbd88 word 0x7c.
    pub frame: u32,
    pub rotation: [i32; 3],
    pub slot: i8,
    pub pose: [i16; 2],
    pub base_pose: i16,
    pub motion: u32,
    pub model: u32,
    /// Player word 0x370 and float 0x184.
    pub word_370: u32,
    pub f184: f32,
}
/// Section 1 + 0xc67c: on even frames, draws the player's charge effect at
/// its position and rotation (Y as 0x8000 minus the entity's yaw). Returns
/// the table slots it selects and the draw call's arguments.
fn super_charge_draw(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperChargeDrawFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let mut ev = Vec::new();
    if f.frame & 1 == 0 {
        let stack = rel
            .reference(1, 0xc6de)
            .filter(|r| r.module == 0 && r.kind == 6)
            .ok_or_else(|| Error::invalid("missing matrix stack reference"))?
            .addend;
        ev.push(call(0xc6e4, serde_json::json!([stack]))?);
        ev.push(call(0xc6ec, serde_json::json!([f.entity + 0x14]))?);
        ev.push(call(0xc6f8, serde_json::json!([0, f.rotation[2] as u32]))?);
        ev.push(call(0xc704, serde_json::json!([0, f.rotation[0] as u32]))?);
        ev.push(call(
            0xc71c,
            serde_json::json!([0, 0x8000i32.wrapping_sub(f.rotation[1]) as u32]),
        )?);
        let mut draw = call(
            0xc75c,
            serde_json::json!([f.motion, f.model, f.word_370, 0, f.slot as i32 as u32]),
        )?;
        draw["floats"] = serde_json::json!([f.f184]);
        ev.push(draw);
        ev.push(call(0xc760, serde_json::json!([]))?);
    }
    Ok(serde_json::json!({"events":ev,"motion_slot":f.pose[1],"model_slot":f.base_pose}))
}

/// Engine object word the draw routine swaps while drawing a player with
/// an effect block: `target` is word 0x2c of the 0x801e6288 entry at
/// 0xaec (character 9) or 0xbc4 (character 10); `texture` is word 4 of the
/// effect block's word 0x18 object; `saved` is the target's word 4 before.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperDrawSwap {
    pub target: u32,
    pub texture: u32,
    pub saved: u32,
}
/// Inputs to section 1 + 0xd2dc. Engine table words are given as the values
/// the routine selects: `idle_motion` and `pose_motion` (0x801e6288, by
/// the player's first and current pose entries), `pose_model` (0x801e5928,
/// by the current pose's base entry), `bc_model` (word 0 of the player's
/// word 0xbc object), `ring_scale` (float at 0x803ad544) and
/// `effect_mode` / `restore_mode` (bytes at 0x803ad524 + slot and
/// 0x803ad526).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperDrawFrame {
    pub entity: u32,
    pub active_player: i8,
    pub global_42: i8,
    pub slot: i8,
    pub character: i8,
    pub action: i8,
    pub effect_mode: i8,
    pub restore_mode: i8,
    pub rotation: [i32; 3],
    pub word_370: u32,
    pub word_bc: u32,
    pub mode_174: i16,
    pub word_19c: u32,
    pub f184: f32,
    pub swap: Option<SuperDrawSwap>,
    pub idle_motion: u32,
    pub pose_motion: u32,
    pub pose_model: u32,
    pub bc_model: u32,
    pub ring_scale: f32,
}
/// Section 1 + 0xd2dc: draws only the active super form (or both while
/// global byte 0x42 is 1). It sets the draw state, places the model at the
/// entity's position, rotation and scale, temporarily swaps in the effect
/// texture, and draws the current pose. Action 54 uses the 0xbc model
/// instead. Module and code addresses in arguments are written as
/// "88:SECTION+OFFSET".
fn super_draw(rel: &crate::city_rel::CityRel<'_>, f: &SuperDrawFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let mut ev = Vec::new();
    if f.active_player != f.slot && f.global_42 != 1 {
        return Ok(serde_json::json!({"events":ev,"drawn":false}));
    }
    let slot = f.slot as i32 as u32;
    ev.push(call(0xd33c, serde_json::json!([]))?);
    ev.push(call(0xd348, serde_json::json!([0, 0x200]))?);
    if f.effect_mode != 1 && f.effect_mode != 5 {
        ev.push(call(0xd37c, serde_json::json!([0x2400]))?);
    }
    ev.push(call(0xd388, serde_json::json!([slot]))?);
    ev.push(serde_json::json!({"site":0xd394,"store":"module","offset":"88:6+0x4b0","size":4,"value":"player"}));
    ev.push(call(0xd3a4, serde_json::json!(["88:6+0x4b4"]))?);
    ev.push(call(0xd3ac, serde_json::json!(["88:6+0x4b4"]))?);
    let callback = if f.character == 9 {
        (0xd3c8, "88:1+0xc77c")
    } else {
        (0xd3d8, "88:1+0xccf8")
    };
    ev.push(call(callback.0, serde_json::json!([callback.1]))?);
    ev.push(call(0xd3e4, serde_json::json!([f.word_370]))?);
    ev.push(call(0xd3e8, serde_json::json!([]))?);
    ev.push(call(0xd3f0, serde_json::json!([f.entity + 0x14]))?);
    ev.push(call(0xd3fc, serde_json::json!([0, f.rotation[2] as u32]))?);
    ev.push(call(0xd408, serde_json::json!([0, f.rotation[0] as u32]))?);
    ev.push(call(
        0xd420,
        serde_json::json!([0, 0x8000i32.wrapping_sub(f.rotation[1]) as u32]),
    )?);
    ev.push(call(0xd428, serde_json::json!([f.entity + 0x20]))?);
    let swap_sites = match f.character {
        9 => Some((0xd468, 0xd59c)),
        10 => Some((0xd498, 0xd5bc)),
        _ => None,
    };
    let swapped = match (&f.swap, swap_sites) {
        (Some(s), Some((store, restore))) => {
            ev.push(
                serde_json::json!({"site":store,"store":"engine","offset":s.target.wrapping_add(4),
                "size":4,"value":s.texture}),
            );
            Some((restore, s))
        }
        _ => None,
    };
    if f.action == 54 && f.word_bc != 0 {
        ev.push(call(0xd4c0, serde_json::json!([slot]))?);
        let mut draw = call(0xd4f4, serde_json::json!([f.idle_motion, f.bc_model]))?;
        draw["floats"] = serde_json::json!([f.ring_scale]);
        ev.push(draw);
        ev.push(call(0xd500, serde_json::json!([slot]))?);
    } else {
        let (site, model) = if f.mode_174 == 2 {
            (0xd56c, f.word_19c)
        } else {
            (0xd55c, f.pose_model)
        };
        let mut draw = call(site, serde_json::json!([f.pose_motion, model]))?;
        draw["floats"] = serde_json::json!([f.f184]);
        ev.push(draw);
    }
    if let Some((site, s)) = swapped {
        ev.push(
            serde_json::json!({"site":site,"store":"engine","offset":s.target.wrapping_add(4),
            "size":4,"value":s.saved}),
        );
    }
    ev.push(call(0xd5c0, serde_json::json!([]))?);
    ev.push(call(0xd5c8, serde_json::json!([0]))?);
    ev.push(call(0xd5d0, serde_json::json!([0x2400]))?);
    ev.push(call(0xd5d4, serde_json::json!([]))?);
    ev.push(call(
        0xd5e8,
        serde_json::json!([f.restore_mode as i32 as u32]),
    )?);
    Ok(serde_json::json!({"events":ev,"drawn":true}))
}

/// Inputs to section 1 + 0xd60c. `spin_models` are word 4 of the engine
/// 0x801e6288 entries at 0xbf4/0xc14/0xc04/0xc24 (odd frames) or
/// 0xbfc/0xc1c/0xc0c/0xc2c (even frames), in draw order. `aura_halfword`
/// is module halfword section 5 + 0x3c9b6 before the update.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperEffectsFrame {
    pub entity: u32,
    pub player: u32,
    pub active_player: i8,
    pub global_42: i8,
    pub global_43: i8,
    pub frame: u32,
    pub slot: i8,
    pub character: i8,
    pub pose: i16,
    pub counter_35c: i8,
    pub word_370: u32,
    pub entity_flags: i16,
    pub rotation: [i32; 3],
    pub aura_halfword: u16,
    pub spin_models: [u32; 4],
}
/// Section 1 + 0xd60c: effects drawn around the active super form. A spin
/// of four models for character 10 in poses 8-10; an aura (pose 100) whose
/// texture frame follows player byte 0x35c; after frame 14400 a fade that
/// brightens 40 module material colors; and the boost trail (entity flag
/// 0x400), turned by the boost pose. Module addresses are reported as
/// "88:SECTION+OFFSET"; material colors are read from the module data.
fn super_effects(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperEffectsFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let sym = |section: u8, offset: u32| format!("88:{section}+{offset:#x}");
    let mut ev = Vec::new();
    if f.active_player != f.slot && f.global_42 != 1 {
        return Ok(serde_json::json!({"events":ev}));
    }
    let (e, frame) = (f.entity, f.frame);
    let rot_y = 0x8000i32.wrapping_sub(f.rotation[1]) as u32;
    ev.push(call(0xd66c, serde_json::json!([f.word_370]))?);
    ev.push(call(0xd670, serde_json::json!([]))?);
    if f.character == 10 && (f.pose as u16).wrapping_sub(8) <= 2 {
        let angle = (frame & 31) << 11;
        ev.push(call(0xd6a8, serde_json::json!([angle]))?);
        ev.push(call(0xd6b8, serde_json::json!([angle]))?);
        ev.push(call(0xd6bc, serde_json::json!([]))?);
        for (i, (matrix, place, draw)) in [
            (0x5d4u32, 0xd6f8u32, 0xd700u32),
            (0x5a4, 0xd73c, 0xd744),
            (0x634, 0xd780, 0xd788),
            (0x604, 0xd7c4, 0xd7cc),
        ]
        .into_iter()
        .enumerate()
        {
            ev.push(call(place, serde_json::json!([0, sym(6, matrix)]))?);
            ev.push(call(draw, serde_json::json!([f.spin_models[i]]))?);
        }
        ev.push(call(0xd7d0, serde_json::json!([]))?);
    }
    ev.push(call(0xd7d8, serde_json::json!([e + 0x14]))?);
    ev.push(call(0xd7e4, serde_json::json!([0, f.rotation[2] as u32]))?);
    ev.push(call(0xd7f0, serde_json::json!([0, f.rotation[0] as u32]))?);
    ev.push(call(0xd808, serde_json::json!([0, rot_y]))?);
    ev.push(call(0xd810, serde_json::json!([e + 0x20]))?);
    let flags = |fl: &[f32]| serde_json::json!(fl);
    let mut aura = f.aura_halfword;
    let mut colored = Vec::new();
    if f.pose == 100 {
        ev.push(call(0xd828, serde_json::json!([sym(5, 0x3c9a0)]))?);
        let extra = if f.character == 10 { 6 } else { 0 };
        let low = (f.counter_35c as i32 + extra + 3) as u16;
        aura = (aura & 0xc000) | low;
        ev.push(
            serde_json::json!({"site":0xd870,"store":"module","offset":sym(5, 0x3c9b6),
            "size":2,"value":aura}),
        );
        ev.push(call(0xd874, serde_json::json!([]))?);
        let mut c = call(0xd898, serde_json::json!([0]))?;
        c["floats"] = flags(&[k(0xd87e)?, k(0xd88a)?, k(0xd87e)?]);
        ev.push(c);
        ev.push(call(0xd8a4, serde_json::json!([sym(5, 0x3d144)]))?);
        ev.push(call(0xd8a8, serde_json::json!([]))?);
    } else if f.entity_flags & 0x400 == 0 && f.global_43 != 1 {
        // Fade-in after frame 14400: (frame - 14400) * 255 / 18000 + 128.
        let x = (frame as i32).wrapping_sub(14400).wrapping_mul(255);
        let hi = ((0x7482296bi64 * x as i64) >> 32) as i32;
        let q = (hi >> 13) + ((hi >> 13) as u32 >> 31) as i32;
        let color = if q <= 0 {
            0u32
        } else if q + 0x80 > 255 {
            u32::MAX
        } else {
            let v = (q + 0x80) as u32;
            v | v << 8 | v << 16 | v << 24
        };
        if color != 0 {
            let source = rel.data(5)?;
            for i in 0..49u32 {
                let at = 0x3eb2c + 0x14 + 16 * i as usize;
                let w = u32::from_be_bytes(source[at..at + 4].try_into().unwrap());
                if w >> 24 == 0xff {
                    colored.push(i);
                    ev.push(serde_json::json!({"site":fade_site(i),"store":"module",
                        "offset":sym(5, 0x3d450 + 0x14 + 16 * i),"size":4,"value":color}));
                }
            }
            ev.push(call(0xda78, serde_json::json!([sym(5, 0x3c9a0)]))?);
            ev.push(call(
                0xda90,
                serde_json::json!([sym(5, 0x3d76c), sym(5, 0x3d9b0), frame]),
            )?);
            ev.push(call(0xda94, serde_json::json!([]))?);
            ev.push(call(0xda98, serde_json::json!([]))?);
            ev.push(call(0xdaa0, serde_json::json!([f.player + 0x218]))?);
            ev.push(call(0xdab0, serde_json::json!([0, 0xd800]))?);
            for (site, a, b) in [
                (0xdad4u32, 0xdabausize, 0xdac6usize),
                (0xdaf8, 0xdade, 0xdaea),
                (0xdb14, 0xdaba, 0xdb06),
            ] {
                let mut c = call(site, serde_json::json!([0]))?;
                c["floats"] = flags(&[k(a)?, k(b)?, k(a)?]);
                ev.push(c);
            }
            ev.push(call(0xdb20, serde_json::json!([sym(5, 0x3d784)]))?);
        }
    }
    ev.push(call(0xdb24, serde_json::json!([]))?);
    if f.entity_flags & 0x400 != 0 {
        ev.push(call(0xdb38, serde_json::json!([]))?);
        ev.push(call(0xdb44, serde_json::json!([sym(5, 0x3c9a0)]))?);
        ev.push(call(0xdb4c, serde_json::json!([f.player + 0x230]))?);
        ev.push(call(0xdb58, serde_json::json!([0, f.rotation[2] as u32]))?);
        ev.push(call(0xdb64, serde_json::json!([0, f.rotation[0] as u32]))?);
        ev.push(call(0xdb7c, serde_json::json!([0, rot_y]))?);
        ev.push(call(0xdb84, serde_json::json!([e + 0x20]))?);
        let trail = match f.character {
            9 => Some((0xdbbcu32, 0x3e07cu32, 0x3e25cu32)),
            10 => Some((0xdbf0, 0x3e928, 0x3eb08)),
            _ => None,
        };
        let (site, model, list) =
            trail.ok_or_else(|| Error::invalid("boost trail needs character 9 or 10"))?;
        let texture = rel
            .local_pointer(5, model + 4)?
            .ok_or_else(|| Error::invalid("missing trail texture pointer"))?;
        ev.push(call(
            site,
            serde_json::json!([sym(texture.0, texture.1), sym(5, list), frame]),
        )?);
        let turn = match f.pose {
            p if p >= 98 => None,
            p if p >= 96 => Some((0xdc68, 0x105b)),
            p if p >= 94 => Some((0xdc58, 0xf4a0)),
            p if p >= 92 => Some((0xdc44, 0x4000)),
            p if p >= 90 => Some((0xdc34, 0xc000)),
            _ => None,
        };
        if let Some((site, z)) = turn {
            ev.push(call(site, serde_json::json!([0, z]))?);
        }
        ev.push(call(0xdc70, serde_json::json!([sym(5, model)]))?);
        ev.push(call(0xdc74, serde_json::json!([]))?);
    }
    Ok(serde_json::json!({"events":ev,"aura_halfword":aura,"faded_materials":colored}))
}
/// Store site for material `i`: the fade loop at 0xd974 is unrolled seven
/// times and runs seven times, so sites repeat every seven materials.
fn fade_site(i: u32) -> u32 {
    [0xd988, 0xd9a8, 0xd9cc, 0xd9f0, 0xda14, 0xda38, 0xda5c][(i % 7) as usize]
}

/// Inputs to section 1 + 0x19470, which stage setup installs as the hit
/// ring-drop handler. `rings` is what the engine ring count 0x80062a24
/// returns, `character` is byte 1 of the slot's 0x801e7328 entry,
/// `position` is floats 0x14..0x1c of its 0x801e7368 entry, `random` is the
/// 0x8010d940 result, and `allocated` says, per dropped ring, whether the
/// engine task allocation 0x80071ba4 succeeded.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingDropFrame {
    pub slot: i8,
    pub rings: i32,
    pub character: i8,
    pub position: [f32; 3],
    pub random: i32,
    pub allocated: Vec<bool>,
}
/// Section 1 + 0x19470: a hit with no rings defeats the player (except
/// characters 6 and 7). Otherwise all rings are taken and up to 20 ring
/// objects (update 0x196f0) are spawned at the player, spread evenly
/// around a random start angle.
fn ring_drop(rel: &crate::city_rel::CityRel<'_>, f: &RingDropFrame) -> Result<serde_json::Value> {
    let k = |site: usize| stage_f32(rel, site, 4, 0);
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let slot = f.slot as i32 as u32;
    let mut ev = vec![call(0x1948c, serde_json::json!([]))?];
    let start = k(0x194c6)? * (k(0x194b6)? * (f.random as f64 as f32));
    ev.push(call(0x194d8, serde_json::json!([slot]))?);
    let count = f.rings as i16 as i32;
    let mut angles = Vec::new();
    if count == 0 {
        if f.character != 7 && f.character != 6 {
            ev.push(call(0x19528, serde_json::json!([slot]))?);
            ev.push(call(0x1952c, serde_json::json!([]))?);
        }
    } else {
        let n = count.min(20);
        ev.push(call(0x19538, serde_json::json!([slot]))?);
        ev.push(call(
            0x19544,
            serde_json::json!([slot, f.rings.wrapping_neg() as u32]),
        )?);
        for i in 0..n.max(0) {
            ev.push(call(
                0x19598,
                serde_json::json!([10, 2, "88:1+0x196f0", "88:5+0x4780c"]),
            )?);
            let ok =
                f.allocated.get(i as usize).copied().ok_or_else(|| {
                    Error::invalid("ring drop needs an allocation result per ring")
                })?;
            if !ok {
                continue;
            }
            for (j, site) in [0x195b4u32, 0x195b8, 0x195c0].into_iter().enumerate() {
                ev.push(
                    serde_json::json!({"site":site,"store":"ring","index":i,"offset":0x14 + 4 * j,
                    "size":4,"value":f.position[j].to_bits()}),
                );
            }
            let step = (i * 360) / n;
            // Degrees to the engine's 65536-unit circle (0x88c = 65536 / 360).
            let angle = (k(0x19576)? * (start + step as f64 as f32)) as f64 as i32;
            angles.push(angle);
            ev.push(
                serde_json::json!({"site":0x19600,"store":"ring","index":i,"offset":0xc,
                "size":4,"value":angle as u32}),
            );
        }
        ev.push(call(0x19628, serde_json::json!([0x8014, 0, 0, 0]))?);
    }
    Ok(serde_json::json!({"events":ev,"start_angle":start,"ring_angles":angles}))
}

/// Engine state section 1 + 0xa078 reads: the hit handler pointer at
/// 0x801a2638, the two player task pointers at 0x801e7388, and module
/// word section 6 + 0x484.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSetupFrame {
    pub hit_handler: u32,
    pub player_tasks: [u32; 2],
    pub module_484: u32,
}
/// Section 1 + 0xa078: Finalhazard stage setup. It sets up the engine
/// lights and shadows from module data, installs the hit ring drop
/// (0x19470) unless a handler is already installed, fills the engine
/// object hook table at 0x801e73d8, replaces both players with the two super
/// forms (0xa87c for slot 0, 0xab04 for slot 1), creates the boss and
/// stage objects, and starts the 600-frame timer object unless module word
/// 0x484 is set. Module and code addresses are reported as
/// "88:SECTION+OFFSET".
fn stage_setup(
    rel: &crate::city_rel::CityRel<'_>,
    f: &StageSetupFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, address: u32, value: serde_json::Value| serde_json::json!({"site":site,"store":"engine","address":address,"size":4,"value":value});
    let lights = |o: u32| format!("88:5+{:#x}", 0x38220 + o);
    let mut ev = vec![
        call(
            0xa098,
            serde_json::json!([0xff00_0000u32, 0xff00_0000u32, 0xff00_0000u32]),
        )?,
        call(0xa09c, serde_json::json!([]))?,
        call(0xa0a0, serde_json::json!([]))?,
    ];
    for (site, light, offset) in [
        (0xa0b4u32, 0u32, 0u32),
        (0xa0c4, 1, 0x20),
        (0xa0d0, 2, 0x20),
        (0xa0e0, 8, 0x40),
        (0xa0ec, 9, 0x40),
        (0xa0f8, 6, 0x60),
    ] {
        ev.push(call(site, serde_json::json!([light, lights(offset)]))?);
    }
    ev.extend([
        call(0xa104, serde_json::json!([0, 0]))?,
        call(0xa110, serde_json::json!([0, 1]))?,
        call(0xa118, serde_json::json!([2]))?,
        call(0xa120, serde_json::json!([2]))?,
        local(0xa12c, 0x10a20, serde_json::json!(["88:5+0x3c7ac"])),
    ]);
    if f.hit_handler == 0 {
        ev.push(st(0xa14c, 0x801a2638, serde_json::json!("88:1+0x19470")));
    }
    ev.extend([
        call(0xa160, serde_json::json!(["88:5+0x38200", "88:5+0x381b4"]))?,
        call(0xa174, serde_json::json!(["88:5+0x3c79c", "88:5+0x3c784"]))?,
        call(0xa17c, serde_json::json!([0]))?,
        call(0xa188, serde_json::json!(["88:5+0x3c7bc"]))?,
        st(0xa19c, 0x801e73d8, serde_json::json!(0x800dfe70u32)),
        st(0xa1a8, 0x801e73dc, serde_json::json!("88:1+0x12d14")),
        st(0xa1b4, 0x801e73e0, serde_json::json!("88:1+0x13808")),
        st(0xa1c0, 0x801e73e4, serde_json::json!("88:1+0x1385c")),
    ]);
    for (i, site) in (0xa1c8u32..=0xa1ecu32).step_by(4).enumerate() {
        ev.push(st(site, 0x801e73e8 + 4 * i as u32, serde_json::json!(0)));
    }
    for (slot, (free, make, ctor, after)) in [
        (0xa204u32, 0xa20cu32, 0xa87cu32, 0xa214u32),
        (0xa22c, 0xa234, 0xab04, 0xa23c),
    ]
    .into_iter()
    .enumerate()
    {
        if f.player_tasks[slot] != 0 {
            ev.push(call(free, serde_json::json!([f.player_tasks[slot]]))?);
        }
        ev.push(local(make, ctor, serde_json::json!([slot])));
        ev.push(call(after, serde_json::json!([slot]))?);
    }
    ev.extend([
        local(0xa240, 0x6e58, serde_json::json!([])),
        local(0xa24c, 0x14b28, serde_json::json!(["88:5+0x3c7c8"])),
        call(0xa264, serde_json::json!(["88:5+0x3c7d0", "88:4+0x490", 2]))?,
    ]);
    if f.module_484 == 0 {
        ev.push(local(0xa284, 0x9b3c, serde_json::json!([0, 600])));
    }
    Ok(serde_json::json!({"events":ev,
        "scope":"Call and store sequence of 0xa078; the engine setup routines and the local constructors it calls run elsewhere."}))
}

/// Engine results and state the super-form constructors read: the task
/// and player-block allocations, the task's entity (word 0x34) and its
/// collision inner object (entity word 0x2c, then word 0xc), player float
/// 0x130 after the engine set-up, the results of 0x800722e0, 0x8004ffa8,
/// 0x8004fe2c and 0x8002a958, the vector at 0x801e6288 + 0xa5c / 0xb34 (3
/// words, 8 apart), the 0x8002a958 argument (word 0x2c of the 0x801e6288
/// entry at 0xaec / 0xbc4), the entity position, global byte 0x801cbd88,
/// module word section 6 + 0x4ac and the timer task allocation.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperConstructFrame {
    pub slot: i8,
    pub character: i8,
    pub task: u32,
    pub player: u32,
    pub entity: u32,
    pub inner: u32,
    pub f130: f32,
    pub results: [u32; 3],
    pub table_vector: [u32; 3],
    pub fx_source: u32,
    pub fx: u32,
    pub position: [f32; 3],
    pub global_byte: i8,
    pub module_4ac: u32,
    pub timer_task: u32,
}
/// Sections 1 + 0xa87c and 0xab04: build a super-form player (character 9
/// or 10) in `slot`. The two constructors are the same code with their own
/// module data, engine table offsets and effect kind; sites are reported
/// for the constructor that runs.
fn super_construct(
    rel: &crate::city_rel::CityRel<'_>,
    f: &SuperConstructFrame,
) -> Result<serde_json::Value> {
    let delta = match f.character {
        9 => 0,
        10 => 0x288,
        _ => {
            return Err(Error::invalid(
                "super-form constructors exist for characters 9 and 10",
            ))
        }
    };
    let code = rel.data(1)?;
    let at = |site: u32| site + delta;
    let word = |site: u32| {
        u32::from_be_bytes(
            code[at(site) as usize..at(site) as usize + 4]
                .try_into()
                .unwrap(),
        )
    };
    let sym = |site: u32| -> Result<serde_json::Value> {
        let r = rel
            .reference(1, at(site))
            .ok_or_else(|| Error::invalid("missing constructor reference"))?;
        Ok(if r.module == 88 {
            serde_json::json!(format!("88:{}+{:#x}", r.section, r.addend))
        } else {
            serde_json::json!(r.addend)
        })
    };
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":at(site),"call":engine_import(rel, at(site))?,"args":args}))
    };
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":at(site),"store":region,"offset":offset,"size":size,"value":value})
    };
    let slot = f.slot as i32 as u32;
    let ch = f.character as u32;
    let mut ev = vec![call(
        0xa8ac,
        serde_json::json!([7, 1, sym(0xa89e)?, sym(0xa8a6)?]),
    )?];
    if f.task == 0 {
        return Ok(serde_json::json!({"events":ev,"built":false}));
    }
    ev.push(call(0xa8c4, serde_json::json!([1, 0x37c]))?);
    if f.player == 0 {
        ev.push(call(0xa8d8, serde_json::json!([f.task]))?);
        return Ok(serde_json::json!({"events":ev,"built":false}));
    }
    let n = |v: u32| serde_json::json!(v);
    ev.extend([
        st(0xa8e0, "task", n(0x40), 4, n(f.player)),
        st(0xa8ec, "player", n(0), 1, n(slot & 0xff)),
        st(0xa8f4, "player", n(1), 1, n(ch)),
        st(0xa8f8, "player", n(3), 1, n(ch)),
        st(0xa900, "entity", n(1), 1, n(0)),
        st(0xa908, "entity", n(0), 1, n(0x3b)),
        st(0xa90c, "player", n(0x18), 2, n(0)),
        call(0xa918, serde_json::json!([slot, f.task]))?,
        st(0xa924, "player", n(0x198), 4, sym(0xa91e)?),
        st(0xa928, "player", n(0x176), 2, n(0)),
        call(0xa940, serde_json::json!([f.task, sym(0xa932)?, 2, 0]))?,
        st(0xa950, "inner", n(0x14), 4, n(f.f130.to_bits())),
        call(0xa95c, serde_json::json!([sym(0xa956)?]))?,
        st(0xa960, "player", n(0x370), 4, n(f.results[0])),
        call(0xa96c, serde_json::json!([ch]))?,
        call(0xa978, serde_json::json!([sym(0xa972)?]))?,
        st(0xa97c, "player", n(0x374), 4, n(f.results[1])),
        call(0xa988, serde_json::json!([sym(0xa982)?]))?,
        st(0xa98c, "player", n(0x378), 4, n(f.results[2])),
        call(0xa994, serde_json::json!([f.player + 0x174]))?,
    ]);
    let vector = sym(0xa9a6)?;
    let base = vector.as_str().unwrap_or_default().to_string();
    let (head, off) = base.rsplit_once('+').unwrap_or(("", "0x0"));
    let off = u32::from_str_radix(off.trim_start_matches("0x"), 16).unwrap_or(0);
    for (i, site) in [0xa9acu32, 0xa9b4, 0xa9bc].into_iter().enumerate() {
        ev.push(st(
            site,
            "module",
            serde_json::json!(format!("{head}+{:#x}", off + 4 * i as u32)),
            4,
            n(f.table_vector[i]),
        ));
    }
    ev.extend([
        call(0xa9c4, serde_json::json!([slot]))?,
        call(0xa9d0, serde_json::json!([f.fx_source]))?,
        st(0xa9d4, "player", n(0x36c), 4, n(f.fx)),
        st(0xa9e0, "fx", n(0), 1, n(word(0xa9d8) & 0xffff)),
        st(
            0xa9f4,
            "fx",
            n(4),
            4,
            n(stage_f32(rel, at(0xa9e6) as usize, 4, 0)?.to_bits()),
        ),
        st(0xa9fc, "fx", n(8), 4, n(0)),
        st(0xaa08, "fx", n(0x10), 4, n(0x400)),
        st(0xaa10, "fx", n(0x1c), 4, n(0)),
    ]);
    for (i, site) in [0xaa1cu32, 0xaa20, 0xaa28].into_iter().enumerate() {
        ev.push(st(
            site,
            "player",
            n(0x360 + 4 * i as u32),
            4,
            n(f.position[i].to_bits()),
        ));
    }
    ev.extend([
        call(0xaa34, serde_json::json!([slot]))?,
        st(0xaa40, "task", n(0x18), 4, sym(0xaa3a)?),
        st(0xaa4c, "task", n(0x14), 4, sym(0xaa46)?),
        st(0xaa58, "task", n(0x24), 4, sym(0xaa52)?),
        st(
            if f.global_byte != 0 { 0xaa7c } else { 0xaa8c },
            "player",
            n(0x24),
            4,
            n(0x8000),
        ),
        serde_json::json!({"site":at(0xaa90),"local":0xff7c,"args":[]}),
    ]);
    if f.module_4ac == 0 {
        ev.extend([
            call(0xaab8, serde_json::json!([sym(0xaaaa)?, sym(0xaab2)?]))?,
            call(
                0xaad4,
                serde_json::json!([0, 0, sym(0xaac6)?, sym(0xaace)?]),
            )?,
            st(
                0xaad8,
                "module",
                serde_json::json!("88:6+0x4ac"),
                4,
                n(f.timer_task),
            ),
            st(0xaae8, "timer_task", n(0x18), 4, sym(0xaade)?),
        ]);
    }
    let table = rel
        .reference(1, at(0xa99a))
        .ok_or_else(|| Error::invalid("missing constructor table"))?
        .addend;
    let reads: Vec<u32> = [0xa9a0u32, 0xa9b0, 0xa9b8, 0xa9c8]
        .into_iter()
        .map(|s| table + (word(s) & 0xffff))
        .collect();
    Ok(serde_json::json!({"events":ev,"built":true,"table_reads":reads}))
}

/// State the stage teardown reads: module words 6:0x484 and 6:0x480, the
/// hit handler word 0x801a2638 and the player tasks at 0x801e7388.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageTeardownFrame {
    pub module_484: u32,
    /// An engine address as a number, or a module address as
    /// "88:SECTION+0xOFFSET".
    pub hit_handler: serde_json::Value,
    pub module_480: u32,
    pub player_tasks: [u32; 2],
}
/// Section 1 + 0xa2a0: free the timer and stage objects, remove the ring
/// drop hit handler if it is still installed, and free both player tasks.
fn stage_teardown(
    rel: &crate::city_rel::CityRel<'_>,
    f: &StageTeardownFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32| serde_json::json!({"site":site,"local":target,"args":[]});
    let st = |site: u32, address: serde_json::Value| serde_json::json!({"site":site,"store":"engine","address":address,"size":4,"value":0});
    let module = |site: u32| -> Result<String> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88)
            .ok_or_else(|| Error::invalid("missing teardown module reference"))?;
        Ok(format!("88:{}+{:#x}", r.section, r.addend))
    };
    let mut ev = Vec::new();
    if f.module_484 != 0 {
        ev.push(call(0xa2c4, serde_json::json!([f.module_484]))?);
        ev.push(st(0xa2cc, serde_json::json!(module(0xa2b2)?)));
    }
    let handler = rel
        .reference(1, 0xa2d2)
        .ok_or_else(|| Error::invalid("missing hit handler reference"))?
        .addend;
    let ring_drop = rel
        .reference(1, 0xa2de)
        .filter(|r| r.module == 88 && r.section == 1)
        .ok_or_else(|| Error::invalid("missing ring drop reference"))?
        .addend;
    if f.hit_handler == serde_json::json!(format!("88:1+{ring_drop:#x}")) {
        ev.push(st(0xa2f0, serde_json::json!(handler)));
    }
    ev.extend([
        call(0xa2f4, serde_json::json!([]))?,
        call(
            0xa308,
            serde_json::json!([module(0xa2fa)?, module(0xa302)?]),
        )?,
        call(
            0xa31c,
            serde_json::json!([module(0xa30e)?, module(0xa316)?]),
        )?,
    ]);
    if f.module_480 != 0 {
        ev.push(call(0xa334, serde_json::json!([f.module_480]))?);
        ev.push(st(0xa33c, serde_json::json!(module(0xa322)?)));
    }
    ev.extend([
        local(0xa340, 0x14aa4),
        call(0xa344, serde_json::json!([]))?,
        local(0xa348, 0x6ec4),
    ]);
    for (site, task) in [(0xa360u32, f.player_tasks[1]), (0xa378, f.player_tasks[0])] {
        if task != 0 {
            ev.push(call(site, serde_json::json!([task]))?);
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// Module word section 5 + 0x382b8 read by the stage object set-up.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageObjectFrame {
    pub module_382b8: i32,
}
/// Section 1 + 0xa390: run the boss set-up 0x414 when module word
/// 5 + 0x382b8 is set, then install the object's handlers.
fn stage_object(
    rel: &crate::city_rel::CityRel<'_>,
    f: &StageObjectFrame,
) -> Result<serde_json::Value> {
    let code = |site: u32| -> Result<serde_json::Value> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 1)
            .ok_or_else(|| Error::invalid("missing stage object handler"))?;
        Ok(serde_json::json!(format!("88:1+{:#x}", r.addend)))
    };
    let mut ev = Vec::new();
    if f.module_382b8 != 0 {
        ev.push(serde_json::json!({"site":0xa3b8,"local":0x414,"args":[]}));
    }
    for (site, reloc, offset) in [
        (0xa3c4u32, 0xa3beu32, 0x10u32),
        (0xa3d0, 0xa3ca, 0x14),
        (0xa3dc, 0xa3d6, 0x1c),
    ] {
        ev.push(serde_json::json!({"site":site,"store":"object","offset":offset,"size":4,"value":code(reloc)?}));
    }
    Ok(serde_json::json!({"events":ev}))
}

/// Engine and local results and the state the boss constructor reads: the
/// task allocation and its entity (word 0x34), the results of locals
/// 0x1afe8 and 0x1b018, the 0x40-byte allocation, module words 5:0x510
/// and byte 5:0x50c, the entity's collision object (word 0x2c) and its
/// flags half-word 4, the work block's word 0xc, the entity position after
/// local 0x328, the byte module word 6:0x4a8 points to, and the result of
/// local 0x8528.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossConstructFrame {
    pub task: u32,
    pub entity: u32,
    pub work: u32,
    pub block: u32,
    pub allocation: u32,
    pub module_510: u32,
    pub module_50c: i8,
    pub collision: u32,
    pub collision_flags: u16,
    pub work_0c: u32,
    pub position: [f32; 3],
    pub stage_byte: u8,
    pub object: u32,
}
/// Section 1 + 0x414: build the boss task (update 0x1bb8, handlers 0x1e04
/// and 0x638), its work block and entity, and its child object 0x8528.
/// Returns the task, or 0 when an allocation fails.
fn boss_construct(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossConstructFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let sym = |site: u32| -> Result<serde_json::Value> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88)
            .ok_or_else(|| Error::invalid("missing boss constructor reference"))?;
        Ok(serde_json::json!(format!(
            "88:{}+{:#x}",
            r.section, r.addend
        )))
    };
    let rodata = |site: u32, offset: usize, len: usize| -> Result<&[u8]> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing boss constant"))?;
        let at = r.addend as usize + offset;
        rel.data(4)?
            .get(at..at + len)
            .ok_or_else(|| Error::invalid("boss constant out of range"))
    };
    let be = |b: &[u8]| u32::from_be_bytes(b.try_into().unwrap());
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value})
    };
    let n = |v: u32| serde_json::json!(v);
    let mut ev = vec![call(
        0x448,
        serde_json::json!([2, 2, sym(0x43a)?, sym(0x442)?]),
    )?];
    if f.task == 0 {
        return Ok(serde_json::json!({"events":ev,"returned":0}));
    }
    ev.extend([
        st(0x46c, "task", n(0x14), 4, sym(0x466)?),
        st(0x478, "task", n(0x18), 4, sym(0x472)?),
        local(0x488, 0x1afe8, serde_json::json!([f.task, 0, 1])),
        local(0x498, 0x1b018, serde_json::json!([f.work, 0x1c])),
    ]);
    if f.block == 0 {
        ev.push(call(0x4ac, serde_json::json!([f.task]))?);
        return Ok(serde_json::json!({"events":ev,"returned":0}));
    }
    ev.push(call(0x4c0, serde_json::json!([0x40, 1]))?);
    if f.allocation == 0 {
        ev.push(call(0x4d0, serde_json::json!([f.task]))?);
        return Ok(serde_json::json!({"events":ev,"returned":0}));
    }
    ev.push(st(0x4dc, "task", n(0x38), 4, n(f.allocation)));
    let start = rodata(0x4e2, 0, 12)?;
    for (i, site) in [0x4f0u32, 0x4f4, 0x4fc].into_iter().enumerate() {
        ev.push(st(
            site,
            "entity",
            n(0x14 + 4 * i as u32),
            4,
            n(be(&start[4 * i..4 * i + 4])),
        ));
    }
    ev.push(st(0x50c, "entity", n(0xc), 4, n(f.module_510)));
    let scale = be(rodata(0x512, 0, 4)?);
    for (site, offset) in [(0x51cu32, 0x28u32), (0x520, 0x24), (0x524, 0x20)] {
        ev.push(st(site, "entity", n(offset), 4, n(scale)));
    }
    let magic = f64::from_bits(u64::from_be_bytes(rodata(0x53a, 0, 8)?.try_into().unwrap()));
    let biased =
        f64::from_bits(0x4330_0000_0000_0000 | u64::from(f.module_50c as i32 as u32 ^ 0x8000_0000));
    let byte_float = (biased - magic) as f32;
    ev.extend([
        st(0x55c, "work", n(0x28), 4, n(byte_float.to_bits())),
        call(0x574, serde_json::json!([f.task, sym(0x566)?, 0x22, 3]))?,
        st(
            0x588,
            "collision",
            n(4),
            2,
            n(u32::from(f.collision_flags & !0x40)),
        ),
    ]);
    let radius = f32::from_bits(be(rodata(0x592, 0, 4)?));
    ev.push(serde_json::json!({"site":0x59c,"call":engine_import(rel, 0x59c)?,"args":[f.task],"floats":[f64::from(radius)]}));
    ev.extend([
        call(0x5ac, serde_json::json!([f.work_0c, sym(0x5a6)?]))?,
        serde_json::json!({"site":0x5b8,"store":"module","offset":sym(0x5b2)?,"size":4,"value":f.task}),
        local(0x5c4, 0x328, serde_json::json!([f.task, 0])),
        serde_json::json!({"site":0x5d4,"store":"module","offset":sym(0x5ce)?,"size":1,"value":0}),
    ]);
    for (i, site) in [0x5e0u32, 0x5e4, 0x5ec].into_iter().enumerate() {
        ev.push(st(
            site,
            "work",
            n(0x1c + 4 * i as u32),
            4,
            n(f.position[i].to_bits()),
        ));
    }
    ev.extend([
        local(0x5f4, 0x2078, serde_json::json!([f.work + 0x1c])),
        local(0x60c, 0x8528, serde_json::json!([f.task, f.stage_byte])),
        st(0x610, "block", n(0x10), 4, n(f.object)),
    ]);
    Ok(serde_json::json!({"events":ev,"returned":f.task}))
}

/// What the boss update reads, each at the point the native code reads it.
/// `data` is task word 0x40, `state` is data word 0x10 and `entity` is task
/// word 0x34. Global bytes 0x801cbd88 + 0x37 and + 0x39 gate the update.
/// The stage block is the one module word 6:0x4a8 points to: its byte 1 is
/// read once, and its byte 0 at entry, after local 0xd04, after the
/// dispatch and at the end. State bytes 4/5, half-word 6, words 0xc, 0x10,
/// 0x14, 0x18, the entity action byte and flags half-word 4 are read where
/// the code reads them, as is the child entity (word 0x34 of state word
/// 0x10).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossUpdateFrame {
    pub task: u32,
    pub data: u32,
    pub state: u32,
    pub entity: u32,
    pub action: i8,
    pub global_37: i8,
    pub global_39: i8,
    pub stage_lock: i8,
    pub stage_id: u8,
    pub last_stage: i8,
    pub handles: [u32; 2],
    pub data_0c: u32,
    pub entry_stage_id: u8,
    pub state_0c: i32,
    pub state_06: u16,
    pub dispatch_action: i8,
    pub after: BossUpdateAfter,
}
/// Values read after the dispatch (section 1 + 0x1d74 onward).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossUpdateAfter {
    pub data_0c: u32,
    pub child: u32,
    pub child_entity: u32,
    pub stage_id: u8,
    pub entity_flags: i16,
    pub entity_flags_after: i16,
    pub final_stage_id: u8,
}
/// Section 1 + 0x1bb8: per-frame boss update. Mirrors the entity action into
/// the state block, handles a stage change, runs the action routine from the
/// jump table at 5:0xfdc, then the child follow 0x1fc8, 0x744 and 0x920.
fn boss_update(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossUpdateFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let code = rel.data(1)?;
    let ins = |at: u32| -> Result<u32> {
        code.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(|| Error::invalid("boss update code out of range"))
    };
    let mut ev = vec![st(0x1be0, "state", 0, 1, f.action as u8 as u32)];
    if f.global_37 != 0 {
        return Ok(serde_json::json!({"events":ev}));
    }
    if f.global_39 != 0 {
        ev.push(call(0x1c10, serde_json::json!([f.data_0c]))?);
        return Ok(serde_json::json!({"events":ev}));
    }
    let changed = f.last_stage != f.stage_id as i8;
    if f.stage_lock != 0 {
        if changed {
            for (i, (free, clear)) in [(0x1c58u32, 0x1c60u32), (0x1c70, 0x1c78)]
                .into_iter()
                .enumerate()
            {
                if f.handles[i] != 0 {
                    ev.push(call(free, serde_json::json!([f.handles[i]]))?);
                    ev.push(st(clear, "state", 0x14 + 4 * i as u32, 4, 0));
                }
            }
        }
        ev.push(call(0x1c80, serde_json::json!([f.data_0c]))?);
        return Ok(serde_json::json!({"events":ev}));
    }
    if changed {
        ev.extend([
            local(0x1ca0, 0xd04, serde_json::json!([])),
            local(0x1cac, 0x210c, serde_json::json!([f.entry_stage_id])),
            st(0x1cb4, "state", 4, 1, 0xff),
            local(0x1cc0, 0xdc4, serde_json::json!([f.task, 2])),
        ]);
    }
    if f.state_0c == 1 {
        let left = f.state_06.wrapping_sub(1);
        ev.push(st(0x1cd8, "state", 6, 2, left as u32));
        if left == 0 {
            ev.push(local(0x1ce8, 0xd04, serde_json::json!([])));
        }
    } else if (f.dispatch_action as i32 as u32) <= 13 {
        let entry = rel
            .reference(5, 0xfdc + 4 * f.dispatch_action as u32)
            .filter(|r| r.module == 88 && r.section == 1)
            .ok_or_else(|| Error::invalid("missing boss action table entry"))?
            .addend;
        if entry != 0x1d74 {
            let bl = ins(entry + 4)?;
            let mut li = (bl & 0x03ff_fffc) as i32;
            if li & 0x0200_0000 != 0 {
                li -= 0x0400_0000;
            }
            ev.push(local(
                entry + 4,
                (entry as i32 + 4 + li) as u32,
                serde_json::json!([f.task]),
            ));
        }
    }
    let a = &f.after;
    ev.push(call(0x1d78, serde_json::json!([a.data_0c]))?);
    if a.child != 0 {
        ev.push(local(
            0x1da4,
            0x1fc8,
            serde_json::json!([a.child_entity + 0x14, f.data + 0x1c, a.stage_id]),
        ));
        if a.entity_flags & 0x100 != 0 {
            ev.push(local(0x1db8, 0x1f94, serde_json::json!([])));
            ev.push(st(
                0x1dc4,
                "entity",
                4,
                2,
                (a.entity_flags_after as u16 & !0x100) as u32,
            ));
        }
    }
    ev.extend([
        local(0x1dcc, 0x744, serde_json::json!([f.task])),
        local(0x1dd4, 0x920, serde_json::json!([f.task])),
        st(0x1de8, "state", 5, 1, a.final_stage_id as u32),
    ]);
    Ok(serde_json::json!({"events":ev}))
}

/// The boss as the helpers see it. `boss` is module word 6:0x0 (the boss
/// task the constructor stores); `data` is its word 0x40, `state` data word
/// 0x10, `child` state word 0x10. `action` and `flags` are the boss entity's
/// byte 0 and half-word 4. `state_0c` and `state_04` are state word 0xc and
/// byte 4. `task`, `data_28` and `argument` are what the routine is called
/// with: 0xd40 reads its task's data float 0x28 and state byte 4 (given as
/// `state_04`), 0xdc4 takes the action to set in `argument`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossHelperFrame {
    pub routine: u32,
    pub boss: u32,
    pub data: u32,
    pub state: u32,
    pub child: u32,
    pub action: i8,
    pub flags: i16,
    pub state_0c: u32,
    pub state_04: u8,
    pub data_28: f32,
    pub argument: i8,
}
/// PowerPC fctiwz: round toward zero to a signed word, saturating; NaN
/// gives 0x80000000.
fn fctiwz(x: f64) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x.trunc().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }
}
/// Section 1 + 0x9d4 (boss task), 0x9f4 (child), 0xa40 (state), 0xa88
/// (stunned: -1 without a boss, 1 for action 13 or entity flag 4), 0xc60
/// (state word 0xc as a signed byte), 0xc98 (start the 180-frame state 1),
/// 0xd04 (clear the state timer), 0xd40 (next entry of the pattern table at
/// 5:0x514) and 0xdc4 (set the entity action unless stunned).
fn boss_helper(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossHelperFrame,
) -> Result<serde_json::Value> {
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let state = || {
        if f.boss == 0 || f.data == 0 {
            0
        } else {
            f.state
        }
    };
    let stunned = || -> i32 {
        if f.boss == 0 {
            -1
        } else if f.action == 13 || f.flags & 4 != 0 {
            1
        } else {
            0
        }
    };
    let mut ev = Vec::new();
    let returned: i64 = match f.routine {
        0x9d4 => f.boss.into(),
        0x9f4 => {
            if f.boss == 0 {
                0
            } else {
                f.child.into()
            }
        }
        0xa40 => state().into(),
        0xa88 => stunned().into(),
        0xc60 => {
            if state() == 0 {
                -1
            } else {
                (f.state_0c as u8 as i8).into()
            }
        }
        0xc98 => {
            if f.boss == 0 || stunned() as i8 == 1 || state() == 0 {
                -1
            } else {
                ev.push(st(0xce8, "state", 6, 2, 0xb4));
                ev.push(st(0xcf0, "state", 0xc, 4, 1));
                f.state.into()
            }
        }
        0xd04 => {
            if state() == 0 {
                -1
            } else {
                ev.push(st(0xd28, "state", 6, 2, 0));
                ev.push(st(0xd2c, "state", 0xc, 4, 0));
                f.state.into()
            }
        }
        0xd40 => {
            let table = rel
                .reference(1, 0xd76)
                .filter(|r| r.module == 88 && r.section == 5)
                .ok_or_else(|| Error::invalid("missing boss pattern table"))?
                .addend as i64;
            let row = (fctiwz(f64::from(f.data_28)) as i8).wrapping_sub(1) as i64;
            let byte = |column: i8| -> Result<u8> {
                let at = table + 4 * row + column as i64;
                usize::try_from(at)
                    .ok()
                    .and_then(|at| rel.data(5).ok()?.get(at).copied())
                    .ok_or_else(|| Error::invalid("boss pattern index out of range"))
            };
            let mut next = f.state_04.wrapping_add(1) as i8;
            if (byte(next)? as i8) < 0 {
                next = 0;
            }
            ev.push(st(0xd9c, "state", 4, 1, next as u8 as u32));
            byte(next)?.into()
        }
        0xdc4 => {
            if stunned() as i8 == 0 {
                ev.push(st(0xe08, "task_entity", 0, 1, f.argument as u8 as u32));
                ev.push(st(0xe10, "task_entity", 1, 1, 0));
            }
            return Ok(serde_json::json!({"events":ev}));
        }
        _ => return Err(Error::invalid("not a boss helper routine")),
    };
    Ok(serde_json::json!({"events":ev,"returned":returned as u32}))
}

/// What boss action 0 reads: the task, its entity (word 0x34) and data
/// (word 0x40), the data's motion object (word 0xc), entity byte 1 at entry
/// and again after the set-up calls, motion float 0x1c, the result of
/// 0x8002cb10, stage byte 0 (through module word 6:0x4a8), the result of
/// 0x8001dd1c, and the stage block pointer read at the end.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossActionZeroFrame {
    pub task: u32,
    pub entity: u32,
    pub data: u32,
    pub motion: u32,
    pub step: i8,
    pub step_after: u8,
    pub motion_frame: f32,
    pub motion_done: i32,
    pub stage_id: i8,
    pub players: i32,
}
/// Section 1 + 0xe2c: boss action 0. Step 0 starts the motion and the
/// light and camera set-up; while the motion frame is inside the window at
/// 4:0xe0 / 4:0xe8 a sound and voice play; when the motion ends the boss
/// spawns its ring burst (0x18ed0, up to 3), switches to action 2 and plays
/// the next sound. The stage block's half-word 0xa is cleared every frame.
fn boss_action_zero(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossActionZeroFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let constant = |site: u32, offset: usize, len: usize| -> Result<&[u8]> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing boss action constant"))?;
        let at = r.addend as usize + offset;
        rel.data(4)?
            .get(at..at + len)
            .ok_or_else(|| Error::invalid("boss action constant out of range"))
    };
    let single = |site: u32, offset: usize| -> Result<f64> {
        Ok(f64::from(f32::from_be_bytes(
            constant(site, offset, 4)?.try_into().unwrap(),
        )))
    };
    let module = |site: u32| -> Result<String> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88)
            .ok_or_else(|| Error::invalid("missing boss action reference"))?;
        Ok(format!("88:{}+{:#x}", r.section, r.addend))
    };
    let mut ev = Vec::new();
    if f.step >= 0 && f.step <= 1 {
        if f.step == 0 {
            ev.extend([
                call(0xe78, serde_json::json!([3]))?,
                serde_json::json!({"site":0xe90,"local":0x1ed8,"args":[0, 0],"floats":[single(0xe82, 0x18)?]}),
                call(0xe9c, serde_json::json!([f.motion, 0]))?,
                local(0xea8, 0x1b0f4, serde_json::json!([f.task, f.data + 0x28])),
                local(0xeb8, 0x1ccf4, serde_json::json!([f.task, module(0xeb2)?])),
                serde_json::json!({"site":0xec4,"store":"entity","offset":1,"size":1,
                    "value":f.step_after.wrapping_add(1)}),
            ]);
        }
        let frame = f64::from(f.motion_frame);
        let upper = f64::from_be_bytes(constant(0xee2, 0, 8)?.try_into().unwrap());
        if frame > single(0xece, 0)? && frame < upper {
            ev.extend([
                call(0xf08, serde_json::json!([0xf, f.task, 1, 0, 0x96]))?,
                call(0xf18, serde_json::json!([f.task, 3, 0x7f]))?,
            ]);
        }
        ev.push(call(0xf20, serde_json::json!([f.motion]))?);
        if f.motion_done != 0 {
            ev.extend([
                call(0xf40, serde_json::json!([f.stage_id as i32 as u32]))?,
                call(0xf4c, serde_json::json!([f.motion, 1]))?,
                serde_json::json!({"site":0xf54,"store":"state","offset":4,"size":1,"value":0xff}),
                serde_json::json!({"site":0xf64,"local":0x1d10c,"args":[],"floats":[single(0xf5a, 0)?]}),
                call(0xf68, serde_json::json!([]))?,
                local(0xf90, 0x18ed0, serde_json::json!([f.entity + 0x14, 0, 0, f.players.min(3) as u32, 1, f.task])),
                local(0xf9c, 0xdc4, serde_json::json!([f.task, 2])),
                call(0xfb4, serde_json::json!([3, f.task, 1, 0, 0x96]))?,
                call(0xfc4, serde_json::json!([f.task, 3, 0x78]))?,
            ]);
        }
    }
    ev.push(serde_json::json!({"site":0xfd8,"store":"stage","offset":0xa,"size":2,"value":0}));
    Ok(serde_json::json!({"events":ev}))
}

/// What boss action 2 reads: entity byte 1 at entry and after local
/// 0x86b8, state word 0x10 (the child), stage byte 0 (through module word
/// 6:0x4a8), entity half-word 6 (the wait timer) and the result of local
/// 0xd40.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossActionTwoFrame {
    pub task: u32,
    pub step: i8,
    pub step_after: u8,
    pub child: u32,
    pub stage_id: u8,
    pub timer: u16,
    pub pattern: i8,
}
/// Section 1 + 0xff4: boss action 2. Step 0 sets a 60-frame wait and runs
/// 0x86b8 on the child; when the wait runs out the next pattern entry
/// (0xd40) becomes the action (0xdc4).
fn boss_action_two(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossActionTwoFrame,
) -> Result<serde_json::Value> {
    let _ = rel;
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":"entity","offset":offset,"size":size,"value":value});
    let mut ev = Vec::new();
    if (0..=1).contains(&f.step) {
        if f.step == 0 {
            ev.extend([
                st(0x103c, 6, 2, 0x3c),
                local(0x1054, 0x86b8, serde_json::json!([f.child, f.stage_id])),
                st(0x1060, 1, 1, f.step_after.wrapping_add(1).into()),
            ]);
        }
        let left = f.timer.wrapping_sub(1);
        ev.push(st(0x106c, 6, 2, left.into()));
        if left == 0 {
            ev.push(local(0x1080, 0xd40, serde_json::json!([f.task])));
            ev.push(local(
                0x108c,
                0xdc4,
                serde_json::json!([f.task, f.pattern as i32 as u32]),
            ));
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What boss action 3 reads: entity byte 1 at entry and after the step's
/// calls, state words 0x14 / 0x18 (the stage handles) and 0x10 (the
/// child), motion half-word 0 and float 0x1c, the player task and object
/// words at 0x801e7348 / 0x801e7328, the result of 0x8002cb2c and the
/// entity flags half-word 4 as read before it is cleared.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossActionThreeFrame {
    pub task: u32,
    pub motion: u32,
    pub step: i8,
    pub step_after: u8,
    pub handles: [u32; 2],
    pub child: u32,
    pub motion_flags: u16,
    pub player_tasks: [u32; 2],
    pub player_objects: [u32; 2],
    pub motion_frame: f32,
    pub motion_done: i32,
    pub entity_flags: i16,
}
/// Section 1 + 0x10ac: boss action 3. Step 0 frees the stage handles and
/// starts motion 2; step 1 waits for motion flag 2 to clear, then resets the
/// lights, the child (0x866c), the camera and both players; step 2 plays the
/// sound and voice inside the motion window 4:0xc4..4:0xf4 and, when the
/// motion ends, clears entity flag 4, runs 0x8818 on the child, starts
/// motion 1, sets action 1, raises stage byte 4 and starts the handoff
/// (0xec04). The stage half-word 0xa is cleared every frame.
fn boss_action_three(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossActionThreeFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let single = |site: u32, offset: usize| -> Result<f64> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing boss action constant"))?;
        let at = r.addend as usize + offset;
        let b = rel
            .data(4)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("boss action constant out of range"))?;
        Ok(f64::from(f32::from_be_bytes(b.try_into().unwrap())))
    };
    let mut ev = Vec::new();
    match f.step {
        0 => {
            for (i, (free, clear)) in [(0x1114u32, 0x111cu32), (0x112c, 0x1134)]
                .into_iter()
                .enumerate()
            {
                if f.handles[i] != 0 {
                    ev.push(call(free, serde_json::json!([f.handles[i]]))?);
                    ev.push(st(clear, "state", 0x14 + 4 * i as u32, 4, 0));
                }
            }
            ev.extend([
                call(0x1144, serde_json::json!([f.motion, 2, 0x5a]))?,
                call(0x1158, serde_json::json!([0, 3, 0x18, 2]))?,
                st(0x1164, "entity", 1, 1, f.step_after.wrapping_add(1).into()),
            ]);
        }
        1 if f.motion_flags & 2 == 0 => {
            ev.extend([
                serde_json::json!({"site":0x1190,"local":0x1ed8,"args":[1, 0],"floats":[single(0x1182, 0x50)?]}),
                local(0x1198, 0x866c, serde_json::json!([f.child])),
                call(0x11ac, serde_json::json!([0, 3, 0x18, 4]))?,
                call(0x11b4, serde_json::json!([3]))?,
                call(0x11d0, serde_json::json!([f.player_tasks[0], f.player_objects[0]]))?,
                call(0x11d8, serde_json::json!([0]))?,
                call(0x11e4, serde_json::json!([f.player_tasks[1], f.player_objects[1]]))?,
                call(0x11ec, serde_json::json!([1]))?,
                st(0x11f8, "entity", 1, 1, f.step_after.wrapping_add(1).into()),
            ]);
        }
        2 => {
            let frame = f64::from(f.motion_frame);
            if frame > single(0x1206, 0)? && frame < single(0x121a, 0)? {
                ev.extend([
                    call(0x1240, serde_json::json!([0x10, f.task, 1, 0, 0xb4]))?,
                    call(0x1250, serde_json::json!([f.task, 0x10, 0x7f]))?,
                ]);
            }
            ev.push(call(0x1258, serde_json::json!([f.motion]))?);
            if f.motion_done != 0 {
                ev.extend([
                    st(0x126c, "entity", 4, 2, (f.entity_flags as u16 & !4).into()),
                    local(0x1274, 0x8818, serde_json::json!([f.child])),
                    st(0x127c, "state", 4, 1, 0xff),
                    call(0x128c, serde_json::json!([f.motion, 1, 0x3c]))?,
                    local(0x1298, 0xdc4, serde_json::json!([f.task, 1])),
                    st(0x12ac, "stage", 4, 1, 1),
                    local(0x12b0, 0xec04, serde_json::json!([])),
                ]);
            }
        }
        _ => {}
    }
    ev.push(st(0x12c4, "stage", 0xa, 2, 0));
    Ok(serde_json::json!({"events":ev}))
}

/// What boss actions 5..7 read, each where the code reads it: entity byte
/// 1 (the step) at entry, entity byte 0 (the action) on step 0, the result
/// of local 0x62a0, entity byte 1 again before each increment, entity
/// half-word 6 (the wind-up timer), state word 0x14 (the attack object) as
/// read before 0x63e8 / 0x6410 and again after 0x6410, the result of
/// 0x6410 and of 0xd40.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossAttackFrame {
    pub task: u32,
    pub step: i8,
    pub action: i8,
    pub spawned: u32,
    pub step_after: [u8; 2],
    pub timer: u16,
    pub attack: u32,
    pub attack_state: i32,
    pub attack_after: u32,
    pub pattern: i8,
}
/// Section 1 + 0x12e8: boss actions 5, 6 and 7. Every frame runs the voice
/// roll 0xaf0. Step 0 spawns attack kind 0, 1 or 2 (0x62a0) into state word
/// 0x14 and sets a 60-frame wind-up; when it runs out the attack is
/// launched (0x63e8); step 2 waits for the attack to report 1 (0x6410),
/// frees it and takes the next pattern entry as the action.
fn boss_attack(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossAttackFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let mut ev = vec![local(0x1318, 0xaf0, serde_json::json!([f.task]))];
    if (0..=1).contains(&f.step) {
        if f.step == 0 {
            let kind = match f.action {
                5 => 0,
                6 => 1,
                7 => 2,
                _ => {
                    return Err(Error::invalid(
                        "actions other than 5..7 leave the attack kind unset",
                    ))
                }
            };
            ev.extend([
                local(0x1398, 0x62a0, serde_json::json!([f.task, 0, 0, kind])),
                st(0x139c, "state", 0x14, 4, f.spawned),
                st(0x13a4, "entity", 6, 2, 0x3c),
                st(
                    0x13b0,
                    "entity",
                    1,
                    1,
                    f.step_after[0].wrapping_add(1).into(),
                ),
            ]);
        }
        let left = f.timer.wrapping_sub(1);
        ev.push(st(0x13bc, "entity", 6, 2, left.into()));
        if left == 0 {
            ev.push(local(0x13d0, 0x63e8, serde_json::json!([f.attack])));
            ev.push(st(
                0x13dc,
                "entity",
                1,
                1,
                f.step_after[1].wrapping_add(1).into(),
            ));
        }
    } else if f.step == 2 {
        ev.push(local(0x13e8, 0x6410, serde_json::json!([f.attack])));
        if f.attack_state == 1 {
            if f.attack_after != 0 {
                ev.push(call(0x1400, serde_json::json!([f.attack_after]))?);
                ev.push(st(0x1408, "state", 0x14, 4, 0));
            }
            ev.push(local(0x1410, 0xd40, serde_json::json!([f.task])));
            ev.push(local(
                0x141c,
                0xdc4,
                serde_json::json!([f.task, f.pattern as i32 as u32]),
            ));
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What boss actions 8..10 read, each where the code reads it: entity byte
/// 1 (the step) at entry and before each increment, entity byte 0 (the
/// action) and state word 0x10 (the child) on step 0, the result of local
/// 0x3e58, entity half-word 6 (the timer), state word 0x18 (the beam) as
/// read before 0x3fc0 / 0x403c and again after 0x403c, and the results of
/// 0x403c and 0xd40.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossBeamFrame {
    pub task: u32,
    pub step: i8,
    pub action: i8,
    pub child: u32,
    pub spawned: u32,
    pub step_after: [u8; 3],
    pub timer: u16,
    pub beam: u32,
    pub beam_state: i8,
    pub beam_after: u32,
    pub pattern: i8,
}
/// Section 1 + 0x1440: boss actions 8, 9 and 10. Every frame runs the voice
/// roll 0xaf0. Step 0 builds the beam parameter block (two bytes by action,
/// the work vectors at 6:0x128 + 0x3c / + 0x48, the child) on the stack,
/// spawns the beam (0x3e58) into state word 0x18 and waits 60 frames;
/// step 1 waits 600 frames; step 2 counts down again and fires the beam
/// (0x3fc0); step 3 waits for the beam to report 1 (0x403c), frees it and
/// takes the next pattern entry.
fn boss_beam(rel: &crate::city_rel::CityRel<'_>, f: &BossBeamFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let work = |site: u32, offset: u32| -> Result<String> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88)
            .ok_or_else(|| Error::invalid("missing beam work reference"))?;
        Ok(format!("88:{}+{:#x}", r.section, r.addend + offset))
    };
    let countdown = |ev: &mut Vec<serde_json::Value>, site: u32| -> bool {
        let left = f.timer.wrapping_sub(1);
        ev.push(st(site, "entity", 6, 2, left.into()));
        left == 0
    };
    let mut ev = vec![local(0x146c, 0xaf0, serde_json::json!([f.task]))];
    match f.step {
        0 | 1 => {
            if f.step == 0 {
                let (head, a, b) = match f.action {
                    8 => (0x14deu32, 0u32, 1u32),
                    9 => (0x150e, 1, 1),
                    10 => (0x1542, 1, 2),
                    _ => {
                        return Err(Error::invalid(
                            "actions other than 8..10 leave the beam block unset",
                        ))
                    }
                };
                ev.extend([
                    serde_json::json!({"site":0x1568,"local":0x3e58,"args":[f.task],
                        "block":[a, b, work(head, 0x3c)?, work(head, 0x48)?, f.child]}),
                    st(0x156c, "state", 0x18, 4, f.spawned),
                    st(0x1574, "entity", 6, 2, 0x3c),
                    st(
                        0x1580,
                        "entity",
                        1,
                        1,
                        f.step_after[0].wrapping_add(1).into(),
                    ),
                ]);
            }
            if countdown(&mut ev, 0x158c) {
                ev.push(st(0x15a0, "entity", 6, 2, 0x258));
                ev.push(st(
                    0x15ac,
                    "entity",
                    1,
                    1,
                    f.step_after[1].wrapping_add(1).into(),
                ));
            }
        }
        2 => {
            if countdown(&mut ev, 0x15bc) {
                ev.push(local(0x15d0, 0x3fc0, serde_json::json!([f.beam])));
                ev.push(st(
                    0x15dc,
                    "entity",
                    1,
                    1,
                    f.step_after[2].wrapping_add(1).into(),
                ));
            }
        }
        3 => {
            ev.push(local(0x15e8, 0x403c, serde_json::json!([f.beam])));
            if f.beam_state == 1 {
                if f.beam_after != 0 {
                    ev.push(call(0x1604, serde_json::json!([f.beam_after]))?);
                    ev.push(st(0x160c, "state", 0x18, 4, 0));
                }
                ev.push(local(0x1614, 0xd40, serde_json::json!([f.task])));
                ev.push(local(
                    0x1620,
                    0xdc4,
                    serde_json::json!([f.task, f.pattern as i32 as u32]),
                ));
            }
        }
        _ => {}
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What boss actions 11 and 12 read, each where the code reads it: the
/// step (entity byte 1) at entry and before each increment, state word
/// 0x10 (the child), the results of locals 0x3e58 and 0x62a0, entity
/// half-word 6 and state half-word 8 (the timers), state word 0x14 (the
/// attack) before 0x63e8 / 0x6410 and again after them, the entity action
/// after 0x63e8, state word 0x18 (the beam) before 0x3fc0 / 0x403c and
/// after 0x403c, the results of 0x6410, 0x403c and 0xd40, and stage byte 0
/// (through module word 6:0x4a8).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossComboFrame {
    pub task: u32,
    pub step: i8,
    pub child: u32,
    pub spawned_beam: u32,
    pub spawned_attack: u32,
    pub step_after: [u8; 3],
    pub timer: u16,
    pub attack: u32,
    pub action_after: i8,
    pub attack_after: u32,
    pub attack_state: i32,
    pub beam: u32,
    pub beam_state: i8,
    pub beam_after: u32,
    pub pattern: i8,
    pub state_timer: u16,
    pub stage_id: u8,
}
/// Section 1 + 0x1640: boss actions 11 and 12. Every frame runs the voice
/// roll 0xaf0. Step 0 spawns the beam (block 1/2, as action 10) and attack
/// kind 2 and waits 60 frames; then the attack launches (0x63e8). Action 11
/// moves on to step 2: when the attack reports 1 it is freed and the beam
/// fires (0x3fc0); step 3 waits for the beam (0x403c), frees it and takes
/// the next pattern entry. Action 12 instead hands the attack to 0x6460 and
/// enters step 4, which runs 0x86b8 on the child every 600 frames.
fn boss_combo(rel: &crate::city_rel::CityRel<'_>, f: &BossComboFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let work = |offset: u32| -> Result<String> {
        let r = rel
            .reference(1, 0x16b6)
            .filter(|r| r.module == 88)
            .ok_or_else(|| Error::invalid("missing beam work reference"))?;
        Ok(format!("88:{}+{:#x}", r.section, r.addend + offset))
    };
    let mut ev = vec![local(0x166c, 0xaf0, serde_json::json!([f.task]))];
    match f.step {
        0 | 1 => {
            if f.step == 0 {
                ev.extend([
                    serde_json::json!({"site":0x16dc,"local":0x3e58,"args":[f.task],
                        "block":[1, 2, work(0x3c)?, work(0x48)?, f.child]}),
                    st(0x16e0, "state", 0x18, 4, f.spawned_beam),
                    local(0x16f4, 0x62a0, serde_json::json!([f.task, 0, 0, 2])),
                    st(0x16f8, "state", 0x14, 4, f.spawned_attack),
                    st(0x1700, "entity", 6, 2, 0x3c),
                    st(
                        0x170c,
                        "entity",
                        1,
                        1,
                        f.step_after[0].wrapping_add(1).into(),
                    ),
                ]);
            }
            let left = f.timer.wrapping_sub(1);
            ev.push(st(0x1718, "entity", 6, 2, left.into()));
            if left == 0 {
                ev.push(local(0x172c, 0x63e8, serde_json::json!([f.attack])));
                if f.action_after == 11 {
                    ev.push(st(
                        0x1748,
                        "entity",
                        1,
                        1,
                        f.step_after[1].wrapping_add(1).into(),
                    ));
                } else {
                    ev.extend([
                        local(0x1754, 0x6460, serde_json::json!([f.attack_after])),
                        st(0x175c, "state", 8, 2, 0x258),
                        st(0x1764, "entity", 1, 1, 4),
                    ]);
                }
            }
        }
        2 => {
            ev.push(local(0x1770, 0x6410, serde_json::json!([f.attack])));
            if f.attack_state == 1 {
                if f.attack_after != 0 {
                    ev.push(call(0x1788, serde_json::json!([f.attack_after]))?);
                    ev.push(st(0x1790, "state", 0x14, 4, 0));
                }
                ev.push(local(0x1798, 0x3fc0, serde_json::json!([f.beam])));
                ev.push(st(
                    0x17a4,
                    "entity",
                    1,
                    1,
                    f.step_after[2].wrapping_add(1).into(),
                ));
            }
        }
        3 => {
            ev.push(local(0x17b0, 0x403c, serde_json::json!([f.beam])));
            if f.beam_state == 1 {
                if f.beam_after != 0 {
                    ev.push(call(0x17cc, serde_json::json!([f.beam_after]))?);
                    ev.push(st(0x17d4, "state", 0x18, 4, 0));
                }
                ev.push(local(0x17dc, 0xd40, serde_json::json!([f.task])));
                ev.push(local(
                    0x17e8,
                    0xdc4,
                    serde_json::json!([f.task, f.pattern as i32 as u32]),
                ));
            }
        }
        4 => {
            let left = f.state_timer.wrapping_sub(1);
            ev.push(st(0x17f8, "state", 8, 2, left.into()));
            if left == 0 {
                ev.push(local(
                    0x181c,
                    0x86b8,
                    serde_json::json!([f.child, f.stage_id]),
                ));
                ev.push(st(0x1824, "state", 8, 2, 0x258));
            }
        }
        _ => {}
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What boss action 13 reads, each where the code reads it: the step
/// (entity byte 1) at entry and before each increment, state words 0x14 /
/// 0x18 (the stage handles) and 0x10 (the child), the entity flags
/// half-word 4 as read before it is cleared, motion float 0x1c before each
/// window check, the global frame word 0x801cbd88 + 0x7c before each
/// period check, the results of 0x8002cb2c and 0x800c3974, and stage byte 0
/// (through module word 6:0x4a8).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossDefeatFrame {
    pub task: u32,
    pub motion: u32,
    pub step: i8,
    pub step_after: [u8; 2],
    pub handles: [u32; 2],
    pub child: u32,
    pub entity_flags: i16,
    pub motion_frame: [f32; 3],
    pub frame_count: [u32; 3],
    pub motion_done: i32,
    pub clock: u32,
    pub stage_id: i8,
}
/// The compiler's unsigned division by a constant: the high word of the
/// product with `magic`, an optional add-back step, then a shift.
fn magic_divide(n: u32, magic: u32, add: bool, shift: u32) -> u32 {
    let high = ((u64::from(n) * u64::from(magic)) >> 32) as u32;
    if add {
        (((n - high) >> 1) + high) >> shift
    } else {
        high >> shift
    }
}
/// Section 1 + 0x1844: boss action 13, the defeat sequence. Step 0 frees
/// the stage handles, starts motion 3, sets the lights, resets the child
/// (0x866c), clears entity flag 4 and the camera. Every frame of step 0/1:
/// a rumble every 45 frames before motion frame 4:0xf8, three sound and
/// voice windows (the second on task + 1 with a long voice), the explosion
/// 0x92c0 every 30 frames and the flash 0x9518 every 10 frames before 4:0x114;
/// when the motion ends the result clock is set and the story advances
/// (0x8001c78c with the stage byte). Stage half-word 0xa is cleared.
fn boss_defeat(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossDefeatFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32, offset: usize| -> Result<f64> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing boss defeat constant"))?;
        let at = r.addend as usize + offset;
        let b = rel
            .data(4)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("boss defeat constant out of range"))?;
        Ok(f64::from(f32::from_be_bytes(b.try_into().unwrap())))
    };
    let code = rel.data(1)?;
    let imm = |at: usize| u32::from(u16::from_be_bytes([code[at + 2], code[at + 3]]));
    let magic = |hi: usize| (imm(hi) << 16).wrapping_add(imm(hi + 4) as u16 as i16 as i32 as u32);
    let mut ev = Vec::new();
    if (0..=1).contains(&f.step) {
        if f.step == 0 {
            for (i, (free, clear)) in [(0x18acu32, 0x18b4u32), (0x18c4, 0x18cc)]
                .into_iter()
                .enumerate()
            {
                if f.handles[i] != 0 {
                    ev.push(call(free, serde_json::json!([f.handles[i]]))?);
                    ev.push(st(clear, "state", 0x14 + 4 * i as u32, 4, 0));
                }
            }
            ev.extend([
                call(0x18d8, serde_json::json!([f.motion, 3]))?,
                serde_json::json!({"site":0x18f0,"local":0x1ed8,"args":[2, 0],"floats":[k(0x18e2, 0x6c)?]}),
                local(0x18f8, 0x866c, serde_json::json!([f.child])),
                st(0x1904, "entity", 4, 2, (f.entity_flags as u16 & !4).into()),
                call(0x190c, serde_json::json!([3]))?,
                st(0x1918, "entity", 1, 1, f.step_after[0].wrapping_add(1).into()),
            ]);
        }
        let frame = f64::from(f.motion_frame[0]);
        if frame < k(0x1922, 0)? {
            let g = f.frame_count[0];
            if g - magic_divide(g, magic(0x1940), true, 5) * 45 == 0 {
                ev.push(call(0x197c, serde_json::json!([1, 0, 0, 0]))?);
                ev.push(call(0x198c, serde_json::json!([0, 0, 0x64]))?);
            }
        }
        let window =
            |lo: u32, hi: u32| -> Result<bool> { Ok(frame > k(lo, 0)? && frame < k(hi, 0)?) };
        if window(0x1992, 0x19a6)? {
            ev.push(call(0x19cc, serde_json::json!([0x10, f.task, 1, 0, 0xb4]))?);
            ev.push(call(0x19dc, serde_json::json!([f.task, 0x10, 0x7f]))?);
        }
        if window(0x19e2, 0x19f6)? {
            let t = f.task.wrapping_add(1);
            ev.push(call(0x1a20, serde_json::json!([0x10, t, 1, 0, 0xb4]))?);
            ev.push(call(0x1a30, serde_json::json!([t, 0x10, 0x7f]))?);
            ev.push(call(0x1a40, serde_json::json!([t, 0x10, 0x3fff]))?);
        }
        if window(0x1a46, 0x1a5a)? {
            ev.push(call(0x1a80, serde_json::json!([0x12, f.task, 1, 0, 0xf0]))?);
            ev.push(call(0x1a90, serde_json::json!([f.task, 0x12, 0x7f]))?);
        }
        let g = f.frame_count[1];
        if g - magic_divide(g, magic(0x1aa4), false, 4) * 30 == 0
            && f64::from(f.motion_frame[1]) < k(0x1aca, 0)?
        {
            ev.push(local(0x1ae0, 0x92c0, serde_json::json!([f.task])));
        }
        let g = f.frame_count[2];
        if g - magic_divide(g, magic(0x1ae8), false, 3) * 10 == 0
            && f64::from(f.motion_frame[2]) < k(0x1b0e, 0)?
        {
            ev.push(call(0x1b30, serde_json::json!([0, 3, 0xa, 1]))?);
            ev.push(local(0x1b34, 0x9518, serde_json::json!([])));
        }
        ev.push(call(0x1b3c, serde_json::json!([f.motion]))?);
        if f.motion_done != 0 {
            ev.extend([
                call(0x1b4c, serde_json::json!([0]))?,
                call(0x1b58, serde_json::json!([0, f.clock]))?,
                call(0x1b70, serde_json::json!([f.stage_id as i32 as u32]))?,
                st(
                    0x1b7c,
                    "entity",
                    1,
                    1,
                    f.step_after[1].wrapping_add(1).into(),
                ),
            ]);
        }
    }
    ev.push(st(0x1b90, "stage", 0xa, 2, 0));
    Ok(serde_json::json!({"events":ev}))
}

/// The boss task and its word 0x38 (the 0x40-byte block) at deletion.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossDeleteFrame {
    pub task: u32,
    pub block: u32,
}
fn module_ref(rel: &crate::city_rel::CityRel<'_>, site: u32, offset: u32) -> Result<String> {
    let r = rel
        .reference(1, site)
        .filter(|r| r.module == 88)
        .ok_or_else(|| Error::invalid("missing boss module reference"))?;
    Ok(format!(
        "88:{}+{:#x}",
        r.section,
        r.addend.wrapping_add(offset)
    ))
}
fn boss_constant(rel: &crate::city_rel::CityRel<'_>, site: u32) -> Result<f32> {
    let r = rel
        .reference(1, site)
        .filter(|r| r.module == 88 && r.section == 4)
        .ok_or_else(|| Error::invalid("missing boss constant"))?;
    let at = r.addend as usize;
    let b = rel
        .data(4)?
        .get(at..at + 4)
        .ok_or_else(|| Error::invalid("boss constant out of range"))?;
    Ok(f32::from_be_bytes(b.try_into().unwrap()))
}
/// Section 1 + 0x638: free the boss block, run 0x1b070 and clear the boss
/// task word 6:0x0.
fn boss_delete(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossDeleteFrame,
) -> Result<serde_json::Value> {
    let mut ev = Vec::new();
    if f.block != 0 {
        ev.push(
            serde_json::json!({"site":0x658,"call":engine_import(rel, 0x658)?,"args":[f.block]}),
        );
        ev.push(serde_json::json!({"site":0x660,"store":"task","offset":0x38,"size":4,"value":0}));
    }
    ev.push(serde_json::json!({"site":0x668,"local":0x1b070,"args":[f.task]}));
    ev.push(serde_json::json!({"site":0x678,"store":"module","offset":module_ref(rel, 0x672, 0)?,"size":4,"value":0}));
    Ok(serde_json::json!({"events":ev}))
}

/// Inputs of 0x690: the from and to points, the output pointer, the extra
/// distance (f1) and what the engine returns: the distance 0x800279c4
/// reports and the direction 0x801159dc leaves after normalising.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossAimFrame {
    pub from: u32,
    pub to: u32,
    pub out: u32,
    pub extra: f32,
    pub distance: f32,
    pub direction: [f32; 3],
}
/// Section 1 + 0x690: out = from + normalise(to - from) * (extra +
/// distance), through the engine vector calls. Single-precision
/// arithmetic as the native fadds / fmuls.
fn boss_aim(rel: &crate::city_rel::CityRel<'_>, f: &BossAimFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let scale = f.extra + f.distance;
    let scaled: Vec<f64> = f.direction.iter().map(|d| f64::from(d * scale)).collect();
    let mut last = call(0x71c, serde_json::json!([f.from, "stack", f.out]))?;
    last["vectors"] = serde_json::json!({"1": scaled});
    Ok(serde_json::json!({"events":[
        call(0x6c8, serde_json::json!([f.from, f.to]))?,
        call(0x6dc, serde_json::json!([f.to, f.from, "stack"]))?,
        call(0x6e4, serde_json::json!(["stack"]))?,
        last,
    ]}))
}

/// What the boss hit check reads, each where the code reads it: the
/// result of local 0x87e4 on the child (state word 0x10), the entity
/// action, data float 0x28 (the health), the entity flags after 0xdc4, the
/// hit record 0x800065a8 returns and its collider index (byte 0), the
/// entity's collider array (collision object word 0xc), the player entity
/// word at 0x801e7368 + 4 * stage byte 0, the point 0x690 writes,
/// the offset 0x80027a80 writes and the result of 0x7154.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossHitFrame {
    pub task: u32,
    pub child: u32,
    pub child_hit: i8,
    pub action: i8,
    pub health: f32,
    pub entity_flags: i16,
    pub hit: u32,
    pub collider: i8,
    pub colliders: u32,
    pub player: u32,
    pub point: [f32; 3],
    pub offset: [f32; 3],
    pub hit_kind: i32,
}
/// Section 1 + 0x744: per-frame boss hit check. When the child reports a hit
/// (0x87e4 == 1) outside action 3 the boss loses 4:0xac health and goes to
/// action 3, or to action 13 at or below 4:0xc0, and gets entity flag 4.
/// Otherwise a player hit (0x800065a8) on a collider pushes a point out
/// from the collider towards the player (0x690), then either starts the
/// stunned timer (0x7154 == 1: 0x71e0 with 4:0xc8, 0xc98) or knocks the
/// player back (0x71e0 with 4:0xc4, camera shake), spawns the spark
/// 0x108f4 and plays the hit sound and voice.
fn boss_hit(rel: &crate::city_rel::CityRel<'_>, f: &BossHitFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let v = |x: &[f32; 3]| x.iter().map(|c| f64::from(*c)).collect::<Vec<_>>();
    let mut ev = vec![local(0x774, 0x87e4, serde_json::json!([f.child]))];
    if f.child_hit == 1 && f.action != 3 {
        let health = f.health - boss_constant(rel, 0x79a)?;
        ev.push(serde_json::json!({"site":0x7a8,"store":"data","offset":0x28,"size":4,"value":health.to_bits()}));
        if f64::from(health) > f64::from(boss_constant(rel, 0x7b2)?) {
            ev.push(local(0x7cc, 0xdc4, serde_json::json!([f.task, 3])));
        } else {
            ev.push(local(0x7dc, 0xdc4, serde_json::json!([f.task, 13])));
        }
        ev.push(
            serde_json::json!({"site":0x7e8,"store":"entity","offset":4,"size":2,
            "value":(f.entity_flags as u16 | 4)}),
        );
        return Ok(serde_json::json!({"events":ev}));
    }
    ev.push(call(0x7f4, serde_json::json!([f.task]))?);
    if f.hit == 0 {
        return Ok(serde_json::json!({"events":ev}));
    }
    let collider = f
        .colliders
        .wrapping_add((f.collider as i32).wrapping_mul(48) as u32);
    if collider == 0 {
        return Ok(serde_json::json!({"events":ev}));
    }
    let knock = boss_constant(rel, 0x85a)?;
    let mut back = call(0x878, serde_json::json!(["stack", collider + 8, "stack"]))?;
    back["vectors"] = serde_json::json!({"0": v(&f.point)});
    ev.extend([
        serde_json::json!({"site":0x868,"local":0x690,"args":[collider + 8, f.player + 0x14, "stack"],
            "floats":[f64::from(knock)]}),
        back,
        local(0x884, 0x7154, serde_json::json!([f.task, f.hit])),
    ]);
    if f.hit_kind == 1 {
        ev.push(serde_json::json!({"site":0x8a0,"local":0x71e0,"args":[0],
            "floats":[f64::from(boss_constant(rel, 0x896)?)]}));
        ev.push(local(0x8a4, 0xc98, serde_json::json!([])));
    } else {
        ev.push(
            serde_json::json!({"site":0x8b4,"local":0x71e0,"args":["stack"],
            "floats":[f64::from(knock)],"vectors":{"0": v(&f.offset)}}),
        );
        ev.push(call(0x8c8, serde_json::json!([0, 1, 5, 0]))?);
    }
    ev.extend([
        serde_json::json!({"site":0x8d4,"local":0x108f4,"args":["stack", "stack"],
            "vectors":{"0": v(&f.point), "1": v(&f.offset)}}),
        call(0x8ec, serde_json::json!([9, f.task, 1, 0, 0x1e]))?,
        call(0x8fc, serde_json::json!([f.task, 9, 0x1e]))?,
    ]);
    Ok(serde_json::json!({"events":ev}))
}

/// The boss task for the attachment point update.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossPointsFrame {
    pub task: u32,
}
/// Section 1 + 0x920: for each of the 34 attachment entries at 5:0xb8c
/// (32 bytes each) whose byte 0 is positive, place the entry (0x80027ab4
/// with the work vector 6:0x128 + 12 * byte 0) and attach it to the boss
/// (0x80005540); then 0x8000f4ec.
fn boss_points(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossPointsFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let table = rel
        .reference(1, 0x942)
        .filter(|r| r.module == 88 && r.section == 5)
        .ok_or_else(|| Error::invalid("missing attachment table"))?
        .addend;
    let data = rel.data(5)?;
    let mut ev = Vec::new();
    for i in 0..34u32 {
        let entry = table + 0x20 * i;
        let count = *data
            .get(entry as usize)
            .ok_or_else(|| Error::invalid("attachment table out of range"))?
            as i8;
        if count > 0 {
            let out = format!("88:5+{:#x}", entry + 0x10);
            ev.push(call(
                0x984,
                serde_json::json!([
                    module_ref(rel, 0x956, 12 * count as u32)?,
                    format!("88:5+{:#x}", entry + 4),
                    out
                ]),
            )?);
            ev.push(call(0x994, serde_json::json!([f.task, i, out]))?);
        }
    }
    ev.push(call(0x9b0, serde_json::json!([f.task]))?);
    Ok(serde_json::json!({"events":ev}))
}

/// What the voice roll reads: the player entity word at 0x801e7368 + 4 *
/// stage byte 0, the distance 0x800279c4 returns, the two random words
/// from 0x8010d940 and the f1 result of 0x80112050 (single precision).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossVoiceFrame {
    pub task: u32,
    pub player: u32,
    pub distance: f32,
    pub randoms: [i32; 2],
    pub curve: f32,
}
/// The native signed int to single conversion: the 0x43300000 bias trick
/// with the magic double, then fsubs rounding.
fn biased_float(rel: &crate::city_rel::CityRel<'_>, site: u32, value: i32) -> Result<f32> {
    let r = rel
        .reference(1, site)
        .filter(|r| r.module == 88 && r.section == 4)
        .ok_or_else(|| Error::invalid("missing conversion constant"))?;
    let at = r.addend as usize;
    let b = rel
        .data(4)?
        .get(at..at + 8)
        .ok_or_else(|| Error::invalid("conversion constant out of range"))?;
    let magic = f64::from_bits(u64::from_be_bytes(b.try_into().unwrap()));
    let biased = f64::from_bits(0x4330_0000_0000_0000 | u64::from(value as u32 ^ 0x8000_0000));
    Ok((biased - magic) as f32)
}
/// Section 1 + 0xaf0: while the player is within 4:0xcc of the work point
/// 6:0x200, a random roll below 4:0xd4 plays voice 0x11 on the boss with a
/// random length scaled through 0x80112050. The negated comparisons keep
/// the native branch-if-not-less behaviour for NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn boss_voice(rel: &crate::city_rel::CityRel<'_>, f: &BossVoiceFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let point = module_ref(rel, 0xb3e, 0xd8)?;
    let mut ev = vec![call(0xb4c, serde_json::json!([f.player + 0x14, point]))?];
    if !(f.distance < boss_constant(rel, 0xb52)?) {
        return Ok(serde_json::json!({"events":ev}));
    }
    ev.push(call(0xb64, serde_json::json!([]))?);
    let unit = boss_constant(rel, 0xb8e)?;
    let roll = unit * biased_float(rel, 0xb6a, f.randoms[0])?;
    if !(roll < boss_constant(rel, 0xb9e)?) {
        return Ok(serde_json::json!({"events":ev}));
    }
    ev.push(call(
        0xbc8,
        serde_json::json!([0x11, f.task, 1, 0x5a, 0xf0, point]),
    )?);
    ev.push(call(0xbcc, serde_json::json!([]))?);
    let second = biased_float(rel, 0xbd2, f.randoms[1])? * unit;
    let length = fctiwz(f64::from(boss_constant(rel, 0xbfa)? * second));
    ev.push(call(0xc14, serde_json::json!([length as u32]))?);
    let volume = fctiwz(f64::from(boss_constant(rel, 0xc1a)? * f.curve));
    ev.push(call(
        0xc3c,
        serde_json::json!([f.task, 0x11, volume as u32]),
    )?);
    Ok(serde_json::json!({"events":ev}))
}

/// One call of a boss support routine, by its section 1 offset.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum BossSupportFrame {
    /// 0x1e04: the boss draw handler. Entity words 8 / 0xc / 0x10 are the
    /// rotation, `motion` is data word 0xc, `byte` the engine byte at
    /// 0x803ad526.
    Draw {
        entity: u32,
        rotation: [u32; 3],
        motion: u32,
        byte: i8,
    },
    /// 0x1ed8: screen effect `index` (0..2) with `flag` and `f1`; `count` and
    /// `clock` are the two 0x800c3974 results.
    ScreenEffect {
        index: i8,
        flag: i8,
        f1: f32,
        count: i32,
        clock: u32,
    },
    /// 0x1f94: `clock` is the 0x800c3974 result.
    ScreenReset { clock: u32 },
    /// 0x1fc8: copy `source` to `target`, mirrored in z and pushed off the
    /// plane by 4:0x118 / 4:0x11c.
    Follow { source: [f32; 3], flag: i8 },
    /// 0x2078: effect block from 5:0x2de0 with its first word set.
    EffectBlock { first: u32 },
    /// 0x210c: effect kind as a signed byte.
    EffectKind { kind: i8 },
}
/// The boss draw handler (0x1e04) and its engine wrappers: the screen
/// effect 0x1ed8 (parameter block on the stack, the clock reset after 13),
/// 0x1f94, the mirrored follow point 0x1fc8, the effect block 0x2078 and
/// 0x210c.
fn boss_support(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossSupportFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let mut ev = Vec::new();
    match f {
        BossSupportFrame::Draw {
            entity,
            rotation,
            motion,
            byte,
        } => {
            ev.extend([
                call(0x1e24, serde_json::json!([0x2400]))?,
                call(0x1e28, serde_json::json!([]))?,
                serde_json::json!({"site":0x1e2c,"local":0x4304,"args":[]}),
                call(0x1e34, serde_json::json!([entity + 0x14]))?,
                call(0x1e40, serde_json::json!([0, rotation[2]]))?,
                call(0x1e4c, serde_json::json!([0, rotation[0]]))?,
                call(0x1e58, serde_json::json!([0, rotation[1]]))?,
                call(0x1e64, serde_json::json!([0, 0x4000]))?,
                call(0x1e6c, serde_json::json!([entity + 0x20]))?,
                call(0x1e7c, serde_json::json!([module_ref(rel, 0x1e72, 0)?]))?,
                call(0x1e84, serde_json::json!([8]))?,
                call(
                    0x1e94,
                    serde_json::json!([module_ref(rel, 0x1e8e, 0)?, motion]),
                )?,
                call(0x1ea8, serde_json::json!([*byte as i32 as u32]))?,
                call(0x1eb0, serde_json::json!([0]))?,
                call(0x1eb4, serde_json::json!([]))?,
                call(0x1ebc, serde_json::json!([0x2400]))?,
            ]);
        }
        BossSupportFrame::ScreenEffect {
            index,
            flag,
            f1,
            count,
            clock,
        } => {
            let table = rel
                .reference(1, 0x1f3e)
                .filter(|r| r.module == 88 && r.section == 5)
                .ok_or_else(|| Error::invalid("missing screen effect table"))?
                .addend;
            let entry = rel
                .reference(5, table.wrapping_add((*index as i32 as u32) << 2))
                .filter(|r| r.module == 88)
                .ok_or_else(|| Error::invalid("screen effect index has no table entry"))?;
            let bits = if flag & 1 != 0 { 0x41 } else { 0x48 };
            ev.push(call(0x1f58, serde_json::json!([0]))?);
            if *count > 13 {
                ev.push(call(0x1f68, serde_json::json!([0]))?);
                ev.push(call(0x1f74, serde_json::json!([0, clock]))?);
            }
            let mut last = call(0x1f80, serde_json::json!([0, "stack"]))?;
            last["block"] = serde_json::json!([
                f1.to_bits(),
                0,
                format!("88:{}+{:#x}", entry.section, entry.addend),
                module_ref(rel, 0x1ee6, 0)?,
                bits,
                0
            ]);
            ev.push(last);
        }
        BossSupportFrame::ScreenReset { clock } => {
            ev.push(call(0x1fa4, serde_json::json!([0]))?);
            ev.push(call(0x1fb4, serde_json::json!([0, clock, 9]))?);
        }
        BossSupportFrame::Follow { source, flag } => {
            let mut z = -source[2];
            let near = boss_constant(rel, 0x2012)?;
            if f64::from(z.abs()) < f64::from(near) {
                z = if *flag == 0 {
                    boss_constant(rel, 0x2032)?
                } else {
                    near
                };
            }
            let st = |site: u32, offset: u32, value: u32| serde_json::json!({"site":site,"store":"target","offset":offset,"size":4,"value":value});
            ev.push(serde_json::json!({"site":0x200c,"local":0x2130,"args":[],"floats":[f64::from(-source[2])]}));
            ev.extend([
                st(0x2050, 0, source[0].to_bits()),
                st(0x2054, 4, source[1].to_bits()),
                st(0x205c, 8, z.to_bits()),
            ]);
        }
        BossSupportFrame::EffectBlock { first } => {
            let base = rel
                .reference(1, 0x2086)
                .filter(|r| r.module == 88 && r.section == 5)
                .ok_or_else(|| Error::invalid("missing effect block template"))?
                .addend as usize;
            let data = rel.data(5)?;
            let mut block = Vec::new();
            for i in 0..12 {
                let at = base + 4 * i;
                let w = data
                    .get(at..at + 4)
                    .ok_or_else(|| Error::invalid("effect block template out of range"))?;
                block.push(u32::from_be_bytes(w.try_into().unwrap()));
            }
            block[0] = *first;
            let mut last = call(0x20f8, serde_json::json!([0, "stack"]))?;
            last["block"] = serde_json::json!(block);
            ev.push(last);
        }
        BossSupportFrame::EffectKind { kind } => {
            ev.push(call(0x211c, serde_json::json!([*kind as i32 as u32]))?);
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// A texture scroll draw: `routine` is 0x2138, 0x2218 or 0x2300; `angle`
/// is the r3 argument of the first two; `speed` is module word 5:0x31f0 and
/// `frame_count` the global frame word 0x801cbd88 + 0x7c, each as read for
/// the first and the second scroll.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossScrollFrame {
    pub routine: u32,
    pub angle: u32,
    pub speed: [i32; 2],
    pub frame_count: [u32; 2],
}
/// Sections 1 + 0x2138 / 0x2218 / 0x2300: scroll two textures by
/// (speed * frame) & 0x1fff (0x2218 negates the speed) and draw the models
/// between them; the first two draw a model rotated by `angle`.
fn boss_scroll(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossScrollFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let m = |site: u32| module_ref(rel, site, 0);
    let negate = f.routine == 0x2218;
    let offset = |i: usize| {
        let speed = if negate {
            f.speed[i].wrapping_neg()
        } else {
            f.speed[i]
        };
        (speed as u32).wrapping_mul(f.frame_count[i]) & 0x1fff
    };
    let ev = match f.routine {
        0x2138 | 0x2218 => {
            // Call sites, then relocation sites, of the two copies.
            let (c, r) = if negate {
                (
                    [
                        0x227cu32, 0x2284, 0x2288, 0x2294, 0x22a0, 0x22a4, 0x22d4, 0x22dc,
                    ],
                    [0x223au32, 0x224a, 0x229a, 0x22aa, 0x22b6],
                )
            } else {
                (
                    [
                        0x2198, 0x21a0, 0x21a4, 0x21b0, 0x21bc, 0x21c0, 0x21ec, 0x21f4,
                    ],
                    [0x215a, 0x216a, 0x21b6, 0x21c6, 0x21d2],
                )
            };
            vec![
                call(c[0], serde_json::json!([m(r[0])?, m(r[1])?, 0, offset(0)]))?,
                call(c[1], serde_json::json!([m(r[0])?]))?,
                call(c[2], serde_json::json!([]))?,
                call(c[3], serde_json::json!([0, f.angle]))?,
                call(c[4], serde_json::json!([m(r[2])?]))?,
                call(c[5], serde_json::json!([]))?,
                call(c[6], serde_json::json!([m(r[3])?, m(r[4])?, 0, offset(1)]))?,
                call(c[7], serde_json::json!([m(r[3])?]))?,
            ]
        }
        0x2300 => vec![
            call(
                0x2358,
                serde_json::json!([m(0x231a)?, m(0x232a)?, 0, offset(0)]),
            )?,
            call(0x2360, serde_json::json!([m(0x231a)?]))?,
            call(
                0x238c,
                serde_json::json!([m(0x2366)?, m(0x2372)?, 0, offset(1)]),
            )?,
            call(0x2394, serde_json::json!([m(0x2366)?]))?,
        ],
        _ => return Err(Error::invalid("not a boss texture scroll routine")),
    };
    Ok(serde_json::json!({"events":ev}))
}

/// One call of a boss core routine. `task` is the core task; its entity is
/// task word 0x34 and its block task word 0x40. Values are as the code
/// reads them, after any call that comes first.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum BossCoreFrame {
    /// 0x8328: core update; `pause` is global byte 0x801cbd88 + 0x37.
    Update { task: u32, pause: i8, action: i8 },
    /// 0x8528: core construction for `parent` with stage byte `stage`; the
    /// task and block allocations, and byte 3 of the 0x80005588 result.
    Construct {
        parent: u32,
        stage: u8,
        task: u32,
        block: u32,
        collision_flags: u8,
    },
    /// 0x866c: core reset.
    Reset { task: u32 },
    /// 0x86b8: set the core stage `value`; `block_stage` and `block_slot` are
    /// block bytes 0 and 1, `slot` the result of 0x743c, `action` the
    /// entity action, `collision_word` word 4 of the 0x80005588 result.
    SetStage {
        task: u32,
        value: i8,
        block_stage: i8,
        block_slot: u8,
        slot: u8,
        action: i8,
        collision_word: u32,
    },
    /// 0x87e4: hit query; `flags` is entity half-word 4.
    HitQuery { task: u32, flags: i16 },
    /// 0x8818: clear the hit; `flags` is entity half-word 4 after the calls.
    Clear { task: u32, flags: i16 },
    /// 0x8880: table byte for block bytes 0 and 1.
    Pattern {
        task: u32,
        block_stage: i8,
        block_slot: i8,
    },
    /// 0x8ac8: free task word 0x38.
    Release { block: u32 },
}
/// The boss core object: update dispatch (actions 1, 2, 3, 5 to 0x783c,
/// 0x79dc, 0x7d6c, 0x7f58, then 0x7684 and 0x8000f4ec), construction, reset,
/// stage set, hit query and clear, the pattern table 5:0x36d2c and release.
fn boss_core(rel: &crate::city_rel::CityRel<'_>, f: &BossCoreFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: serde_json::Value| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!(boss_constant(rel, site)?.to_bits()))
    };
    let n = |v: u32| serde_json::json!(v);
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        BossCoreFrame::Update {
            task,
            pause,
            action,
        } => {
            if *pause == 0 {
                let target = match action {
                    1 => Some((0x8390, 0x783c)),
                    2 => Some((0x839c, 0x79dc)),
                    3 => Some((0x83a8, 0x7d6c)),
                    5 => Some((0x83b4, 0x7f58)),
                    _ => None,
                };
                if let Some((site, routine)) = target {
                    ev.push(local(site, routine, serde_json::json!([task])));
                }
                ev.push(local(0x83bc, 0x7684, serde_json::json!([task])));
                ev.push(call(0x83c4, serde_json::json!([task]))?);
            }
        }
        BossCoreFrame::Construct {
            parent,
            stage,
            task,
            block,
            collision_flags,
        } => {
            returned = Some(0);
            if *parent != 0 {
                ev.push(call(
                    0x8564,
                    serde_json::json!([2, module_ref(rel, 0x855e, 0)?]),
                )?);
                if *task != 0 {
                    ev.extend([
                        st(
                            0x8584,
                            "task",
                            0x14,
                            4,
                            serde_json::json!(module_ref(rel, 0x857e, 0)?),
                        ),
                        st(
                            0x8590,
                            "task",
                            0x18,
                            4,
                            serde_json::json!(module_ref(rel, 0x858a, 0)?),
                        ),
                        call(0x85a0, serde_json::json!([1, 0x28]))?,
                    ]);
                    if *block == 0 {
                        ev.push(call(0x85b0, serde_json::json!([task]))?);
                    } else {
                        ev.extend([
                            st(0x85bc, "task", 0x40, 4, n(*block)),
                            st(0x85c4, "entity", 0, 1, n(0)),
                            st(0x85c8, "block", 0, 1, n((*stage).into())),
                            st(0x85d8, "block", 0x24, 4, k(0x85ce)?),
                            st(0x85e8, "entity", 0x24, 4, k(0x85de)?),
                            st(0x85f8, "entity", 0x20, 4, k(0x85ee)?),
                            st(0x8600, "entity", 0x28, 4, k(0x85ee)?),
                            call(0x8618, serde_json::json!([task, module_ref(rel, 0x860a, 0)?, 1, 3]))?,
                            serde_json::json!({"site":0x862c,"call":engine_import(rel, 0x862c)?,"args":[task],
                                "floats":[f64::from(boss_constant(rel, 0x8622)?)]}),
                            call(0x8638, serde_json::json!([task, 0]))?,
                            st(0x8648, "collision", 3, 1, n(u32::from(collision_flags | 0xc))),
                        ]);
                        returned = Some(*task);
                    }
                }
            }
        }
        BossCoreFrame::Reset { task } => {
            ev.extend([
                st(0x8688, "block", 3, 1, n(0xff)),
                st(0x868c, "block", 2, 1, n(0xff)),
                local(0x8690, 0x8818, serde_json::json!([task])),
                st(0x8698, "entity", 0, 1, n(3)),
                st(0x86a0, "entity", 1, 1, n(0)),
            ]);
        }
        BossCoreFrame::SetStage {
            task,
            value,
            block_stage,
            block_slot,
            slot,
            action,
            collision_word,
        } => {
            let v = n(*value as u8 as u32);
            ev.push(local(0x86e8, 0x8818, serde_json::json!([task])));
            if value != block_stage {
                ev.push(local(0x8704, 0x743c, serde_json::json!([u32::MAX])));
            } else {
                ev.push(local(0x8710, 0x743c, serde_json::json!([block_slot])));
            }
            let s = n((*slot).into());
            match action {
                2 => ev.extend([
                    st(0x8724, "block", 2, 1, v),
                    st(0x8728, "block", 3, 1, s),
                    st(0x8730, "entity", 0, 1, n(3)),
                    st(0x8738, "entity", 1, 1, n(0)),
                ]),
                3 => ev.extend([st(0x8748, "block", 1, 1, s), st(0x874c, "block", 0, 1, v)]),
                _ => {
                    ev.extend([
                        st(0x8758, "block", 1, 1, s),
                        st(0x875c, "block", 0, 1, v),
                        st(0x876c, "entity", 0x20, 4, k(0x8762)?),
                        st(0x8774, "entity", 0x28, 4, k(0x8762)?),
                        call(0x8780, serde_json::json!([task, 0]))?,
                        st(0x878c, "collision", 4, 4, n(collision_word & !0x10)),
                        local(0x8790, 0x1f94, serde_json::json!([])),
                        st(0x8798, "entity", 0, 1, n(1)),
                        st(0x87a0, "entity", 1, 1, n(0)),
                    ]);
                }
            }
            ev.push(call(0x87b0, serde_json::json!([task, 0, 0]))?);
            ev.push(call(0x87c0, serde_json::json!([task, 0, 3]))?);
        }
        BossCoreFrame::HitQuery { task, flags } => {
            returned = Some(if *task == 0 {
                u32::MAX
            } else if flags & 4 != 0 {
                1
            } else {
                0
            });
        }
        BossCoreFrame::Clear { task, flags } => {
            if *task != 0 {
                ev.extend([
                    call(0x8848, serde_json::json!([task, 0, 1]))?,
                    call(0x8858, serde_json::json!([task, 0, 0]))?,
                    st(0x8864, "entity", 4, 2, n(u32::from(*flags as u16 & !4))),
                ]);
            }
        }
        BossCoreFrame::Pattern {
            task,
            block_stage,
            block_slot,
        } => {
            returned = Some(if *task == 0 {
                u32::MAX
            } else {
                let table = rel
                    .reference(1, 0x88a2)
                    .filter(|r| r.module == 88 && r.section == 5)
                    .ok_or_else(|| Error::invalid("missing core pattern table"))?
                    .addend as i64;
                let at = table + 8 * i64::from(*block_stage) + i64::from(*block_slot);
                u32::from(
                    usize::try_from(at)
                        .ok()
                        .and_then(|at| rel.data(5).ok()?.get(at).copied())
                        .ok_or_else(|| Error::invalid("core pattern index out of range"))?,
                )
            });
        }
        BossCoreFrame::Release { block } => {
            if *block != 0 {
                ev.push(call(0x8aec, serde_json::json!([block]))?);
                ev.push(st(0x8af4, "task", 0x38, 4, n(0)));
            }
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the core contact routines. Values are as the code reads
/// them; engine float results are single precision.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum CoreContactFrame {
    /// 0x7154: does `attacker` beat the hit record? `hit_word` is hit word 4,
    /// `hit_bytes` hit bytes 0 and 1, `defense` / `attack` the 0x80005874 and
    /// 0x800058bc results.
    Compare {
        attacker: u32,
        hit: u32,
        hit_word: u32,
        hit_bytes: [i8; 2],
        defense: i32,
        attack: i32,
    },
    /// 0x71e0: knockback with strength f1. Without a vector the stage byte
    /// picks the player (`player_y` is its float 0x18); with one, `vector`
    /// is its value and `normal` what local 0x6f10 leaves in the copy.
    Knockback {
        strength: f32,
        vector: Option<[f32; 3]>,
        normal: [f32; 3],
        stage_id: i8,
        player_y: f32,
    },
    /// 0x72e0: proximity voice for `task`; `player` is the player entity
    /// word for the stage byte, `distance` the 0x800279c4 result, `flags`
    /// entity half-word 4, `random` and `curve` the 0x8010d940 and
    /// 0x80112050 results.
    Voice {
        task: u32,
        entity: u32,
        player: u32,
        distance: f32,
        flags: i16,
        random: i32,
        curve: f32,
    },
}
/// Section 1 + 0x7154 / 0x71e0 / 0x72e0.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn core_contact(
    rel: &crate::city_rel::CityRel<'_>,
    f: &CoreContactFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let k = |site: u32| boss_constant(rel, site);
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        CoreContactFrame::Compare {
            attacker,
            hit,
            hit_word,
            hit_bytes,
            defense,
            attack,
        } => {
            returned = Some(0u32);
            if *hit != 0 && *attacker != 0 {
                ev.push(call(
                    0x7198,
                    serde_json::json!([hit_word, hit_bytes[1] as i32 as u32]),
                )?);
                ev.push(call(
                    0x71ac,
                    serde_json::json!([attacker, hit_bytes[0] as i32 as u32]),
                )?);
                returned = Some(u32::from(attack > defense));
            }
        }
        CoreContactFrame::Knockback {
            strength,
            vector,
            normal,
            stage_id,
            player_y,
        } => {
            let sid = *stage_id as i32 as u32;
            match vector {
                None => {
                    let side = if *stage_id == 0 {
                        *strength
                    } else {
                        -*strength
                    };
                    let lift = if f64::from(*player_y) > f64::from(k(0x723e)?) {
                        k(0x7252)?
                    } else {
                        k(0x7262)?
                    };
                    let mut e = call(0x7278, serde_json::json!([sid]))?;
                    e["floats"] = serde_json::json!([
                        f64::from(k(0x726e)?),
                        f64::from(lift),
                        f64::from(side)
                    ]);
                    ev.push(e);
                }
                Some(v) => {
                    ev.push(
                        serde_json::json!({"site":0x72a4,"local":0x6f10,"args":["stack", "stack"],
                        "vectors":{"0": v.iter().map(|c| f64::from(*c)).collect::<Vec<_>>()}}),
                    );
                    let mut e = call(0x72c8, serde_json::json!([sid]))?;
                    e["floats"] = serde_json::json!([
                        f64::from(normal[0]),
                        f64::from(v[1]),
                        f64::from(normal[2])
                    ]);
                    ev.push(e);
                }
            }
        }
        CoreContactFrame::Voice {
            task,
            entity,
            player,
            distance,
            flags,
            random,
            curve,
        } => {
            ev.push(call(
                0x7330,
                serde_json::json!([player + 0x14, entity + 0x14]),
            )?);
            let mut next = *flags as u16 & !0x100;
            if f64::from(*distance) < f64::from(k(0x733e)?) {
                next |= 0x100;
            }
            if *flags as u16 & 0x100 == 0 && next & 0x100 != 0 {
                ev.push(call(
                    0x7398,
                    serde_json::json!([0x11, task, 1, 0x46, 0x78, module_ref(rel, 0x738e, 0xd8)?]),
                )?);
                ev.push(call(0x739c, serde_json::json!([]))?);
                let roll = k(0x73c6)? * biased_float(rel, 0x73a2, *random)?;
                let length = fctiwz(f64::from(k(0x73d6)? * roll));
                ev.push(call(0x73f0, serde_json::json!([length as u32]))?);
                let volume = fctiwz(f64::from(k(0x73f6)? * *curve));
                ev.push(call(
                    0x7418,
                    serde_json::json!([task, 0x11, volume as u32]),
                )?);
            }
            ev.push(serde_json::json!({"site":0x741c,"store":"entity","offset":4,"size":2,"value":next}));
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the core slot, attachment or hit routines; values are as
/// the code reads them.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum CoreHitFrame {
    /// 0x743c: roll a slot other than `previous` (8 slots when it is
    /// negative, else 7); `random` is the 0x8010d940 result and `rounded`
    /// the f1 result of 0x80111fd8.
    Slot {
        previous: i8,
        random: i32,
        rounded: f64,
    },
    /// 0x7548: place attachment (`row`, `column`) of table 5:0x36b6c on the
    /// core entity and block.
    Attach {
        entity: u32,
        block: u32,
        row: i8,
        column: i8,
    },
    /// 0x7684: hit reaction. `flags` is entity half-word 4 at entry and
    /// `flags_after` before the final set, `hit` the 0x800065a8 record,
    /// `hit_word` / `hit_byte` its word 4 and byte 1, `defense` / `attack`
    /// the 0x800058bc / 0x80005874 results, `player` the player entity word
    /// for the stage byte, `direction` what 0x801159dc leaves, `block_bytes`
    /// block bytes 0 and 1.
    Hit {
        task: u32,
        entity: u32,
        block: u32,
        flags: i16,
        hit: u32,
        hit_word: u32,
        hit_byte: i8,
        defense: i32,
        attack: i32,
        player: u32,
        direction: [f32; 3],
        block_bytes: [i8; 2],
        flags_after: i16,
    },
}
/// Section 1 + 0x743c / 0x7548 / 0x7684.
fn core_hit(rel: &crate::city_rel::CityRel<'_>, f: &CoreHitFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let k = |site: u32| boss_constant(rel, site);
    let kv = |site: u32| -> Result<Vec<f64>> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing core vector constant"))?;
        let d = rel.data(4)?;
        (0..3)
            .map(|i| {
                let at = r.addend as usize + 4 * i;
                d.get(at..at + 4)
                    .map(|b| f64::from(f32::from_be_bytes(b.try_into().unwrap())))
                    .ok_or_else(|| Error::invalid("core vector constant out of range"))
            })
            .collect()
    };
    let table = |site: u32| -> Result<u32> {
        Ok(rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing core attachment table"))?
            .addend)
    };
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        CoreHitFrame::Slot {
            previous,
            random,
            rounded,
        } => {
            let count: i32 = if *previous < 0 { 8 } else { 7 };
            ev.push(call(0x746c, serde_json::json!([]))?);
            let unit = k(0x7496)? * biased_float(rel, 0x7472, *random)?;
            let scaled = biased_float(rel, 0x74aa, count)? * unit;
            let mut e = call(0x74cc, serde_json::json!([]))?;
            e["floats"] = serde_json::json!([f64::from(scaled)]);
            ev.push(e);
            let pick = fctiwz(*rounded) as i8 as i32;
            let mut skip = 0i32;
            let mut found = None;
            for slot in 0..count {
                if slot == i32::from(*previous) {
                    skip += 1;
                }
                if pick == slot {
                    found = Some(skip);
                    break;
                }
                skip += 1;
            }
            returned =
                Some(found.ok_or_else(|| {
                    Error::invalid("slot roll out of range leaves r3 to the engine")
                })? as u32);
        }
        CoreHitFrame::Attach {
            entity,
            block,
            row,
            column,
        } => {
            if *row >= 0 && *column >= 0 {
                let entry = table(0x758e)? + 224 * *row as u32 + 28 * *column as u32;
                let kind = *rel
                    .data(5)?
                    .get(entry as usize)
                    .ok_or_else(|| Error::invalid("core attachment table out of range"))?
                    as i8;
                let e = |o: u32| format!("88:5+{:#x}", entry + o);
                let mut first = call(0x7608, serde_json::json!([0, "stack", "stack"]))?;
                first["vectors"] = serde_json::json!({"1": kv(0x75a2)?});
                let mut third = call(0x764c, serde_json::json!([0, "stack", "stack"]))?;
                third["vectors"] = serde_json::json!({"1": kv(0x7622)?});
                ev.extend([
                    call(0x75c0, serde_json::json!([]))?,
                    call(
                        0x75e4,
                        serde_json::json!([0, module_ref(rel, 0x75da, (48 * kind as i32) as u32)?]),
                    )?,
                    call(0x75ec, serde_json::json!([e(0x10)]))?,
                    call(0x75f8, serde_json::json!([e(4), 0]))?,
                    first,
                    call(
                        0x761c,
                        serde_json::json!([module_ref(rel, 0x760e, 0)?, "stack", entity + 0x14]),
                    )?,
                    third,
                    call(
                        0x765c,
                        serde_json::json!([entity + 0x14, "stack", block + 0x18]),
                    )?,
                    call(0x7660, serde_json::json!([]))?,
                ]);
            }
        }
        CoreHitFrame::Hit {
            task,
            entity,
            block,
            flags,
            hit,
            hit_word,
            hit_byte,
            defense,
            attack,
            player,
            direction,
            block_bytes,
            flags_after,
        } => {
            if flags & 4 == 0 {
                ev.push(call(0x76c0, serde_json::json!([task]))?);
                if *hit != 0 {
                    ev.push(call(
                        0x76dc,
                        serde_json::json!([hit_word, *hit_byte as i32 as u32]),
                    )?);
                    ev.push(call(0x76ec, serde_json::json!([task, 0]))?);
                    if attack < defense {
                        let push = k(0x773e)?;
                        let lift = k(0x7752)?;
                        let v = [
                            f64::from(direction[0] * push),
                            f64::from(lift),
                            f64::from(direction[2] * push),
                        ];
                        let entry = table(0x77b2)?
                            .wrapping_add((224 * i32::from(block_bytes[0])) as u32)
                            .wrapping_add((28 * i32::from(block_bytes[1])) as u32);
                        ev.extend([
                            call(0x772c, serde_json::json!([player + 0x14, entity + 0x14, "stack"]))?,
                            call(0x7734, serde_json::json!(["stack"]))?,
                            serde_json::json!({"site":0x7780,"local":0x71e0,"args":["stack"],
                                "floats":[f64::from(k(0x7776)?)],"vectors":{"0": v}}),
                            call(0x7790, serde_json::json!([task, 0, 0]))?,
                            call(0x77a0, serde_json::json!([task, 0, 3]))?,
                            serde_json::json!({"site":0x77d8,"local":0x9228,
                                "args":[task, block + 0x18, format!("88:5+{:#x}", entry + 4)]}),
                            serde_json::json!({"site":0x77e4,"store":"entity","offset":4,"size":2,
                                "value":(*flags_after as u16 | 4)}),
                            serde_json::json!({"site":0x77ec,"store":"entity","offset":6,"size":2,"value":0x5a}),
                            call(0x7800, serde_json::json!([0, 0, 0, 0]))?,
                            call(0x7810, serde_json::json!([0, 0, 0x7f]))?,
                            serde_json::json!({"site":0x7818,"store":"entity","offset":0,"size":1,"value":5}),
                        ]);
                    }
                }
            }
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// What core action 1 reads: block float 0x24 and entity float 0x24 at
/// entry, block bytes 0 and 1, block float 0x24 again after 0x7548 and
/// after the sound call.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreGrowFrame {
    pub task: u32,
    pub entity: u32,
    pub block_scale: f32,
    pub entity_scale: f32,
    pub block_bytes: [u8; 2],
    pub block_scale_after: f32,
    pub block_scale_final: f32,
}
/// Section 1 + 0x783c: core action 1. Both scales grow by 5:0x36d3c (the
/// entity's clamped at 4:0x314), the attachment is placed (0x7548), a hum
/// plays with a pitch from the scale clamped to -128..70, and once the
/// block scale reaches 4:0x30c the core settles and switches to action 2.
fn core_grow(rel: &crate::city_rel::CityRel<'_>, f: &CoreGrowFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let step = {
        let r = rel
            .reference(1, 0x786a)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing core growth step"))?;
        let at = r.addend as usize;
        let b = rel
            .data(5)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("core growth step out of range"))?;
        f32::from_be_bytes(b.try_into().unwrap())
    };
    let entity_scale = f.entity_scale + step;
    let mut ev = vec![
        st(0x7878, "block", 0x24, 4, (f.block_scale + step).to_bits()),
        st(0x7888, "entity", 0x24, 4, entity_scale.to_bits()),
    ];
    let limit = k(0x7892)?;
    if f64::from(entity_scale) >= f64::from(limit) {
        ev.push(st(0x78a8, "entity", 0x24, 4, limit.to_bits()));
    }
    ev.push(serde_json::json!({"site":0x78b8,"local":0x7548,"args":[f.task, f.block_bytes[0], f.block_bytes[1]]}));
    let raw = fctiwz(f64::from(k(0x78be)? + k(0x78ca)? * f.block_scale_after)) as u32;
    let mut pitch = raw;
    if (raw as u16 as i16) < -128 {
        pitch = (-128i32) as u32;
    }
    if (pitch as u16 as i16) > 70 {
        pitch = 70;
    }
    ev.push(call(
        0x7928,
        serde_json::json!([5, f.task, 1, pitch, 5, f.entity + 0x14]),
    )?);
    let full = k(0x7932)?;
    if f64::from(f.block_scale_final) >= f64::from(full) {
        let unit = k(0x7956)?;
        ev.extend([
            st(0x7948, "block", 0x24, 4, full.to_bits()),
            st(0x7950, "entity", 8, 4, 0),
            st(0x7960, "entity", 0x24, 4, unit.to_bits()),
            st(0x7968, "entity", 0x28, 4, unit.to_bits()),
            st(0x7970, "entity", 0x20, 4, unit.to_bits()),
            st(0x7974, "block", 4, 4, 0),
            st(0x7978, "block", 8, 4, 0),
            st(0x7980, "block", 0xc, 4, 0x4000),
            st(0x7984, "block", 0x10, 4, 0),
            st(0x798c, "block", 0x14, 4, full.to_bits()),
            call(0x799c, serde_json::json!([f.task, 0, 1]))?,
            call(0x79ac, serde_json::json!([f.task, 0, 0]))?,
            st(0x79b4, "entity", 0, 1, 2),
            st(0x79b8, "entity", 1, 1, 0),
        ]);
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What core action 2 reads: entity word 8 (the spin angle), block bytes 0
/// and 1, block words 8, 4 and 0xc (the wobble angles), the four random
/// words and the engine sine results (single precision) for the spin, the
/// pulse phase, block words 4 and 8, and the cosine of block word 0xc.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorePulseFrame {
    pub task: u32,
    pub entity: u32,
    pub spin: u32,
    pub block_bytes: [u8; 2],
    pub angles: [i32; 3],
    pub randoms: [i32; 4],
    pub sines: [f32; 4],
    pub cosine: f32,
}
/// Section 1 + 0x79dc: core action 2. The block scale follows a sine of the
/// spin, which advances by 0x400; the attachment is placed; a random pulse
/// phase and three random wobble steps (angles wrapped above 0x10000) drive
/// the entity's three scales; the hum plays and the proximity voice runs.
fn core_pulse(rel: &crate::city_rel::CityRel<'_>, f: &CorePulseFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":4,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let wrap = |v: i32| {
        if v > 0x10000 {
            v.wrapping_sub(0x10000)
        } else {
            v
        }
    };
    let mut ev = vec![
        call(0x7a00, serde_json::json!([f.spin]))?,
        st(0x7a14, "block", 0x24, (k(0x7a06)? * f.sines[0]).to_bits()),
        st(0x7a20, "entity", 8, f.spin.wrapping_add(0x400)),
        serde_json::json!({"site":0x7a30,"local":0x7548,"args":[f.task, f.block_bytes[0], f.block_bytes[1]]}),
        call(0x7a34, serde_json::json!([]))?,
    ];
    let unit = k(0x7a5e)?;
    let k324 = k(0x7a6e)?;
    let phase_raw = fctiwz(f64::from(
        k324 + k324 * (unit * biased_float(rel, 0x7a3a, f.randoms[0])?),
    ));
    ev.push(st(0x7a8c, "block", 0x10, phase_raw as u32));
    let phase = wrap(phase_raw);
    if phase != phase_raw {
        ev.push(st(0x7aa4, "block", 0x10, phase as u32));
    }
    ev.push(call(0x7aac, serde_json::json!([phase as u32]))?);
    let pulse = k(0x7ac2)? + k(0x7ab2)? * f.sines[1];
    ev.push(st(0x7ad0, "block", 0x14, pulse.to_bits()));
    // (call site, random, unit site, scale site, base site, magic site, store, wrap store, offset)
    let steps = [
        (
            0x7ad4u32, 1usize, 0x7afeu32, 0x7b0eu32, 0x7b1eu32, 0x7b32u32, 0x7b60u32, 0x7b78u32,
            8u32, 0usize,
        ),
        (
            0x7b7c, 2, 0x7ba6, 0x7bb6, 0x7bc6, 0x7bda, 0x7c08, 0x7c20, 4, 1,
        ),
        (
            0x7c24, 3, 0x7c4e, 0x7c5e, 0x7c6e, 0x7c82, 0x7cb0, 0x7cc8, 0xc, 2,
        ),
    ];
    let mut angles = [0i32; 3];
    for (site, r, us, ss, bs, ms, store, wrap_store, offset, angle) in steps {
        ev.push(call(site, serde_json::json!([]))?);
        let delta = k(bs)? + k(ss)? * (k(us)? * biased_float(rel, site + 6, f.randoms[r])?);
        let raw = fctiwz(f64::from(biased_float(rel, ms, f.angles[angle])? + delta));
        ev.push(st(store, "block", offset, raw as u32));
        let v = wrap(raw);
        if v != raw {
            ev.push(st(wrap_store, "block", offset, v as u32));
        }
        angles[angle] = v;
    }
    let size = k(0x7cd6)?;
    let base = k(0x7ce6)?;
    ev.extend([
        call(0x7cd0, serde_json::json!([angles[1] as u32]))?,
        st(0x7cf4, "entity", 0x20, (base + size * f.sines[2]).to_bits()),
        call(0x7cfc, serde_json::json!([angles[0] as u32]))?,
        st(
            0x7d10,
            "entity",
            0x24,
            (base + pulse * f.sines[3]).to_bits(),
        ),
        call(0x7d18, serde_json::json!([angles[2] as u32]))?,
        st(0x7d2c, "entity", 0x28, (base + size * f.cosine).to_bits()),
        call(
            0x7d48,
            serde_json::json!([5, f.task, 1, 0x46, 5, f.entity + 0x14]),
        )?,
        serde_json::json!({"site":0x7d50,"local":0x72e0,"args":[f.task]}),
    ]);
    Ok(serde_json::json!({"events":ev}))
}

/// What core action 3 reads: block float 0x24 and entity float 0x24 at
/// entry, block bytes 0 and 1, block float 0x24 after 0x7548, block bytes 2
/// and 3 (the pending stage and slot) and word 4 of the 0x80005588 result.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreShrinkFrame {
    pub task: u32,
    pub entity: u32,
    pub block_scale: f32,
    pub entity_scale: f32,
    pub block_bytes: [u8; 2],
    pub block_scale_after: f32,
    pub pending: [u8; 2],
    pub collision_word: u32,
}
/// Section 1 + 0x7d6c: core action 3. Both scales shrink by 5:0x36d44
/// (the entity's held at 4:0x30c), the hum pitch follows the block scale
/// (clamped to -128..60), the attachment is placed, and once the block
/// scale reaches 4:0x350 the core resets its scales and either takes the
/// pending stage and slot and grows again (action 1) or, with none
/// pending, goes to action 4 with collision flag 0x10.
fn core_shrink(
    rel: &crate::city_rel::CityRel<'_>,
    f: &CoreShrinkFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let k = |site: u32| boss_constant(rel, site);
    let step = {
        let r = rel
            .reference(1, 0x7d9a)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing core shrink step"))?;
        let at = r.addend as usize + 8;
        let b = rel
            .data(5)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("core shrink step out of range"))?;
        f32::from_be_bytes(b.try_into().unwrap())
    };
    let block_scale = f.block_scale - step;
    let entity_scale = f.entity_scale - step;
    let mut ev = vec![
        st(0x7dac, "block", 0x24, 4, block_scale.to_bits()),
        st(0x7dbc, "entity", 0x24, 4, entity_scale.to_bits()),
    ];
    let floor = k(0x7dc6)?;
    if f64::from(entity_scale) < f64::from(floor) {
        ev.push(st(0x7dd8, "entity", 0x24, 4, floor.to_bits()));
    }
    let raw = fctiwz(f64::from(k(0x7dde)? + k(0x7dea)? * block_scale)) as u32;
    let mut pitch = raw;
    if (raw as u16 as i16) < -128 {
        pitch = (-128i32) as u32;
    }
    if (pitch as u16 as i16) > 60 {
        pitch = 60;
    }
    ev.push(call(
        0x7e48,
        serde_json::json!([5, f.task, 1, pitch, 5, f.entity + 0x14]),
    )?);
    ev.push(local(
        0x7e58,
        0x7548,
        serde_json::json!([f.task, f.block_bytes[0], f.block_bytes[1]]),
    ));
    let empty = k(0x7e62)?;
    if f64::from(f.block_scale_after) <= f64::from(empty) {
        let unit = k(0x7e7e)?;
        let base = k(0x7e8e)?;
        ev.extend([
            st(0x7e78, "block", 0x24, 4, empty.to_bits()),
            st(0x7e88, "entity", 0x24, 4, unit.to_bits()),
            st(0x7e98, "entity", 0x20, 4, base.to_bits()),
            st(0x7ea0, "entity", 0x28, 4, base.to_bits()),
            st(0x7ea8, "block", 0x14, 4, unit.to_bits()),
            st(0x7eb0, "block", 0x10, 4, 0),
        ]);
        if f.pending[1] as i8 >= 0 {
            ev.extend([
                call(0x7ecc, serde_json::json!([f.task, 0]))?,
                st(0x7ed8, "collision", 4, 4, f.collision_word & !0x10),
                st(0x7ee0, "block", 0, 1, f.pending[0].into()),
                st(0x7ee8, "block", 1, 1, f.pending[1].into()),
                st(0x7ef0, "block", 3, 1, 0xff),
                st(0x7ef4, "block", 2, 1, 0xff),
                local(0x7ef8, 0x1f94, serde_json::json!([])),
                st(0x7f00, "entity", 0, 1, 1),
                st(0x7f04, "entity", 1, 1, 0),
            ]);
        } else {
            ev.extend([
                call(0x7f14, serde_json::json!([f.task, 0]))?,
                st(0x7f20, "collision", 4, 4, f.collision_word | 0x10),
                st(0x7f28, "entity", 0, 1, 4),
                st(0x7f2c, "entity", 1, 1, 0),
            ]);
        }
        ev.push(local(0x7f34, 0x8818, serde_json::json!([f.task])));
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What core action 5 reads: entity word 8 (the spin) and half-word 6 (the
/// stun timer), block bytes 0 and 1, block words 8, 4 and 0xc, the three
/// random words, and the engine sine results for the spin and block word
/// 4 and the cosine of block word 0xc (single precision).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreStunFrame {
    pub task: u32,
    pub entity: u32,
    pub spin: u32,
    pub timer: u16,
    pub block_bytes: [u8; 2],
    pub angles: [i32; 3],
    pub randoms: [i32; 3],
    pub sines: [f32; 2],
    pub cosine: f32,
}
/// Section 1 + 0x7f58: core action 5, the stunned core. A fast pulse from
/// the spin (+0x400 a frame) with its hum, the attachment placed, three
/// random wobble steps, a squash that eases out over the 90-frame stun
/// timer, and when the timer is out the core shrinks by 5:0x36d44 and,
/// below 4:0x374, goes to action 3.
fn core_stun(rel: &crate::city_rel::CityRel<'_>, f: &CoreStunFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let wrap = |v: i32| {
        if v > 0x10000 {
            v.wrapping_sub(0x10000)
        } else {
            v
        }
    };
    let scale = k(0x7f8a)? * f.sines[0];
    let raw = fctiwz(f64::from(k(0x7f9e)? - k(0x7faa)? * scale)) as u32;
    let mut pitch = raw;
    if (raw as u16 as i16) < -128 {
        pitch = (-128i32) as u32;
    }
    if (pitch as u16 as i16) > 70 {
        pitch = 70;
    }
    let mut ev = vec![
        call(0x7f84, serde_json::json!([f.spin]))?,
        st(0x7f98, "block", 0x24, 4, scale.to_bits()),
        call(
            0x8008,
            serde_json::json!([5, f.task, 1, pitch, 5, f.entity + 0x14]),
        )?,
        st(0x8014, "entity", 8, 4, f.spin.wrapping_add(0x400)),
        serde_json::json!({"site":0x8024,"local":0x7548,"args":[f.task, f.block_bytes[0], f.block_bytes[1]]}),
    ];
    // (call, unit, scale, base, angle magic, store, wrap store, offset, angle)
    let steps = [
        (
            0x8028u32, 0x8052u32, 0x8062u32, 0x8072u32, 0x8086u32, 0x80b4u32, 0x80ccu32, 8u32,
            0usize,
        ),
        (0x80d0, 0x80fa, 0x810a, 0x811a, 0x812e, 0x815c, 0x8174, 4, 1),
        (
            0x8178, 0x81a2, 0x81b2, 0x81c2, 0x81d6, 0x8204, 0x821c, 0xc, 2,
        ),
    ];
    let mut angles = [0i32; 3];
    for (i, (site, us, ss, bs, ms, store, wrap_store, offset, angle)) in
        steps.into_iter().enumerate()
    {
        ev.push(call(site, serde_json::json!([]))?);
        let delta = k(bs)? + k(ss)? * (k(us)? * biased_float(rel, site + 6, f.randoms[i])?);
        let raw = fctiwz(f64::from(biased_float(rel, ms, f.angles[angle])? + delta));
        ev.push(st(store, "block", offset, 4, raw as u32));
        let v = wrap(raw);
        if v != raw {
            ev.push(st(wrap_store, "block", offset, 4, v as u32));
        }
        angles[angle] = v;
    }
    let size = k(0x822a)?;
    let base = k(0x823a)?;
    let squash = base - k(0x8252)? * biased_float(rel, 0x8266, 0x5a - i32::from(f.timer))?;
    ev.extend([
        call(0x8224, serde_json::json!([angles[1] as u32]))?,
        st(
            0x8248,
            "entity",
            0x20,
            4,
            (base + size * f.sines[1]).to_bits(),
        ),
        st(0x8290, "entity", 0x24, 4, squash.to_bits()),
        call(0x8298, serde_json::json!([angles[2] as u32]))?,
        st(
            0x82ac,
            "entity",
            0x28,
            4,
            (base + size * f.cosine).to_bits(),
        ),
    ]);
    if f.timer != 0 {
        ev.push(st(0x82c0, "entity", 6, 2, u32::from(f.timer - 1)));
    } else {
        let step = {
            let r = rel
                .reference(1, 0x82ce)
                .filter(|r| r.module == 88 && r.section == 5)
                .ok_or_else(|| Error::invalid("missing core shrink step"))?;
            let at = r.addend as usize + 8;
            let b = rel
                .data(5)?
                .get(at..at + 4)
                .ok_or_else(|| Error::invalid("core shrink step out of range"))?;
            f32::from_be_bytes(b.try_into().unwrap())
        };
        let left = scale - step;
        ev.push(st(0x82dc, "block", 0x24, 4, left.to_bits()));
        if f64::from(left) < f64::from(k(0x82e6)?) {
            ev.push(st(0x82fc, "entity", 0, 1, 3));
            ev.push(st(0x8304, "entity", 1, 1, 0));
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the core draw routines; values are as the code reads them.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum CoreDrawFrame {
    /// 0x83dc: draw the core. `scales` are entity floats 0x20..0x28,
    /// `block_scale` block float 0x24, `block_bytes` block bytes 0 and 1,
    /// `byte` the engine byte at 0x803ad526.
    Draw {
        scales: [f32; 3],
        block_scale: f32,
        block_bytes: [i8; 2],
        byte: i8,
    },
    /// 0x88c0: drifting spark update. `pause` and `freeze` are global bytes
    /// 0x801cbd88 + 0x37 and + 9; `spark` is task word 0x38; `velocity` the
    /// spark's floats 0xc..0x14 after the two 0x80114ba0 calls; `size`,
    /// `growth` and `fade` entity floats 0x20, 0x24 and 0x28; `timer` entity
    /// half-word 6.
    Drift {
        task: u32,
        entity: u32,
        spark: u32,
        pause: i8,
        freeze: i8,
        velocity: [f32; 3],
        size: f32,
        growth: f32,
        fade: f32,
        timer: u16,
    },
    /// 0x89e8: spark draw. `position` is entity floats 0x14..0x1c (copied as
    /// words), `size` and `fade` entity floats 0x20 and 0x28, `frame` the
    /// signed entity byte 1.
    Glow {
        position: [u32; 3],
        size: f32,
        fade: f32,
        frame: i8,
    },
}
/// Sections 1 + 0x83dc / 0x88c0 / 0x89e8.
fn core_draw(rel: &crate::city_rel::CityRel<'_>, f: &CoreDrawFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let fcall =
        |site: u32, args: serde_json::Value, floats: Vec<f64>| -> Result<serde_json::Value> {
            let mut e = call(site, args)?;
            e["floats"] = serde_json::json!(floats);
            Ok(e)
        };
    let st = |site: u32, region: &str, offset: serde_json::Value, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let k5 = |site: u32, offset: usize| -> Result<f32> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing core draw data"))?;
        let at = r.addend as usize + offset;
        let b = rel
            .data(5)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("core draw data out of range"))?;
        Ok(f32::from_be_bytes(b.try_into().unwrap()))
    };
    let d = |v: f32| f64::from(v);
    let mut ev = Vec::new();
    match f {
        CoreDrawFrame::Draw {
            scales,
            block_scale,
            block_bytes,
            byte,
        } => {
            let table = rel
                .reference(1, 0x844e)
                .filter(|r| r.module == 88 && r.section == 5)
                .ok_or_else(|| Error::invalid("missing core attachment table"))?
                .addend;
            let entry = table
                .wrapping_add((224 * i32::from(block_bytes[0])) as u32)
                .wrapping_add((28 * i32::from(block_bytes[1])) as u32);
            let kind = *rel
                .data(5)?
                .get(entry as usize)
                .ok_or_else(|| Error::invalid("core attachment table out of range"))?
                as i8;
            let one = d(k(0x8426)?);
            let full = d(k(0x84d2)?);
            ev.extend([
                call(0x83fc, serde_json::json!([]))?,
                call(0x8400, serde_json::json!([]))?,
                call(0x8408, serde_json::json!([0x820]))?,
                call(0x8414, serde_json::json!([1, 0x800]))?,
                call(0x841c, serde_json::json!([9]))?,
                fcall(
                    0x8438,
                    serde_json::json!([]),
                    vec![d(*block_scale), one, one, one],
                )?,
                call(0x843c, serde_json::json!([]))?,
                call(
                    0x8488,
                    serde_json::json!([0, module_ref(rel, 0x847e, (48 * i32::from(kind)) as u32)?]),
                )?,
                call(
                    0x8490,
                    serde_json::json!([format!("88:5+{:#x}", entry + 0x10)]),
                )?,
                call(
                    0x849c,
                    serde_json::json!([format!("88:5+{:#x}", entry + 4), 0]),
                )?,
                fcall(
                    0x84b0,
                    serde_json::json!([0]),
                    scales.iter().map(|v| d(*v)).collect(),
                )?,
                call(0x84bc, serde_json::json!([module_ref(rel, 0x84b6, 0)?]))?,
                call(0x84c8, serde_json::json!([module_ref(rel, 0x84c2, 0)?]))?,
                call(0x84cc, serde_json::json!([]))?,
                fcall(0x84e8, serde_json::json!([]), vec![full, full, full, full])?,
                call(0x84ec, serde_json::json!([]))?,
                call(0x84f0, serde_json::json!([]))?,
                call(0x8504, serde_json::json!([*byte as i32 as u32]))?,
            ]);
        }
        CoreDrawFrame::Drift {
            task,
            entity,
            spark,
            pause,
            freeze,
            velocity,
            size,
            growth,
            fade,
            timer,
        } => {
            if *pause == 0 && *freeze == 0 {
                let drag = k5(0x892a, 0x20)?;
                ev.extend([
                    call(0x8914, serde_json::json!([spark, spark + 0xc]))?,
                    call(0x8920, serde_json::json!([entity + 0x14, spark]))?,
                ]);
                for (i, site) in [0x893cu32, 0x894c, 0x895c].into_iter().enumerate() {
                    ev.push(st(
                        site,
                        "spark",
                        serde_json::json!(0xc + 4 * i as u32),
                        4,
                        (velocity[i] * drag).to_bits(),
                    ));
                }
                if d(*size) < d(k5(0x892a, 0x18)?) {
                    ev.push(st(
                        0x8978,
                        "entity",
                        serde_json::json!(0x20),
                        4,
                        (size + growth).to_bits(),
                    ));
                }
                if *timer == 0 {
                    let left = fade - k5(0x898e, 0x24)?;
                    ev.push(st(
                        0x899c,
                        "entity",
                        serde_json::json!(0x28),
                        4,
                        left.to_bits(),
                    ));
                    if d(left) < d(k(0x89a6)?) {
                        ev.push(call(0x89bc, serde_json::json!([task]))?);
                    }
                } else {
                    ev.push(st(
                        0x89c8,
                        "entity",
                        serde_json::json!(6),
                        2,
                        u32::from(timer - 1),
                    ));
                }
            }
        }
        CoreDrawFrame::Glow {
            position,
            size,
            fade,
            frame,
        } => {
            let sprite = module_ref(rel, 0x8a06, 0)?;
            let texture = module_ref(rel, 0x89fe, 0)?;
            let m = |o: u32| -> Result<serde_json::Value> {
                Ok(serde_json::json!(module_ref(rel, 0x8a06, o)?))
            };
            let factor = {
                let r = rel
                    .reference(1, 0x8a2e)
                    .filter(|r| r.module == 88 && r.section == 4)
                    .ok_or_else(|| Error::invalid("missing spark size factor"))?;
                let at = r.addend as usize;
                let b = rel
                    .data(4)?
                    .get(at..at + 8)
                    .ok_or_else(|| Error::invalid("spark size factor out of range"))?;
                f64::from_bits(u64::from_be_bytes(b.try_into().unwrap()))
            };
            let scaled = (factor * d(*size)) as f32;
            let tex = |o: u32| -> Result<serde_json::Value> {
                Ok(serde_json::json!(module_ref(rel, 0x89fe, o)?))
            };
            ev.extend([
                serde_json::json!({"site":0x8a10,"store":"module","offset":m(0x1c)?,"size":4,"value":texture}),
                st(0x8a1c, "module", m(0)?, 4, position[0]),
                st(0x8a20, "module", m(4)?, 4, position[1]),
                st(0x8a28, "module", m(8)?, 4, position[2]),
                st(0x8a44, "module", m(0x10)?, 4, scaled.to_bits()),
                st(0x8a48, "module", m(0xc)?, 4, scaled.to_bits()),
                st(
                    0x8a5c,
                    "module",
                    tex(0x10)?,
                    2,
                    ((i32::from(*frame) + 6) as u32) & 0xffff,
                ),
                fcall(0x8a70, serde_json::json!([]), vec![d(*fade); 4])?,
                call(0x8a7c, serde_json::json!([0, 8]))?,
                call(0x8a88, serde_json::json!([1, 0xa]))?,
                call(0x8a98, serde_json::json!([sprite, 0, 0x32]))?,
                call(0x8aa4, serde_json::json!([0, 8]))?,
                call(0x8ab0, serde_json::json!([1, 6]))?,
            ]);
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the attack object interface; `task` is the attack task,
/// its entity is task word 0x34 and its block task word 0x40.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum AttackFrame {
    /// 0x62a0: create an attack for `parent` aimed at `target` with
    /// `count` and `kind` (0..2). `task` / `block` are the allocation
    /// results; `block_initial` is the block's half-word 0 as allocated,
    /// which the code copies when the kind is out of range.
    Create {
        parent: u32,
        target: u32,
        count: u32,
        kind: i8,
        task: u32,
        block: u32,
        block_initial: i16,
    },
    /// 0x63e8: launch.
    Launch { task: u32 },
    /// 0x6410: finished? `action` is the entity action.
    Finished { task: u32, action: i8 },
    /// 0x6460: hand over (entity flag 0x100).
    Release { task: u32, flags: i16 },
    /// 0x6488: block word 0x10.
    Target { block_word: u32 },
    /// 0x6494: count down block half-word 2.
    Tick { timer: i16 },
    /// 0x64b0: clear the 512-byte table at 6:0x274.
    Clear,
}
/// Section 1 + 0x62a0 (create), 0x63e8 (launch), 0x6410 (finished),
/// 0x6460 (release), 0x6488, 0x6494 and 0x64b0 (table clear).
fn attack_object(rel: &crate::city_rel::CityRel<'_>, f: &AttackFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value})
    };
    let n = |v: u32| serde_json::json!(v);
    let mut ev = Vec::new();
    let mut returned: Option<u32> = None;
    match f {
        AttackFrame::Create {
            parent,
            target,
            count,
            kind,
            task,
            block,
            block_initial,
        } => {
            returned = Some(0);
            if *parent != 0 {
                ev.push(call(
                    0x62e4,
                    serde_json::json!([2, module_ref(rel, 0x62da, 0)?, parent]),
                )?);
                if *task != 0 {
                    ev.push(st(
                        0x6304,
                        "task",
                        n(0x18),
                        4,
                        serde_json::json!(module_ref(rel, 0x62fe, 0)?),
                    ));
                    ev.push(call(0x6310, serde_json::json!([1, 0x14]))?);
                    if *block == 0 {
                        ev.push(call(0x6320, serde_json::json!([task]))?);
                    } else {
                        ev.push(st(0x632c, "task", n(0x40), 4, n(*block)));
                        let pick = match kind {
                            0 => Some((0x6364u32, 0x635au32)),
                            1 => Some((0x6378, 0x636e)),
                            2 => Some((0x638c, 0x6382)),
                            _ => None,
                        };
                        let mut first = *block_initial as u16;
                        if let Some((site, reloc)) = pick {
                            let r = rel
                                .reference(1, reloc)
                                .filter(|r| r.module == 88 && r.section == 5)
                                .ok_or_else(|| Error::invalid("missing attack timer table"))?;
                            let at = r.addend as usize;
                            let b = rel
                                .data(5)?
                                .get(at..at + 2)
                                .ok_or_else(|| Error::invalid("attack timer table out of range"))?;
                            first = u16::from_be_bytes([b[0], b[1]]);
                            ev.push(st(site, "block", n(0), 2, n(first.into())));
                        }
                        ev.push(st(0x6394, "block", n(2), 2, n(first.into())));
                        ev.push(st(0x639c, "block", n(0x10), 4, n(0)));
                        if count & 0xffff != 0 {
                            ev.push(st(0x63ac, "block", n(8), 4, n(*target)));
                            ev.push(st(0x63b0, "block", n(4), 2, n(count & 0xffff)));
                        } else {
                            ev.push(st(0x63b8, "block", n(8), 4, n(0)));
                            ev.push(st(0x63bc, "block", n(4), 2, n(0)));
                        }
                        ev.push(serde_json::json!({"site":0x63c0,"local":0x64b0,"args":[]}));
                        returned = Some(*task);
                    }
                }
            }
        }
        AttackFrame::Launch { task } => {
            if *task != 0 {
                ev.extend([
                    st(0x63f8, "entity", n(6), 2, n(0x1e)),
                    st(0x6400, "entity", n(0), 1, n(2)),
                    st(0x6408, "entity", n(1), 1, n(0)),
                ]);
            }
        }
        AttackFrame::Finished { task, action } => {
            returned = Some(1);
            if *task != 0 {
                if *action == 4 {
                    ev.push(call(0x6440, serde_json::json!([task]))?);
                } else {
                    returned = Some(0);
                }
            }
        }
        AttackFrame::Release { task, flags } => {
            returned = Some(u32::MAX);
            if *task != 0 {
                ev.push(st(
                    0x647c,
                    "entity",
                    n(4),
                    2,
                    n(u32::from(*flags as u16 | 0x100)),
                ));
                returned = Some(1);
            }
        }
        AttackFrame::Target { block_word } => {
            returned = Some(*block_word);
        }
        AttackFrame::Tick { timer } => {
            if *timer > 0 {
                ev.push(st(
                    0x64a8,
                    "block",
                    n(2),
                    2,
                    n(u32::from((*timer - 1) as u16)),
                ));
            }
        }
        AttackFrame::Clear => {
            for row in 0..16u32 {
                for (i, site) in [
                    0x64d0u32, 0x64d4, 0x64d8, 0x64dc, 0x64e0, 0x64e4, 0x64e8, 0x64ec,
                ]
                .into_iter()
                .enumerate()
                {
                    ev.push(st(
                        site,
                        "module",
                        serde_json::json!(module_ref(rel, 0x64b2, 0x20 * row + 4 * i as u32)?),
                        4,
                        n(0),
                    ));
                }
            }
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the attack object update routines; values are as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum AttackUpdateFrame {
    /// 0x6158: update. `pause` / `freeze` are global bytes 0x801cbd88 +
    /// 0x37 / + 9, `boss_state` the 0xc60 result, `stage_lock` stage byte 1
    /// (through module word 6:0x4a8), `action` the entity action.
    Update {
        task: u32,
        pause: i8,
        freeze: i8,
        boss_state: i8,
        stage_lock: i8,
        action: i8,
    },
    /// 0x5f34: every 5 frames (global frame word + 0x7c) try two spawns;
    /// `spawned` are the 0x58a8 results in order.
    Spawn {
        task: u32,
        frame_count: u32,
        spawned: Vec<u32>,
    },
    /// 0x60a8: try two spawns now (released with 0x5808).
    Burst { task: u32, spawned: Vec<u32> },
    /// 0x5fe4: on step 0 release every live slot of the table 6:0x274 up
    /// to block half-word 0 (`count`); then park once task word 0xc
    /// (`child`) is gone.
    Sweep {
        step: i8,
        count: i16,
        slots: Vec<u32>,
        step_after: u8,
        child: u32,
    },
    /// 0x612c: rest timer (entity half-word 6).
    Rest { timer: u16 },
    /// 0x6264: deleter. `owner` is task word 8, `owner_block` its word
    /// 0x40, `owner_state` that block's word 0x10 and `link` its word 0x14.
    Delete {
        task: u32,
        owner: u32,
        owner_block: u32,
        owner_state: u32,
        link: u32,
    },
}
/// Sections 1 + 0x6158 (dispatch through the table at 5:0x3ffc), 0x5f34,
/// 0x60a8, 0x5fe4, 0x612c and 0x6264.
fn attack_update(
    rel: &crate::city_rel::CityRel<'_>,
    f: &AttackUpdateFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let code = rel.data(1)?;
    let mut ev = Vec::new();
    let spawn_loop = |ev: &mut Vec<serde_json::Value>,
                      task: u32,
                      spawned: &[u32],
                      base: u32,
                      release: u32|
     -> Result<()> {
        for i in 0..2 {
            ev.push(local(base, 0x58a8, serde_json::json!([task])));
            let got = *spawned
                .get(i)
                .ok_or_else(|| Error::invalid("missing spawn result"))?;
            if got == 0 {
                ev.push(st(base + 0x10, "entity", 0, 1, 4));
                ev.push(st(base + 0x18, "entity", 1, 1, 0));
                return Ok(());
            }
            ev.push(local(base + 0x20, release, serde_json::json!([got])));
        }
        Ok(())
    };
    match f {
        AttackUpdateFrame::Update {
            task,
            pause,
            freeze,
            boss_state,
            stage_lock,
            action,
        } => {
            if *pause == 0 && *freeze == 0 {
                ev.push(local(0x619c, 0xc60, serde_json::json!([])));
                if *boss_state != 1 {
                    if *stage_lock != 0 {
                        ev.push(call(0x61cc, serde_json::json!([task]))?);
                    } else if (*action as i32 as u32) <= 8 {
                        let entry = rel
                            .reference(5, 0x3ffc + 4 * *action as u32)
                            .filter(|r| r.module == 88 && r.section == 1)
                            .ok_or_else(|| Error::invalid("missing attack action table entry"))?
                            .addend;
                        if entry != 0x624c {
                            let at = entry as usize + 4;
                            let bl = u32::from_be_bytes(code[at..at + 4].try_into().unwrap());
                            let mut li = (bl & 0x03ff_fffc) as i32;
                            if li & 0x0200_0000 != 0 {
                                li -= 0x0400_0000;
                            }
                            ev.push(local(
                                entry + 4,
                                (entry as i32 + 4 + li) as u32,
                                serde_json::json!([task]),
                            ));
                        }
                    }
                }
            }
        }
        AttackUpdateFrame::Spawn {
            task,
            frame_count,
            spawned,
        } => {
            if frame_count - magic_divide(*frame_count, 0xcccc_cccd, false, 2) * 5 == 0 {
                spawn_loop(&mut ev, *task, spawned, 0x5f94, 0x5820)?;
            }
        }
        AttackUpdateFrame::Burst { task, spawned } => {
            spawn_loop(&mut ev, *task, spawned, 0x60dc, 0x5808)?;
        }
        AttackUpdateFrame::Sweep {
            step,
            count,
            slots,
            step_after,
            child,
        } => {
            if (0..=1).contains(step) {
                if *step == 0 {
                    for i in 0..i32::from(*count).max(0) as usize {
                        let w = *slots
                            .get(i)
                            .ok_or_else(|| Error::invalid("missing attack slot word"))?;
                        if w != 0 {
                            ev.push(local(0x604c, 0x5854, serde_json::json!([w])));
                        }
                    }
                    ev.push(st(
                        0x6070,
                        "entity",
                        1,
                        1,
                        step_after.wrapping_add(1).into(),
                    ));
                }
                if *child == 0 {
                    ev.push(st(0x6084, "entity", 0, 1, 4));
                    ev.push(st(0x608c, "entity", 1, 1, 0));
                }
            }
        }
        AttackUpdateFrame::Rest { timer } => {
            let left = timer.wrapping_sub(1);
            ev.push(st(0x6138, "entity", 6, 2, left.into()));
            if left == 0 {
                ev.push(st(0x614c, "entity", 0, 1, 0));
                ev.push(st(0x6150, "entity", 1, 1, 0));
            }
        }
        AttackUpdateFrame::Delete {
            task,
            owner,
            owner_block,
            owner_state,
            link,
        } => {
            if *owner != 0 && *owner_block != 0 && *owner_state != 0 && link == task {
                ev.push(st(0x6298, "owner_state", 0x14, 4, 0));
            }
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the attack piece helpers; values are as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum AttackPieceFrame {
    /// 0x5808: park the piece (action 4).
    Park,
    /// 0x5820: send the piece to action 5 and clear its slot (block byte
    /// 1) in the table at 6:0x274.
    Spend { slot: u8 },
    /// 0x5854: the same with action 6.
    Sweep { slot: u8 },
    /// 0x5888: copy block words 0x98..0xa0 to the output.
    Position { words: [u32; 3] },
    /// 0x58a8: pick a live slot. `count` is block half-word 0 at entry and
    /// `count_after` after the calls, `random` the 0x8010d940 result,
    /// `slots` the table words from 6:0x274.
    Pick {
        task: u32,
        count: i16,
        count_after: i16,
        random: i32,
        slots: Vec<u32>,
    },
}
/// Sections 1 + 0x5808 / 0x5820 / 0x5854 / 0x5888 / 0x58a8.
fn attack_piece(
    rel: &crate::city_rel::CityRel<'_>,
    f: &AttackPieceFrame,
) -> Result<serde_json::Value> {
    let st = |site: u32, region: &str, offset: serde_json::Value, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let n = |v: u32| serde_json::json!(v);
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        AttackPieceFrame::Park => {
            ev.push(st(0x5810, "entity", n(0), 1, 4));
            ev.push(st(0x5818, "entity", n(1), 1, 0));
        }
        AttackPieceFrame::Spend { slot } | AttackPieceFrame::Sweep { slot } => {
            let (action, d) = if matches!(f, AttackPieceFrame::Spend { .. }) {
                (5, 0)
            } else {
                (6, 0x34)
            };
            ev.extend([
                st(0x582c + d, "entity", n(0), 1, action),
                st(0x5834 + d, "entity", n(1), 1, 0),
                st(
                    0x584c + d,
                    "module",
                    serde_json::json!(module_ref(rel, 0x5842 + d, 4 * u32::from(*slot))?),
                    4,
                    0,
                ),
            ]);
        }
        AttackPieceFrame::Position { words } => {
            for (i, site) in [0x5894u32, 0x5898, 0x58a0].into_iter().enumerate() {
                ev.push(st(site, "out", n(4 * i as u32), 4, words[i]));
            }
        }
        AttackPieceFrame::Pick {
            task,
            count,
            count_after,
            random,
            slots,
        } => {
            ev.push(serde_json::json!({"site":0x58c4,"local":0x7008,"args":[task]}));
            ev.push(
                serde_json::json!({"site":0x58c8,"call":engine_import(rel, 0x58c8)?,"args":[]}),
            );
            let unit = boss_constant(rel, 0x58f2)? * biased_float(rel, 0x58ce, *random)?;
            let start = fctiwz(f64::from(
                biased_float(rel, 0x5906, i32::from(*count_after))? * unit,
            ));
            let mut index = start;
            let mut left = *count;
            let mut found = 0u32;
            while left > 0 {
                let i = index as i16 as i32;
                let w = *usize::try_from(i)
                    .ok()
                    .and_then(|i| slots.get(i))
                    .ok_or_else(|| Error::invalid("slot pick reads past the given table"))?;
                if w != 0 {
                    found = w;
                    break;
                }
                index = index.wrapping_add(1);
                left = left.wrapping_sub(1);
                if index as i16 as i32 == i32::from(*count_after) {
                    index = 0;
                }
            }
            returned = Some(found);
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// What the formation spawn reads: block half-word 0 (the piece count),
/// the f1 results of local 0x40cc (square root) and 0x80111fd8 (floor),
/// stage byte 0 (through module word 6:0x4a8), the engine sine and cosine
/// results in call order (single precision), the pieces local 0x55d8
/// returns in order, and the entity flags half-word 4.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackFormationFrame {
    pub task: u32,
    pub count: i16,
    pub root: f32,
    pub floor: f64,
    pub stage_id: i8,
    pub trig: Vec<f32>,
    pub pieces: Vec<u32>,
    pub flags: i16,
}
/// Section 1 + 0x59b0: spawn `count` pieces in a grid of floor(sqrt(count))
/// rows across the sphere of radius 5:0x3ff8, the columns sweeping from the
/// stage side, each piece stored in the table at 6:0x274. A failed spawn
/// trims the count and stops; otherwise the attack moves to action 1 (with
/// a 60-frame hold when entity flag 0x100 is set).
fn attack_formation(
    rel: &crate::city_rel::CityRel<'_>,
    f: &AttackFormationFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let n = |v: u32| serde_json::json!(v);
    let radius = {
        let r = rel
            .reference(1, 0x5a86)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing formation radius"))?;
        let at = r.addend as usize + 8;
        let b = rel
            .data(5)?
            .get(at..at + 4)
            .ok_or_else(|| Error::invalid("formation radius out of range"))?;
        f32::from_be_bytes(b.try_into().unwrap())
    };
    let mut trig = f.trig.iter().copied();
    let mut next_trig = || {
        trig.next()
            .ok_or_else(|| Error::invalid("missing trig result"))
    };
    let mut pieces = f.pieces.iter().copied();
    let count = i32::from(f.count);
    let mut root = serde_json::json!({"site":0x5a00,"local":0x40cc,"args":[],
        "floats":[f64::from(biased_float(rel, 0x59de, count)?)]});
    root["result"] = serde_json::json!(f64::from(f.root));
    let mut floor = call(0x5a04, serde_json::json!([]))?;
    floor["floats"] = serde_json::json!([f64::from(f.root)]);
    let mut ev = vec![root, floor];
    let rows = fctiwz(f.floor) as i16 as i32;
    if rows == 0 {
        return Err(Error::invalid(
            "zero rows divides by zero in the native code",
        ));
    }
    let rest = (count - rows * rows) as i16 as i32;
    let q = rest / rows;
    let extra = if (rest - q * rows) as i8 == 0 {
        q as i8 as i32
    } else {
        q + 1
    };
    let row_step = 0x6000 / (rows + 1);
    let columns = (rows + extra as i8 as i32) as i16 as i32;
    let column_step = 0x6000 / (columns + 1);
    let mut row_angle = -0x3000i32;
    let mut index = 0i32;
    let mut slot = 0u32;
    let mut row = 0i32;
    let finish = |ev: &mut Vec<serde_json::Value>, a: u32, b: u32, c: u32| {
        if f.flags & 0x100 != 0 {
            ev.push(st(a, "entity", n(6), 2, n(0x3c)));
        }
        ev.push(st(b, "entity", n(0), 1, n(1)));
        ev.push(st(c, "entity", n(1), 1, n(0)));
    };
    while (row as i8 as i32) < rows {
        row_angle = row_angle.wrapping_add(row_step);
        ev.push(call(0x5aa8, serde_json::json!([row_angle as u32]))?);
        let height = -(radius * next_trig()?);
        ev.push(call(0x5abc, serde_json::json!([row_angle as u32]))?);
        let ring = radius * next_trig()?;
        let mut column_angle: i32 = if f.stage_id == 0 { -0x7000 } else { 0x1000 };
        let mut column = 0i32;
        while (column as i8 as i32) < columns {
            column_angle = column_angle.wrapping_add(column_step);
            ev.push(call(0x5b08, serde_json::json!([column_angle as u32]))?);
            let x = ring * next_trig()?;
            ev.push(call(0x5b1c, serde_json::json!([column_angle as u32]))?);
            let z = -(ring * next_trig()?);
            ev.push(serde_json::json!({"site":0x5b3c,"local":0x55d8,"args":[f.task, "stack", 0, index as u32],
                "vectors":{"1": [f64::from(x), f64::from(height), f64::from(z)]}}));
            let piece = pieces
                .next()
                .ok_or_else(|| Error::invalid("missing spawned piece"))?;
            if piece == 0 {
                ev.push(st(
                    0x5b4c,
                    "block",
                    n(0),
                    2,
                    n((index as i8 as i32 as u32) & 0xffff),
                ));
                return Ok(serde_json::json!({"events":ev}));
            }
            ev.push(st(
                0x5b54,
                "module",
                serde_json::json!(module_ref(rel, 0x5a62, 4 * slot)?),
                4,
                n(piece),
            ));
            column += 1;
            index += 1;
            if index as i8 as i32 == count {
                finish(&mut ev, 0x5b84, 0x5b8c, 0x5b94);
                return Ok(serde_json::json!({"events":ev}));
            }
            slot += 1;
        }
        row += 1;
    }
    finish(&mut ev, 0x5bd0, 0x5bd8, 0x5be0);
    Ok(serde_json::json!({"events":ev}))
}

/// Entity flags half-word 4 and half-word 6 (the hold timer).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackHoldFrame {
    pub flags: i16,
    pub timer: u16,
}
/// Section 1 + 0x5c04: while entity flag 0x100 is set, count the hold down
/// and relaunch (30 frames, action 2) when it runs out.
fn attack_hold(
    _rel: &crate::city_rel::CityRel<'_>,
    f: &AttackHoldFrame,
) -> Result<serde_json::Value> {
    let st = |site: u32, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":"entity","offset":offset,"size":size,"value":value});
    let mut ev = Vec::new();
    if f.flags & 0x100 != 0 {
        let left = f.timer.wrapping_sub(1);
        ev.push(st(0x5c20, 6, 2, left.into()));
        if left == 0 {
            ev.extend([
                st(0x5c34, 6, 2, 0x1e),
                st(0x5c3c, 0, 1, 2),
                st(0x5c44, 1, 1, 0),
            ]);
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What the volley release reads: task word 0xc (the child), entity flags
/// half-word 4 and half-word 6 (the timer), block half-words 0 (the piece
/// count) and 2 (pieces left), the table words at 6:0x274, and in call
/// order the 0x8010d940 random words and the 0x8010abf4 integer results.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackVolleyFrame {
    pub child: u32,
    pub flags: i16,
    pub timer: u16,
    pub count: i16,
    pub remaining: i16,
    pub slots: Vec<u32>,
    pub randoms: Vec<i32>,
    pub indices: Vec<i32>,
}
/// Section 1 + 0x5c4c: with no child left the attack parks (action 4) or,
/// with flag 0x100, waits 120 frames in action 8. Every 30 frames it fires
/// one piece, or two when the count is not the kind-0 count 5:0x3f90: it
/// rolls a slot, walks to the next live one, releases it (0x57f0) and
/// clears it, then rolls again; a walk that runs out of pieces releases the
/// last slot it fired.
fn attack_volley(
    rel: &crate::city_rel::CityRel<'_>,
    f: &AttackVolleyFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: serde_json::Value, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let n = |v: u32| serde_json::json!(v);
    let k = |site: u32| boss_constant(rel, site);
    let slot_ref = |i: i32| -> Result<serde_json::Value> {
        Ok(serde_json::json!(module_ref(rel, 0x5d2e, (4 * i) as u32)?))
    };
    let mut ev = Vec::new();
    let mut timer = f.timer;
    if f.child == 0 {
        if f.flags & 0x100 == 0 {
            ev.push(st(0x5c8c, "entity", n(0), 1, 4));
            ev.push(st(0x5c94, "entity", n(1), 1, 0));
        } else {
            ev.push(st(0x5ca0, "entity", n(6), 2, 0x78));
            ev.push(st(0x5ca8, "entity", n(0), 1, 8));
            ev.push(st(0x5cb0, "entity", n(1), 1, 0));
            timer = 0x78;
        }
    }
    if timer != 0 {
        ev.push(st(0x5f18, "entity", n(6), 2, u32::from(timer - 1)));
        return Ok(serde_json::json!({"events":ev}));
    }
    let mut randoms = f.randoms.iter().copied();
    let mut indices = f.indices.iter().copied();
    let mut slots = f.slots.clone();
    let count = i32::from(f.count);
    let unit = k(0x5cee)?;
    let mut roll = |ev: &mut Vec<serde_json::Value>,
                    rand_site: u32,
                    magic: u32,
                    count_magic: u32,
                    cvt: u32|
     -> Result<i32> {
        ev.push(call(rand_site, serde_json::json!([]))?);
        let r = randoms
            .next()
            .ok_or_else(|| Error::invalid("missing random word"))?;
        let scaled = biased_float(rel, count_magic, count)? * (unit * biased_float(rel, magic, r)?);
        let mut e = call(cvt, serde_json::json!([]))?;
        e["floats"] = serde_json::json!([f64::from(scaled)]);
        ev.push(e);
        indices
            .next()
            .ok_or_else(|| Error::invalid("missing converted index"))
    };
    let table_count = {
        let r = rel
            .reference(1, 0x5d3a)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing kind count"))?;
        let at = r.addend as usize;
        let b = rel
            .data(5)?
            .get(at..at + 2)
            .ok_or_else(|| Error::invalid("kind count out of range"))?;
        i32::from(i16::from_be_bytes([b[0], b[1]]))
    };
    let mut r3 = roll(&mut ev, 0x5cc4, 0x5cca, 0x5d02, 0x5d24)?;
    let mut p = r3;
    let mut shots: i32 = if count == table_count { 1 } else { 2 };
    let mut left = count;
    let mut last: i32 = -1;
    let mut remaining = f.remaining;
    let read = |slots: &Vec<u32>, i: i32| -> Result<u32> {
        usize::try_from(i)
            .ok()
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| Error::invalid("volley reads past the given table"))
    };
    while (shots as i8) > 0 && (left as i16) > 0 {
        let w = read(&slots, p)?;
        let mut advance = true;
        if w != 0 {
            last = r3 as i16 as i32;
            ev.push(serde_json::json!({"site":0x5d8c,"local":0x57f0,"args":[w]}));
            ev.push(st(0x5d94, "module", slot_ref(p)?, 4, 0));
            slots[p as usize] = 0;
            r3 = roll(&mut ev, 0x5d98, 0x5d9e, 0x5dca, 0x5dec)?;
            remaining = remaining.wrapping_sub(1);
            ev.push(st(0x5df8, "block", n(2), 2, (remaining as u16).into()));
            shots -= 1;
            if (shots as i8) > 0 {
                left = count;
                r3 = roll(&mut ev, 0x5e10, 0x5e16, 0x5e42, 0x5e64)?;
                p = r3;
                advance = false;
            }
        }
        if advance {
            r3 = r3.wrapping_add(1);
            left -= 1;
            if r3 as u32 == count as u32 {
                r3 = 0;
                p = 0;
            } else {
                p += 1;
            }
        }
    }
    if (shots as i8) > 0 && (left as i16) == 0 && last != -1 {
        let w = read(&slots, last)?;
        if w != 0 {
            ev.push(serde_json::json!({"site":0x5ef0,"local":0x57f0,"args":[w]}));
            ev.push(st(0x5ef8, "module", slot_ref(last)?, 4, 0));
            remaining = remaining.wrapping_sub(1);
            ev.push(st(0x5f04, "block", n(2), 2, (remaining as u16).into()));
        }
    }
    ev.push(st(0x5f0c, "entity", n(6), 2, 0x1e));
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the attack piece object; values are as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum PieceFrame {
    /// 0x55d8: spawn a piece for `parent` at `vector` (words), with block
    /// bytes `kind` and `slot`. `task`, `block` and `ready` are the engine
    /// results of the allocations and 0x8000f8b8, `target` the 0x6488
    /// result, `random` the 0x8010d940 result.
    Spawn {
        parent: u32,
        vector: [u32; 3],
        kind: u8,
        slot: u8,
        task: u32,
        block: u32,
        target: u32,
        random: i32,
        ready: i32,
    },
    /// 0x5328: update; `pause` is global byte 0x801cbd88 + 0x37.
    Update { task: u32, pause: i8, action: i8 },
    /// 0x55b4: deleter; `slot` is block byte 1.
    Delete { slot: u8 },
    /// 0x57f0: release (action 2).
    Release,
    /// 0x5304: copy the entity position (words) to block words 0x98..0xa0.
    Position { position: [u32; 3] },
    /// 0x5264: distance to the player position the word at 0x80174f98 +
    /// 4 * stage byte 0 points at (`player`, read as three floats from
    /// offset 0). `root` is the f1 result of local 0x40cc.
    Distance {
        player: u32,
        player_position: [f32; 3],
        position: [f32; 3],
        root: f32,
    },
}
/// Sections 1 + 0x55d8 (spawn: update 0x5328, draw 0x5404, deleter 0x55b4,
/// the start delay from 5:0x3f9c), 0x5328 (dispatch on actions 0..5 to
/// 0x4524, 0x46e0, 0x4888, 0x4c20, 0x50a4, 0x4fa8, then 0x5264, 0x5304,
/// 0x5130), 0x55b4, 0x57f0, 0x5304 and 0x5264.
fn piece(rel: &crate::city_rel::CityRel<'_>, f: &PieceFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value})
    };
    let n = |v: u32| serde_json::json!(v);
    let k = |site: u32| boss_constant(rel, site);
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        PieceFrame::Spawn {
            parent,
            vector,
            kind,
            slot,
            task,
            block,
            target,
            random,
            ready,
        } => {
            returned = Some(0);
            if *parent != 0 {
                ev.push(call(
                    0x561c,
                    serde_json::json!([2, module_ref(rel, 0x5612, 0)?, parent]),
                )?);
                if *task != 0 {
                    ev.extend([
                        st(
                            0x563c,
                            "task",
                            n(0x14),
                            4,
                            serde_json::json!(module_ref(rel, 0x5636, 0)?),
                        ),
                        st(
                            0x5648,
                            "task",
                            n(0x18),
                            4,
                            serde_json::json!(module_ref(rel, 0x5642, 0)?),
                        ),
                        call(0x5658, serde_json::json!([1, 0xa4]))?,
                    ]);
                    if *block == 0 {
                        ev.push(call(0x566c, serde_json::json!([task]))?);
                    } else {
                        ev.push(st(0x5678, "task", n(0x40), 4, n(*block)));
                        for (i, site) in [0x5684u32, 0x5688, 0x5690].into_iter().enumerate() {
                            ev.push(st(site, "block", n(0x68 + 4 * i as u32), 4, n(vector[i])));
                        }
                        let start = {
                            let r = rel
                                .reference(1, 0x5696)
                                .filter(|r| r.module == 88 && r.section == 4)
                                .ok_or_else(|| Error::invalid("missing piece start"))?;
                            let d = rel.data(4)?;
                            (0..3)
                                .map(|i| {
                                    let at = r.addend as usize + 4 * i;
                                    d.get(at..at + 4)
                                        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
                                        .ok_or_else(|| Error::invalid("piece start out of range"))
                                })
                                .collect::<Result<Vec<_>>>()?
                        };
                        for (i, site) in [0x56a4u32, 0x56a8, 0x56b0].into_iter().enumerate() {
                            ev.push(st(site, "entity", n(0x14 + 4 * i as u32), 4, n(start[i])));
                        }
                        let one = n(k(0x56b6)?.to_bits());
                        for (site, offset) in [
                            (0x56c0u32, 0x40u32),
                            (0x56c4, 0x3c),
                            (0x56c8, 0x38),
                            (0x56d0, 0x4c),
                            (0x56d4, 0x48),
                            (0x56d8, 0x44),
                        ] {
                            ev.push(st(site, "block", n(offset), 4, one.clone()));
                        }
                        ev.extend([
                            st(0x56dc, "block", n(0), 1, n((*kind).into())),
                            st(0x56e0, "block", n(1), 1, n((*slot).into())),
                            local(0x56e8, 0x6488, serde_json::json!([parent])),
                            st(0x56ec, "block", n(0x10), 4, n(*target)),
                            st(0x56f4, "entity", n(0), 1, n(0)),
                            call(0x5704, serde_json::json!([]))?,
                        ]);
                        let base = {
                            let r = rel
                                .reference(1, 0x56fa)
                                .filter(|r| r.module == 88 && r.section == 5)
                                .ok_or_else(|| Error::invalid("missing piece delay base"))?;
                            let at = r.addend as usize;
                            let b = rel
                                .data(5)?
                                .get(at..at + 4)
                                .ok_or_else(|| Error::invalid("piece delay base out of range"))?;
                            i32::from_be_bytes(b.try_into().unwrap())
                        };
                        let unit = k(0x572e)? * biased_float(rel, 0x570a, *random)?;
                        let spread = biased_float(rel, 0x573e, base)? * unit;
                        let scale = k(0x5762)?;
                        let delay = fctiwz(f64::from(
                            scale * biased_float(rel, 0x5772, base)? + scale * spread,
                        ));
                        ev.push(st(0x57a0, "block", n(0xc), 4, n(delay as u32)));
                        ev.push(call(
                            0x57b8,
                            serde_json::json!([task, module_ref(rel, 0x57aa, 0)?, 1, 3]),
                        )?);
                        if *ready == 0 {
                            ev.push(call(0x57c8, serde_json::json!([task]))?);
                        } else {
                            returned = Some(*task);
                        }
                    }
                }
            }
        }
        PieceFrame::Update {
            task,
            pause,
            action,
        } => {
            if *pause == 0 {
                let target = match action {
                    0 => Some((0x5398, 0x4524)),
                    1 => Some((0x53a4, 0x46e0)),
                    2 => Some((0x53b0, 0x4888)),
                    3 => Some((0x53bc, 0x4c20)),
                    4 => Some((0x53c8, 0x50a4)),
                    5 => Some((0x53d4, 0x4fa8)),
                    _ => None,
                };
                if let Some((site, routine)) = target {
                    ev.push(local(site, routine, serde_json::json!([task])));
                }
                ev.extend([
                    local(0x53dc, 0x5264, serde_json::json!([task])),
                    local(0x53e4, 0x5304, serde_json::json!([task])),
                    local(0x53ec, 0x5130, serde_json::json!([task])),
                ]);
            }
        }
        PieceFrame::Delete { slot } => {
            ev.push(st(
                0x55d0,
                "module",
                serde_json::json!(module_ref(rel, 0x55c6, 4 * u32::from(*slot))?),
                4,
                n(0),
            ));
        }
        PieceFrame::Release => {
            ev.push(st(0x57f8, "entity", n(0), 1, n(2)));
            ev.push(st(0x5800, "entity", n(1), 1, n(0)));
        }
        PieceFrame::Position { position } => {
            for (i, site) in [0x5314u32, 0x5318, 0x5320].into_iter().enumerate() {
                ev.push(st(site, "block", n(0x98 + 4 * i as u32), 4, n(position[i])));
            }
        }
        PieceFrame::Distance {
            player,
            player_position,
            position,
            root,
        } => {
            if *player != 0 {
                let d = |i: usize| player_position[i] - position[i];
                let sum = d(2) * d(2) + (d(0) * d(0) + d(1) * d(1));
                ev.push(serde_json::json!({"site":0x52e8,"local":0x40cc,"args":[],"floats":[f64::from(sum)]}));
                ev.push(st(0x52ec, "block", n(0x18), 4, n(root.to_bits())));
            }
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the piece burst, explosion or contact routines; values are
/// as the code reads them.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum PieceActionFrame {
    /// 0x4fa8: piece action 5. `step` / `step_after` are entity byte 1,
    /// `timer` entity half-word 6, `fade` block float 0x28.
    Burst {
        task: u32,
        entity: u32,
        step: i8,
        step_after: u8,
        timer: u16,
        fade: f32,
    },
    /// 0x50a4: piece action 4. `owner` is task word 8, `action` the entity
    /// action.
    Explode {
        task: u32,
        entity: u32,
        owner: u32,
        action: i8,
    },
    /// 0x5130: contact check. `player` is the player entity word at
    /// 0x801e7368 + 4 * stage byte 0, `player_position` its floats 0x14 and
    /// 0x1c (x, z), `position` the piece's x and z, `root` the f1 result of
    /// local 0x40cc, `hit` the 0x800065a8 result, `kind` the 0x7154 result.
    Contact {
        task: u32,
        entity: u32,
        block: u32,
        player_position: [f32; 2],
        position: [f32; 2],
        root: f32,
        hit: u32,
        kind: i32,
    },
}
/// Sections 1 + 0x4fa8 (spark burst at 60 and 30 frames, then fade by
/// 4:0x254 and free below 4:0x258), 0x50a4 (explosion effect 0x800e26e0
/// with the vector 4:0x25c and size 4:0x268; the owner's timer ticks below
/// action 2) and 0x5130 (flat distance to the player; within 4:0x26c a
/// player hit either knocks the player back and stuns the boss or just
/// explodes the piece).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn piece_action(
    rel: &crate::city_rel::CityRel<'_>,
    f: &PieceActionFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let mut ev = Vec::new();
    match f {
        PieceActionFrame::Burst {
            task,
            entity,
            step,
            step_after,
            timer,
            fade,
        } => {
            let mut fading = true;
            if (0..=1).contains(step) {
                let mut timer = *timer;
                if *step == 0 {
                    ev.extend([
                        local(0x4ff8, 0x6d64, serde_json::json!([task, entity + 0x14, 0])),
                        st(0x5000, "entity", 6, 2, 0x3c),
                        st(0x500c, "entity", 1, 1, step_after.wrapping_add(1).into()),
                    ]);
                    timer = 0x3c;
                }
                if timer == 30 {
                    ev.push(local(
                        0x5028,
                        0x6d64,
                        serde_json::json!([task, entity + 0x14, 0]),
                    ));
                }
                ev.push(st(0x5034, "entity", 6, 2, timer.wrapping_sub(1).into()));
                if timer == 0 {
                    ev.push(call(0x5044, serde_json::json!([task]))?);
                    fading = false;
                }
            }
            if fading {
                let left = fade - k(0x5052)?;
                ev.push(st(0x5060, "block", 0x28, 4, left.to_bits()));
                if f64::from(left) < f64::from(k(0x506a)?) {
                    ev.push(call(0x5080, serde_json::json!([task]))?);
                }
            }
        }
        PieceActionFrame::Explode {
            task,
            entity,
            owner,
            action,
        } => {
            if *action <= 1 {
                ev.push(local(0x50f4, 0x6494, serde_json::json!([owner])));
            }
            let vector = {
                let r = rel
                    .reference(1, 0x50c2)
                    .filter(|r| r.module == 88 && r.section == 4)
                    .ok_or_else(|| Error::invalid("missing explosion vector"))?;
                let d = rel.data(4)?;
                (0..3)
                    .map(|i| {
                        let at = r.addend as usize + 4 * i;
                        d.get(at..at + 4)
                            .map(|b| f64::from(f32::from_be_bytes(b.try_into().unwrap())))
                            .ok_or_else(|| Error::invalid("explosion vector out of range"))
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let mut e = call(0x510c, serde_json::json!([entity + 0x14, "stack"]))?;
            e["floats"] = serde_json::json!([f64::from(k(0x5102)?)]);
            e["vectors"] = serde_json::json!({"1": vector});
            ev.push(e);
            ev.push(call(0x5114, serde_json::json!([task]))?);
        }
        PieceActionFrame::Contact {
            task,
            entity,
            block,
            player_position,
            position,
            root,
            hit,
            kind,
        } => {
            let dx = player_position[0] - position[0];
            let dz = player_position[1] - position[1];
            ev.push(serde_json::json!({"site":0x51a8,"local":0x40cc,"args":[],"floats":[f64::from(dx * dx + dz * dz)]}));
            ev.push(st(0x51ac, "block", 0x14, 4, root.to_bits()));
            if !(f64::from(*root) > f64::from(k(0x51b6)?)) {
                ev.push(call(0x51cc, serde_json::json!([task]))?);
                if *hit != 0 {
                    ev.push(local(0x51e0, 0x7154, serde_json::json!([task, hit])));
                    if *kind == 1 {
                        let mut knock = local(0x5214, 0x71e0, serde_json::json!([0]));
                        knock["floats"] = serde_json::json!([f64::from(k(0x520a)?)]);
                        ev.extend([
                            call(0x5200, serde_json::json!([0xa, 0, 0, 0x1e, entity + 0x14]))?,
                            knock,
                            local(0x521c, 0x50a4, serde_json::json!([task])),
                            local(0x5220, 0xc98, serde_json::json!([])),
                        ]);
                    } else {
                        ev.push(local(0x522c, 0x50a4, serde_json::json!([task])));
                    }
                }
                ev.push(call(0x523c, serde_json::json!([task, 0, block + 0x98]))?);
                ev.push(call(0x5244, serde_json::json!([task]))?);
            }
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the piece motion routines; values are as the code reads
/// them (engine and local results single precision).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum PieceMotionFrame {
    /// 0x4524: fly out to the slot position (block words 0x68..0x70,
    /// `target`). `random` is the 0x8010d940 result, `velocity` the
    /// vector local 0x6f10 leaves, `speed` the length 0x80114eac returns
    /// for block 0x2c, `velocity_now` block floats 0x2c..0x34 at that point,
    /// `reach` the lengths of the entity position and of the target.
    Fly {
        entity: u32,
        block: u32,
        step: i8,
        step_after: u8,
        random: i32,
        velocity: [f32; 3],
        speed: f32,
        velocity_now: [f32; 3],
        reach: [f32; 2],
        target: [u32; 3],
    },
    /// 0x46e0: wobble. `scale` is block float 0x38 (words 0x38..0x40 are
    /// copied first as `scale_words`), `spring` the f1 result of local
    /// 0x434c, `phase` block word 4, `sines` the three 0x80112050 results,
    /// `mode` block word 0x10, `spin` / `spin_step` block words 8 and 0xc.
    Wobble {
        block: u32,
        scale_words: [u32; 3],
        spring: f32,
        phase: i32,
        sines: [f32; 3],
        mode: i32,
        spin: i32,
        spin_step: i32,
    },
}
/// Sections 1 + 0x4524 and 0x46e0.
fn piece_motion(
    rel: &crate::city_rel::CityRel<'_>,
    f: &PieceMotionFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let mut ev = Vec::new();
    match f {
        PieceMotionFrame::Fly {
            entity,
            block,
            step,
            step_after,
            random,
            velocity,
            speed,
            velocity_now,
            reach,
            target,
        } => {
            if (0..=1).contains(step) {
                if *step == 0 {
                    let pace = k(0x45be)?
                        + k(0x45ae)? * (k(0x459e)? * biased_float(rel, 0x457a, *random)?);
                    let mut scale =
                        serde_json::json!({"site":0x45d4,"local":0x6f10,"args":["stack", "stack"]});
                    scale["floats"] = serde_json::json!([f64::from(pace)]);
                    ev.extend([
                        call(
                            0x4570,
                            serde_json::json!([block + 0x68, entity + 0x14, "stack"]),
                        )?,
                        call(0x4574, serde_json::json!([]))?,
                        scale,
                        st(0x45e0, "block", 0x2c, 4, velocity[0].to_bits()),
                        st(0x45e4, "block", 0x30, 4, velocity[1].to_bits()),
                        st(0x45ec, "block", 0x34, 4, velocity[2].to_bits()),
                        st(0x45f8, "entity", 1, 1, step_after.wrapping_add(1).into()),
                    ]);
                }
                ev.push(call(
                    0x4604,
                    serde_json::json!([entity + 0x14, block + 0x2c]),
                )?);
                ev.push(call(0x460c, serde_json::json!([block + 0x2c]))?);
                if f64::from(*speed) < f64::from(k(0x4612)?) {
                    let drag = k(0x462a)?;
                    for (i, site) in [0x4638u32, 0x4648, 0x4658].into_iter().enumerate() {
                        ev.push(st(
                            site,
                            "block",
                            0x2c + 4 * i as u32,
                            4,
                            (velocity_now[i] * drag).to_bits(),
                        ));
                    }
                }
                ev.push(call(0x4660, serde_json::json!([entity + 0x14]))?);
                ev.push(call(0x466c, serde_json::json!([block + 0x68]))?);
                if f64::from(reach[1]) <= f64::from(reach[0]) {
                    let rest = k(0x4696)?.to_bits();
                    ev.extend([
                        st(0x4680, "entity", 0x14, 4, target[0]),
                        st(0x4688, "entity", 0x18, 4, target[1]),
                        st(0x4690, "entity", 0x1c, 4, target[2]),
                        st(0x46a0, "block", 0x2c, 4, rest),
                        st(0x46a8, "block", 0x30, 4, rest),
                        st(0x46b0, "block", 0x34, 4, rest),
                        st(0x46b8, "entity", 0, 1, 1),
                        st(0x46c0, "entity", 1, 1, 0),
                    ]);
                }
            }
        }
        PieceMotionFrame::Wobble {
            block,
            scale_words,
            spring,
            phase,
            sines,
            mode,
            spin,
            spin_step,
        } => {
            let old = f32::from_bits(scale_words[0]);
            ev.extend([
                st(0x4704, "block", 0x44, 4, scale_words[0]),
                st(0x4708, "block", 0x48, 4, scale_words[1]),
                st(0x4710, "block", 0x4c, 4, scale_words[2]),
            ]);
            let mut e = serde_json::json!({"site":0x4720,"local":0x434c,"args":[block + 0x50, 0]});
            e["floats"] = serde_json::json!([f64::from(old)]);
            ev.extend([
                e,
                st(0x4724, "block", 0x38, 4, spring.to_bits()),
                st(0x472c, "block", 0x40, 4, spring.to_bits()),
                st(0x4730, "block", 0x3c, 4, spring.to_bits()),
            ]);
            let delta = k(0x475e)? + k(0x476a)? * (k(0x4776)? * (old - k(0x4786)?));
            let raw = fctiwz(f64::from(biased_float(rel, 0x473a, *phase)? + delta));
            ev.push(st(0x47b0, "block", 4, 4, raw as u32));
            let mut p = raw;
            if p >= 0x10000 {
                p = p.wrapping_sub(0x10000);
                ev.push(st(0x47c8, "block", 4, 4, p as u32));
            }
            let size = k(0x47d6)?;
            let base = k(0x47e6)?;
            ev.extend([
                call(0x47d0, serde_json::json!([p as u32]))?,
                st(0x47f4, "block", 0x44, 4, (base + size * sines[0]).to_bits()),
                call(0x4804, serde_json::json!([p.wrapping_add(0x8000) as u32]))?,
                st(0x4818, "block", 0x48, 4, (base + size * sines[1]).to_bits()),
                call(0x4820, serde_json::json!([p as u32]))?,
                st(0x4834, "block", 0x4c, 4, (base + size * sines[2]).to_bits()),
            ]);
            if *mode != 1 {
                let s = spin.wrapping_add(*spin_step);
                ev.push(st(0x4850, "block", 8, 4, s as u32));
                if s > 0x10000 {
                    ev.push(st(0x4868, "block", 8, 4, s.wrapping_sub(0x10000) as u32));
                }
            }
        }
    }
    Ok(serde_json::json!({"events":ev}))
}

/// What piece action 3 reads, each where the code reads it. `position` is
/// the entity position (words) at entry, `target` block words 0x68..0x70
/// (the aim point), `distance` / `reach` the f1 results of local 0x40cc
/// for the distance to the target and from the origin, `boss_states` the
/// two 0xc60 results, `timer` entity half-word 6, `player_position` the
/// player entity's words 0x14..0x1c (through 0x801e7368 + 4 * stage byte),
/// `speed` block float 0x24, `curve` block words 0x74..0x94, `curve_point`
/// what 0x80111d70 writes, `lengths` the two 0x80114eac results (velocity
/// then gap), `stepped` what 0x80027ab4 writes, `growth` block floats 0x1c
/// and 0x20.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceHomingFrame {
    pub task: u32,
    pub entity: u32,
    pub block: u32,
    pub position: [u32; 3],
    pub target: [u32; 3],
    pub distance: f32,
    pub reach: f32,
    pub boss_states: [i8; 2],
    pub timer: u16,
    pub player_position: [u32; 3],
    pub speed: f32,
    pub curve: [u32; 9],
    pub curve_point: [u32; 3],
    pub lengths: [f32; 2],
    pub stepped: [u32; 3],
    pub growth: [f32; 2],
}
/// Section 1 + 0x4c20: the homing shot. It hums, explodes on reaching the
/// target (4:0x218), when too far out (4:0x240) or, while the boss is in
/// state 1, past the plane (4:0x244). Far from the target and while the
/// curve parameter is below 4:0x228 it follows the curve 0x80111d70 through
/// the block points to a target refreshed from the player each frame of
/// the timer; otherwise it steps straight at the target. The curve speed
/// grows, and the velocity and trail (0x6794) follow.
fn piece_homing(
    rel: &crate::city_rel::CityRel<'_>,
    f: &PieceHomingFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":4,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let fl = |w: u32| f32::from_bits(w);
    let words = |w: &[u32]| w.iter().map(|v| f64::from(fl(*v))).collect::<Vec<_>>();
    let e = f.entity;
    let mut ev = vec![call(
        0x4c60,
        serde_json::json!([0xb, f.task, 1, 0, 5, e + 0x14]),
    )?];
    let p = f.position.map(fl);
    let t = f.target.map(fl);
    let d = |i: usize| t[i] - p[i];
    let mut root = local(0x4cb4, 0x40cc, serde_json::json!([]));
    root["floats"] = serde_json::json!([f64::from(d(2) * d(2) + (d(0) * d(0) + d(1) * d(1)))]);
    ev.push(root);
    if f64::from(f.distance) < f64::from(k(0x4cbe)?) {
        ev.extend([
            call(0x4ce4, serde_json::json!([0xa, 0, 0, 0x1e, e + 0x14]))?,
            local(0x4cec, 0x50a4, serde_json::json!([f.task])),
            call(0x4d08, serde_json::json!([0x8015, 0, 0, 0, e + 0x14]))?,
        ]);
        return Ok(serde_json::json!({"events":ev}));
    }
    let mut out = local(0x4d30, 0x40cc, serde_json::json!([]));
    out["floats"] = serde_json::json!([f64::from(p[2] * p[2] + (p[0] * p[0] + p[1] * p[1]))]);
    ev.push(out);
    if f64::from(f.reach) > f64::from(k(0x4d36)?) {
        ev.push(local(0x4d4c, 0x50a4, serde_json::json!([f.task])));
        return Ok(serde_json::json!({"events":ev}));
    }
    ev.push(local(0x4d54, 0xc60, serde_json::json!([])));
    if f.boss_states[0] == 1 {
        let mut abs = local(0x4d68, 0x2130, serde_json::json!([]));
        abs["floats"] = serde_json::json!([f64::from(p[2])]);
        ev.push(abs);
        if f64::from(p[2].abs()) > f64::from(k(0x4d6e)?) {
            ev.push(local(0x4d84, 0x50a4, serde_json::json!([f.task])));
            return Ok(serde_json::json!({"events":ev}));
        }
    }
    let curved = f64::from(f.distance) > f64::from(k(0x4d8e)?)
        && f64::from(f.speed) <= f64::from(k(0x4da6)?);
    if curved {
        let mut aim = f.target;
        if f.timer != 0 {
            ev.push(local(0x4dc8, 0xc60, serde_json::json!([])));
            if f.boss_states[1] == 0 {
                aim = f.player_position;
                for (i, site) in [0x4e08u32, 0x4e0c, 0x4e14].into_iter().enumerate() {
                    ev.push(st(site, "block", 0x68 + 4 * i as u32, aim[i]));
                }
            }
            ev.push(serde_json::json!({"site":0x4e20,"store":"entity","offset":6,"size":2,"value":f.timer - 1}));
        }
        let mut points: Vec<u32> = f.curve.to_vec();
        points.extend(aim);
        let mut c = call(0x4e94, serde_json::json!(["stack", "stack", 0]))?;
        c["floats"] = serde_json::json!([f64::from(f.speed)]);
        c["block"] = serde_json::json!(points);
        ev.push(c);
        for (i, site) in [0x4ea0u32, 0x4ea4, 0x4eac].into_iter().enumerate() {
            ev.push(st(site, "entity", 0x14 + 4 * i as u32, f.curve_point[i]));
        }
    } else {
        let step = if f64::from(f.lengths[1]) < f64::from(f.lengths[0]) {
            f.lengths[1]
        } else {
            f.lengths[0]
        };
        let mut scale = local(0x4ef0, 0x6f10, serde_json::json!(["stack", "stack"]));
        scale["floats"] = serde_json::json!([f64::from(step)]);
        ev.extend([
            call(
                0x4ec0,
                serde_json::json!([f.block + 0x68, e + 0x14, "stack"]),
            )?,
            call(0x4ec8, serde_json::json!([f.block + 0x2c]))?,
            call(0x4ed4, serde_json::json!(["stack"]))?,
            scale,
            call(0x4f00, serde_json::json!([e + 0x14, "stack", "stack"]))?,
        ]);
        for (i, site) in [0x4f0cu32, 0x4f10, 0x4f18].into_iter().enumerate() {
            ev.push(st(site, "entity", 0x14 + 4 * i as u32, f.stepped[i]));
        }
    }
    let first = f.speed + f.growth[0];
    let speed = first + f.growth[1];
    ev.push(st(0x4f28, "block", 0x24, first.to_bits()));
    ev.push(st(0x4f38, "block", 0x24, speed.to_bits()));
    if f64::from(f.growth[1]) < f64::from(k(0x4f42)?) {
        ev.push(st(
            0x4f64,
            "block",
            0x20,
            (f.growth[1] + k(0x4f56)?).to_bits(),
        ));
    }
    let mut trail = local(0x4f84, 0x6794, serde_json::json!([0, "stack", e + 0x14]));
    trail["vectors"] = serde_json::json!({"1": words(&f.position)});
    ev.push(call(
        0x4f74,
        serde_json::json!([e + 0x14, "stack", f.block + 0x2c]),
    )?);
    ev.push(trail);
    Ok(serde_json::json!({"events":ev}))
}

fn rel_word(rel: &crate::city_rel::CityRel<'_>, site: u32, section: u8) -> Result<u32> {
    let r = rel
        .reference(1, site)
        .filter(|r| r.module == 88 && r.section == section)
        .ok_or_else(|| Error::invalid("missing module data reference"))?;
    let at = r.addend as usize;
    let b = rel
        .data(section)?
        .get(at..at + 4)
        .ok_or_else(|| Error::invalid("module data out of range"))?;
    Ok(u32::from_be_bytes(b.try_into().unwrap()))
}

/// What piece action 2 reads, each where the code reads it: entity byte 1
/// (the step), the collision object (entity word 0x2c) and its flags
/// half-word 4, block word 8 (the bob angle) and its 0x80112050 sine, the
/// runtime float 6:0x270, the entity height (float 0x18) and position
/// (words), entity half-word 6 (the timer), the player position words
/// (through 0x801e7368 + 4 * stage byte), the two random words, the
/// rotated aim vector 0x801151a4 leaves, the 0x800279c4 distance and the
/// two square roots local 0x40cc returns.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceLaunchFrame {
    pub task: u32,
    pub entity: u32,
    pub block: u32,
    pub step: i8,
    pub step_after: u8,
    pub collision_flags: u16,
    pub bob_angle: u32,
    pub sine: f32,
    pub bob: f32,
    pub height: f32,
    pub timer: u16,
    pub player_position: [u32; 3],
    pub position: [u32; 3],
    pub randoms: [i32; 2],
    pub rotated: [f32; 3],
    pub distance: f32,
    pub roots: [f32; 2],
}
/// Section 1 + 0x4888: piece action 2. Step 0 settles the piece (30-frame
/// timer, collision flag 0x40 cleared, height bobbed by the runtime float
/// 6:0x270, spark, sound). When the timer runs out it aims at the player:
/// the aim vector is turned by a random angle (4:0x230 + 4:0x230 * roll,
/// sign from a second roll's parity), two curve handles are placed at
/// 4:0x234 and 4:0x238 of it, the curve speed is set from the distance
/// (5:0x3fa0 / distance) and the piece goes to action 3 with the timer
/// from 5:0x3fec.
fn piece_launch(
    rel: &crate::city_rel::CityRel<'_>,
    f: &PieceLaunchFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32, region: &str, offset: u32, size: u32, value: u32| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let k = |site: u32| boss_constant(rel, site);
    let e = f.entity;
    let b = f.block;
    let mut ev = Vec::new();
    if !(0..=1).contains(&f.step) {
        return Ok(serde_json::json!({"events":ev}));
    }
    let mut timer = f.timer;
    if f.step == 0 {
        ev.extend([
            st(0x48d4, "entity", 6, 2, 0x1e),
            st(
                0x48e8,
                "collision",
                4,
                2,
                u32::from(f.collision_flags & !0x40),
            ),
            call(0x48f0, serde_json::json!([f.bob_angle]))?,
            st(
                0x490c,
                "entity",
                0x18,
                4,
                (f.height + f.bob * f.sine).to_bits(),
            ),
            st(0x4914, "block", 8, 4, 0),
            local(0x4924, 0x6d64, serde_json::json!([f.task, e + 0x14, 0])),
            call(0x493c, serde_json::json!([0xc, 0, 0, 0, e + 0x14]))?,
            st(0x4948, "entity", 1, 1, f.step_after.wrapping_add(1).into()),
        ]);
        timer = 0x1e;
    }
    ev.push(st(0x4954, "entity", 6, 2, timer.wrapping_sub(1).into()));
    if timer != 0 {
        return Ok(serde_json::json!({"events":ev}));
    }
    for (i, site) in [0x4990u32, 0x4994, 0x499c].into_iter().enumerate() {
        ev.push(st(
            site,
            "block",
            0x68 + 4 * i as u32,
            4,
            f.player_position[i],
        ));
    }
    for (i, site) in [0x49a8u32, 0x49ac, 0x49b4].into_iter().enumerate() {
        ev.push(st(site, "block", 0x74 + 4 * i as u32, 4, f.position[i]));
    }
    let unit = k(0x49f2)?;
    let span = k(0x4a02)?;
    let mut angle = fctiwz(f64::from(
        span + span * (unit * biased_float(rel, 0x49ce, f.randoms[0])?),
    ));
    let parity = fctiwz(f64::from(
        k(0x4a4e)? * (unit * biased_float(rel, 0x4a26, f.randoms[1])?),
    )) as i8 as i32;
    if parity % 2 == 0 {
        angle = angle.wrapping_neg();
    }
    ev.extend([
        call(0x49c4, serde_json::json!([b + 0x68, e + 0x14, "stack"]))?,
        call(0x49c8, serde_json::json!([]))?,
        call(0x4a20, serde_json::json!([]))?,
        call(0x4a88, serde_json::json!([]))?,
        call(0x4a94, serde_json::json!([0, angle as u32]))?,
        call(0x4aa4, serde_json::json!([0, "stack", "stack"]))?,
        call(0x4aa8, serde_json::json!([]))?,
    ]);
    for (site, scale_site, out) in [(0x4b08u32, 0x4acau32, 0x80u32), (0x4b68, 0x4b2a, 0x8c)] {
        let s = k(scale_site)?;
        let v: Vec<f64> = f.rotated.iter().map(|c| f64::from(c * s)).collect();
        let mut c = call(site, serde_json::json!([e + 0x14, "stack", b + out]))?;
        c["vectors"] = serde_json::json!({"1": v});
        ev.push(c);
    }
    ev.push(call(0x4b74, serde_json::json!([e + 0x14, b + 0x68]))?);
    for site in [0x4b80u32, 0x4b98] {
        let mut r = local(site, 0x40cc, serde_json::json!([]));
        r["floats"] = serde_json::json!([f64::from(f.distance)]);
        ev.push(r);
    }
    let one = k(0x4b86)?;
    let pace =
        f32::from_bits(rel_word(rel, 0x4baa, 5)?) * ((one / f.roots[1]) * (one / f.roots[0]));
    ev.extend([
        st(0x4bb8, "block", 0x1c, 4, pace.to_bits()),
        st(0x4bc8, "block", 0x24, 4, k(0x4bbe)?.to_bits()),
        st(0x4bd8, "block", 0x20, 4, k(0x4bce)?.to_bits()),
        st(0x4bec, "entity", 6, 2, rel_word(rel, 0x4bde, 5)? & 0xffff),
        st(0x4bf4, "entity", 0, 1, 3),
        st(0x4bfc, "entity", 1, 1, 0),
    ]);
    Ok(serde_json::json!({"events":ev}))
}

/// One call of the utility routines; values are as the code reads them.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum BossUtilityFrame {
    /// 0x6d64: spawn a spark of `kind` for `parent` at the vector `position`
    /// points at (`position_words`); `task` is the engine task.
    SparkSpawn {
        parent: u32,
        position: u32,
        position_words: [u32; 3],
        kind: i8,
        task: u32,
    },
    /// 0x6e58: load the four boss texture lists.
    LoadTextures,
    /// 0x6ec4: release them.
    ReleaseTextures,
    /// 0x6f10: scale `source` to `length` into the target vector. `magnitude` and
    /// `normal` are the 0x80114eac and 0x801159dc results.
    ScaleTo {
        source: [u32; 3],
        length: f32,
        magnitude: f32,
        normal: [f32; 3],
    },
    /// 0x7008: step the LCG at 6:0x478 (`seed`) and reseed the engine.
    Reseed { seed: u32 },
    /// 0x705c: yaw from the entity to `point` (x, z); `angle` is the
    /// 0x80110e20 result.
    Yaw {
        point: [f32; 2],
        position: [f32; 2],
        angle: f64,
    },
    /// 0x70b8: pitch from the entity to `point` plus `offset`; `root` is the
    /// f1 result of local 0x40cc, `angle` the 0x80110e20 result.
    Pitch {
        point: [f32; 3],
        position: [f32; 3],
        offset: i32,
        root: f32,
        angle: f64,
    },
}
/// Sections 1 + 0x6d64, 0x6e58, 0x6ec4, 0x6f10, 0x7008, 0x705c, 0x70b8.
fn boss_utility(
    rel: &crate::city_rel::CityRel<'_>,
    f: &BossUtilityFrame,
) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value})
    };
    let n = |v: u32| serde_json::json!(v);
    let m = |site: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!(module_ref(rel, site, 0)?))
    };
    let k = |site: u32| boss_constant(rel, site);
    let kd = |site: u32| -> Result<f64> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing angle scale"))?;
        let at = r.addend as usize;
        let b = rel
            .data(4)?
            .get(at..at + 8)
            .ok_or_else(|| Error::invalid("angle scale out of range"))?;
        Ok(f64::from_bits(u64::from_be_bytes(b.try_into().unwrap())))
    };
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        BossUtilityFrame::SparkSpawn {
            parent,
            position,
            position_words,
            kind,
            task,
        } => {
            returned = Some(0);
            if *parent != 0 {
                ev.push(call(
                    0x6db8,
                    serde_json::json!([2, module_ref(rel, 0x6dae, 0)?, parent]),
                )?);
                if *task != 0 {
                    let table = rel
                        .reference(1, 0x6d8e)
                        .filter(|r| r.module == 88 && r.section == 5)
                        .ok_or_else(|| Error::invalid("missing spark table"))?
                        .addend
                        .wrapping_add((28 * i32::from(*kind)) as u32)
                        as usize;
                    let d = rel.data(5)?;
                    let half = |o: usize| -> Result<u16> {
                        d.get(table + o..table + o + 2)
                            .map(|b| u16::from_be_bytes([b[0], b[1]]))
                            .ok_or_else(|| Error::invalid("spark table out of range"))
                    };
                    ev.extend([
                        st(0x6dd8, "task", n(0x14), 4, m(0x6dd2)?),
                        st(0x6de4, "task", n(0x18), 4, m(0x6dde)?),
                        st(0x6df4, "entity", n(0x14), 4, n(position_words[0])),
                        st(0x6df8, "entity", n(0x18), 4, n(position_words[1])),
                        st(0x6e00, "entity", n(0x1c), 4, n(position_words[2])),
                        st(0x6e08, "entity", n(6), 2, n(half(0)?.into())),
                        st(0x6e0c, "entity", n(3), 1, n(*kind as u8 as u32)),
                        st(0x6e10, "entity", n(8), 4, n(*position)),
                        st(0x6e20, "entity", n(0x28), 4, n(k(0x6e16)?.to_bits())),
                        serde_json::json!({"site":0x6e38,"local":0x6a54,
                            "args":[parent, position, half(4)? as u8 as i8 as i32 as u32, *kind as i32 as u32]}),
                    ]);
                    returned = Some(*task);
                }
            }
        }
        BossUtilityFrame::LoadTextures => {
            for (site, a, b) in [
                (0x6e74u32, 0x6e66u32, 0x6e6eu32),
                (0x6e88, 0x6e7a, 0x6e82),
                (0x6e9c, 0x6e8e, 0x6e96),
                (0x6eb0, 0x6ea2, 0x6eaa),
            ] {
                ev.push(call(
                    site,
                    serde_json::json!([module_ref(rel, a, 0)?, module_ref(rel, b, 0)?]),
                )?);
            }
        }
        BossUtilityFrame::ReleaseTextures => {
            for (site, a) in [
                (0x6ed8u32, 0x6ed2u32),
                (0x6ee4, 0x6ede),
                (0x6ef0, 0x6eea),
                (0x6efc, 0x6ef6),
            ] {
                ev.push(call(site, serde_json::json!([module_ref(rel, a, 0)?]))?);
            }
        }
        BossUtilityFrame::ScaleTo {
            source,
            length,
            magnitude,
            normal,
        } => {
            let zero = f64::from(k(0x6f4a)?);
            let mut out = *source;
            if f64::from(*length) != zero {
                ev.push(call(0x6f7c, serde_json::json!(["stack"]))?);
                if f64::from(*magnitude) != zero {
                    ev.push(call(0x6fac, serde_json::json!(["stack"]))?);
                    out = normal.map(|c| (c * length).to_bits());
                }
            }
            let (a, b, c) = if f64::from(*length) == zero {
                (0x6f64u32, 0x6f68u32, 0x6f70u32)
            } else if f64::from(*magnitude) == zero {
                (0x6f94, 0x6f98, 0x6fa0)
            } else {
                (0x6fdc, 0x6fe0, 0x6fe8)
            };
            for (i, site) in [a, b, c].into_iter().enumerate() {
                ev.push(st(site, "target", n(4 * i as u32), 4, n(out[i])));
            }
        }
        BossUtilityFrame::Reseed { seed } => {
            let next = seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
            ev.push(call(0x7038, serde_json::json!([next]))?);
            ev.push(st(0x7040, "module", m(0x701e)?, 4, n(next)));
        }
        BossUtilityFrame::Yaw {
            point,
            position,
            angle,
        } => {
            let mut e = call(0x7088, serde_json::json!([]))?;
            e["floats"] = serde_json::json!([
                f64::from(point[0] - position[0]),
                f64::from(point[1] - position[1])
            ]);
            ev.push(e);
            returned = Some(fctiwz(kd(0x708e)? * angle) as u32);
        }
        BossUtilityFrame::Pitch {
            point,
            position,
            offset,
            root,
            angle,
        } => {
            let dx = point[0] - position[0];
            let dz = point[2] - position[2];
            ev.push(serde_json::json!({"site":0x7104,"local":0x40cc,"args":[],"floats":[f64::from(dx * dx + dz * dz)]}));
            let mut e = call(0x7114, serde_json::json!([]))?;
            e["floats"] = serde_json::json!([f64::from(*root), f64::from(point[1] - position[1])]);
            ev.push(e);
            returned = Some(fctiwz(kd(0x711a)? * angle).wrapping_add(*offset) as u32);
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the glow spark object; values are as the code reads them.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum GlowFrame {
    /// 0x6a54: spawn a glow of `kind` that lives `frames` for `parent`;
    /// `task` is the engine task, `owner` its word 8.
    Spawn {
        parent: u32,
        frames: i8,
        kind: i8,
        task: u32,
        entity: u32,
        owner: u32,
    },
    /// 0x6b48: update. `pause` / `freeze` are global bytes 0x801cbd88 +
    /// 0x37 / + 9, `owner` task word 8, `kind` entity byte 3, `fade` entity
    /// float 0x28, `timer` entity half-word 6.
    Update {
        task: u32,
        entity: u32,
        owner: u32,
        pause: i8,
        freeze: i8,
        kind: u8,
        fade: f32,
        timer: u16,
    },
    /// 0x6c28: draw (blinking by the kind's period). `byte` is the engine
    /// byte at 0x803ad526.
    Draw {
        entity: u32,
        kind: u8,
        timer: u16,
        fade: f32,
        byte: i8,
    },
}
/// Unsigned word to single through the 0x43300000 bias and the magic
/// double the code loads at `site`.
fn unsigned_float(rel: &crate::city_rel::CityRel<'_>, site: u32, value: u32) -> Result<f32> {
    let r = rel
        .reference(1, site)
        .filter(|r| r.module == 88 && r.section == 4)
        .ok_or_else(|| Error::invalid("missing conversion constant"))?;
    let at = r.addend as usize;
    let b = rel
        .data(4)?
        .get(at..at + 8)
        .ok_or_else(|| Error::invalid("conversion constant out of range"))?;
    let magic = f64::from_bits(u64::from_be_bytes(b.try_into().unwrap()));
    Ok((f64::from_bits(0x4330_0000_0000_0000 | u64::from(value)) - magic) as f32)
}
/// Sections 1 + 0x6a54 (spawn: update 0x6894, draw 0x6968, table
/// 5:0x40f4), 0x6b48 and 0x6c28.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn glow(rel: &crate::city_rel::CityRel<'_>, f: &GlowFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let fcall = |site: u32, floats: Vec<f64>| -> Result<serde_json::Value> {
        let mut e = call(site, serde_json::json!([]))?;
        e["floats"] = serde_json::json!(floats);
        Ok(e)
    };
    let st = |site: u32, region: &str, offset: u32, size: u32, value: serde_json::Value| serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value});
    let n = |v: u32| serde_json::json!(v);
    let k = |site: u32| boss_constant(rel, site);
    let table = |site: u32, kind: i32, offset: usize, len: usize| -> Result<Vec<u8>> {
        let base = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 5)
            .ok_or_else(|| Error::invalid("missing spark table"))?
            .addend
            .wrapping_add((28 * kind) as u32) as usize
            + offset;
        rel.data(5)?
            .get(base..base + len)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Error::invalid("spark table out of range"))
    };
    let float_at = |site: u32, kind: i32, offset: usize| -> Result<f32> {
        Ok(f32::from_be_bytes(
            table(site, kind, offset, 4)?.try_into().unwrap(),
        ))
    };
    let mut ev = Vec::new();
    let mut returned = None;
    match f {
        GlowFrame::Spawn {
            parent,
            frames,
            kind,
            task,
            entity,
            owner,
        } => {
            returned = Some(0);
            if *frames > 0 && *parent != 0 {
                ev.push(call(
                    0x6ab0,
                    serde_json::json!([2, module_ref(rel, 0x6aa6, 0)?, parent]),
                )?);
                if *task != 0 {
                    ev.extend([
                        st(0x6ad0, "task", 0x14, 4, serde_json::json!(module_ref(rel, 0x6aca, 0)?)),
                        st(0x6adc, "task", 0x18, 4, serde_json::json!(module_ref(rel, 0x6ad6, 0)?)),
                        serde_json::json!({"site":0x6aec,"local":0x5888,"args":[owner, entity + 0x14]}),
                        st(0x6b08, "entity", 0x20, 4, n(float_at(0x6af2, i32::from(*kind), 8)?.to_bits())),
                        st(0x6b18, "entity", 0x28, 4, n(k(0x6b0e)?.to_bits())),
                        st(0x6b1c, "entity", 1, 1, n(*frames as u8 as u32)),
                        st(0x6b20, "entity", 3, 1, n(*kind as u8 as u32)),
                    ]);
                    returned = Some(*task);
                }
            }
        }
        GlowFrame::Update {
            task,
            entity,
            owner,
            pause,
            freeze,
            kind,
            fade,
            timer,
        } => {
            if *pause == 0 && *freeze == 0 {
                ev.push(
                    serde_json::json!({"site":0x6b94,"local":0x5888,"args":[owner, entity + 0x14]}),
                );
                let left = fade - float_at(0x6b9e, i32::from(*kind), 0x18)?;
                ev.push(st(0x6bb8, "entity", 0x28, 4, n(left.to_bits())));
                if f64::from(left) <= f64::from(k(0x6bc2)?) {
                    ev.push(call(0x6bdc, serde_json::json!([task]))?);
                } else if *timer < 0xffff {
                    let t = timer - 1;
                    ev.push(st(0x6bf4, "entity", 6, 2, n(t.into())));
                    if t == 0 {
                        ev.push(call(0x6c08, serde_json::json!([task]))?);
                    }
                }
            }
        }
        GlowFrame::Draw {
            entity,
            kind,
            timer,
            fade,
            byte,
        } => {
            let period =
                u16::from_be_bytes(table(0x6c3e, i32::from(*kind), 2, 2)?.try_into().unwrap());
            if period == 0 {
                return Err(Error::invalid(
                    "a zero blink period divides by zero in the native code",
                ));
            }
            let phase = i32::from(*timer) % i32::from(period);
            let shown = biased_float(rel, 0x6c66, phase)?;
            let limit = k(0x6c8a)? * unsigned_float(rel, 0x6c96, u32::from(period))?;
            if !(f64::from(shown) >= f64::from(limit)) {
                let a = f64::from(*fade);
                let w = f64::from(k(0x6d2a)?);
                ev.extend([
                    call(0x6cc0, serde_json::json!([]))?,
                    call(0x6cc4, serde_json::json!([]))?,
                    call(0x6ccc, serde_json::json!([0x820]))?,
                    call(0x6cd8, serde_json::json!([0, 0x800]))?,
                    call(0x6ce0, serde_json::json!([9]))?,
                    call(0x6ce4, serde_json::json!([]))?,
                    call(0x6cec, serde_json::json!([entity + 0x14]))?,
                    fcall(0x6d00, vec![a; 4])?,
                    call(0x6d0c, serde_json::json!([module_ref(rel, 0x6d06, 0)?]))?,
                    call(0x6d10, serde_json::json!([]))?,
                    call(0x6d24, serde_json::json!([*byte as i32 as u32]))?,
                    fcall(0x6d40, vec![w; 4])?,
                    call(0x6d44, serde_json::json!([]))?,
                    call(0x6d48, serde_json::json!([]))?,
                ]);
            }
        }
    }
    let mut out = serde_json::json!({"events":ev});
    if let Some(r) = returned {
        out["returned"] = serde_json::json!(r);
    }
    Ok(out)
}

/// One call of the trail and glow chain routines; values are as the code
/// reads them, engine float results in single precision unless noted.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, tag = "routine", rename_all = "snake_case")]
pub enum TrailFrame {
    /// 0x6508: direction `vector` to rotation angles at `out`. `length` is
    /// the 0x80114eac result, `yaw` / `pitch` the 0x80110e20 and
    /// 0x80110d10 results (double), `roots` the two local 0x40cc results.
    Angles {
        vector: [f32; 3],
        out: u32,
        length: f32,
        yaw: f64,
        roots: [f32; 2],
        pitch: f64,
    },
    /// 0x6604: trail draw. `player` is the word at 0x80174f98, `distance`
    /// the 0x800279c4 result, `frame` entity half-word 6, `rotation` entity
    /// words 8 and 0xc, `length` entity float 0x20.
    Draw {
        player: u32,
        entity: u32,
        distance: f32,
        frame: u16,
        rotation: [u32; 2],
        length: f32,
    },
    /// 0x6728: trail update (frees itself after 12 frames).
    Update {
        task: u32,
        pause: i8,
        freeze: i8,
        frame: u16,
    },
    /// 0x6794: spawn a trail for `parent` from the point `from` points at
    /// (`from_words`) toward `toward`; `task` / `entity` are the engine task
    /// and entity, `length` the 0x80114eac result.
    Spawn {
        parent: u32,
        from: u32,
        from_words: [u32; 3],
        toward: [u32; 3],
        task: u32,
        entity: u32,
        length: f32,
    },
    /// 0x6894: glow chain update. `kind` / `count` are entity bytes 3 and
    /// 1, `size` / `fade` entity floats 0x20 / 0x28, `owner` task word 8,
    /// `anchor` entity word 8.
    Chain {
        task: u32,
        pause: i8,
        freeze: i8,
        kind: u8,
        count: u8,
        size: f32,
        fade: f32,
        owner: u32,
        anchor: u32,
    },
    /// 0x6968: glow draw. `fade` / `size` are entity floats 0x28 / 0x20,
    /// `position` entity words 0x14..0x1c.
    GlowDraw {
        fade: f32,
        size: f32,
        position: [u32; 3],
    },
}
/// Sections 1 + 0x6508, 0x6604, 0x6728, 0x6794 (update 0x6728, draw
/// 0x6604, deleter 0x6790), 0x6894 and 0x6968.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn trail(rel: &crate::city_rel::CityRel<'_>, f: &TrailFrame) -> Result<serde_json::Value> {
    let call = |site: u32, args: serde_json::Value| -> Result<serde_json::Value> {
        Ok(serde_json::json!({"site":site,"call":engine_import(rel, site)?,"args":args}))
    };
    let fcall =
        |site: u32, args: serde_json::Value, floats: Vec<f64>| -> Result<serde_json::Value> {
            let mut e = call(site, args)?;
            e["floats"] = serde_json::json!(floats);
            Ok(e)
        };
    let local = |site: u32, target: u32, args: serde_json::Value| serde_json::json!({"site":site,"local":target,"args":args});
    let st = |site: u32,
              region: &str,
              offset: serde_json::Value,
              size: u32,
              value: serde_json::Value| {
        serde_json::json!({"site":site,"store":region,"offset":offset,"size":size,"value":value})
    };
    let n = |v: u32| serde_json::json!(v);
    let m = |site: u32, o: u32| -> Result<serde_json::Value> {
        Ok(serde_json::json!(module_ref(rel, site, o)?))
    };
    let k = |site: u32| boss_constant(rel, site);
    let kd = |site: u32| -> Result<f64> {
        let r = rel
            .reference(1, site)
            .filter(|r| r.module == 88 && r.section == 4)
            .ok_or_else(|| Error::invalid("missing angle scale"))?;
        let at = r.addend as usize;
        let b = rel
            .data(4)?
            .get(at..at + 8)
            .ok_or_else(|| Error::invalid("angle scale out of range"))?;
        Ok(f64::from_bits(u64::from_be_bytes(b.try_into().unwrap())))
    };
    let mut ev = Vec::new();
    match f {
        TrailFrame::Angles {
            vector,
            out,
            length,
            yaw,
            roots,
            pitch,
        } => {
            ev.push(call(0x6538, serde_json::json!([*out]))?);
            if f64::from(*length) != f64::from(k(0x6542)?) {
                let scale = kd(0x6562)?;
                let one = k(0x658a)?;
                let sine = -vector[1] * ((one / roots[1]) * (one / roots[0]));
                ev.extend([
                    fcall(0x655c, serde_json::json!([]), vec![f64::from(vector[0]), f64::from(vector[2])])?,
                    st(0x657c, "out", n(4), 4, n(fctiwz(scale * yaw) as u32)),
                    serde_json::json!({"site":0x6584,"local":0x40cc,"args":[],"floats":[f64::from(*length)]}),
                    serde_json::json!({"site":0x659c,"local":0x40cc,"args":[],"floats":[f64::from(*length)]}),
                    fcall(0x65b8, serde_json::json!([]), vec![f64::from(sine)])?,
                    st(0x65d0, "out", n(0), 4, n(fctiwz(scale * pitch) as u32)),
                    st(0x65d8, "out", n(8), 4, n(0)),
                ]);
            }
        }
        TrailFrame::Draw {
            player,
            entity,
            distance,
            frame,
            rotation,
            length,
        } => {
            ev.push(call(0x662c, serde_json::json!([player, entity + 0x14]))?);
            if !(f64::from(*distance) > f64::from(k(0x6632)?)) {
                let index = i32::from(*frame as u8 as i8);
                let table = rel
                    .reference(1, 0x6652)
                    .filter(|r| r.module == 88 && r.section == 5)
                    .ok_or_else(|| Error::invalid("missing trail colour table"))?
                    .addend
                    .wrapping_add((4 * index) as u32) as usize;
                let d = rel.data(5)?;
                let w = |o: usize| -> Result<u32> {
                    d.get(table + o..table + o + 4)
                        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
                        .ok_or_else(|| Error::invalid("trail colour table out of range"))
                };
                let (a, b) = (w(0)?, w(4)?);
                for (site, o, v) in [
                    (0x6668u32, 0x6c, a),
                    (0x666c, 0x4c, a),
                    (0x6670, 0x2c, a),
                    (0x6674, 0xc, a),
                    (0x667c, 0x7c, b),
                    (0x6680, 0x5c, b),
                    (0x6684, 0x3c, b),
                    (0x6688, 0x1c, b),
                ] {
                    ev.push(st(site, "module", m(0x6662, o)?, 4, n(v)));
                }
                let w21c = f64::from(k(0x66ce)?);
                ev.extend([
                    call(0x6694, serde_json::json!([0, 8]))?,
                    call(0x66a0, serde_json::json!([1, 0xa]))?,
                    call(0x66a4, serde_json::json!([]))?,
                    call(0x66ac, serde_json::json!([entity + 0x14]))?,
                    call(0x66b8, serde_json::json!([0, rotation[1]]))?,
                    call(0x66c4, serde_json::json!([0, rotation[0]]))?,
                    fcall(
                        0x66e0,
                        serde_json::json!([0]),
                        vec![w21c, w21c, f64::from(*length)],
                    )?,
                    call(
                        0x66f0,
                        serde_json::json!([module_ref(rel, 0x6662, 0)?, 8, 1]),
                    )?,
                    call(0x66f4, serde_json::json!([]))?,
                    call(0x6700, serde_json::json!([0, 8]))?,
                    call(0x670c, serde_json::json!([1, 6]))?,
                ]);
            }
        }
        TrailFrame::Update {
            task,
            pause,
            freeze,
            frame,
        } => {
            if *pause == 0 && *freeze == 0 {
                let t = frame.wrapping_add(1);
                ev.push(st(0x6768, "entity", n(6), 2, n(t.into())));
                if t >= 12 {
                    ev.push(call(0x6778, serde_json::json!([task]))?);
                }
            }
        }
        TrailFrame::Spawn {
            parent,
            from,
            from_words,
            toward,
            task,
            entity,
            length,
        } => {
            if *parent == 0 {
                ev.push(call(
                    0x67d8,
                    serde_json::json!([
                        2,
                        2,
                        module_ref(rel, 0x67ca, 0)?,
                        module_ref(rel, 0x67d2, 0)?
                    ]),
                )?);
            } else {
                ev.push(call(
                    0x67f0,
                    serde_json::json!([2, module_ref(rel, 0x67e6, 0)?, parent]),
                )?);
            }
            if *task != 0 {
                ev.extend([
                    st(0x6804, "task", n(0x18), 4, m(0x67fe, 0)?),
                    st(0x6810, "task", n(0x14), 4, m(0x680a, 0)?),
                    st(0x681c, "entity", n(6), 2, n(0)),
                    st(0x6820, "entity", n(0), 1, n(0)),
                    st(0x682c, "entity", n(0x14), 4, n(from_words[0])),
                    st(0x6830, "entity", n(0x18), 4, n(from_words[1])),
                    st(0x6838, "entity", n(0x1c), 4, n(from_words[2])),
                ]);
                let mut sub = call(0x685c, serde_json::json!(["stack", from]))?;
                sub["vectors"] =
                    serde_json::json!({"0": toward.map(|w| f64::from(f32::from_bits(w)))});
                ev.extend([
                    sub,
                    local(0x6868, 0x6508, serde_json::json!(["stack", entity + 8])),
                    call(0x6870, serde_json::json!(["stack"]))?,
                    st(0x6874, "entity", n(0x20), 4, n(length.to_bits())),
                ]);
            }
        }
        TrailFrame::Chain {
            task,
            pause,
            freeze,
            kind,
            count,
            size,
            fade,
            owner,
            anchor,
        } => {
            if *pause == 0 && *freeze == 0 {
                let base = rel
                    .reference(1, 0x68b6)
                    .filter(|r| r.module == 88 && r.section == 5)
                    .ok_or_else(|| Error::invalid("missing spark table"))?
                    .addend as usize
                    + 28 * usize::from(*kind);
                let d = rel.data(5)?;
                let tf = |o: usize| -> Result<f32> {
                    d.get(base + o..base + o + 4)
                        .map(|b| f32::from_be_bytes(b.try_into().unwrap()))
                        .ok_or_else(|| Error::invalid("spark table out of range"))
                };
                let mut fade = *fade;
                if f64::from(*size) < f64::from(tf(0xc)?) {
                    ev.push(st(
                        0x6900,
                        "entity",
                        n(0x20),
                        4,
                        n((size + tf(0x10)?).to_bits()),
                    ));
                } else {
                    fade -= tf(0x14)?;
                    ev.push(st(0x6914, "entity", n(0x28), 4, n(fade.to_bits())));
                }
                if f64::from(fade) < f64::from(k(0x691e)?) {
                    ev.push(local(
                        0x6944,
                        0x6a54,
                        serde_json::json!([
                            owner,
                            anchor,
                            u32::from(*count).wrapping_sub(1),
                            u32::from(*kind)
                        ]),
                    ));
                    ev.push(call(0x694c, serde_json::json!([task]))?);
                }
            }
        }
        TrailFrame::GlowDraw {
            fade,
            size,
            position,
        } => {
            let a = f64::from(*fade);
            let w = f64::from(k(0x69fe)?);
            ev.extend([
                call(0x6984, serde_json::json!([]))?,
                call(0x6988, serde_json::json!([]))?,
                call(0x6994, serde_json::json!([0, 8]))?,
                call(0x69a0, serde_json::json!([1, 0xa]))?,
                fcall(0x69b4, serde_json::json!([]), vec![a; 4])?,
                call(0x69c0, serde_json::json!([module_ref(rel, 0x69ba, 0)?]))?,
                st(0x69d0, "module", m(0x69ca, 0x10)?, 4, n(size.to_bits())),
                st(0x69d4, "module", m(0x69ca, 0xc)?, 4, n(size.to_bits())),
                st(0x69e0, "module", m(0x69ca, 0)?, 4, n(position[0])),
                st(0x69e4, "module", m(0x69ca, 4)?, 4, n(position[1])),
                st(0x69ec, "module", m(0x69ca, 8)?, 4, n(position[2])),
                call(
                    0x69f8,
                    serde_json::json!([module_ref(rel, 0x69ca, 0)?, 0, 0x32]),
                )?,
                fcall(0x6a14, serde_json::json!([]), vec![w; 4])?,
                call(0x6a20, serde_json::json!([0, 8]))?,
                call(0x6a2c, serde_json::json!([1, 6]))?,
                call(0x6a30, serde_json::json!([]))?,
                call(0x6a34, serde_json::json!([]))?,
            ]);
        }
    }
    Ok(serde_json::json!({"events":ev}))
}
