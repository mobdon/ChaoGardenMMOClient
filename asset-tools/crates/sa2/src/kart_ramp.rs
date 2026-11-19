//! Kart jump-panel initialization after its native work allocation succeeds.
use crate::{city_rel::CityRel, manifest, Error, Result};
use serde::{Deserialize, Serialize};
const HASH: &str = "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPose {
    pub position: [f32; 3],
    pub yaw: i32,
}
#[derive(Serialize)]
pub struct RampInit {
    pub callback_offset: u32,
    pub template_offset: u32,
    /// Big-endian source words expressed as numbers; attachment word is a section-5 offset.
    pub node_words: [u32; 14],
    pub attachment_section: u8,
    pub first_call_address: u32,
    pub first_call_flags: u32,
    pub second_call_address: u32,
    pub collision_info_section: u8,
    pub collision_info_offset: u32,
    pub collision_info_count: u32,
    pub collision_info_kind: u32,
    pub task_word_28: u32,
}
pub fn initialize(bytes: &[u8], pose: TaskPose) -> Result<RampInit> {
    if manifest::sha256(bytes) != HASH {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    if !pose.position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("nonfinite task position"));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let err = || Error::invalid("unexpected kart jump-panel relocation");
    // Object list slot 1: callback and independent name pointer.
    let (1, callback_offset) = rel.local_pointer(5, 0x15f0)?.ok_or_else(err)? else {
        return Err(err());
    };
    let (s, n) = rel.local_pointer(5, 0x15f4)?.ok_or_else(err)?;
    if callback_offset != 0xbc98 || !rel.data(s)?[n as usize..].starts_with(b"JUMP PANEL\0") {
        return Err(err());
    }
    let pair = |at: u32| -> Result<(u8, u32)> {
        let hi = rel.reference(1, at + 2).ok_or_else(err)?;
        let lo = rel.reference(1, at + 6).ok_or_else(err)?;
        if hi.module != 80
            || lo.module != 80
            || hi.kind != 6
            || lo.kind != 4
            || hi.section != lo.section
            || hi.addend != lo.addend
        {
            return Err(err());
        }
        Ok((hi.section, hi.addend))
    };
    let imported = |at| -> Result<u32> {
        let r = rel.reference(1, at).ok_or_else(err)?;
        if r.module != 0 || r.kind != 10 {
            return Err(err());
        }
        Ok(r.addend)
    };
    let (s, template_offset) = pair(0xbd08)?;
    let data = rel.data(s)?;
    let words = &data[template_offset as usize..template_offset as usize + 56];
    let mut node_words =
        std::array::from_fn(|i| u32::from_be_bytes(words[i * 4..i * 4 + 4].try_into().unwrap()));
    let (attachment_section, attachment) =
        rel.local_pointer(s, template_offset + 4)?.ok_or_else(err)?;
    node_words[1] = attachment;
    node_words[0] &= !3;
    node_words[2..5].copy_from_slice(&pose.position.map(f32::to_bits));
    node_words[5..8].copy_from_slice(&[0, pose.yaw as u32, 0]);
    let (collision_info_section, collision_info_offset) = pair(0xbdcc)?;
    let (sec, off) = pair(0xbde0)?;
    let task_word_28 = u32::from_be_bytes(
        rel.data(sec)?[off as usize..off as usize + 4]
            .try_into()
            .unwrap(),
    );
    Ok(RampInit {
        callback_offset,
        template_offset,
        node_words,
        attachment_section,
        first_call_address: imported(0xbdc4)?,
        first_call_flags: 0x20000001,
        second_call_address: imported(0xbddc)?,
        collision_info_section,
        collision_info_offset,
        collision_info_count: 1,
        collision_info_kind: 4,
        task_word_28,
    })
}

