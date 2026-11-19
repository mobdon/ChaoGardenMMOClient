//! Big Foot's stage task creates the boss, then runs the slot-zero arena constraint.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
#[path = "big_foot_collision.rs"]
mod collision;
#[path = "big_foot_damage.rs"]
mod damage;
#[path = "big_foot_takeoff.rs"]
mod takeoff;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    work_allocation_ok: bool,
    frames: Vec<Frame>,
    intro: Option<Intro>,
    collision: Option<Vec<collision::Tick>>,
    damage: Option<Vec<damage::Frame>>,
    takeoff: Option<Vec<takeoff::Tick>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    player_position: [f32; 3],
    vector_result: Option<[f32; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intro {
    updates: u16,
    helper_task_created: bool,
    exit: Option<IntroExit>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntroExit {
    query_result: i32,
    player_position: Option<[f32; 3]>,
}
fn intro(position: [f32; 3], cfg: Intro, scalar: &impl Fn(u32) -> Result<f32>) -> Result<Value> {
    if cfg.updates > 451 || (cfg.updates == 451) != cfg.exit.is_some() {
        return Err(Error::invalid("expected at most 451 mode-one callbacks, with exit observations only on the final callback"));
    }
    let mut mode = 1;
    let mut submode = 0;
    let mut timer = 0u16;
    let mut flags = 1;
    let focus = [position[0], position[1] + scalar(0xdc4)?, position[2]];
    let mut rows = Vec::new();
    for _ in 0..cfg.updates {
        let mut calls = vec![];
        if submode == 0 {
            calls.push(json!({"openingView":{"slot":0,"args":[scalar(0xe0c)?,24576,scalar(0xe34)?,scalar(0xe44)?,0x471c],"focus":focus}}));
            calls.push(json!({"setPlayerPosition":{"slot":0,"position":[-position[0],0.,0.]}}));
            calls.push(json!({"setMotionLinkSync":6}));
            calls.push(json!("createHelper53da8"));
            calls.push(json!("createHelper59bc4"));
            calls.push(json!("setup54b74"));
            timer = 450;
            submode = 1;
        } else {
            timer -= 1;
            if timer == 0 {
                let exit = cfg.exit.as_ref().unwrap();
                calls.push(json!("endOpeningView"));
                calls.push(json!({"nextView":{"slot":0,"focus":focus,"args":[scalar(0xf44)?,scalar(0xf54)?,10,1,0,scalar(0xf64)?,scalar(0xf8c)?,0]}}));
                calls.push(json!("engineCall80022c1c"));
                flags |= 0x1000;
                if cfg.helper_task_created {
                    calls.push(json!("freeHelper59bc4"));
                }
                let index = exit.query_result.wrapping_sub(1);
                if (index >= 0) != exit.player_position.is_some() {
                    return Err(Error::invalid(
                        "exit requires player position only on the spawn branch",
                    ));
                }
                if let Some([x, y, z]) = exit.player_position {
                    let point = [x, y + scalar(0x547b4)?, z];
                    if !point.iter().all(|v| v.is_finite()) {
                        return Err(Error::invalid("invalid exit position"));
                    }
                    calls.push(json!({"local2e4b4":{"position":point,"slot":0,"argument":index.min(2),"kind":2}}));
                }
                mode = 2;
                submode = 0;
            }
        }
        rows.push(json!({"mode":mode,"submode":submode,"timer":timer,"flags":flags,"calls":calls}));
    }
    Ok(json!({"focus":focus,"frames":rows}))
}
pub fn replay(module: &[u8], request: &[u8]) -> Result<Value> {
    let invalid = || Error::invalid("unverified Big Foot stage program");
    let info = stage_land::catalog()
        .iter()
        .find(|i| i.filename == "boss_bigfootD.rel")
        .ok_or_else(invalid)?;
    if module.len() != info.size || manifest::sha256(module) != info.sha256 {
        return Err(invalid());
    }
    let rel = CityRel::read_identified(module, info.module)?;
    let local = |pc: u32, section: u8| -> Result<u32> {
        let a = rel.reference(1, pc + 2).ok_or_else(invalid)?;
        let b = rel.reference(1, pc + 6).ok_or_else(invalid)?;
        if (
            a.module, a.section, a.kind, b.module, b.section, b.kind, b.addend,
        ) != (81, section, 6, 81, section, 4, a.addend)
        {
            return Err(invalid());
        }
        Ok(a.addend)
    };
    let scalar = |pc| -> Result<f32> {
        let at = local(pc, 4)? as usize;
        Ok(f32::from_be_bytes(
            rel.data(4)?
                .get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let name = local(0x2c, 5)? as usize;
    if rel.data(5)?.get(name..name + 16) != Some(b"BossBigFootExec\0") {
        return Err(invalid());
    }
    let descriptor = local(0x6440, 5)?;
    let start = rel.local_pointer(5, descriptor + 20)?.ok_or_else(invalid)?;
    if start != (1, 0x66bc) {
        return Err(invalid());
    }
    let position = [scalar(0x66d0)?, scalar(0x66e0)?, scalar(0x66e0)?];
    let radius = scalar(0x675c)?;
    let req: Request = serde_json::from_slice(request)?;
    if req.frames.len() > 2048 {
        return Err(Error::invalid("too many Big Foot frames"));
    }
    let boss = if req.work_allocation_ok {
        json!({"created":true,"position":position,"yaw":-16384,"executor":local(0x24,1)?,"mode":1,"timer":0,"taskByte2":6,"animationRequest":6,"collision":{"descriptorOffset":local(0x170,5)?,"count":11,"id":3}})
    } else {
        json!({"created":false,"calls":["freeBossTask"]})
    };
    if req.takeoff.is_some() && !req.intro.as_ref().is_some_and(|i| i.updates == 451) {
        return Err(Error::invalid("takeoff requires a completed intro"));
    }
    let intro_trace = match req.intro {
        Some(i) if req.work_allocation_ok => Some(intro(position, i, &scalar)?),
        Some(_) => return Err(Error::invalid("intro requires an allocated boss")),
        None => None,
    };
    let mut frames = Vec::new();
    for f in req.frames {
        let [x, y, z] = f.player_position;
        let squared = x * x + z * z;
        if !squared.is_finite() || !y.is_finite() {
            return Err(Error::invalid("unrepresentable Big Foot player position"));
        }
        let outside = squared.sqrt() > radius;
        if outside != f.vector_result.is_some()
            || f.vector_result
                .is_some_and(|v| !v.iter().all(|x| x.is_finite()))
        {
            return Err(Error::invalid(
                "outside the arena supply the native vector helper result",
            ));
        }
        let result = f
            .vector_result
            .map_or(f.player_position, |v| [radius * v[0], y, radius * v[2]]);
        if !result.iter().all(|x| x.is_finite()) {
            return Err(Error::invalid("arena position overflow"));
        }
        frames.push(json!({"calledVectorHelper":outside,"playerPosition":result}));
    }
    let mut output = json!({"boss":boss,"stageExecutor":local(0x6704,1)?,"arenaRadius":radius,"frames":frames,"scope":"Boss setup and slot-zero arena constraint; player motion, vector helper, combat and mission resolution are not executed."});
    if let Some(trace) = intro_trace {
        output["intro"] = trace;
    }
    if let Some(ticks) = req.takeoff {
        output["takeoff"] = takeoff::run(ticks)?;
    }
    if let Some(ticks) = req.collision {
        if !req.work_allocation_ok {
            return Err(Error::invalid(
                "collision update requires an allocated boss",
            ));
        }
        output["collision"] = collision::run(&rel, ticks)?;
    }
    if let Some(frames) = req.damage {
        if !req.work_allocation_ok {
            return Err(Error::invalid("damage requires an allocated boss"));
        }
        output["damage"] = damage::run(scalar(0x294)?, frames)?;
    }
    Ok(output)
}
