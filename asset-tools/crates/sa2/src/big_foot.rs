//! Big Foot's stage task creates the boss, then runs the slot-zero arena constraint.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    work_allocation_ok: bool,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    player_position: [f32; 3],
    vector_result: Option<[f32; 3]>,
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
    Ok(
        json!({"boss":boss,"stageExecutor":local(0x6704,1)?,"arenaRadius":radius,"frames":frames,"scope":"Boss setup and slot-zero arena constraint; player motion, vector helper, combat and mission resolution are not executed."}),
    )
}
