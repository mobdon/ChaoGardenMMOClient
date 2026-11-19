//! CartLimousineExec: task initialization and active update in installed CartD.
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    position: [f32; 3],
    lead: f32,
    latch: i32,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Spawn {
        segment_xz: [[f32; 2]; 4],
        parent_task_y: f32,
        mode: i32,
        latch: i32,
    },
    Tick {
        state: State,
        active_word: i32,
        parent_work_x: f32,
        parent_work_7c: f32,
        mode: i32,
    },
}

#[derive(Serialize)]
pub struct Output {
    state: State,
    cue_id: Option<i32>,
    installs_callbacks: bool,
}

pub fn step(bytes: &[u8], input: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let constants = crate::city_rel::CityRel::read_identified(bytes, 80)?.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    let mut cue_id = None;
    let (state, installs_callbacks) = match input {
        Input::Spawn {
            segment_xz,
            parent_task_y,
            mode,
            latch,
        } => {
            let [x, z] = segment_xz[2];
            (
                State {
                    position: [
                        x + f(0x478),
                        if mode == 1 {
                            parent_task_y + f(0x47c)
                        } else {
                            parent_task_y
                        },
                        z,
                    ],
                    lead: f(0x478),
                    latch,
                },
                true,
            )
        }
        Input::Tick {
            mut state,
            active_word,
            parent_work_x,
            parent_work_7c,
            mode,
        } => {
            if active_word > 0 {
                if state.latch != 0 {
                    state.position[0] = f(0x47c) + parent_work_x;
                } else {
                    state.lead += f(0x480) - parent_work_7c;
                    if state.lead > f(0x484) {
                        state.lead = f(0x484);
                    } else if state.lead < f(0x47c) {
                        cue_id = match mode {
                            0 => Some(0x701),
                            1 => Some(0x708),
                            _ => None,
                        };
                        state.latch = 1;
                    }
                    state.position[0] = parent_work_x + state.lead;
                }
            }
            (state, active_word > 0)
        }
    };
    Ok(Output {
        state,
        cue_id,
        installs_callbacks,
    })
}
