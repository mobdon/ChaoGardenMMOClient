//! Native stage whistle pipe: observed range/query and matrix inputs, local task state.
use crate::{set::Placement, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Frame {
    range_out: bool,
    #[serde(default)]
    has_set_condition: bool,
    /// Previously set flag in the stage condition before this replay.
    #[serde(default)]
    condition_flag_20: bool,
    /// Query result, zero-based: the selected player and its native task mode.
    nearby: Option<Nearby>,
    transformed_point: Option<[f32; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nearby {
    slot: u8,
    mode: i8,
}

pub(crate) fn replay(p: &Placement, character: i8, frames: Vec<Frame>) -> Result<Value> {
    let radius = if matches!(character, 6 | 7) {
        25.0
    } else {
        13.0
    };
    let (mut bound, mut flagged, mut timer) = (false, false, 0u16);
    let (mut offset, mut speed) = (0.0f32, 0.0f32);
    let mut trace = Vec::with_capacity(frames.len());
    for f in frames {
        let mut calls = Vec::new();
        let eligible =
            bound && !f.range_out && f.has_set_condition && !(flagged || f.condition_flag_20);
        let release = eligible && timer == 270;
        if f.nearby.as_ref().is_some_and(|n| !eligible || n.slot > 1)
            || f.transformed_point.is_some() != release
            || f.transformed_point
                .is_some_and(|v| !v.iter().all(|n| n.is_finite()))
        {
            return Err(Error::invalid(
                "invalid whistle pipe observation for this frame",
            ));
        }
        if !f.range_out {
            if !bound {
                bound = true;
                calls.push(json!({"initCollision":{"count":1,"id":4}}));
            } else {
                if eligible {
                    calls.push(json!({"queryPlayers":{"center":p.position,"radius":radius}}));
                    if timer != 0 {
                        if release {
                            let scale = p.parameters[0].trunc();
                            if !(0.0..4294967296.0).contains(&scale) {
                                return Err(Error::invalid(
                                    "unsupported whistle pipe spawn selector",
                                ));
                            }
                            calls.push(json!({"spawn":{"selector":(scale as u32)%3,"position":f.transformed_point}}));
                            calls.push(json!({"setCondition":1}));
                            calls.push(json!({"sound":0x100a}));
                            calls.push(json!({"setFlag":0x20}));
                            flagged = true;
                        }
                        timer -= 1;
                    } else if let Some(n) = f.nearby {
                        if n.mode == 42 {
                            timer = 300;
                            offset = 0.;
                            speed = 0.08;
                        } else {
                            calls.push(json!({"selectAction":{"slot":n.slot,"action":42}}));
                        }
                    }
                }
                calls.push(json!("registerCollision"));
                offset += speed;
                speed = 0.95 * speed - 0.15 * offset;
            }
        }
        trace.push(json!({"calls":calls,"bound":bound,"timer":timer,"offset":offset,"speed":speed,"setFlag20":flagged}));
    }
    Ok(json!({"callback":"ObjectChaoPipe","frames":trace}))
}
