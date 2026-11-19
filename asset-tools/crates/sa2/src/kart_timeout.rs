//! Countdown consumer and CartTimeOverExec (CartD 0x26fc, 0x9884 and 0x9814).
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    VehiclePostMove {
        remaining: i32,
        vehicle_state: i32,
        mode: i32,
        frame: Frame,
    },
    VehicleCountdown {
        remaining: i32,
        vehicle_state: i32,
        mode: i32,
    },
    Spawn {
        remaining: i32,
        mode: i32,
    },
    Tick {
        timer: u16,
        engine_halfword_64: i16,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    cooldown: i32,
    work_288: i32,
    work_108: i32,
    engine_byte_23: i8,
    engine_halfword_68: i16,
    global_24d60: i32,
    segment: i32,
    segment_count: i32,
    position: [f32; 3],
    previous_position: [f32; 3],
    velocity: [f32; 3],
    angles: [i32; 3],
    work_ec: u32,
    #[serde(skip_serializing)]
    magnitude_return: Option<f32>,
    #[serde(skip_serializing)]
    d968_return: Option<u32>,
}
#[derive(Serialize)]
pub struct Update {
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<Frame>,
    vehicle_state: Option<i32>,
    timer: Option<u16>,
    cue_id: Option<i32>,
    action: Option<i32>,
    next: &'static str,
}
fn spawn(r: &mut Update, remaining: i32, mode: i32) {
    r.timer = Some(if remaining > 0 { 120 } else { 0 });
    if remaining <= 0 {
        r.cue_id = match mode {
            0 => Some(0x706),
            1 => Some(0x70d),
            _ => None,
        };
    }
}
pub fn step(bytes: &[u8], s: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    let mut r = Update {
        frame: None,
        vehicle_state: None,
        timer: None,
        cue_id: None,
        action: None,
        next: "return",
    };
    match s {
        Input::VehiclePostMove {
            remaining,
            mut vehicle_state,
            mode,
            mut frame,
        } => {
            let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
            let constants = rel.data(4)?;
            let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
            frame.cooldown = frame.cooldown.wrapping_sub(1).max(0);
            if frame.work_288 != 0 {
                frame.work_288 = frame.work_288.wrapping_sub(1);
                if frame.work_288 == 0 {
                    frame.work_108 = 0;
                }
            }
            if mode <= 1 {
                if frame.engine_byte_23 == 1
                    && frame.engine_halfword_68 >= 100
                    && vehicle_state != 10
                    && frame.global_24d60 == 0
                {
                    vehicle_state = 10;
                    frame.global_24d60 = 1;
                }
                if remaining <= 0 {
                    if vehicle_state != 10 {
                        vehicle_state = 10;
                        spawn(&mut r, remaining, mode);
                    }
                } else if frame.segment > frame.segment_count.wrapping_sub(20) {
                    if frame.engine_byte_23 == 1 {
                        frame.position = [f(0x1e4), f(0x1f4), f(0x1f8)];
                        frame.previous_position = frame.position;
                        frame.velocity = [
                            f(0x1c8),
                            f(0x1c8),
                            frame.magnitude_return.ok_or_else(|| {
                                crate::Error::invalid(
                                    "route reset needs observed raw velocity magnitude",
                                )
                            })?,
                        ];
                        frame.angles = [0; 3];
                        frame.segment = 0;
                    } else {
                        frame.segment = frame.segment_count.wrapping_sub(25);
                        frame.position[0] -= f(0x1fc);
                        if frame.work_ec == 0 {
                            frame.work_ec = frame.d968_return.ok_or_else(|| {
                                crate::Error::invalid("missing CartD d968 task return")
                            })?;
                        }
                    }
                }
            }
            r.vehicle_state = Some(vehicle_state);
            r.next = "vehicle_2814";
            r.frame = Some(frame);
        }
        Input::VehicleCountdown {
            remaining,
            vehicle_state,
            mode,
        } => {
            r.vehicle_state = Some(vehicle_state);
            if remaining > 0 {
                r.next = "vehicle_272c";
            } else {
                r.next = "vehicle_2814";
                if vehicle_state != 10 {
                    r.vehicle_state = Some(10);
                    spawn(&mut r, remaining, mode);
                }
            }
        }
        Input::Spawn { remaining, mode } => spawn(&mut r, remaining, mode),
        Input::Tick {
            timer,
            engine_halfword_64,
        } => {
            let timer = timer.wrapping_add(1);
            r.timer = Some(timer);
            if timer > 180 {
                r.action = Some(if engine_halfword_64 < 1 { 8 } else { 4 });
            }
        }
    }
    Ok(r)
}
