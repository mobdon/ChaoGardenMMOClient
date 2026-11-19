//! Stage 20 registered entry task: initial service request and position tracking.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    mode: i8,
    position: [f32; 3],
    targets: Vec<Option<[f32; 3]>>,
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
    Ok(json!({"module":19,"entry":0x1b90,"frames":frames}))
}
