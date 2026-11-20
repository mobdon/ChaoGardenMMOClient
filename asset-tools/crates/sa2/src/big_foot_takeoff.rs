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
pub(super) struct Handoff {
    pub phase: [i32; 3],
    pub flags: u16,
    pub pass_count: u8,
}

pub(super) fn run(ticks: Vec<Tick>) -> Result<(Value, Option<Handoff>)> {
    from_ground(ticks, 0x1001, 0., 0.)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Restart {
    velocity_x: f32,
    acceleration_y: f32,
    ticks: Vec<Tick>,
}
impl Restart {
    pub(super) fn run(self, flags: u16) -> Result<Value> {
        let (rows, _) = from_ground(self.ticks, flags, self.velocity_x, self.acceleration_y)?;
        Ok(rows)
    }
}
fn from_ground(
    ticks: Vec<Tick>,
    entry_flags: u16,
    mut ax: f32,
    mut ay: f32,
) -> Result<(Value, Option<Handoff>)> {
    if !ax.is_finite() || !ay.is_finite() {
        return Err(Error::invalid("invalid motion work at takeoff"));
    }
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many takeoff callbacks"));
    }
    let (mut mode, mut sub, mut flags, mut timer) = (2, 0, entry_flags, 0);
    let (mut thrust, mut burst) = (0f32, 0f32);
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
                    json!("helper58f6c"),
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
                        json!("helper59024"),
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
    Ok((
        json!(rows),
        (mode == 3).then_some(Handoff {
            phase,
            flags,
            pass_count: 3,
        }),
    ))
}
