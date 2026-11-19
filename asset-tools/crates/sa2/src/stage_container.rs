//! Fresh stationary CONTCHAO task. Engine collision, RNG and shadow observations are inputs.
use crate::{set::Placement, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Frame {
    #[serde(default)]
    range_out: bool,
    #[serde(default)]
    equipment: u32,
    /// Present only when the hit bit and break predicate both succeeded.
    impact: Option<Impact>,
    shadow_result: Option<f32>,
    release: Option<Release>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Impact {
    player_slot: Option<u8>,
    rolls: [[u16; 3]; 14],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    has_condition: bool,
    flag_20: bool,
    broken: bool,
    has_key: bool,
    quarter_turn_sin_cos: Option<[[f32; 2]; 4]>,
}

const NO_FLOOR: f32 = -1000000.;
pub(crate) fn replay(
    p: &Placement,
    mut flags: u32,
    frames: Vec<Frame>,
    mut key: Option<crate::stage_key::Request>,
) -> Result<Value> {
    let [x, y, z] = p.position;
    let (mut bound, mut mode, mut timer, mut floor) = (false, 0u8, 0u16, NO_FLOOR);
    let mut trace = Vec::new();
    let mut key_replay = None;
    for f in frames {
        let mut calls = Vec::new();
        let mut collision_attr_bit = None;
        let exec = bound && (mode != 0 || !f.range_out);
        let hit = exec && mode == 0 && f.impact.is_some();
        let shadow_query =
            exec && mode == 0 && !hit && floor == NO_FLOOR && timer.wrapping_add(1) & 31 == 0;
        if f.impact.is_some() != hit
            || f.release.is_some() != (exec && mode == 2)
            || f.shadow_result.is_some() != shadow_query
            || f.shadow_result.is_some_and(|v| !v.is_finite())
        {
            return Err(Error::invalid(
                "observation does not match container callback phase",
            ));
        }
        if !bound && !f.range_out {
            bound = true;
            calls.push(json!({"initCollision":{"count":1,"id":4}}));
            calls.push(json!("containerSetup"));
        } else if exec {
            let equipped = f.equipment & 0x0200_0410 != 0;
            match mode {
                0 => {
                    if let Some(impact) = f.impact {
                        if impact.player_slot.is_some_and(|i| i > 1)
                            || impact.rolls.iter().flatten().any(|&v| v > 32767)
                        {
                            return Err(Error::invalid(
                                "invalid container impact slot or native rand value",
                            ));
                        }
                        calls.push(json!({"sound":{"id":0x1011,"priority":127,"duration":80,"position":p.position}}));
                        for [a, b, c] in impact.rolls {
                            let unit = |v: u16| f32::from(v) / 32768.;
                            calls.push(json!({"fragment":{"position":[x,y+10.,z],"args":[7.+3.*unit(c),0.5+unit(b),2.5+4.*unit(a)]}}));
                        }
                        mode = 1;
                        timer = 0;
                        calls.push(json!({"addScore":20}));
                        if let Some(slot) = impact.player_slot {
                            calls.push(json!({"setVelocity":{"slot":slot,"velocity":[0.,0.,0.]}}));
                        }
                    } else {
                        timer = timer.wrapping_add(1);
                        if let Some(result) = f.shadow_result {
                            calls.push(json!({"shadowQuery":{"origin":[x,y+5.,z],"yaw":p.rotation[1] as i16}}));
                            floor = result;
                        }
                        collision_attr_bit = Some(equipped);
                        calls.push(json!("registerCollision"));
                    }
                }
                1 => {
                    if timer == 1 {
                        calls.push(json!({"breakEffect":p.position}));
                    }
                    let old = timer;
                    timer = timer.wrapping_add(1);
                    if old > 2 {
                        calls.push(json!({"explosion":{"position":p.position,"yaw":p.rotation[1] as i16,"pieces":9,"args":[0.3,1.2],"floor":if floor==NO_FLOOR {y}else{floor},"velocity":[0.,0.4,0.]}}));
                        mode = 2;
                    }
                    collision_attr_bit = Some(equipped);
                    if !equipped {
                        calls.push(json!("registerCollision"));
                    }
                }
                2 => {
                    let r = f.release.as_ref().unwrap();
                    let four = r.has_condition
                        && !r.flag_20
                        && !r.broken
                        && r.has_key
                        && flags & 0x100 == 0;
                    if four != r.quarter_turn_sin_cos.is_some()
                        || r.quarter_turn_sin_cos
                            .iter()
                            .flatten()
                            .flatten()
                            .any(|v| !(-1.0..=1.0).contains(v))
                    {
                        return Err(Error::invalid("expected four native sin/cos observations exactly on the four-spawn route"));
                    }
                    if r.has_condition && !r.flag_20 && !r.broken {
                        if !r.has_key {
                            if let Some(req) = key.take() {
                                key_replay = Some(crate::stage_key::replay(
                                    p.position,
                                    if floor == NO_FLOOR { y } else { floor },
                                    req,
                                )?);
                            }

                            calls.push(json!({"spawnKey":{"position":p.position,"floor":if floor==NO_FLOOR {y}else{floor},"speed":2.5}}));
                        } else if flags & 0x100 == 0 {
                            flags |= 0x100;
                            for ([sin, cos], kind) in r
                                .quarter_turn_sin_cos
                                .unwrap()
                                .into_iter()
                                .zip([2, 1, 2, 1])
                            {
                                let dx = 10.0 * sin;
                                let dz = 10.0 * cos;
                                calls.push(
                                    json!({"spawn":{"position":[x+dx,y+10.,z+dz],"kind":kind}}),
                                );
                            }
                        } else if flags & 0x200 == 0 {
                            flags |= 0x200;
                            calls.push(json!({"spawn":{"position":[x,y+10.,z],"kind":0}}));
                        }
                    }
                    if r.has_condition {
                        calls.extend([json!("setBroken"), json!("setFlag20"), json!("deadOut")]);
                    } else {
                        calls.push(json!("freeTask"));
                    }
                }
                _ => unreachable!(),
            }
        }
        let finished = exec && f.release.is_some();
        trace.push(json!({"bound":bound,"mode":mode,"timer":timer,"floor":floor,"flags":flags,"collisionAttrBit4000":collision_attr_bit,"calls":calls,"finished":finished}));
        if finished {
            break;
        }
    }
    if key.is_some() {
        return Err(Error::invalid("no key spawned by the container trace"));
    }
    let mut output = json!({"callback":"ContainerChao","frames":trace});
    if let Some(replay) = key_replay {
        output["key"] = replay;
    }
    Ok(output)
}
