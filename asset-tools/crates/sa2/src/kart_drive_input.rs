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
        s.history[s.cursor] = i32::from(f.stick_x);
        s.cursor = (s.cursor + 1) % 60;
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
    Ok(Motion {
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
