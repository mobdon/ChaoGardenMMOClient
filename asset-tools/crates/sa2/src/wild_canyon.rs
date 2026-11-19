//! Wild Canyon stage registration and its first scheduled callback (GSNE8P module 15).
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde_json::{json, Value};

/// A big-endian CCL_INFO as supplied by the stage to CCL_Init. Do not
/// interpret the attribute bits or overwrite the stage's source record.
#[derive(serde::Serialize)]
struct CollisionRecord {
    kind: i8,
    form: u8,
    push: i8,
    damage: i8,
    attr: u32,
    center: [f32; 3],
    parameters: [f32; 4],
    angles: [i32; 3],
}
fn collision_record(data: &[u8], offset: u32) -> Result<CollisionRecord> {
    let raw = data
        .get(offset as usize..)
        .and_then(|v| v.get(..48))
        .ok_or_else(|| Error::invalid("truncated emerald collision record"))?;
    let word = |i: usize| u32::from_be_bytes(raw[i..i + 4].try_into().unwrap());
    let f = |i| f32::from_bits(word(i));
    Ok(CollisionRecord {
        kind: raw[0] as i8,
        form: raw[1],
        push: raw[2] as i8,
        damage: raw[3] as i8,
        attr: word(4),
        center: std::array::from_fn(|i| f(8 + 4 * i)),
        parameters: std::array::from_fn(|i| f(20 + 4 * i)),
        angles: std::array::from_fn(|i| word(36 + 4 * i) as i32),
    })
}

