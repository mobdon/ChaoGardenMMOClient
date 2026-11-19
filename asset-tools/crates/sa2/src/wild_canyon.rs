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
        "created":child,"nextMain":pair(0x91c0,1)?}),
    )
}
