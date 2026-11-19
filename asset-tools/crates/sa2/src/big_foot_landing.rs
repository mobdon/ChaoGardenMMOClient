//! Uninterrupted, full-health mode-five descent, touchdown, and animation exit.
use super::aim::Entry;
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(super) enum Tick {
    Align {
        velocity_y: f32,
        acceleration_y: f32,
        adjusted_angles: [i32; 3],
        heading_difference: Option<i32>,
    },
    Descend {
        height: f32,
    },
    Recover {
        motion_ready: bool,
    },
}

pub(super) fn run(mut s: Entry, ticks: Vec<Tick>, spawn_order: &[u8; 16]) -> Result<Value> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many landing updates"));
    }
    let (mut mode, mut sub) = (5, 6);
    let mut thrust = 0.8f32;
    let mut rows = Vec::new();
    for t in ticks {
        let mut calls = Vec::new();
        let mut vertical = None;
        let mut height = None;
        match (mode, sub, t) {
            (
                5,
                6,
                Tick::Align {
                    velocity_y,
                    acceleration_y,
                    adjusted_angles,
                    heading_difference,
                },
            ) => {
                if !velocity_y.is_finite()
                    || !acceleration_y.is_finite()
                    || (velocity_y >= -0.1) != heading_difference.is_some()
                {
                    return Err(Error::invalid("invalid landing alignment observation"));
                }
                let mut ay = acceleration_y + 0.05;
                for (i, v) in [0, 2, 1].into_iter().zip(adjusted_angles) {
                    let target = if i == 1 { s.target[1] } else { 0 };
                    calls.push(json!({"angle":[s.angles[i],target,256]}));
                    s.angles[i] = v;
                }
                thrust = 0.8;
                calls.push(json!({"helper6b8":[thrust,thrust,thrust]}));
                if velocity_y < -0.1 {
                    ay += 0.005;
                    vertical = Some([velocity_y, ay]);
                } else {
                    vertical = Some([0., ay]);
                    calls.push(json!({"compareYaw":[s.angles[1],s.target[1]]}));
                    if heading_difference == Some(0) && s.angles[0] == 0 && s.angles[2] == 0 {
                        calls.push(json!({"engine2558":[11,0,0,10]}));
                        calls.push(json!({"motion":5}));
                        for pair in spawn_order.as_chunks::<2>().0 {
                            let offset = if pair[0] == 0 { 48 } else { 0 };
                            calls.push(json!({"helper57524":[offset+12*u32::from(pair[1]),120]}));
                        }
                        s.flags |= 0x100;
                        calls.push(json!({"disableCollision":8}));
                        sub = 7;
                    }
                }
            }
            (5, 7, Tick::Descend { height: y }) if y.is_finite() => {
                thrust = (thrust - 0.05).max(0.);
                calls.push(json!({"helper6b8":[thrust,thrust,thrust]}));
                height = Some(y);
                if y <= 0. {
                    s.angles[0] = 0;
                    s.angles[2] = 0;
                    s.rates = [0.; 3];
                    s.flags |= 1;
                    height = Some(0.);
                    calls.extend([
                        json!("helper55148"),
                        json!({"engine2558":[7,0,0,100]}),
                        json!("engine276c"),
                        json!({"effectDescriptor":844}),
                        json!({"engine27c4":[0,2,45,1]}),
                    ]);
                    sub = 8;
                }
            }
            (5, 8, Tick::Recover { motion_ready }) => {
                calls.push(json!("motionReady"));
                if motion_ready {
                    mode = 8;
                    sub = 0;
                }
            }
            _ => return Err(Error::invalid("landing update does not match native state")),
        }
        rows.push(json!({"mode":mode,"submode":sub,"flags":s.flags,"angles":s.angles,"rates":s.rates,"intensity":thrust,
            "vertical":vertical,"height":height,"calls":calls}));
    }
    Ok(json!(rows))
}
