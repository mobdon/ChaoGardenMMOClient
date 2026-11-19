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
    state_two: Option<StateTwo>,
    state_five: Option<StateFive>,
    state_four: Option<StateFour>,
    state_six: Option<StateSix>,
    state_seven: Option<StateSeven>,
    state_eight: Option<StateEight>,
    state_one: Option<StateOne>,
    mode: i8,
    engine_bytes: [i8; 3],
    player_flags: [u16; 2],
    opening: Option<Opening>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateSix {
    counter: i16,
    limit: i16,
    slot_byte: i8,
    scalar: f32,
    range: Option<RangeObservation>,
    timer: Option<SixTimer>,
    steering: Option<SixSteering>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SixTimer {
    slot_byte: i8,
    draw: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SixSteering {
    slot0: [f32; 3],
    slot1: [f32; 3],
    final_vector: [f32; 3],
    angle_return: f64,
    final_slot_byte: i8,
    wave: Option<SixWave>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SixWave {
    phase_word: u32,
    service_return: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFour {
    range: RangeObservation,
    close_range: Option<CloseRange>,
    slot_byte: i8,
    scalar: f32,
    counter: i16,
    limit: i16,
    draw: Option<i32>,
    steering: Option<FourSteering>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FourSteering {
    slot0: [f32; 3],
    slot1: [f32; 3],
    phase_words: [u32; 2],
    axis_returns: [f32; 2],
    angle_return: f64,
    final_slot_byte: i8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFive {
    player_word: i16,
    slot0: [f32; 3],
    slot1: [f32; 3],
    angle_return: Option<f64>,
    range: RangeObservation,
    decision: Option<WalkerDecision>,
    slot_byte: i8,
    counter: i16,
    pulse_draw: Option<i32>,
    timer_draw: Option<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateTwo {
    decision: SteeringDecision,
    after_steering: Option<AfterSteering>,
    slot0: [f32; 3],
    slot1: [f32; 3],
    angle_return: f64,
    slot_byte: i8,
    turn: Option<TurnObservation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AfterSteering {
    expiry: Option<WalkerExpiry>,
    pose_word: u16,
    scalar: f32,
    counter: i16,
    limit: i16,
    range: Option<RangeObservation>,
    close_range: Option<CloseRange>,
    draw: Option<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalkerExpiry {
    decision: WalkerDecision,
    draws: Vec<i32>,
    slot_bytes: Vec<i8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseRange {
    cooldown: i16,
    services: Option<CloseServices>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseServices {
    queries: [i32; 2],
    slot_byte: i8,
    variant: i8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RangeObservation {
    slot0_byte: i8,
    distance_return: f64,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum SteeringDecision {
    Observed(i32),
    Inputs(SteeringInputs),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SteeringInputs {
    slot0_byte: i8,
    distance_return: f64,
    facing: Option<FacingObservation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FacingObservation {
    angle_return: f64,
    previous_yaw: i32,
    difference_return: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnObservation {
    previous_yaw: i32,
    result: i32,
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
    let heading = |site, angle: f64| -> Result<i32> {
        let at = field(site, 4)? as usize;
        let bytes = rel.data(4)?.get(at..at + 8).ok_or_else(invalid)?;
        let units = (angle * f64::from_be_bytes(bytes.try_into().unwrap())).trunc();
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&units) {
            return Err(invalid());
        }
        Ok(0x4000i32.wrapping_sub(units as i32))
    };
    let close_entry = |close: CloseRange| -> Result<Value> {
        let mut calls = Vec::new();
        let mut call = |site, args: Vec<i32>| -> Result<()> {
            let service = rel
                .reference(1, site)
                .filter(|r| r.module == 0 && r.kind == 10)
                .ok_or_else(invalid)?;
            calls.push(json!({"engine":service.addend,"args":args}));
            Ok(())
        };
        let cooldown = match (close.cooldown > 0, close.services) {
            (true, None) => close.cooldown,
            (false, Some(s)) => {
                for (slot, result) in s.queries.into_iter().enumerate() {
                    call(0x694, vec![3, slot as i32])?;
                    if result == 1 {
                        call(0x6a4, vec![slot as i32])?;
                    }
                }
                let request = match (s.slot_byte, s.variant) {
                    (7, 61) => Some((0x6ec, 0x816)),
                    (7, _) => Some((0x6fc, 0x817)),
                    (6, 62) => Some((0x71c, 0x8d6)),
                    (6, _) => Some((0x72c, 0x8d7)),
                    _ => None,
                };
                if let Some((site, id)) = request {
                    call(site, vec![id, 3])?;
                }
                480
            }
            _ => return Err(invalid()),
        };
        Ok(json!({"cooldown":cooldown,"calls":calls}))
    };
    if let Some(v) = c.state_six {
        if target != 0x11a8 {
            return Err(invalid());
        }
        let f = |site| number(4, field(site, 4)?);
        mode = if v.counter > v.limit {
            2
        } else if v.slot_byte == 6 && v.scalar < f(0x11e2)? {
            8
        } else {
            6
        };
        match (mode == 6 && v.slot_byte == 6, v.range) {
            (true, Some(r)) => {
                if (r.slot0_byte == 7 && r.distance_return < f64::from(f(0x2ba)?))
                    || r.distance_return <= f64::from(f(0x2d6)?)
                {
                    mode = 2;
                }
            }
            (false, None) => {}
            _ => return Err(invalid()),
        }
        let mut steering = None;
        match (mode, v.timer, v.steering) {
            (2, Some(t), None) => {
                let (s, r, b) = if t.slot_byte == 7 {
                    (0x586, 0x596, 0x5a6)
                } else {
                    (0x5f2, 0x602, 0x612)
                };
                timer = Some((f(b)? + f(r)? * (f(s)? * t.draw as f32)) as i32 as i16 as i32);
            }
            (8, None, None) => {}
            (6, None, Some(h)) => {
                let (wave_arg, scalar) = match (h.final_slot_byte, h.wave) {
                    (6, None) => (None, f(0x12de)?),
                    (6, Some(_)) | (_, None) => return Err(invalid()),
                    (_, Some(w)) => (
                        Some(w.phase_word << 10),
                        f(0x1316)? + f(0x1306)? * w.service_return,
                    ),
                };
                steering = Some(
                    json!({"vectorArgs":[h.slot0,h.slot1],"angleArgs":[h.final_vector[0],h.final_vector[2]],
                    "yaw":heading(0x12a2,h.angle_return)?,"waveArg":wave_arg,"scalar":scalar}),
                );
            }
            _ => return Err(invalid()),
        }
        report = json!({"dispatch":target,"continueAt":0x1424,"mode":mode,"counter":if mode==6{v.counter}else{0},
            "timerWrite":timer,"steering":steering});
    }
    if let Some(v) = c.state_four {
        if target != 0xe9c {
            return Err(invalid());
        }
        let f = |site| number(4, field(site, 4)?);
        let near = (v.range.slot0_byte == 7 && v.range.distance_return < f64::from(f(0x2ba)?))
            || v.range.distance_return <= f64::from(f(0x2d6)?);
        let mut steering = None;
        let mut close = None;
        mode = if near {
            5
        } else if v.slot_byte == 6 && v.scalar < f(0xede)? {
            8
        } else if v.counter > v.limit {
            2
        } else {
            4
        };
        match (mode, v.close_range, v.draw, v.steering) {
            (5, Some(c), None, None) => close = Some(close_entry(c)?),
            (8, None, None, None) => {}
            (2, None, Some(d), None) => {
                let (scale, range, base) = if v.slot_byte == 7 {
                    (0x586, 0x596, 0x5a6)
                } else {
                    (0x5f2, 0x602, 0x612)
                };
                timer = Some((f(base)? + f(range)? * (f(scale)? * d as f32)) as i32 as i16 as i32);
            }
            (4, None, None, Some(h)) => {
                let radius = f(0xf3e)?;
                let x = (h.slot0[0] + radius * h.axis_returns[0]) - h.slot1[0];
                let z = (h.slot0[2] + radius * h.axis_returns[1]) - h.slot1[2];
                steering = Some(
                    json!({"axisArgs":h.phase_words.map(|w|w<<8),"angleArgs":[x,z],
                    "yaw":heading(0xf86,h.angle_return)?,"scalar":f(if h.final_slot_byte==6 {0xfc2}else{0xfd6})?}),
                );
            }
            _ => return Err(invalid()),
        }
        report = json!({"dispatch":target,"continueAt":0x1424,"mode":mode,"counter":if mode==4 {v.counter}else{0},
            "steering":steering,"timerWrite":timer,"closeRange":close});
    }
    if let Some(v) = c.state_five {
        if target != 0xfe4 {
            return Err(invalid());
        }
        let f = |site| number(4, field(site, 4)?);
        let steering = match (v.player_word & 3 != 0, v.angle_return) {
            (true, Some(angle)) => Some(
                json!({"angleArgs":[v.slot0[0]-v.slot1[0],v.slot0[2]-v.slot1[2]],
                "yaw":heading(0x1016,angle)?,"scalar":f(0x1042)?}),
            ),
            (false, None) => None,
            _ => return Err(invalid()),
        };
        let near = (v.range.slot0_byte == 7 && v.range.distance_return < f64::from(f(0x2ba)?))
            || v.range.distance_return <= f64::from(f(0x2d6)?);
        let engage = match (near, v.decision) {
            (true, None) => false,
            (false, Some(d)) => d.active(),
            _ => return Err(invalid()),
        };
        let pulse_sites = match (engage, v.slot_byte) {
            (false, 6) if v.counter & 31 == 0 => Some((0x10da, 0x10ea)),
            (false, 7) if v.counter & 63 == 0 => Some((0x1146, 0x1156)),
            _ => None,
        };
        let pulse = match (pulse_sites, v.pulse_draw) {
            (Some((scale, limit)), Some(draw)) => (draw as f32) * f(scale)? < f(limit)?,
            (None, None) => false,
            _ => return Err(invalid()),
        };
        if engage {
            mode = 6;
        } else if !pulse && v.counter > 64 {
            mode = 4;
        }
        let sites = match (mode, v.slot_byte) {
            (6, 7) => Some((0x366, 0x376, 0x376)),
            (6, _) => Some((0x3c6, 0x3d6, 0x3e6)),
            (4, 7) => Some((0x476, 0x486, 0x486)),
            (4, _) => Some((0x4d6, 0x4e6, 0x4f6)),
            _ => None,
        };
        timer = match (sites, v.timer_draw) {
            (Some((scale, range, base)), Some(draw)) => {
                Some((f(base)? + f(range)? * (f(scale)? * draw as f32)) as i32 as i16 as i32)
            }
            (None, None) => None,
            _ => return Err(invalid()),
        };
        report = json!({"dispatch":target,"continueAt":0x1424,"steering":steering,"mode":mode,
            "counter":if mode!=5 || pulse {0}else{v.counter},"timerWrite":timer,
            "byteWrites":if pulse {vec![0x803ad485u32,0x803ad495]}else{vec![]}});
    }
    if let Some(v) = c.state_two {
        if target != 0xad4 {
            return Err(invalid());
        }
        let mut decision_trace = None;
        let direct = match v.decision {
            SteeringDecision::Observed(value) => value != 0,
            SteeringDecision::Inputs(inputs) => {
                let near = inputs.slot0_byte == 7
                    && inputs.distance_return < f64::from(number(4, field(0x1fa, 4)?)?);
                let (direct, difference) = match (near, inputs.facing) {
                    (true, None) => (true, None),
                    (false, Some(f)) => (
                        f.difference_return < 0x3000,
                        Some([heading(0x232, f.angle_return)?, f.previous_yaw]),
                    ),
                    _ => return Err(invalid()),
                };
                decision_trace = Some(json!({"direct":direct,"distanceArgs":[v.slot0,v.slot1],
                    "angleArgs":if near {None} else {Some([v.slot0[0]-v.slot1[0],v.slot0[2]-v.slot1[2]])},
                    "differenceArgs":difference}));
                direct
            }
        };
        let desired = heading(if direct { 0xb0e } else { 0xb8e }, v.angle_return)?;
        let (yaw, turn_args, scalar_site) = match (direct, v.turn) {
            (true, None) => (desired, None, if v.slot_byte == 6 { 0xb4a } else { 0xb5e }),
            (false, Some(t)) => (t.result, Some([t.previous_yaw, desired, 0x180]), 0xbc6),
            _ => return Err(invalid()),
        };
        report = json!({"dispatch":target,"continueAt":0xbd0,"mode":mode,
            "angleArgs":[v.slot0[0]-v.slot1[0],v.slot0[2]-v.slot1[2]],
            "turnArgs":turn_args,"yaw":yaw,"scalar":number(4,field(scalar_site,4)?)?});
        if let Some(trace) = decision_trace {
            report["decision"] = trace;
        }
        if let Some(a) = v.after_steering {
            let f = |site| number(4, field(site, 4)?);
            let mut counter = a.counter;
            let mut pulse = false;
            let mut next = 0x1424;
            let mut draw_used = false;
            if v.slot_byte == 6 && a.pose_word != 0 && a.scalar < f(0xbf6)? {
                if a.range.is_some() {
                    return Err(invalid());
                }
                mode = 8;
                counter = 0;
            } else {
                let r = a.range.ok_or_else(invalid)?;
                if (r.slot0_byte == 7 && r.distance_return < f64::from(f(0x2ba)?))
                    || r.distance_return <= f64::from(f(0x2d6)?)
                {
                    next = 0x640;
                } else {
                    let sites = match v.slot_byte {
                        6 if counter & 31 == 0 => Some((0xc8e, 0xc9e)),
                        7 if counter & 63 == 0 => Some((0xcfa, 0xd0a)),
                        _ => None,
                    };
                    if let Some((scale, threshold)) = sites {
                        draw_used = true;
                        pulse = (a.draw.ok_or_else(invalid)? as f32) * f(scale)? < f(threshold)?;
                    }
                    if !pulse && counter > a.limit {
                        next = 0xd48;
                    }
                }
            }
            if let Some(e) = a.expiry {
                if next != 0xd48 {
                    return Err(invalid());
                }
                let mut draws = e.draws.into_iter();
                let mut bytes = e.slot_bytes.into_iter();
                let mut draw = || draws.next().map(|n| n as f32).ok_or_else(invalid);
                let mut byte = || bytes.next().ok_or_else(invalid);
                mode = if e.decision.active() {
                    6
                } else if draw()? * f(0xd9e)? < f(0xdae)? {
                    4
                } else {
                    let six = byte()? == 6 && draw()? * f(0xd9e)? > f(0xe0e)?;
                    if six || (byte()? == 7 && draw()? * f(0xe5a)? < f(0xe6a)?) {
                        7
                    } else {
                        5
                    }
                };
                if mode == 4 || mode == 6 {
                    let (s, r, b) = match (mode, byte()?) {
                        (6, 7) => (0x366, 0x376, 0x376),
                        (6, _) => (0x3c6, 0x3d6, 0x3e6),
                        (4, 7) => (0x476, 0x486, 0x486),
                        _ => (0x4d6, 0x4e6, 0x4f6),
                    };
                    timer = Some((f(b)? + f(r)? * (f(s)? * draw()?)) as i32 as i16 as i32);
                }
                if draws.next().is_some() || bytes.next().is_some() {
                    return Err(invalid());
                }
                if mode == 5 && a.close_range.is_none() {
                    return Err(invalid());
                }
                next = if mode == 5 { 0x640 } else { 0x1424 };
                counter = 0;
                report["timerWrite"] = json!(timer);
            }
            if let Some(close) = a.close_range {
                if next != 0x640 {
                    return Err(invalid());
                }
                counter = 0;
                mode = 5;
                next = 0x1424;
                report["closeRange"] = close_entry(close)?;
            }
            if !draw_used && a.draw.is_some() {
                return Err(invalid());
            }
            report["continueAt"] = json!(next);
            report["mode"] = json!(mode);
            report["counter"] = json!(counter);
            report["byteWrites"] = json!(if pulse {
                vec![0x803ad485u32, 0x803ad495]
            } else {
                vec![]
            });
        }
    }
    if let Some(v) = c.state_eight {
        if target != 0x137c {
            return Err(invalid());
        }
        let mut counter = v.counter;
        let mut ops = vec![0x1394];
        let yaw = heading(0x139e, v.angle_return)?;
        let pulse = v.pose_word != 0 && counter & 7 == 0;
        if v.pose_word == 0 {
            let draw = v.draw.ok_or_else(invalid)? as f32;
            let (site, scale, range, base) = if v.slot_byte == 7 {
                (0x448, 0x476, 0x486, 0x486)
            } else {
                (0x4a8, 0x4d6, 0x4e6, 0x4f6)
            };
            let f = |site| number(4, field(site, 4)?);
            timer = Some((f(base)? + f(range)? * (f(scale)? * draw)) as i32 as i16 as i32);
            mode = 4;
            counter = 0;
            ops.extend([0x13e8, site]);
        } else {
            if v.draw.is_some() {
                return Err(invalid());
            }
            if pulse {
                counter = 0;
            }
        }
        report = json!({"dispatch":target,"continueAt":0x1424,"mode":mode,"timerWrite":timer,"counter":counter,
            "angleArgs":[v.slot0[0]-v.slot1[0],v.slot0[2]-v.slot1[2]],"yaw":yaw,"scalar":number(4,field(0x13ca,4)?)?,
            "byteWrites":if pulse{vec![0x803ad485u32,0x803ad495]}else{vec![]},"operations":ops});
    }
    if let Some(v) = c.state_one {
        if target != 0x924 {
            return Err(invalid());
        }
        let mut counter = v.counter;
        let mut sub = v.submode;
        let mut ops = Vec::new();
        let mut cooldown = None;
        let mut messenger = None;
        if counter > v.limit {
            if sub == 0 {
                sub = 1;
                let query = v.query.ok_or_else(invalid)?;
                ops.push(json!([0x988]));
                if query != 0 {
                    let group = match query {
                        1 => 0,
                        2 => 1,
                        _ => 2,
                    } + if v.slot0_byte == 7 { 3 } else { 0 };
                    let base = field(0x94e, 5)?;
                    let pos = [
                        number(5, base)?,
                        number(5, base + 4)? + number(4, field(0x972, 4)?)?,
                        number(5, base + 8)?,
                    ];
                    ops.push(json!([0x9f8, pos, 0, 0, group, 2, 0]));
                    if let Some(allocated) = v.messenger_allocated {
                        let name_offset = field(0x26206, 5)? as usize;
                        let data = &rel.data(5)?[name_offset..];
                        let name = std::str::from_utf8(
                            &data[..data.iter().position(|b| *b == 0).ok_or_else(invalid)?],
                        )
                        .map_err(|_| invalid())?;
                        let initial = number(4, field(0x26272, 4)?)?;
                        messenger = Some(json!({"create":[3,2,field(0x261fe,1)?,name],
                            "created":if allocated{Some(json!({"position":pos,"rotation":[0,0,0],"group":group as f32,
                            "parameter":initial,"taskWord":2,"initialize":0x25cac,"extraWord80":0,"extraFloat48":initial,
                            "mode":6,"extraPosition":pos,"finalCall":[0x800b,0,0,0]}))}else{None}}));
                    }
                }
            } else if v.query.is_some() {
                return Err(invalid());
            }
            ops.extend([
                json!([0xa00, 0]),
                json!([0xa04]),
                json!([0xa18]),
                json!([0xa84, 0]),
            ]);
            let f = |site| number(4, field(site, 4)?);
            let value = f(0xa66)? + f(0xa56)? * (f(0xa46)? * v.draw.ok_or_else(invalid)? as f32);
            timer = Some(value as i32 as i16 as i32);
            counter = 0;
            mode = 7;
        } else {
            if v.draw.is_some() || v.query.is_some() {
                return Err(invalid());
            }
            if counter == 120 {
                match v.character {
                    6 => ops.push(json!([0xac4, 0x8d3])),
                    7 => ops.push(json!([0xac4, 0x813])),
                    _ => {}
                }
                cooldown = Some(180);
            }
        }
        report = json!({"dispatch":target,"continueAt":0x1424,"mode":mode,"submode":sub,"counter":counter,
            "timerWrite":timer,"cooldownWrite":cooldown,"operations":ops});
        if let Some(value) = messenger {
            report["messenger"] = value;
        } else if v.messenger_allocated.is_some() {
            return Err(invalid());
        }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEight {
    slot0: [f32; 3],
    slot1: [f32; 3],
    angle_return: f64,
    pose_word: u16,
    slot_byte: i8,
    counter: i16,
    draw: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateOne {
    messenger_allocated: Option<bool>,
    counter: i16,
    limit: i16,
    submode: i8,
    character: i8,
    slot0_byte: i8,
    query: Option<i32>,
    draw: Option<i32>,
}
