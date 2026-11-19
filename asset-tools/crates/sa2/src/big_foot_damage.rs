//! Post-AI Big Foot hit response. The collision search and intervening AI are external.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    mode: i8,
    submode: u8,
    byte3: u8,
    timer: u16,
    flags: u16,
    height: f32,
    yaw: i32,
    child_hit: Option<bool>,
    body_hit: Option<bool>,
    player: Option<Player>,
    damage_work: Option<DamageWork>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Player {
    flags: u16,
    yaw: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DamageWork {
    boss_shape_bytes: [u8; 11],
    child_shape_byte: u8,
    child_flags: u16,
    child_collision_flags: u16,
}
fn push(yaw: i32, velocity: [f32; 3]) -> Value {
    json!({"slot":0,"velocity":velocity,"angles":[0,yaw.wrapping_add(0x8000),0],"duration":60})
}
pub(super) fn run(start: f32, frames: Vec<Frame>) -> Result<Value> {
    if frames.len() > 2048 {
        return Err(Error::invalid("too many boss damage updates"));
    }
    let mut health = start;
    let mut rows = Vec::new();
    for f in frames {
        if !f.height.is_finite() || health <= 0. {
            return Err(Error::invalid("invalid or post-lethal damage update"));
        }
        let (mut mode, mut sm, mut b3, mut timer, mut flags) =
            (f.mode, f.submode, f.byte3, f.timer, f.flags);
        let checks_child = !matches!(mode, 10 | 13);
        let accepted = f.child_hit == Some(true) && flags & 0x1004 == 0;
        let checks_body = checks_child && !accepted;
        let damaged = accepted && mode != 9;
        if checks_child != f.child_hit.is_some()
            || checks_body != f.body_hit.is_some()
            || (f.body_hit == Some(true)) != f.player.is_some()
            || damaged != f.damage_work.is_some()
        {
            return Err(Error::invalid(
                "observations do not match native boss hit queries",
            ));
        }
        let mut impulse = None;
        let mut work = None;
        if accepted {
            impulse = Some(push(f.yaw, [-2.5, 1.8, 0.]));
            if let Some(d) = f.damage_work {
                health -= 1.;
                let damage_byte = |v: u8| (v & 0xf0) | 0x0c;
                work = Some(
                    json!({"bossShapeBytes":d.boss_shape_bytes.map(damage_byte),"childShapeByte":damage_byte(d.child_shape_byte),
                    "childFlags":(d.child_flags & !0x100)|4,"childCollisionFlags":d.child_collision_flags & !0x40}),
                );
                mode = if health > 0. {
                    if flags & 1 != 0 {
                        9
                    } else {
                        12
                    }
                } else {
                    flags &= !4;
                    if f.height < 5. {
                        10
                    } else {
                        13
                    }
                };
                sm = 0;
                b3 = 0;
                timer = 0;
            }
        } else if let Some(p) = f.player {
            if p.flags & 0x100 != 0 && flags & 4 == 0 {
                impulse = Some(push(p.yaw, [-1., 1., 0.]));
            }
        }
        if checks_child && flags & 4 != 0 {
            b3 = b3.wrapping_add(1);
            if b3 == 10 {
                b3 = 0;
            }
        }
        rows.push(json!({"mode":mode,"submode":sm,"byte3":b3,"timer":timer,"flags":flags,"health":health,"impulse":impulse,"damageWork":work}));
    }
    Ok(json!({"initialHealth":start,"frames":rows}))
}
