//! Stage 20 registered entry task: initial service request and position tracking.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    startup: Option<Startup>,
    mode: i8,
    position: [f32; 3],
    targets: Vec<Option<[f32; 3]>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Startup {
    character_queries: Vec<i32>,
    slot_one_present: bool,
    walker: Option<Walker>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Walker {
    task_allocated: bool,
    work_allocated: Option<bool>,
    character: Option<i8>,
    slot_value: Option<f32>,
}
fn walker_setup(rel: &CityRel<'_>, w: Walker) -> Result<Value> {
    let invalid = || Error::invalid("inconsistent walker initialization observation");
    let ptr = |site, section| {
        rel.reference(1, site)
            .filter(|r| r.module == 19 && r.section == section && r.kind == 4)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let data = rel.data(5)?;
    let label = |site| -> Result<&str> {
        let at = ptr(site, 5)? as usize;
        let tail = data.get(at..).ok_or_else(invalid)?;
        std::str::from_utf8(&tail[..tail.iter().position(|&v| v == 0).ok_or_else(invalid)?])
            .map_err(|_| invalid())
    };
    let engine = |site| {
        rel.reference(1, site)
            .filter(|r| r.module == 0 && r.kind == 10)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let mut ops =
        vec![json!({"engine":engine(0x15b4)?,"args":[0,0,ptr(0x15aa,1)?,label(0x15b2)?]})];
    if w.task_allocated != w.work_allocated.is_some() {
        return Err(invalid());
    }
    if let Some(ok) = w.work_allocated {
        ops.push(json!({"engine":engine(0x15cc)?,"args":[1,24]}));
        if !ok {
            ops.push(json!({"engine":engine(0x15e0)?,"args":["task"]}));
        }
    }
    if w.work_allocated == Some(true) {
        let at = ptr(0x15f2, 4)? as usize;
        let factor = f32::from_be_bytes(rel.data(4)?[at..at + 4].try_into().unwrap());
        let v = w.slot_value.ok_or_else(invalid)? * factor;
        ops.push(json!({"taskWorkOffset":64,"value":"work"}));
        ops.push(json!({"local":0x3361c,"args":["task",v]}));
        ops.push(json!({"workByte":0,"value":0}));
        let resource = match w.character.ok_or_else(invalid)? {
            6 => Some((0x1642, 0x164a, 0x164c, 0x1656)),
            7 => Some((0x166e, 0x1676, 0x1678, 0x1682)),
            _ => None,
        };
        if let Some((name, list, site, dest)) = resource {
            ops.push(json!({"engine":engine(site)?,"args":[label(name)?,ptr(list,5)?]}));
            ops.push(json!({"taskCallbackOffset":24,"local":ptr(dest,1)?}));
        }
    } else if w.character.is_some() || w.slot_value.is_some() {
        return Err(invalid());
    }
    Ok(json!(ops))
}
/// Registered startup up to the first unconditional resource call (1:0x176c).
fn opponent_startup(rel: &CityRel<'_>, s: Startup) -> Result<Value> {
    let mut queries = s.character_queries.into_iter();
    let mut calls = Vec::new();
    let mut engine = |site: u32, args: &[i32]| -> Result<()> {
        let target = rel
            .reference(1, site)
            .filter(|r| r.module == 0 && r.kind == 10)
            .ok_or_else(|| Error::invalid("missing duel startup call"))?;
        calls.push(json!({"site":site,"engine":target.addend,"args":args}));
        Ok(())
    };
    let mut query = || {
        queries
            .next()
            .ok_or_else(|| Error::invalid("missing slot-zero character observation"))
    };
    engine(0x16e8, &[0])?;
    if query()? == 7 {
        engine(0x16f8, &[1])?;
    } else {
        engine(0x1704, &[0])?;
        if query()? == 6 {
            engine(0x1714, &[1])?;
        }
    }
    if queries.next().is_some() {
        return Err(Error::invalid("unused slot-zero character observation"));
    }
    if s.slot_one_present {
        engine(0x1730, &[1])?;
        calls.push(json!({"site":0x1744,"slot":1,"workByte":2,"value":3}));
        let r = rel
            .reference(1, 0x174c)
            .ok_or_else(|| Error::invalid("missing duel startup call"))?;
        calls.push(json!({"site":0x174c,"engine":r.addend,"args":[1]}));
        calls.push(json!({"site":0x1750,"local":0x1584,"args":[]}));
        for (site, slot) in [(0x175c, 0), (0x1768, 1)] {
            calls.push(json!({"site":site,"local":0x139e4,"args":[slot,0]}));
        }
    }
    let mut report = json!({"entry":0x16d0,"continueAt":0x176c,"operations":calls});
    if let Some(w) = s.walker {
        if !s.slot_one_present {
            return Err(Error::invalid("walker requires slot one"));
        }
        report["walker"] = walker_setup(rel, w)?;
    }
    Ok(report)
}
pub fn replay(module: &[u8], request: &[u8]) -> Result<Value> {
    let invalid = || Error::invalid("unexpected stage 20 entry module");
    let source = stage_land::catalog()
        .iter()
        .find(|s| s.filename == "stg20D.rel")
        .ok_or_else(invalid)?;
    if module.len() != source.size || manifest::sha256(module) != source.sha256 {
        return Err(invalid());
    }
    let rel = CityRel::read_identified(module, source.module)?;
    let constant = |pc| -> Result<f32> {
        let r = rel.reference(1, pc).ok_or_else(invalid)?;
        if (r.module, r.section, r.kind) != (19, 4, 4) {
            return Err(invalid());
        }
        let at = r.addend as usize;
        Ok(f32::from_be_bytes(
            rel.data(4)?
                .get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let Request {
        startup,
        mut mode,
        mut position,
        targets,
    } = serde_json::from_slice(request)?;
    let mut frames = Vec::with_capacity(targets.len());
    for target in targets {
        let mut call = None;
        if mode == 0 {
            let f = constant(0x1bd6)?;
            call = Some(
                json!({"engine":0x800c82dcu32,"slot":0,"positionAtCall":position,"float4":f,"float8":f,
                "word12":10,"byte16":1,"byte17":0,"float20":constant(0x1bee)?,"float24":constant(0x1c16)?,"word28":0}),
            );
            mode = 1;
        }
        if mode == 1 {
            if let Some(p) = target {
                position = p;
                position[1] += constant(0x1c6a)?;
            }
        }
        frames.push(json!({"mode":mode,"position":position,"call":call}));
    }
    let mut report = json!({"module":19,"entry":0x1b90,"frames":frames});
    if let Some(s) = startup {
        report["startup"] = opponent_startup(&rel, s)?;
    }
    Ok(report)
}
