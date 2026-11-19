//! Countdown consumer and CartTimeOverExec (CartD 0x26fc, 0x9884 and 0x9814).
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
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
#[derive(Serialize)]
pub struct Update {
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
        vehicle_state: None,
        timer: None,
        cue_id: None,
        action: None,
        next: "return",
    };
    match s {
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