/// Match file-backed Steam executable records to the two REL templates,
/// normalizing only the byte order of the eleven 32-bit fields.
pub fn pc_collision_templates(exe: &[u8], module: &[u8]) -> Result<Value> {
    let pe = crate::pe::read_pe(exe)?;
    let rel = CityRel::read_identified(module, 15)?;
    let data = rel.data(5)?;
    let mut matches = Vec::new();
    for (source, rva) in [(0x976ccusize, 0x7435ccu32), (0x97730, 0x7435fc)] {
        let template = data
            .get(source..source + 48)
            .ok_or_else(|| Error::invalid("missing emerald shape template"))?;
        let at = pe.rva_to_offset(rva, 48)?;
        let pc = exe
            .get(at..at + 48)
            .ok_or_else(|| Error::invalid("missing PC emerald shape record"))?;
        if pc[..4] != template[..4]
            || pc[4..]
                .as_chunks::<4>()
                .0
                .iter()
                .zip(template[4..].as_chunks::<4>().0.iter())
                .any(|(a, b)| !a.iter().eq(b.iter().rev()))
        {
            return Err(Error::invalid(
                "PC and REL emerald collision records disagree",
            ));
        }
        matches.push(
            json!({"pcRva":rva,"pcFileOffset":at,"relSection":5,"relOffset":source,
            "pcSha256":manifest::sha256(pc),"relSha256":manifest::sha256(template)}),
        );
    }
    Ok(json!(matches))
}

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
    let path_table = pair(0x8e54, 5)?;
    let section_ptr = |off| {
        rel.local_pointer(5, off)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let f32_at = |bytes: &[u8], at: usize| -> Result<f32> {
        Ok(f32::from_be_bytes(
            bytes
                .get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let half = f32_at(rel.data(4)?, pair(0x21dd0, 4)? as usize)?;
    let count = u32::from(u16::from_be_bytes(
        rel.data(1)?[0x8e5e..0x8e60].try_into().unwrap(),
    ));
    if count != 12 {
        return Err(invalid());
    }
    let mut paths = Vec::new();
    let mut path_geometry = Vec::new();
    for index in 0..count {
        let header = section_ptr(path_table + 4 * index)?;
        let at = header as usize;
        let prefix = data.get(at..at + 4).ok_or_else(invalid)?;
        let n = i16::from_be_bytes(prefix[2..].try_into().unwrap());
        if prefix[..2] != [0, 0] || n <= 0 {
            return Err(invalid());
        }
        let points = section_ptr(header + 8)?;
        let mut vertices = Vec::new();
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for i in 0..n as usize {
            let mut xyz = [0.; 3];
            for j in 0..3 {
                let v = f32_at(data, points as usize + 12 * i + 4 * j)?;
                if !v.is_finite() {
                    return Err(invalid());
                }
                xyz[j] = v;
                lo[j] = lo[j].min(v);
                hi[j] = hi[j].max(v);
            }
            vertices.push(xyz);
        }
        let center: [f32; 3] = std::array::from_fn(|j| (hi[j] + lo[j]) * half);
        paths.push(
            json!({"headerOffset":header,"pointsOffset":points,"points":vertices,"center":center}),
        );
        path_geometry.push((center, vertices));
    }
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
                let shape_offset = pair(0x20808, 5)?;
                Some(
                    json!({"typeWord":word(data,row as usize)?,"lowByte":bound.placement.rotation[0]&255,
                    "position":bound.placement.position,"shapeOffset":shape_offset,"sourceShape":collision_record(data,shape_offset)?,"count":1,"id":6,
                    "shapeParameter":bound.placement.parameters[1]+add,"skipIfModeZeroAndQueryNonzero":true}),
                )
            } else {
                None
            };
            let path_index = if init == 0x220ac {
                let selector = bound.placement.parameters[0];
                if !(0.0..4294967296.).contains(&selector) {
                    return Err(invalid());
                }
                Some((selector as u32) % count)
            } else {
                None
            };
            let alternate_setup = if let Some(i) = path_index {
                let parameter = |pc| f32_at(rel.data(4)?, pair(pc, 4)? as usize);
                let p = &bound.placement;
                let (center, vertices) = &path_geometry[i as usize];
                let translation = crate::emerald_path::unrotated_path_translation(
                    p.position,
                    [p.rotation[1], p.rotation[2]],
                    *center,
                );
                let world_vertices = translation.map(|t| {
                    vertices
                        .iter()
                        .map(|&v| crate::emerald_path::translate_path_point(t, v))
                        .collect::<Vec<_>>()
                });
                let initial_distance = parameter(0x22120)? + p.parameters[2];
                let fresh = if let Some(t) = translation {
                    let (segment, fraction, local, wrapped) =
                        crate::emerald_path::advance_position(
                            vertices,
                            0,
                            0.,
                            initial_distance,
                            |j| {
                                crate::board::point_distance(
                                    vertices[j],
                                    vertices[(j + 1) % vertices.len()],
                                )
                            },
                        )?;
                    Some(
                        json!({"segment":segment,"fraction":fraction,"local":local,"world":crate::emerald_path::translate_path_point(t,local),"wrapped":wrapped}),
                    )
                } else {
                    None
                };
                let shape_offset = pair(0x22220, 5)?;
                Some(json!({
                    "allocation":[1,24], "initialDistance":initial_distance,"freshCursorPosition":fresh,
                    "translate":p.position, "rotationWords":[p.rotation[1],p.rotation[2]],
                    "translateAfterRotation":center.map(|v| -v),"derivedWorldVertices":world_vertices,
                    "typeWord":u32::from_be_bytes(data[row as usize..row as usize+4].try_into().unwrap()),
                    "lowByte":p.rotation[0]&255, "shapeOffset":shape_offset,"sourceShape":collision_record(data,shape_offset)?,
                    "count":1,"id":6,"shapeParameter":parameter(0x22234)? + p.parameters[1]
                }))
            } else {
                None
            };
            emeralds.push(json!({"layer":layer.filename,"index":bound.placement_index,"object":def.name,"variant":variant,
                "main":code(row+20)?,"destroy":code(row+28)?,"display":display,"defaultDisplaySlot":32,
                "nonzeroQueryDisplaySlot":conditional.then_some(20),"initialize":init,"commonSetup":setup,"pathIndex":path_index,"alternateSetup":alternate_setup}));
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
        "emeraldPaths":paths,"emeraldTasks":emeralds,"lightFile":name(pair(0x8c68,5)?)?,"gcSuffix":name(pair(0xace4,5)?)?,
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

/// Replay the common emerald callback from entry through its pickup and removal requests.
/// All engine query results are ordered observations, consumed only when called.
pub fn contacts(
    module: &[u8],
    stage: &crate::fan_stage::MountedStage<'_>,
    request: &[u8],
) -> Result<Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Tick {
        layer: String,
        index: usize,
        mode: i8,
        submode: Option<i8>,
        spin_angle: Option<u32>,
        collision_flags: u16,
        hit_num: i16,
        engine_byte_1e: i8,
        owner_id: u16,
        player_actions: [Option<i8>; 2],
        has_set_entry: bool,
        position: [f32; 3],
        queries: Vec<i32>,
        player_hits: Vec<i8>,
    }
    let rows = enter(module, false, stage)?;
    let tasks = rows["emeraldTasks"]
        .as_array()
        .ok_or_else(|| Error::invalid("missing emerald tasks"))?;
    let rel = CityRel::read_identified(module, 15)?;
    let hi = rel
        .reference(1, 0x20c96)
        .ok_or_else(|| Error::invalid("missing spin table"))?;
    let lo = rel
        .reference(1, 0x20c9a)
        .ok_or_else(|| Error::invalid("missing spin table"))?;
    if (
        hi.module, hi.section, hi.kind, lo.module, lo.section, lo.kind,
    ) != (15, 5, 6, 15, 5, 4)
        || hi.addend != lo.addend
    {
        return Err(Error::invalid("unknown spin table"));
    }
    let offset = hi.addend as usize + 40;
    let spin_step = u32::from_be_bytes(
        rel.data(5)?
            .get(offset..offset + 4)
            .ok_or_else(|| Error::invalid("missing spin step"))?
            .try_into()
            .unwrap(),
    );
    let mut states = std::collections::BTreeMap::new();
    let ticks: Vec<Tick> = serde_json::from_slice(request)?;
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many emerald contacts"));
    }
    let mut frames = Vec::new();
    for t in ticks {
        let task = tasks
            .iter()
            .find(|v| v["layer"] == t.layer && v["index"] == t.index && v["main"] == 0x20940)
            .ok_or_else(|| Error::invalid("expected mounted common emerald"))?;
        let kind = task["commonSetup"]["typeWord"]
            .as_u64()
            .ok_or_else(|| Error::invalid("missing emerald type"))?;
        let low = task["commonSetup"]["lowByte"]
            .as_u64()
            .ok_or_else(|| Error::invalid("missing emerald identifier"))?;
        if !t.position.iter().all(|x| x.is_finite()) {
            return Err(Error::invalid("invalid emerald position"));
        }
        let mut q = t.queries.into_iter();
        let mut next = || {
            q.next()
                .ok_or_else(|| Error::invalid("missing emerald engine observation"))
        };
        let mut hits = t.player_hits.into_iter();
        let key = (t.layer, t.index);
        let (mut sm, mut spin) = match (t.submode, t.spin_angle) {
            (Some(sm), angle) => (sm, angle.unwrap_or(0)),
            (None, None) => *states
                .get(&key)
                .ok_or_else(|| Error::invalid("no live emerald update to resume"))?,
            (None, Some(_)) => return Err(Error::invalid("cannot override spin while resuming")),
        };
        let mut calls = Vec::new();
        let outcome = 'gate: {
            if t.mode == 0 {
                calls.push(json!([0x2097c]));
                if next()? != 0 {
                    break 'gate "return";
                }
            }
            if sm == 1 {
                calls.push(json!([0x209e0, kind, low, 0]));
                match next()? {
                    0 => sm = 3,
                    1..=3 => sm = 2,
                    _ => (),
                }
            } else if sm < 1 {
                sm = 1;
            }
            if sm != 2 || t.mode == 5 || t.collision_flags & 1 == 0 {
                break 'gate "continue";
            }
            if t.hit_num != 0 {
                calls.push(json!([0x20a5c]));
                if next()? == 26 && t.engine_byte_1e == 0 && kind == 9 {
                    break 'gate "continue";
                }
            }
            calls.push(json!([0x2088c]));
            loop {
                calls.push(json!([0x208c0]));
                match hits.next() {
                    Some(0) => break,
                    Some(_) => continue,
                    None => {
                        calls.push(json!([0x208cc]));
                        if next()? == 8 && (matches!(kind, 4 | 5) || (kind == 9 && low == 2)) {
                            break 'gate "continue";
                        }
                        break;
                    }
                }
            }
            if t.owner_id > 1 {
                break 'gate "continue";
            }
            calls.push(json!([0x20b38]));
            let slot = next()?;
            if crate::emerald_path::checks_player_action(kind) {
                let action = t
                    .player_actions
                    .get(usize::try_from(slot).unwrap_or(usize::MAX))
                    .ok_or_else(|| Error::invalid("invalid player lookup index"))?;
                if crate::emerald_path::needs_mode_query(*action) {
                    calls.push(json!([0x1fb28]));
                    if !crate::emerald_path::mode_allows_pickup(next()?) {
                        break 'gate "continue";
                    }
                }
            }
            calls.push(json!([0x20b94]));
            let collector = next()?;
            calls.push(json!([0x20bd0, kind, low, collector, t.position]));
            calls.push(json!([if t.has_set_entry { 0x20be4 } else { 0x20bf0 }]));
            "pickupRequested"
        };
        if q.next().is_some() || hits.next().is_some() {
            return Err(Error::invalid("unused emerald observation"));
        }
        if outcome == "continue" {
            if matches!(t.mode, 4 | 5) && sm >= 2 {
                calls.push(json!([0x20c64, kind, low, t.position]));
            }
            if sm == 2 && t.mode != 5 {
                calls.push(json!([0x20c8c]));
                spin = spin.wrapping_add(spin_step);
            }
        }
        if outcome == "pickupRequested" {
            states.remove(&key);
        } else {
            states.insert(key, (sm, spin));
        }
        frames.push(json!({"submode":sm,"spinAngle":spin,"outcome":outcome,"calls":calls}));
    }
    Ok(json!(frames))
}