/// Observations at entry to the active (non-initializing) task branch.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RampTick {
    pub cooldown: f32,
    pub collision_flags: u16,
    pub registered_tasks: [u32; 6],
    pub yaw: i32,
    pub work_20: f32,
    pub work_24: f32,
    pub contacts: Vec<Contact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub task: u32,
    /// Observed after the opaque kart response call.
    pub post_response_work_44: u32,
    pub post_response_work_70: u32,
    pub post_response_work_48: Option<i32>,
    /// Byte resolved from the live course directory by helper 0x979c.
    pub course_byte: Option<u8>,
    /// Optional changed slot on return from the first external call.
    pub work_48_after_call: Option<i32>,
}
#[derive(Serialize)]
pub struct Dispatch {
    pub contact_index: usize,
    pub task: u32,
    pub matrix_input: [f32; 3],
    pub yaw: i32,
    pub call_8002f8c4: Option<[u32; 4]>,
    pub called_5b90: bool,
    pub call_8006c97c: Option<i32>,
    pub call_8006b710: Option<[i32; 4]>,
}
#[derive(Serialize)]
pub struct TickResult {
    pub cooldown: f32,
    pub dispatches: Vec<Dispatch>,
}
pub fn tick(bytes: &[u8], input: RampTick) -> Result<TickResult> {
    panel_contacts(bytes, input, false)
}
fn panel_contacts(bytes: &[u8], input: RampTick, dash: bool) -> Result<TickResult> {
    if manifest::sha256(bytes) != HASH {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    if input.contacts.len() > i16::MAX as usize
        || ![input.cooldown, input.work_20, input.work_24]
            .into_iter()
            .all(f32::is_finite)
    {
        return Err(Error::invalid("invalid ramp tick observations"));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let hi = rel
        .reference(1, 0x5bca)
        .ok_or_else(|| Error::invalid("missing kart table relocation"))?;
    let lo = rel
        .reference(1, 0x5bce)
        .ok_or_else(|| Error::invalid("missing kart table relocation"))?;
    if hi.module != 80
        || hi.section != 5
        || hi.kind != 6
        || lo.module != 80
        || lo.section != 5
        || lo.kind != 4
        || hi.addend != lo.addend
    {
        return Err(Error::invalid("unexpected kart table relocation"));
    }
    let data = rel.data(5)?;
    let table_offset = hi.addend as usize;
    let mut cooldown = (input.cooldown - 1.).max(0.);
    let mut dispatches = Vec::new();
    if input.collision_flags & 1 != 0 {
        for (contact_index, c) in input.contacts.into_iter().enumerate() {
            if c.task == 0 || !input.registered_tasks.contains(&c.task) {
                continue;
            }
            let called_5b90 = cooldown == 0.;
            let mut first = None;
            let mut second = None;
            if called_5b90 && c.post_response_work_44 != 0 {
                let slot = c
                    .post_response_work_48
                    .ok_or_else(|| Error::invalid("missing kart work+0x48"))?;
                if slot < 0 {
                    return Err(Error::invalid("kart slot precedes mapped table"));
                }
                if slot < 8 {
                    let entry = |slot: i32| -> Result<i32> {
                        if !(0..8).contains(&slot) {
                            return Err(Error::invalid("kart slot outside secondary call table"));
                        }
                        let at = table_offset + slot as usize * 8;
                        Ok(i32::from_be_bytes(data[at..at + 4].try_into().unwrap()))
                    };
                    if entry(slot)? >= 0 {
                        let byte = c
                            .course_byte
                            .ok_or_else(|| Error::invalid("missing indexed course byte"))?
                            as i8;
                        first = Some(if (50..=71).contains(&byte) { 0 } else { -127 });
                        second = Some([entry(c.work_48_after_call.unwrap_or(slot))?, 0, 0, 0]);
                    }
                    cooldown = 30.;
                }
            }
            dispatches.push(Dispatch {
                contact_index,
                task: c.task,
                matrix_input: if dash {
                    [0., 0., -input.work_20]
                } else {
                    [0., input.work_24, input.work_20]
                },
                yaw: input.yaw,
                call_8002f8c4: (c.post_response_work_44 != 0).then_some([
                    c.post_response_work_70,
                    4,
                    15,
                    0,
                ]),
                called_5b90,
                call_8006c97c: first,
                call_8006b710: second,
            });
        }
    }
    Ok(TickResult {
        cooldown,
        dispatches,
    })
}
/// Inputs at CartD section 1 + 0x5524. External math results are observations.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseInput {
    pub vector: [f32; 3],
    pub work_44: u32,
    pub work_274: i32,
    pub magnitude_return: Option<f32>,
    pub angle_return: Option<f64>,
    pub turn_return: Option<i32>,
}
#[derive(Serialize)]
pub struct ResponseWrites {
    pub work_30: Option<u32>,
    pub work_68: Option<u32>,
    pub work_7c: Option<[f32; 3]>,
    pub work_264: Option<f32>,
    pub work_274: Option<i32>,
    pub task_yaw: Option<u16>,
    pub angle_args: Option<[f32; 2]>,
    pub turn_args: Option<[i32; 3]>,
}
pub fn response(bytes: &[u8], input: ResponseInput) -> Result<ResponseWrites> {
    if manifest::sha256(bytes) != HASH {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let constants = rel.data(4)?;
    let threshold = f32::from_be_bytes(constants[0x21c..0x220].try_into().unwrap());
    let factor = f64::from_be_bytes(constants[0x208..0x210].try_into().unwrap());
    let [x, y, z] = input.vector;
    if ![x, y, z].into_iter().all(f32::is_finite) {
        return Err(Error::invalid("nonfinite ramp response vector"));
    }
    let direct = input.work_44 != 0;
    let mut out = ResponseWrites {
        work_30: direct.then_some(0),
        work_68: direct.then_some(0),
        work_7c: direct.then_some(input.vector),
        work_264: None,
        work_274: None,
        task_yaw: None,
        angle_args: None,
        turn_args: None,
    };
    if !direct {
        out.work_264 = Some(if x == 0. && y == 0. && z == 0. {
            0.
        } else {
            input
                .magnitude_return
                .filter(|v| v.is_finite())
                .ok_or_else(|| Error::invalid("missing finite magnitude return"))?
        });
    }
    if !direct || z.abs() >= threshold || x.abs() >= threshold {
        out.angle_args = Some([z, x]);
        let scaled = input
            .angle_return
            .ok_or_else(|| Error::invalid("missing angle return"))?
            * factor;
        let trunc = scaled.trunc();
        if !trunc.is_finite() || trunc < f64::from(i32::MIN) || trunc > f64::from(i32::MAX) {
            return Err(Error::invalid("angle outside verified conversion range"));
        }
        let angle = trunc as i32;
        if direct {
            out.task_yaw = Some(angle.wrapping_add(0x4000).wrapping_neg() as u16);
        } else {
            out.turn_args = Some([input.work_274, angle, 0x4000]);
            out.work_274 = Some(
                input
                    .turn_return
                    .ok_or_else(|| Error::invalid("missing bounded turn return"))?,
            );
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashInput {
    culled: bool,
    mode: i8,
    phase: u16,
    cooldown: f32,
    yaw: i32,
    strength: f32,
    collision_flags: u16,
    registered_tasks: [u32; 6],
    contacts: Vec<Contact>,
    /// Matrix and engine math returns for each dispatched kart, in order.
    responses: Option<Vec<DashMath>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DashMath {
    transformed_vector: [f32; 3],
    work_274: i32,
    magnitude_return: Option<f32>,
    angle_return: Option<f64>,
    turn_return: Option<i32>,
}
#[derive(Serialize)]
pub struct DashUpdate {
    mode: i8,
    phase: u16,
    cooldown: f32,
    install_callbacks: Option<[u32; 2]>,
    collision_init: Option<[u32; 3]>,
    register_collision: bool,
    uv_call: Option<[i32; 3]>,
    uv_writes: Vec<UvWrite>,
    dispatches: Vec<Dispatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vehicle_writes: Option<Vec<ResponseWrites>>,
}
/// Native DASH PANEL callback 0xac40. Keep rotation input separate from the imported matrix result.
pub fn dash(bytes: &[u8], s: DashInput) -> Result<DashUpdate> {
    crate::kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    if rel.local_pointer(5, 0x1600)? != Some((1, 0xac40)) {
        return Err(Error::invalid("unexpected dash panel callback"));
    }
    let mut out = DashUpdate {
        mode: s.mode,
        phase: s.phase,
        cooldown: s.cooldown,
        install_callbacks: None,
        collision_init: None,
        register_collision: false,
        uv_call: None,
        uv_writes: vec![],
        dispatches: vec![],
        vehicle_writes: None,
    };
    let mut math = s.responses.map(Vec::into_iter);
    if s.culled {
        if math.as_ref().is_some_and(|m| m.len() != 0) {
            return Err(Error::invalid("unused dash panel response"));
        }
        return Ok(out);
    }
    if s.mode == 0 {
        out.mode = 1;
        out.install_callbacks = Some([0xae40, 0xaed4]);
        out.collision_init = Some([0x2ed428, 1, 4]);
        out.cooldown = f32::from_be_bytes(rel.data(4)?[0x3d8..0x3dc].try_into().unwrap());
    } else {
        let controls: Vec<_> = s.contacts.iter().map(|c| c.post_response_work_44).collect();
        let r = panel_contacts(
            bytes,
            RampTick {
                cooldown: s.cooldown,
                collision_flags: s.collision_flags,
                registered_tasks: s.registered_tasks,
                yaw: s.yaw,
                work_20: s.strength,
                work_24: 0.,
                contacts: s.contacts,
            },
            true,
        )?;
        out.cooldown = r.cooldown;
        if let Some(m) = math.as_mut() {
            let mut writes = Vec::new();
            for hit in &r.dispatches {
                let obs = m
                    .next()
                    .ok_or_else(|| Error::invalid("missing dash panel response"))?;
                writes.push(response(
                    bytes,
                    ResponseInput {
                        vector: obs.transformed_vector,
                        work_44: controls[hit.contact_index],
                        work_274: obs.work_274,
                        magnitude_return: obs.magnitude_return,
                        angle_return: obs.angle_return,
                        turn_return: obs.turn_return,
                    },
                )?);
            }
            out.vehicle_writes = Some(writes);
        }
        out.dispatches = r.dispatches;
    }
    if math.as_ref().is_some_and(|m| m.len() != 0) {
        return Err(Error::invalid("unused dash panel response"));
    }
    out.register_collision = true;
    let phase = s.phase & 0x3fff;
    out.uv_call = Some([0x42, 0, i32::from(phase)]);
    out.uv_writes = panel_uv(&rel, phase)?;
    out.phase = phase + 60;
    Ok(out)
}

#[derive(Serialize)]
struct UvWrite {
    section5_offset: u32,
    uv: [i16; 2],
}

/// CartD 0xdf38 traverses the model/material directory and resets every matching
/// pair from its saved baseline (not from the result of the preceding update).
fn panel_uv(rel: &CityRel<'_>, phase: u16) -> Result<Vec<UvWrite>> {
    let invalid = || Error::invalid("invalid kart UV directory");
    let d = rel.data(5)?;
    let target = |site| {
        rel.reference(1, site)
            .filter(|r| r.module == 80 && r.kind == 4 && r.section == 5)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let ptr = |field| -> Result<Option<u32>> {
        rel.local_pointer(5, field)?
            .map(|(s, p)| if s == 5 { Ok(p) } else { Err(invalid()) })
            .transpose()
    };
    let field = |at: u32| -> Result<[u8; 4]> {
        d.get(at as usize..)
            .and_then(|p| p.get(..4))
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())
    };
    let count = |at| -> Result<i32> { Ok(i32::from_be_bytes(field(at)?)) };
    let mut row = target(0xaf0e)?;
    let model = target(0xaf16)?;
    let mut writes = Vec::new();
    while let Some(key) = ptr(row)? {
        if key == model {
            let list = ptr(row + 8)?.ok_or_else(invalid)?;
            for i in 0..count(row + 4)? {
                let group = ptr(list + 4 * i as u32)?.ok_or_else(invalid)?;
                if count(group)? != 0x42 {
                    continue;
                }
                let entries = ptr(group + 12)?.ok_or_else(invalid)?;
                for j in 0..count(group + 8)? {
                    let at = entries + 8 * j as u32;
                    let dst = ptr(at)?.ok_or_else(invalid)?;
                    field(dst)?;
                    let [a, b, c, e] = field(at + 4)?;
                    writes.push(UvWrite {
                        section5_offset: dst,
                        uv: [
                            i16::from_be_bytes([a, b]),
                            i16::from_be_bytes([c, e]).wrapping_add(phase as i16),
                        ],
                    });
                }
            }
        }
        row += 12;
    }
    Ok(writes)
}
