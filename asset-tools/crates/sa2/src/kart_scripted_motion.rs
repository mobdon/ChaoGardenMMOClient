//! CartD vehicle-state movement callbacks 8 and 9, before the common contact pass.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    audio: Option<AudioInput>,
    #[serde(flatten)]
    state: State,
}

#[derive(Deserialize, Serialize)]
pub struct State {
    vehicle_state: i32,
    position: [f32; 3],
    angles: [i32; 3],
    work_68: i32,
    work_dc: i32,
    global_21760: i32,
    global_22494: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioInput {
    work_44: i32,
    profile: u32,
}
#[derive(Serialize)]
pub struct Update {
    #[serde(flatten)]
    state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio: Option<AudioDispatch>,
}
#[derive(Serialize)]
#[serde(tag = "path", rename_all = "snake_case")]
enum AudioDispatch {
    Disabled,
    NormalState5820,
    Engine8006b188 {
        cue: u32,
        task_relative_handle: u32,
        arg5: i32,
        arg6: i32,
        position: [f32; 3],
    },
}

pub fn step(bytes: &[u8], input: Input) -> Result<Update> {
    let mut s = input.state;
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let constants = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    match s.vehicle_state {
        8 => {
            s.position[0] += f(0x278);
            s.angles = [0, -0x4000, 0];
        }
        9 => {
            s.position[2] -= f(0x27c);
            let previous = s.global_21760;
            s.global_21760 = previous.wrapping_add(1);
            if previous > 120 {
                s.global_22494 = 0;
                s.vehicle_state = 0;
            }
        }
        _ => return Err(Error::invalid("expected kart movement state 8 or 9")),
    }
    s.work_68 = 0;
    s.work_dc = 0;
    let audio = input
        .audio
        .map(|a| -> Result<_> {
            if a.work_44 == 0 {
                return Ok(AudioDispatch::Disabled);
            }
            if s.vehicle_state == 0 {
                return Ok(AudioDispatch::NormalState5820);
            }
            if a.profile >= 16 {
                return Err(Error::invalid("kart cue profile outside supported table"));
            }
            let data = rel.data(5)?;
            let at = 0x3924 + a.profile as usize * 24;
            let cue = u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
            Ok(AudioDispatch::Engine8006b188 {
                cue,
                task_relative_handle: cue,
                arg5: 1,
                arg6: -20,
                position: s.position,
            })
        })
        .transpose()?;
    Ok(Update { state: s, audio })
}
