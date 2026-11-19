//! The stage-local ChaoKeyTask created on a dropped-key hit.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

const TARGET: [f32; 3] = [-43., 30., -100.];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    existing_task: bool,
    allocation_ok: bool,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Transform {
        point: [f32; 3],
    },
    /// Imported vector routine: scalar return and optional changed argument.
    Advance {
        scalar: f32,
        vector_after_call: Option<[f32; 3]>,
    },
    Destroy {
        still_owner: bool,
    },
}
pub(crate) fn replay(mut pos: [f32; 3], mut byte: i8, req: Request) -> Result<Value> {
    if req.steps.len() > 2048 {
        return Err(Error::invalid("too many key pickup updates"));
    }
    if byte == 1 || req.existing_task || !req.allocation_ok {
        if !req.steps.is_empty() {
            return Err(Error::invalid("no new pickup task for these updates"));
        }
        return Ok(json!({"allocated":false,"existingTask":req.existing_task,"gameByte44":byte}));
    }
    let (mut timer, mut scale) = (0u16, 1f32);
    let mut transformed = false;
    let mut pending = false;
    let mut ended = false;
    let mut out = Vec::new();
    for step in req.steps {
        if ended {
            return Err(Error::invalid("pickup already destroyed"));
        }
        let mut calls = Vec::new();
        match step {
            Step::Destroy { still_owner } => {
                if still_owner {
                    calls.push(json!("clearPickupTaskPointer"));
                }
                byte = 1;
                ended = true;
                calls.push(json!("setCollected"));
            }
            Step::Transform { point } if !transformed && point.iter().all(|v| v.is_finite()) => {
                calls.push(json!({"transformPoint":pos}));
                pos = point;
                transformed = true;
            }
            Step::Advance {
                scalar,
                vector_after_call,
            } if transformed && !pending && scalar.is_finite() => {
                let delta = std::array::from_fn(|i| TARGET[i] - pos[i]);
                let vector = vector_after_call.unwrap_or(delta);
                if !vector.iter().chain(&delta).all(|v| v.is_finite()) {
                    return Err(Error::invalid("invalid pickup vector"));
                }
                calls.push(json!({"vectorQuery":delta}));
                timer = timer.wrapping_add(if scalar < 6. { 1 } else { 3 });
                if scalar < 3. {
                    scale *= 0.9;
                }
                if scalar < 1.5 {
                    byte = 1;
                    calls.push(json!("setCollected"));
                }
                for i in 0..3 {
                    pos[i] += vector[i] * 0.1;
                }
                if !pos.iter().all(|v| v.is_finite()) {
                    return Err(Error::invalid("pickup position overflow"));
                }
                if scale < 0.2 {
                    pending = true;
                    calls.push(json!("requestCleanup"));
                }
            }
            _ => {
                return Err(Error::invalid(
                    "invalid pickup observation or callback order",
                ))
            }
        }
        out.push(json!({"position":pos,"timer":timer,"scale":scale,"gameByte44":byte,"calls":calls,"cleanupRequested":pending,"destroyed":ended}));
    }
    Ok(json!({"allocated":true,"sound":0x8006,"target":TARGET,"frames":out,"gameByte44":byte}))
}
