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
    buttons: u32,
    stick_x: i16,
    work_28c: i32,
}
#[derive(Serialize)]
struct Values {
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
        frames.push(Values {
            work_110: u32::from(f.buttons & 4 != 0),
            work_114: u32::from(f.buttons & 0x402 != 0 && f.work_28c <= 10),
            steering_sum: sum,
            steering_input: scale * (sum as f32),
        });
    }
    Ok(Output {
        history: s.history,
        cursor: s.cursor,
        frames,
    })
}
