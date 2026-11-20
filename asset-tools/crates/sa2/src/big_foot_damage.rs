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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct After {
    position: [f32; 3],
    angles: [i32; 3],
    ticks: Vec<AfterTick>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AfterTick {
    engine_byte_43: i8,
    approach_result: Option<i32>,
}
fn after(low: bool, input: After, target: [f32; 2]) -> Result<Value> {
    let mut position = input.position;
    let mut angles = input.angles;
    if !position.iter().all(|v| v.is_finite()) || input.ticks.len() > 2048 {
        return Err(Error::invalid("invalid post-defeat replay"));
    }
    let mut timer = u16::MAX;
    let mut rows = Vec::new();
    for tick in input.ticks {
        timer = timer.wrapping_add(1);
        if tick.engine_byte_43 == 1 {
            position = [target[0], position[1], target[1]];
            angles[1] = 0x4000; // Overrides the helper's heading result.
        }
        let mut approach = None;
        match (low, tick.approach_result) {
            (false, Some(result)) => {
                approach = Some([angles[0], -0x1000, 0x100]);
                angles[0] = result;
            }
            (true, None) => (),
            _ => {
                return Err(Error::invalid(
                    "post-defeat angle observation does not match route",
                ))
            }
        }
        let effect = (low && timer.is_multiple_of(5))
            .then(|| json!({"kind":5,"position":position,"angles":angles,"parameterOffset":1020}));
        rows.push(json!({"timer":timer,"position":position,"angles":angles,"headingQuery":tick.engine_byte_43==1,"approach":approach,"effect":effect}));
    }
    Ok(json!(rows))
}
pub(super) fn run(
    start: f32,
    frames: Vec<Frame>,
    post: Option<After>,
    target: impl Fn(bool) -> Result<[f32; 2]>,
) -> Result<Value> {
    if frames.len() > 2048 {
        return Err(Error::invalid("too many boss damage updates"));
    }
    let mut health = start;
    let mut rows = Vec::new();
    let mut defeat = None;
    let mut route = None;
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
                if health <= 0. {
                    route = Some(mode == 10);
                    defeat = Some(defeat_clock(mode == 10));
                }
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
    let mut result = json!({"initialHealth":start,"frames":rows,"defeatClock":defeat});
    if let Some(input) = post {
        let low = route.ok_or_else(|| Error::invalid("post_defeat requires lethal damage"))?;
        result["postDefeat"] = after(low, input, target(low)?)?;
    }
    Ok(result)
}

/// Project only the boss-owned timer, starting on the first update after the lethal hit.
/// Both callbacks initialize and decrement on entry, and branch on the pre-decrement value.
fn defeat_clock(low: bool) -> Value {
    let mut frames = Vec::new();
    let mut submode = 1;
    let mut timer: u16 = if low { 90 } else { 240 };
    loop {
        let expired = timer == 0;
        timer = timer.wrapping_sub(1);
        if expired {
            submode += 1;
            if low && submode == 2 {
                timer = 240;
            }
        }
        frames.push(json!({"submode":submode,"timer":timer}));
        if expired && submode == if low { 3 } else { 2 } {
            break;
        }
    }
    json!({"handoffUpdate":frames.len(),"frames":frames,"callsOnHandoff":[
        {"address":"8001c810","argument":0},{"address":"80022e24","argument":0}]})
}
