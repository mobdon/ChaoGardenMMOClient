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

/// Controller prelude after the target task, before the shared timing code at 0x861c.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerInput {
    lap_clock: Option<crate::kart_lap_clock::ClockInput>,
    race_results: Option<ResultContext>,
    remove_task: bool,
    pursuit_latch: i32,
    race_finish_latch: i32,
    elapsed: i32,
    remaining: i32,
    engine_byte_09: i8,
    engine_byte_42: i8,
    flag_3ce24: i32,
    engine_byte_16: i8,
}

#[derive(Serialize)]
pub struct ControllerUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    lap_clock: Option<crate::kart_lap_clock::Clock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    race_results: Option<ResultActions>,
    elapsed: i32,
    remaining: i32,
    flag_3ce24: i32,
    engine_byte_16: i8,
    calls: Vec<(&'static str, Option<i32>)>,
    next: &'static str,
}

pub fn controller(bytes: &[u8], s: ControllerInput) -> Result<ControllerUpdate> {
    kart_progress::verify(bytes)?;
    let mut r = ControllerUpdate {
        lap_clock: None,
        race_results: None,
        elapsed: s.elapsed,
        remaining: s.remaining,
        flag_3ce24: s.flag_3ce24,
        engine_byte_16: s.engine_byte_16,
        calls: vec![],
        next: "shared_timing_861c",
    };
    if s.remove_task {
        r.calls.push(("engine_80071888_task", None));
        r.next = "return";
    } else if s.pursuit_latch != 0 {
        r.calls.push(("engine_800629e4", None));
        r.elapsed = r.elapsed.wrapping_add(1);
        if r.elapsed == 300 {
            r.calls.push(("engine_8005eb24", Some(0)));
        }
        r.flag_3ce24 = 1;
        r.engine_byte_16 = 1;
    } else if s.race_finish_latch != 0 {
        if let Some(ctx) = s.race_results {
            r.race_results = Some(results(&mut r, ctx)?);
        } else {
            r.next = "race_results_83bc";
        }
    } else if s.engine_byte_09 == 0 || s.engine_byte_42 != 0 {
        r.remaining = r.remaining.wrapping_sub(1);
    }
    if r.next == "shared_timing_861c" {
        if let Some(clock) = s.lap_clock {
            r.lap_clock = Some(crate::kart_lap_clock::advance(
                clock,
                s.race_finish_latch != 0,
            )?);
            r.next = "slot_progress_8824";
        }
    }
    Ok(r)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultContext {
    mode: i32,
    first_slot_place: Option<i32>,
    multiplayer_byte: i8,
    engine_byte_23: i8,
    menu_active: bool,
    engine_word_b4: i32,
    time: [i8; 3],
    record: [i8; 3],
    record_index: i32,
    completion_index: i32,
    check_return: Option<i32>,
}
#[derive(Serialize)]
struct ResultActions {
    /// Offsets relative to the original in-memory record base. No save I/O.
    writes: Vec<(i32, Vec<i8>)>,
    engine_byte_47_write: Option<i8>,
    calls: Vec<(&'static str, Vec<i32>)>,
}

fn results(r: &mut ControllerUpdate, s: ResultContext) -> Result<ResultActions> {
    r.calls.push(("engine_800629e4", None));
    r.elapsed = r.elapsed.wrapping_add(1);
    r.engine_byte_16 = 1;
    let mut a = ResultActions {
        writes: vec![],
        engine_byte_47_write: None,
        calls: vec![],
    };
    match r.elapsed {
        300 => {
            r.flag_3ce24 = 1;
            if s.engine_byte_23 != 1 {
                a.calls.push(("engine_8001ef18", vec![8]));
            }
        }
        200 if s.menu_active => r.elapsed = 199,
        60 if s.first_slot_place == Some(1) && s.multiplayer_byte == 0 && s.mode >= 2 => {
            let time_value =
                |[m, sec, cs]: [i8; 3]| i32::from(m) * 6000 + i32::from(sec) * 100 + i32::from(cs);
            if s.mode == 5 {
                if s.engine_word_b4 == 1 || time_value(s.time) >= time_value(s.record) {
                    return Ok(a);
                }
                a.writes.push((
                    s.record_index.wrapping_mul(3).wrapping_add(0x349a),
                    s.time.to_vec(),
                ));
            } else {
                a.writes.push((
                    s.completion_index.wrapping_mul(13).wrapping_add(0x2fbc),
                    vec![1],
                ));
                let [m, sec, cs] = s.time.map(i32::from);
                a.calls.push(("engine_8005a8fc", vec![0, m, sec, cs]));
            }
            a.calls.push(("engine_8005aacc", vec![0]));
            let byte = s.check_return.ok_or_else(|| {
                crate::Error::invalid("missing observed return from engine_8005aacc")
            })? as i8;
            a.engine_byte_47_write = Some(byte);
            if byte == 0 {
                a.calls.push(("engine_8005ba74", vec![]));
            }
        }
        _ => {}
    }
    Ok(a)
}
