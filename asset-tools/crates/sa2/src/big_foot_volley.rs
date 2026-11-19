//! Mode 8 on the uninterrupted opening. Child slots were filled during descent.
#[path = "big_foot_shot.rs"]
mod projectile;
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) struct Entry {
    pub flags: u16,
    pub angles: [i32; 3],
    pub anchors: [u32; 8],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Launch {
    flight: Option<projectile::Replay>,
    random_result: i32,
    slot_zero_heading: i32,
    heading_difference: i32,
    player_position: [f32; 3],
}
pub(super) fn run(s: Entry, launches: [Launch; 8]) -> Result<(Value, u16)> {
    let mut samples = launches.into_iter();
    let mut flags = s.flags;
    let (mut mode, mut sub, mut timer) = (8, 0, 0u16);
    let mut remaining = s.anchors.len();
    let mut burst = 0;
    let mut rows = Vec::new();
    while mode == 8 {
        let mut shot = None;
        let mut motion = None;
        match sub {
            0 | 1 => {
                timer = 1;
                burst = remaining;
                sub = 2;
            }
            2 => {
                timer -= 1;
                if timer == 0 {
                    let obs = samples
                        .next()
                        .ok_or_else(|| Error::invalid("missing launch sample"))?;
                    if !obs.player_position.iter().all(|n| n.is_finite()) {
                        return Err(Error::invalid("invalid volley target"));
                    }
                    let spread =
                        (2048.0f32 - 4096.0 * (obs.random_result as f32 * (1.0 / 32768.0))) as i32;
                    remaining -= 1;
                    let offset = spread.wrapping_add(obs.heading_difference);
                    let mut child = json!({"slot":remaining,"anchorOffset":s.anchors[remaining],"angles":s.angles,
                        "target":obs.player_position,"differenceArgs":[obs.slot_zero_heading,s.angles[1]],
                        "headingOffset":offset,"speed":1.8f32,"mode":1,"submode":0});
                    if let Some(flight) = obs.flight {
                        child["flight"] =
                            projectile::run(flight, obs.player_position, s.angles, offset)?;
                    }
                    shot = Some(child);
                    if remaining.is_multiple_of(4) {
                        timer = 45;
                        sub = 3;
                    } else {
                        timer = 15;
                    }
                }
            }
            3 => {
                timer -= 1;
                if timer == 0 {
                    if remaining == 0 {
                        motion = Some(10);
                        flags &= !0x100;
                        timer = 60;
                        sub = 4;
                    } else {
                        sub = 1;
                    }
                }
            }
            4 => {
                timer -= 1;
                if timer == 0 {
                    mode = 6;
                    sub = 0;
                }
            }
            _ => unreachable!(),
        }
        rows.push(json!({"mode":mode,"submode":sub,"timer":timer,"flags":flags,"remaining":remaining,"burst":burst,"launch":shot,"motion":motion}));
    }
    Ok((json!(rows), flags))
}
