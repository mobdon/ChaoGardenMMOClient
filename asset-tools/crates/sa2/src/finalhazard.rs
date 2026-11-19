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
        "constructor":init,"matrixStride":48,"matrixCaptures":captures,"placements":rows,"scope":"Display records only; matrix contents, selection timing and hit detection are external."}),
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
