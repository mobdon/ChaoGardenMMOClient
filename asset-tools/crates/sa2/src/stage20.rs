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
    Ok(json!({"entry":0x16d0,"continueAt":0x176c,"operations":calls}))
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
