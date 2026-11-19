//! Native kart controller sample/history and two button flags in CartD.
use crate::{kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    history: Vec<i32>,
    cursor: usize,
    frames: Vec<Sample>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    drive: Option<Drive>,
    buttons: u32,
    stick_x: i16,
    work_28c: i32,
}
#[derive(Serialize)]
struct Values {
    #[serde(skip_serializing_if = "Option::is_none")]
    motion: Option<Motion>,
    work_110: u32,
    work_114: u32,
    steering_sum: i32,
    steering_input: f32,
}
#[derive(Serialize)]
pub struct Output {
    history: Vec<i32>,
    cursor: usize,
    frames: Vec<Values>,
}

/// Each frame supplies the already selected 0x34-byte controller row's fields.
pub fn replay(bytes: &[u8], mut s: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    if s.history.len() != 60 || s.cursor >= 60 {
        return Err(Error::invalid(
            "expected 60 kart stick samples and a valid write cursor",
        ));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let scale = f32::from_be_bytes(rel.data(4)?[0x200..0x204].try_into().unwrap());
    let mut frames = Vec::with_capacity(s.frames.len());
    for f in s.frames {
        append_sample(&mut s.history, &mut s.cursor, f.stick_x);
        // Reader starts at the new write cursor, not the last written sample.
        let sum = (0..10).fold(0i32, |sum, i| {
            sum.wrapping_add(s.history[(s.cursor + 60 - i) % 60])
        });
        let work_110 = u32::from(f.buttons & 4 != 0);
        let work_114 = u32::from(f.buttons & 0x402 != 0 && f.work_28c <= 10);
        let steering_input = scale * (sum as f32);
        let motion = f
            .drive
            .map(|d| {
                drive_step(
                    d,
                    steering_input,
                    f.stick_x,
                    work_110 != 0,
                    work_114 != 0,
                    rel.data(4)?,
                )
            })
            .transpose()?;
        frames.push(Values {
            motion,
            work_110,
            work_114,
            steering_sum: sum,
            steering_input,
        });
    }
    Ok(Output {
        history: s.history,
        cursor: s.cursor,
        frames,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Drive {
    drift: Option<DriftInput>,
    position_step: Option<PositionInput>,
    heading: i32,
    work_a4: i32,
    place: i32,
    multiplayer: bool,
    velocity_xz: [f32; 2],
    magnitude_return: Option<f32>,
    acceleration: f32,
    brake_power: f32,
    deceleration: f32,
    speed_cap: f32,
}
#[derive(Serialize)]
struct Motion {
    #[serde(skip_serializing_if = "Option::is_none")]
    drift: Option<DriftUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_step: Option<PositionStep>,
    magnitude_source: &'static str,
    heading: i32,
    work_a4: i32,
    work_68: i32,
    local_displacement: [f32; 3],
}
/// 0x31b4..0x33a4: heading and local displacement before engine matrix calls.
fn drive_step(
    d: Drive,
    steer: f32,
    stick_x: i16,
    accelerate: bool,
    brake: bool,
    constants: &[u8],
) -> Result<Motion> {
    let f = |at: usize| f32::from_be_bytes(constants[at..at + 4].try_into().unwrap());
    let (speed, magnitude_source) = if d.velocity_xz == [0., 0.] {
        (0., "native_zero")
    } else if let Some(speed) = d.magnitude_return {
        (speed, "observed")
    } else {
        let [x, z] = d.velocity_xz.map(f64::from);
        (x.hypot(z) as f32, "mathematical_projection")
    };
    let boost = if d.multiplayer {
        f(0x22c) + f(0x210) * (d.place.wrapping_sub(1) as f32)
    } else {
        f(0x22c)
    };
    let turn = f(0x218) * (f(0x230) * steer);
    let (delta, work_a4) = if accelerate {
        (
            (d.acceleration * boost) * ((d.speed_cap - speed) / d.speed_cap),
            d.work_a4.wrapping_sub(1),
        )
    } else if brake {
        (d.brake_power * boost, 10)
    } else {
        (d.deceleration, 10)
    };
    let z = (delta + speed) * f(0x1e8);
    if !z.is_finite() {
        return Err(Error::invalid("nonfinite kart displacement"));
    }
    let heading = d.heading.wrapping_sub(turn as i32);
    let position_step = d
        .position_step
        .map(|p| position(p, heading, z, f(0x234)))
        .transpose()?;
    let drift = d
        .drift
        .map(|i| drift_update(i, heading, speed, work_a4, accelerate, stick_x, constants))
        .transpose()?;
    Ok(Motion {
        drift,
        position_step,
        magnitude_source,
        heading,
        work_a4,
        work_68: 0,
        local_displacement: [0., 0., z],
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionInput {
    position: [f32; 3],
    velocity_y: f32,
    weight: f32,
    work_74: i32,
    transformed_vector: Option<[f32; 3]>,
}
#[derive(Serialize)]
struct PositionStep {
    position: [f32; 3],
    world_displacement: [f32; 3],
    transform_source: &'static str,
}
/// Original rotation work has zero X and Z. Only heading is written (0x339c).
fn position(
    p: PositionInput,
    heading: i32,
    local_z: f32,
    rising_scale: f32,
) -> Result<PositionStep> {
    let (world_displacement, transform_source) = if let Some(v) = p.transformed_vector {
        (v, "observed")
    } else {
        let radians = f64::from(heading as u16) * (std::f64::consts::TAU / 65536.);
        let (sin, cos) = radians.sin_cos();
        (
            [
                (sin * f64::from(local_z)) as f32,
                0.,
                (cos * f64::from(local_z)) as f32,
            ],
            "mathematical_projection",
        )
    };
    let [mut x, mut y, mut z] = p.position;
    x += world_displacement[0];
    y += world_displacement[1];
    z += world_displacement[2];
    let gravity = if p.velocity_y > 0. && p.work_74 != 0 {
        p.weight * rising_scale
    } else {
        p.weight
    };
    y += p.velocity_y - gravity;
    if ![x, y, z].into_iter().all(f32::is_finite) {
        return Err(Error::invalid("nonfinite kart position"));
    }
    Ok(PositionStep {
        position: [x, y, z],
        world_displacement,
        transform_source,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriftInput {
    /// The 30 motion-history reads returned by 0x52b0, newest first.
    #[serde(alias = "positions")]
    motion_history: Option<[[f32; 3]; 30]>,
    ring: Option<crate::kart_contact::Ring>,
    /// Return of the rotation helper 0x5498, if observed.
    local_offset: Option<[f32; 3]>,
    pitch_roll: Option<[i32; 2]>,
    speed_threshold: f32,
    vehicle_state: i32,
    work_10c: u32,
    work_a8: i32,
}
#[derive(Serialize)]
struct DriftUpdate {
    history_offset: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    projected_local_offset: Option<[f32; 3]>,
    work_f0: Option<i32>,
    work_f8: Option<i32>,
    vehicle_state: i32,
    work_10c: u32,
    work_a8: i32,
    work_a4: i32,
}
fn drift_update(
    s: DriftInput,
    heading: i32,
    speed: f32,
    mut a4: i32,
    accelerate: bool,
    stick: i16,
    constants: &[u8],
) -> Result<DriftUpdate> {
    let history = match (s.motion_history, s.ring) {
        (Some(h), None) => h,
        (None, Some(r)) => r.motion_history()?,
        _ => {
            return Err(Error::invalid(
                "supply exactly one kart motion_history or ring",
            ))
        }
    };
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    let mut sum = [0f32; 3];
    for pos in history {
        for (acc, x) in sum.iter_mut().zip(pos) {
            *acc += x;
        }
    }
    let history_offset = std::array::from_fn(|i| history[0][i] - sum[i] * f(0x238));
    if s.pitch_roll.is_some() && s.local_offset.is_some() {
        return Err(Error::invalid(
            "supply task angles or observed drift offset, not both",
        ));
    }
    let projected_local_offset = s
        .pitch_roll
        .map(|[pitch, roll]| {
            crate::kart_contact::inverse_vehicle_rotation(history_offset, [pitch, heading, roll])
        })
        .transpose()?;
    let local_offset = s.local_offset.or(projected_local_offset);
    let work_f0 = local_offset.map(|[x, _, z]| {
        if x < f(0x23c) {
            640
        } else if x > f(0x240) {
            -640
        } else {
            (z * f(0x244)) as i32
        }
    });
    let work_f8 = local_offset.map(|v| (v[0] * f(0x248)) as i32);
    let (mut vehicle_state, mut work_10c, mut work_a8) = (s.vehicle_state, s.work_10c, s.work_a8);
    if speed > s.speed_threshold && a4 > 0 {
        work_10c |= 1;
        if accelerate && f32::from(stick).abs() > f(0x1e0) {
            vehicle_state = 2;
            work_a8 = i32::from(stick);
            a4 = 0;
        }
    }
    Ok(DriftUpdate {
        projected_local_offset,
        history_offset,
        work_f0,
        work_f8,
        vehicle_state,
        work_10c,
        work_a8,
        work_a4: a4,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryInput {
    heading: i32,
    work_68: i32,
    work_fc: i32,
    stick_x: i16,
    velocity_xz: [f32; 2],
    angle_radians: Option<f64>,
    angle_difference: Option<i32>,
    magnitude_return: Option<f32>,
    ground_angle_return: Option<i32>,
    accelerate: bool,
    brake: bool,
    speed_cap: f32,
    acceleration: f32,
    brake_power: f32,
    deceleration: f32,
    position_step: PositionInput,
}
#[derive(Serialize)]
pub struct Recovery {
    vehicle_state: i32,
    heading: i32,
    work_68: i32,
    work_fc: i32,
    local_displacement: [f32; 3],
    position_step: PositionStep,
}
/// CartD state 1, 0x36b4..0x3998. Horizontal motion controls the state exit.
pub fn recovery(bytes: &[u8], s: RecoveryInput) -> Result<Recovery> {
    kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let (mut heading, mut work_68, mut vehicle_state) = (s.heading, s.work_68, 1);
    let speed = if s.velocity_xz == [0., 0.] {
        0.
    } else {
        let radians = s
            .angle_radians
            .ok_or_else(|| Error::invalid("missing velocity angle return"))?;
        let units = radians * f64::from_be_bytes(data[0x208..0x210].try_into().unwrap());
        if !(-2147483648.0..2147483648.0).contains(&units) {
            return Err(Error::invalid("unsupported recovery angle"));
        }
        heading = (units as i32).wrapping_add(0x4000).wrapping_neg();
        if s.angle_difference
            .ok_or_else(|| Error::invalid("missing recovery angle difference"))?
            < 546
        {
            vehicle_state = 0;
            work_68 = 0;
        } else {
            work_68 /= 2;
            heading = heading.wrapping_add(work_68);
        }
        s.magnitude_return
            .ok_or_else(|| Error::invalid("missing horizontal magnitude return"))?
    };
    heading = heading.wrapping_sub((f(0x218) * (f(0x258) * f32::from(s.stick_x))) as i32);
    let delta = if s.accelerate {
        s.acceleration * ((s.speed_cap - speed) / s.speed_cap)
    } else if s.brake {
        s.brake_power
    } else {
        s.deceleration
    };
    let z = (delta + speed) * f(0x1e8);
    if !z.is_finite() {
        return Err(Error::invalid("nonfinite state 1 displacement"));
    }
    let work_fc = if s.position_step.work_74 != 0 {
        s.ground_angle_return
            .ok_or_else(|| Error::invalid("missing grounded angle return"))?
    } else {
        s.work_fc.wrapping_add(91)
    };
    Ok(Recovery {
        vehicle_state,
        heading,
        work_68,
        work_fc,
        local_displacement: [0., 0., z],
        position_step: position(s.position_step, heading, z, f(0x234))?,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpinInput {
    heading: i32,
    work_68: i32,
    work_b8: i32,
    velocity_xz: [f32; 2],
    angle_radians: Option<f64>,
    magnitude_return: Option<f32>,
    position_step: PositionInput,
}
#[derive(Serialize)]
pub struct Spin {
    heading: i32,
    work_68: i32,
    work_b8: i32,
    vehicle_state: i32,
    local_displacement: [f32; 3],
    position_step: PositionStep,
}
/// State 4 decays the signed angular step and calls 0x30dc on crossing zero.
pub fn spin(bytes: &[u8], s: SpinInput) -> Result<Spin> {
    kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let mut work_68 = s.work_68.wrapping_add(s.work_b8);
    let work_b8 = if s.work_b8 > 0 {
        s.work_b8 - 18
    } else {
        s.work_b8 + 18
    };
    let exit = if s.work_b8 > 0 {
        work_b8 < 0
    } else {
        work_b8 > 0
    };
    let mut heading = s.heading;
    if exit {
        work_68 = 0;
        if s.velocity_xz.iter().any(|v| v.abs() >= f(0x21c)) {
            let radians = s
                .angle_radians
                .ok_or_else(|| Error::invalid("missing exit heading angle"))?;
            let units = radians * f64::from_be_bytes(data[0x208..0x210].try_into().unwrap());
            if !(-2147483648.0..2147483648.0).contains(&units) {
                return Err(Error::invalid("unsupported exit angle"));
            }
            heading = (units as i32).wrapping_add(0x4000).wrapping_neg() & 0xffff;
        }
    }
    let speed = if s.velocity_xz == [0.; 2] {
        0.
    } else {
        s.magnitude_return
            .ok_or_else(|| Error::invalid("missing horizontal magnitude"))?
    };
    let z = f(0x270) * -speed;
    Ok(Spin {
        heading,
        work_b8,
        work_68,
        vehicle_state: if exit { 0 } else { 4 },
        local_displacement: [0., 0., z],
        position_step: position(s.position_step, heading, z, f(0x234))?,
    })
}

fn append_sample(history: &mut [i32], cursor: &mut usize, stick_x: i16) {
    history[*cursor] = i32::from(stick_x);
    *cursor = (*cursor + 1) % 60;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateInput {
    work_70: i32,
    state: i32,
    global_22494: i32,
    global_2ec208: i32,
    controller: Option<GateController>,
    post_contact_fall: Option<crate::kart_fall_recovery::Input>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GateController {
    stick_x: i16,
    history: Vec<i32>,
    cursor: usize,
}

#[derive(Serialize)]
pub struct GateOutput {
    state: i32,
    controller: Option<GateController>,
    callbacks: &'static [u32],
    resume_offset: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    fall: Option<crate::kart_fall_recovery::Output>,
}

/// CartD 0x2214..0x2284. Controller data belongs to the selected player slot.
pub fn update_gate(bytes: &[u8], mut input: GateInput) -> Result<GateOutput> {
    kart_progress::verify(bytes)?;
    if input.work_70 >= 0 {
        if input.work_70 > 1 {
            return Err(Error::invalid("unsupported kart controller slot"));
        }
        let c = input
            .controller
            .as_mut()
            .ok_or_else(|| Error::invalid("missing selected controller"))?;
        if c.history.len() != 60 || c.cursor >= 60 {
            return Err(Error::invalid("invalid selected kart history"));
        }
        append_sample(&mut c.history, &mut c.cursor, c.stick_x);
    }
    let (callbacks, resume_offset): (&[u32], u32) = if input.global_22494 != 0 {
        input.state = 9;
        (&[], 0x2284)
    } else if input.global_2ec208 != 0 {
        (&[0x2ca4, 0x2da8, 0x4b50, 0x3030, 0x302c], 0x287c)
    } else {
        (&[], 0x2284)
    };
    let fall = match (resume_offset, input.post_contact_fall) {
        (0x287c, Some(s)) => Some(crate::kart_fall_recovery::tick(bytes, s)?),
        (_, Some(_)) => {
            return Err(Error::invalid(
                "fall observation supplied on a path that skips the callback",
            ))
        }
        (_, None) => None,
    };
    Ok(GateOutput {
        fall,
        state: input.state,
        controller: input.controller,
        callbacks,
        resume_offset,
    })
}
