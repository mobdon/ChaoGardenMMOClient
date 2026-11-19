//! Big Foot mode 2. The outer engine supplies motion work and the animation query.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tick {
    position: [f32; 3],
    velocity_y: f32,
    motion_id: i32,
    motion_ready: Option<bool>,
}
pub(super) fn run(ticks: Vec<Tick>) -> Result<Value> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many takeoff callbacks"));
    }
    let (mut mode, mut sub, mut flags, mut timer) = (2, 0, 0x1001u16, 0);
    let (mut thrust, mut burst, mut ay, mut ax) = (0f32, 0f32, 0f32, 0f32);
    let mut phase = [0i32; 3];
    let mut rows = Vec::new();
    for t in ticks {
        let [x, mut y, z] = t.position;
        let mut vy = t.velocity_y;
        if mode != 2 || !vy.is_finite() || !t.position.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid("invalid mode-two update"));
        }
        let queried = matches!(sub, 1 | 4) || (matches!(sub, 2 | 3) && t.motion_id == 1);
        if queried != t.motion_ready.is_some() {
            return Err(Error::invalid(
                "motion_ready must match the animation query on this callback",
            ));
        }
        let ready = t.motion_ready == Some(true);
        let mut calls = vec![];
        match sub {
            0 => {
                calls.extend([
                    json!({"enableCollision":8}),
                    json!("helper58c78"),
                    json!({"motion":0}),
                ]);
                sub = 1;
            }
            1 => {
                thrust = (thrust + 0.05).min(1.5);
                calls.push(json!({"helper6b8":[thrust,thrust,thrust]}));
                if ready {
                    ay = 0.8;
                    calls.extend([
                        json!("helper58d30"),
                        json!({"motion":1}),
                        json!({"effectDescriptor":680}),
                    ]);
                    burst += 1.;
                    sub = 2;
                }
            }
            2 => {
                ay = ay * 0.95 - 0.002;
                vy *= 0.9;
                calls.push(json!({"helper6b8":null}));
                if ready {
                    flags &= !1;
                    calls.push(json!({"motion":2}));
                }
                if vy <= 0. {
                    sub = 3;
                }
            }
            3 => {
                phase[0] += 0x210;
                phase[1] += 0x250;
                if ready {
                    flags &= !1;
                    calls.push(json!({"motion":2}));
                }
                calls.extend([json!({"turnArgs":[64,0]}), json!({"helper6b8":null})]);
                let error = (y - 10.).clamp(-3., 3.);
                vy *= 0.8;
                ay -= 0.02 * error;
                if -0.05 < error && error < 0.05 && -0.05 < vy && vy < 0.05 {
                    y = 10.;
                    vy = 0.;
                    ay = 0.;
                    timer = 30;
                    sub = 4;
                }
            }
            4 => {
                phase[0] += 0x210;
                phase[1] += 0x250;
                phase[2] += 0x300;
                ax = if x > 0. { -0.01 } else { 0.01 };
                if ready {
                    mode = 3;
                    sub = 0;
                    timer = 0;
                    calls.push(json!({"motion":3}));
                    flags &= !0x200;
                }
                calls.push(json!({"helper6b8":null}));
            }
            _ => unreachable!(),
        }
        if flags & 1 == 0 {
            ay += 0.05;
        }
        if flags & 0x1000 != 0 && y > 5. {
            flags &= !0x1000;
        }
        if burst > 0. {
            if burst as i16 % 5 == 0 {
                calls.extend([
                    json!({"effectDescriptor":704}),
                    json!({"effectDescriptor":704,"byte0":4,"float4":5.}),
                ]);
            }
            burst += 1.;
            if burst == 15. {
                burst = 0.;
            }
        }
        rows.push(json!({"mode":mode,"submode":sub,"flags":flags,"timer":timer,"position":[x,y,z],"velocityY":vy,"acceleration":[ax,ay],"phase":phase,"intensity":thrust,"burst":burst,"calls":calls}));
    }
    Ok(json!(rows))
}
