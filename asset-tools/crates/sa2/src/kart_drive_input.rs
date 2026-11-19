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
    let speed = if d.velocity_xz == [0., 0.] {
        0.
    } else {
        d.magnitude_return
            .ok_or_else(|| Error::invalid("missing kart planar magnitude return"))?
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
    Ok(Motion {
        heading: d.heading.wrapping_sub(turn as i32),
        work_a4,
        work_68: 0,
        local_displacement: [0., 0., z],
    })
}
