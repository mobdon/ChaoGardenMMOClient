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
    #[serde(default)]
    alternate: bool,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryTick {
    pub(crate) after_movement: Option<AfterMovement>,
    position: Option<[f32; 3]>,
    angles: Option<[i32; 3]>,
    globals: Option<[i32; 2]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AfterMovement {
    pub(crate) velocity: [f32; 3],
    pub(crate) audio: Option<crate::kart_drive_input::StoryVehicleAudio>,
    pub(crate) magnitude_return: Option<f32>,
    pub(crate) contact: Option<crate::kart_drift::DriveContact>,
}
impl StoryTick {
    pub(crate) fn advance(
        self,
        rel: &CityRel,
        state: i32,
        pose: Option<([f32; 3], [i32; 3])>,
        globals: Option<[i32; 2]>,
    ) -> Result<Update> {
        let mut position = self.position;
        let mut angles = self.angles;
        if let Some((p, a)) = pose {
            if position.is_some_and(|x| x.map(f32::to_bits) != p.map(f32::to_bits))
                || angles.is_some_and(|x| x != a)
            {
                return Err(Error::invalid(
                    "scripted pose conflicts with preceding frame",
                ));
            }
            position = Some(p);
            angles = Some(a);
        }
        if self.globals.zip(globals).is_some_and(|(a, b)| a != b) {
            return Err(Error::invalid(
                "scripted globals conflict with preceding update",
            ));
        }
        let [global_21760, global_22494] = globals
            .or(self.globals)
            .ok_or_else(|| Error::invalid("missing initial scripted globals"))?;
        // Both movement callbacks unconditionally overwrite +0x68 and +0xdc.
        step_verified(
            rel,
            Input {
                audio: None,
                state: State {
                    position: position
                        .ok_or_else(|| Error::invalid("missing scripted position"))?,
                    angles: angles.ok_or_else(|| Error::invalid("missing scripted angles"))?,
                    vehicle_state: if global_22494 != 0 { 9 } else { state },
                    work_68: 0,
                    work_dc: 0,
                    global_21760,
                    global_22494,
                },
            },
        )
    }
}
impl Update {
    pub(crate) fn position(&self) -> [f32; 3] {
        self.state.position
    }
    pub(crate) fn state(&self) -> i32 {
        self.state.vehicle_state
    }
    pub(crate) fn globals(&self) -> [i32; 2] {
        [self.state.global_21760, self.state.global_22494]
    }
}
pub fn step(bytes: &[u8], input: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    step_verified(&rel, input)
}
fn step_verified(rel: &CityRel, input: Input) -> Result<Update> {
    let mut s = input.state;
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
            let cue = crate::kart_vehicle_init::cue_row(
                rel.data(5)?,
                a.profile as usize,
                a.alternate,
            )?[0] as u32;
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
