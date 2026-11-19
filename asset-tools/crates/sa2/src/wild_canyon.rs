//! Wild Canyon stage registration and its first scheduled callback (GSNE8P module 15).
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde_json::{json, Value};

pub fn enter(
    module: &[u8],
    allocated: bool,
    stage: &crate::fan_stage::MountedStage<'_>,
) -> Result<Value> {
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
    let code = |row| {
        rel.local_pointer(5, row)?
            .filter(|(s, _)| *s == 1)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let mut emeralds = Vec::new();
    for (layer_index, layer) in stage.layers.iter().enumerate() {
        for bound in &layer.placements {
            let Some((_, Some(def))) = stage.resolve(layer_index, bound.placement_index) else {
                continue;
            };
            if !matches!(def.name.as_str(), "EMERALD" | "EMERALD_F") {
                continue;
            }
            let object = 0x7c6f8 + 16 * u32::from(bound.placement.object_id);
            let label = rel
                .local_pointer(5, object + 12)?
                .filter(|(s, _)| *s == 5)
                .ok_or_else(invalid)?
                .1;
            if name(label)? != def.name || code(object + 8)? != 0x1fcb0 {
                return Err(invalid());
            }
            let variant = (bound.placement.rotation[0] >> 8) % 11;
            let row = pair(0x1fcd4, 5)? + 36 * u32::from(variant);
            let display = code(row + 24)?;
            let conditional = display == pair(0x1fd10, 1)?;
            let init = code(row + 8)?;
            let setup = if init == 0x20780 {
                let word = |bytes: &[u8], at: usize| -> Result<u32> {
                    Ok(u32::from_be_bytes(
                        bytes
                            .get(at..at + 4)
                            .ok_or_else(invalid)?
                            .try_into()
                            .unwrap(),
                    ))
                };
                let add = f32::from_bits(word(rel.data(4)?, pair(0x2081c, 4)? as usize)?);
                Some(
                    json!({"typeWord":word(data,row as usize)?,"lowByte":bound.placement.rotation[0]&255,
                    "position":bound.placement.position,"shapeOffset":pair(0x20808,5)?,"count":1,"id":6,
                    "shapeParameter":bound.placement.parameters[1]+add,"skipIfModeZeroAndQueryNonzero":true}),
                )
            } else {
                None
            };
            emeralds.push(json!({"layer":layer.filename,"index":bound.placement_index,"object":def.name,"variant":variant,
                "main":code(row+20)?,"destroy":code(row+28)?,"display":display,"defaultDisplaySlot":32,
                "nonzeroQueryDisplaySlot":conditional.then_some(20),"initialize":init,"commonSetup":setup}));
        }
    }
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
    let scalar = |pc| -> Result<f32> {
        let i = pair(pc, 4)? as usize;
        Ok(f32::from_be_bytes(
            rel.data(4)?
                .get(i..i + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let table = pair(0x8bf0, 5)?;
    let first = scalar(0x8ba8)?;
    let second = scalar(0x8bb4)?;
    // Keep the engine return opaque. It is saved and passed on without a null check.
    let saved = json!({"returnFrom":0x8b80,"bssOffset":pair(0x8b84,6)?});
    let calls = json!([
        {"engine":engine(0x8afc)?,"args":[50,50,170]},
        {"engine":engine(0x8b18)?,"args":[2,0,pair(0x8b08,1)?,name(pair(0x8b10,5)?)?]},
        {"engine":engine(0x8b24)?,"args":[land]},
        {"engine":engine(0x8b40)?,"args":[groups[0]["textures"],groups[0]["objects"]]},
        {"engine":engine(0x8b4c)?,"args":[groups[0]["textures"],groups[0]["objects"]]},
        {"engine":engine(0x8b68)?,"args":[groups[1]["textures"],groups[1]["objects"]]},
        {"engine":engine(0x8b74)?,"args":[groups[1]["textures"],groups[1]["objects"]]},
        {"engine":engine(0x8b80)?,"args":[0,2048]},
        {"engine":engine(0x8b9c)?,"args":[pair(0x8b90,5)?,saved]},
        {"engine":engine(0x8ba0)?,"args":[]},{"engine":engine(0x8ba4)?,"args":[]},
        {"engine":engine(0x8bc4)?,"floats":[first,second,first]},
        {"engine":engine(0x8bd8)?,"floats":[second,scalar(0x8bcc)?]},
        {"engine":engine(0x8be8)?,"floats":[second,second,second]},
        {"engine":engine(0x8bfc)?,"args":[0,table]},{"engine":engine(0x8c08)?,"args":[1,table+32]},
        {"engine":engine(0x8c14)?,"args":[2,table]},{"engine":engine(0x8c20)?,"args":[3,table]},
        {"engine":engine(0x8c2c)?,"args":[0,0]},{"engine":engine(0x8c38)?,"args":[0,1]},
        {"engine":engine(0x8c40)?,"args":[0]},{"engine":engine(0x8c48)?,"args":[2]},
        {"engine":engine(0x8c64)?,"args":[2,1,pair(0x8c54,1)?,name(pair(0x8c5c,5)?)?]}
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
        "emeraldTasks":emeralds,"lightFile":name(pair(0x8c68,5)?)?,"gcSuffix":name(pair(0xace4,5)?)?,
        "music":{"volumeArgument":0,"engineByteZero":name(pair(0x8c94,5)?)?,"engineByteNonzero":name(pair(0x8ca4,5)?)?},
        "startupResources":{"calls":calls,"groups":groups},"created":child,"nextMain":pair(0x91c0,1)?}),
    )
}

/// Decode the two complete files used by the installed stage. The native helper
/// writes nine words per derived row before the GC file replaces all twelve.
pub fn light_tables(source: &[u8], gc: &[u8]) -> Result<Value> {
    if source.len() != 12 * 32 || gc.len() != 12 * 48 {
        return Err(Error::invalid(
            "expected complete Wild Canyon lighting files",
        ));
    }
    let words = |row: &[u8]| {
        row.as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_be_bytes(*v))
            .collect::<Vec<_>>()
    };
    let mut derived = Vec::new();
    for row in source.as_chunks::<32>().0 {
        let w = words(row);
        let scaled = |i: usize| (f32::from_bits(w[3]) * f32::from_bits(w[i])).to_bits();
        derived.push([
            w[0],
            w[1],
            w[2],
            scaled(5),
            scaled(6),
            scaled(7),
            w[4],
            w[4],
            w[4],
        ]);
    }
    Ok(
        json!({"derivedWords":derived,"activeWords":gc.as_chunks::<48>().0.iter().map(|row|words(row)).collect::<Vec<_>>() }),
    )
}
