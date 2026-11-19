//! Wild Canyon stage registration and its first scheduled callback (GSNE8P module 15).
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde_json::{json, Value};

pub fn enter(module: &[u8], allocated: bool) -> Result<Value> {
    let invalid = || Error::invalid("unverified Wild Canyon stage program");
    let info = stage_land::catalog()
        .iter()
        .find(|s| s.filename == "stg16D.rel")
        .ok_or_else(invalid)?;
    if module.len() != info.size || manifest::sha256(module) != info.sha256 {
        return Err(invalid());
    }
    let rel = CityRel::read_identified(module, info.module)?;
    let pair = |pc: u32, section: u8| -> Result<u32> {
        let h = rel.reference(1, pc + 2).ok_or_else(invalid)?;
        let l = rel.reference(1, pc + 6).ok_or_else(invalid)?;
        if (h.module, h.section, h.kind) != (15, section, 6)
            || (l.module, l.section, l.kind) != (15, section, 4)
            || l.addend != h.addend
        {
            return Err(invalid());
        }
        Ok(h.addend)
    };
    let data = rel.data(5)?;
    let desc = pair(0x8abc, 5)?;
    let callback = |delta| {
        rel.local_pointer(5, desc + delta)?
            .filter(|(s, _)| *s == 1)
            .map(|(_, a)| a)
            .ok_or_else(invalid)
    };
    let name = |offset: u32| -> Result<&str> {
        let tail = data.get(offset as usize..).ok_or_else(invalid)?;
        std::str::from_utf8(&tail[..tail.iter().position(|&b| b == 0).ok_or_else(invalid)?])
            .map_err(|_| invalid())
    };
    let ptr = |at| -> Result<Option<u32>> {
        match rel.local_pointer(5, at)? {
            Some((5, v)) => Ok(Some(v)),
            None => Ok(None),
            _ => Err(invalid()),
        }
    };
    let group = |tex_pc, objects_pc| -> Result<Value> {
        let textures = pair(tex_pc, 5)?;
        let objects = pair(objects_pc, 5)?;
        let mut rows = Vec::new();
        for i in 0..128 {
            let at = textures + 8 * i;
            let Some(label) = ptr(at)? else {
                break;
            };
            rows.push(json!({"name":name(label)?,"textureList":ptr(at+4)?.ok_or_else(invalid)?}));
        }
        let mut entries = Vec::new();
        for i in 0..1024 {
            let Some(entry) = ptr(objects + 4 * i)? else {
                break;
            };
            entries.push(entry);
        }
        if rows.len() == 128 || entries.len() == 1024 {
            return Err(invalid());
        }
        Ok(
            json!({"textures":textures,"objects":objects,"textureRows":rows,"objectOffsets":entries}),
        )
    };
    let land = pair(0x8b1c, 5)?;
    let land_file = rel.sections().get(5).ok_or_else(invalid)?.file_offset as usize + land as usize;
    if land_file != info.file_offset as usize {
        return Err(invalid());
    }
    let engine = |pc| {
        rel.reference(1, pc)
            .filter(|r| r.module == 0 && r.kind == 10)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let groups = [group(0x8b28, 0x8b34)?, group(0x8b50, 0x8b5c)?];
    let calls = json!([
        {"engine":engine(0x8afc)?,"args":[50,50,170]},
        {"engine":engine(0x8b18)?,"args":[2,0,pair(0x8b08,1)?,name(pair(0x8b10,5)?)?]},
        {"engine":engine(0x8b24)?,"args":[land]},
        {"engine":engine(0x8b40)?,"args":[groups[0]["textures"],groups[0]["objects"]]},
        {"engine":engine(0x8b4c)?,"args":[groups[0]["textures"],groups[0]["objects"]]},
        {"engine":engine(0x8b68)?,"args":[groups[1]["textures"],groups[1]["objects"]]},
        {"engine":engine(0x8b74)?,"args":[groups[1]["textures"],groups[1]["objects"]]}
    ]);
    let child = if allocated {
        let mut position = [0.; 3];
        for (v, pc) in position.iter_mut().zip([0x9184, 0x9198, 0x91ac]) {
            let i = pair(pc, 4)? as usize;
            *v = f32::from_be_bytes(
                rel.data(4)?
                    .get(i..i + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            );
        }
        Some(json!({"position":position,"callbackAt20":pair(0x9178,1)?}))
    } else {
        None
    };
    Ok(
        json!({"descriptor":name(desc)?,"startup":callback(12u32)?,"teardown":callback(16u32)?,"entry":callback(20u32)?,
        "create":{"arguments":[2,2],"name":name(pair(0x9164,5)?)?,"main":pair(0x915c,1)?},
        "startupResources":{"calls":calls,"groups":groups},"created":child,"nextMain":pair(0x91c0,1)?}),
    )
}
