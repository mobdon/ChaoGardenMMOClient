//! Stage 20 registered entry task: initial service request and position tracking.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    startup: Option<Startup>,
    controller: Option<Controller>,
    tail: Option<Tail>,
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
        controller,
        tail,
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
    if let Some(c) = controller {
        report["controller"] = dispatch_controller(&rel, c)?;
    }
    if let Some(t) = tail {
        report["tail"] = walker_tail(t, constant)?;
    }
    Ok(report)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Controller {
    state_seven: Option<StateSeven>,
    mode: i8,
    engine_bytes: [i8; 3],
    player_flags: [u16; 2],
    opening: Option<Opening>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Opening {
    query: i32,
    character: i8,
    yaw: u32,
    position: [f32; 3],
}
fn dispatch_controller(rel: &CityRel<'_>, c: Controller) -> Result<Value> {
    let invalid = || Error::invalid("unexpected walker controller observation");
    let field = |pc, sect| {
        rel.reference(1, pc)
            .filter(|r| r.module == 19 && r.section == sect && r.kind == 4)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let number = |section: u8, at: u32| -> Result<f32> {
        let p = at as usize;
        Ok(f32::from_be_bytes(
            rel.data(section)?
                .get(p..p + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let [a, b, d] = c.engine_bytes;
    let gated = a == 1 || b & 2 != 0 || d == 1;
    let mut mode = c.mode;
    if !gated && c.player_flags.iter().any(|f| f & 0x4000 != 0) {
        mode = 9;
    }
    let target = if gated {
        0x1518
    } else if !(0..=10).contains(&mode) {
        0x1424
    } else {
        rel.local_pointer(5, field(0x816, 5)? + 4 * mode as u32)?
            .filter(|(s, _)| *s == 1)
            .ok_or_else(invalid)?
            .1
    };
    let mut ops = Vec::new();
    let mut timer = None;
    let mut next = target;
    if target == 0x828 {
        let o = c.opening.ok_or_else(invalid)?;
        ops.push(json!([0x828]));
        if o.query == 16 {
            ops.extend([json!([0x838, 3]), json!([0x83c])]);
            mode = 1;
            timer = Some(360);
            if matches!(o.character, 6 | 7) {
                ops.push(json!([
                    if o.character == 6 { 0x86c } else { 0x888 },
                    0,
                    field(if o.character == 6 { 0x86a } else { 0x886 }, 5)?
                ]));
            }
            let pos = [
                o.position[0],
                number(4, field(0x8ca, 4)?)? + number(5, field(0x8d6, 5)? + 4)?,
                o.position[2],
            ];
            ops.push(json!([
                0x91c,
                0,
                number(4, field(0x892, 4)?)?,
                o.yaw.wrapping_sub(0xf000),
                pos,
                number(4, field(0x8f2, 4)?)?,
                number(4, field(0x902, 4)?)?,
                0x6000
            ]));
        }
        next = 0x1424;
    } else if c.opening.is_some() {
        return Err(invalid());
    }
    let mut report = json!({"dispatch":target,"continueAt":next,"mode":mode,"timerWrite":timer,"operations":ops});
    if let Some(v) = c.state_seven {
        if target != 0x1328 {
            return Err(invalid());
        }
        let mut ops = Vec::new();
        let engage = if v.slot_byte == 6 {
            ops.push(0x1344);
            match (v.decision, v.helper_result) {
                (Some(d), None) => d.active(),
                (None, Some(observed)) => observed != 0,
                _ => return Err(invalid()),
            }
        } else {
            if v.helper_result.is_some() || v.decision.is_some() {
                return Err(invalid());
            }
            false
        };
        let sites = if engage {
            ops.extend([0x1358, 0x398]);
            Some((6, 0x3c6, 0x3d6, 0x3e6))
        } else if v.counter > 120 {
            ops.push(0x1374);
            if v.slot_byte == 7 {
                ops.push(0x558);
                Some((2, 0x586, 0x596, 0x5a6))
            } else {
                ops.push(0x5c4);
                Some((2, 0x5f2, 0x602, 0x612))
            }
        } else {
            None
        };
        let mut counter = v.counter;
        if let Some((m, scale, range, base)) = sites {
            let draw = v.draw.ok_or_else(invalid)? as f32;
            let f = |site| number(4, field(site, 4)?);
            let value = f(base)? + f(range)? * (f(scale)? * draw);
            timer = Some(value.trunc() as i32 as i16 as i32);
            mode = m;
            counter = 0;
        } else if v.draw.is_some() {
            return Err(invalid());
        }
        report = json!({"dispatch":target,"continueAt":0x1424,"mode":mode,"timerWrite":timer,"counter":counter,"operations":ops});
    }
    Ok(report)
}

/// Observations after helper 1:0x0, including the child link after a removal request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tail {
    mode: i8,
    submode: i8,
    counter: i16,
    cooldown: i16,
    slot0_flags: u16,
    queries: Vec<i32>,
    engine_word: u32,
    slot1_value: f32,
    child: bool,
    child_after_remove: Option<bool>,
}
fn walker_tail(mut t: Tail, constant: impl Fn(u32) -> Result<f32>) -> Result<Value> {
    let mut calls = vec![0x142c];
    let mut q = t.queries.into_iter();
    let mut reset = t.slot0_flags & 0x4000 != 0;
    for (site, stop) in [(0x1440, 9), (0x144c, 12), (0x1458, 13)] {
        if reset {
            break;
        }
        calls.push(site);
        reset = q
            .next()
            .ok_or_else(|| Error::invalid("missing walker tail query"))?
            == stop;
    }
    if q.next().is_some() {
        return Err(Error::invalid("unused walker tail query"));
    }
    reset |= t.engine_word == 0;
    if reset {
        t.slot1_value = constant(0x147e)?;
        if t.child {
            calls.push(0x1494);
            t.child = t
                .child_after_remove
                .take()
                .ok_or_else(|| Error::invalid("missing post-removal child observation"))?;
        }
        t.mode = 0;
        t.submode = 0;
        t.counter = 0;
    } else {
        t.counter = t.counter.wrapping_add(1);
        if t.cooldown > 0 {
            t.cooldown -= 1;
        }
    }
    if t.child_after_remove.is_some() {
        return Err(Error::invalid("unused post-removal child observation"));
    }
    let scaled = constant(0x14d2)? * t.slot1_value;
    if t.slot1_value > constant(0x14f2)? && !t.child {
        calls.push(0x1514);
    }
    Ok(
        json!({"mode":t.mode,"submode":t.submode,"counter":t.counter,"cooldown":t.cooldown,
        "slot1Value":t.slot1_value,"scaled":scaled,"calls":calls}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateSeven {
    slot_byte: i8,
    counter: i16,
    helper_result: Option<i32>,
    decision: Option<WalkerDecision>,
    draw: Option<i32>,
}

/// State 7's predicate at 1:0x15c ignores the two position arguments.
/// None denotes a null slot-one work link, not a missing slot-one task.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalkerDecision {
    work_word: Option<u32>,
    engine_byte: i8,
}
impl WalkerDecision {
    fn active(&self) -> bool {
        self.work_word.is_some_and(|word| word != 0) || self.engine_byte != 0
    }
}
