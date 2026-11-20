//! Stage 42's rival power gauge task (`BossPowerGuageExec`): the creator
//! (+0x20470) and update (+0x2052C). The display (+0x20700) is in
//! `duel_display`. Work layout: +0 mode, +6 timer, +8 pointer to the
//! rival's gauge, +0x14 shown length, +0x1C fill rate, +0x24 drain rate,
//! +0x28 starting gauge.
use crate::{duel_platform, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GaugeConstants {
    pub zero: f32,
    pub scale: f32,
    pub unit: f32,
    pub gone: f32,
    pub rates: [f32; 2],
}
pub fn constants(rel_bytes: &[u8]) -> Result<GaugeConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    let table = duel_platform::target(&rel, 0x20502, 5)? as usize;
    let d = rel.data(5)?;
    let word = |at: usize| -> Result<f32> {
        d.get(at..at + 4)
            .map(|b| f32::from_bits(u32::from_be_bytes(b.try_into().unwrap())))
            .ok_or_else(duel_platform::invalid)
    };
    Ok(GaugeConstants {
        zero: f(0x204da)?,
        scale: f(0x205be)?,
        unit: f(0x205ca)?,
        gone: f(0x206de)?,
        rates: [word(table + 4)?, word(table + 8)?],
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GaugeWork {
    pub mode: i8,
    pub timer: u16,
    pub shown: f32,
    pub fill: f32,
    pub drain: f32,
    pub start: f32,
}

/// +0x20470 run on a freshly created child task's work: point at the gauge,
/// copy its value, clear the shown length and set the two rates.
pub fn create(c: &GaugeConstants, gauge: f32, w: &mut GaugeWork) {
    w.start = gauge;
    w.shown = c.zero;
    w.fill = c.rates[0];
    w.drain = c.rates[1];
}

/// +0x2052C. Returns the engine calls (the task frees itself when emptied).
#[allow(clippy::neg_cmp_op_on_partial_ord)] // fcmpo with bge/ble on NaN
pub fn update(
    c: &GaugeConstants,
    w: &mut GaugeWork,
    gauge: f32,
    paused: bool,
    menu: bool,
) -> Vec<Value> {
    let mut calls = Vec::new();
    if paused || menu {
        return calls;
    }
    let full = |g: f32| c.scale * (c.unit * g);
    match w.mode {
        0 => {
            let t = w.timer;
            w.timer = t.wrapping_add(1);
            if t == 0x28 {
                w.mode = w.mode.wrapping_add(1);
            }
        }
        1 => {
            let max = full(w.start);
            w.shown += w.fill;
            // cror gt|eq: reached when shown >= max.
            if w.shown >= max {
                w.shown = max;
                w.mode = w.mode.wrapping_add(1);
            }
        }
        2 => {
            let target = full(gauge);
            if w.shown > target {
                w.shown -= w.drain;
                if w.shown < target {
                    w.shown = target;
                }
            }
            // cror lt|eq: empty when shown <= 0.
            if w.shown <= c.zero {
                w.shown = full(w.start);
                w.mode = w.mode.wrapping_add(1);
            }
        }
        3 => {
            w.shown -= c.scale * w.fill;
            if w.shown < c.gone {
                calls.push(json!({ "engine": "0x80071878", "args": ["task"] }));
            }
        }
        _ => {}
    }
    calls
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    work: GaugeWork,
    gauge: f32,
    paused: bool,
    menu: bool,
    #[serde(default)]
    create: bool,
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let c = constants(rel_bytes)?;
    let mut r: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("gauge request: {e}")))?;
    if r.create {
        create(&c, r.gauge, &mut r.work);
        return Ok(json!({ "calls": [], "work": r.work }));
    }
    let calls = update(&c, &mut r.work, r.gauge, r.paused, r.menu);
    Ok(json!({ "calls": calls, "work": r.work }))
}
