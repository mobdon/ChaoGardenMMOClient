//! Stage 42's fog tasks: the manager `FogtaskManExec` (update +0xAE04,
//! delete +0xAE94), the worker `FogTaskExec` it spawns (update +0xAD48,
//! delete +0xADE4), and the apply (+0xAA50) and blend (+0xAB5C) helpers that
//! drive the engine fog calls. Module state lives in section 6 +0x640..+0x654;
//! fog parameter blocks (flags, ARGB colour, far, near) are inputs, as are
//! the engine colour lerp (0x800334b0) and float-to-unsigned (0x8010af3c)
//! results.
use crate::{duel_platform, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn s(section: u8, offset: u32) -> String {
    format!("s{section}+0x{offset:x}")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FogConstants {
    pub disabled_far: f32,
    pub default_near: f32,
    pub one: f32,
    pub zero: f32,
    pub blend_near: f32,
    pub delete_far: f32,
    /// Section 5 offset of the fog-type table indexed by flags bits 16..19.
    pub types: u32,
    pub worker_name: String,
}
pub fn constants(rel_bytes: &[u8]) -> Result<FogConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    Ok(FogConstants {
        disabled_far: f(0xaa6e)?,
        default_near: f(0xaa8e)?,
        one: f(0xabf2)?,
        zero: f(0xac0a)?,
        blend_near: f(0xacde)?,
        delete_far: f(0xaf0a)?,
        types: duel_platform::target(&rel, 0xaad6, 5)?,
        worker_name: {
            let at = duel_platform::target(&rel, 0xae4e, 5)? as usize;
            let d = rel.data(5)?.get(at..).ok_or_else(duel_platform::invalid)?;
            let end = d
                .iter()
                .take(64)
                .position(|b| *b == 0)
                .ok_or_else(duel_platform::invalid)?;
            String::from_utf8(d[..end].to_vec()).map_err(|_| duel_platform::invalid())?
        },
    })
}

/// One fog parameter block as the module reads it.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FogParams {
    pub flags: u32,
    pub color: u32,
    pub far: f32,
    pub near: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FogState {
    /// s6+0x640 worker task, s6+0x654 manager task (0: none).
    pub worker: u32,
    pub manager: u32,
    /// s6+0x644 / +0x648: current and target parameter block (0: none).
    pub current: u32,
    pub target: u32,
    /// s6+0x64C blend factor.
    pub blend: f32,
    /// s6+0x650 busy flag.
    pub busy: u32,
    /// 0x803adbc8.
    pub engine_fog: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FogInputs {
    /// Parameter blocks by address (the ones `current`/`target` point to).
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, FogParams>,
    /// 0x80071e0c result for a worker spawn (0: null).
    #[serde(default)]
    pub spawned: u32,
    /// 0x800334b0 colour lerp and 0x8010af3c conversion results for a blend.
    #[serde(default)]
    pub lerp_color: u32,
    #[serde(default)]
    pub far_int: u32,
}

fn call(calls: &mut Vec<Value>, a: u32, args: Value) {
    calls.push(json!({ "engine": format!("0x{a:08x}"), "args": args }));
}
fn rgba(c: u32) -> u32 {
    c.rotate_left(8)
}

pub struct Fog<'a> {
    pub c: FogConstants,
    pub rel: crate::city_rel::CityRel<'a>,
}
impl<'a> Fog<'a> {
    pub fn new(rel_bytes: &'a [u8]) -> Result<Self> {
        Ok(Self {
            c: constants(rel_bytes)?,
            rel: duel_platform::module(rel_bytes)?,
        })
    }
    fn fog_type(&self, flags: u32) -> Result<u32> {
        let at = (self.c.types + 4 * ((flags >> 16) & 0xf)) as usize;
        self.rel
            .data(5)?
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    }
    fn disable(&self, calls: &mut Vec<Value>, p: &FogParams) {
        call(calls, 0x8011c308, json!([0]));
        call(
            calls,
            0x8011c2fc,
            json!([self.c.default_near.to_bits(), p.far.to_bits()]),
        );
        call(calls, 0x8011c2c8, json!([rgba(p.color)]));
        call(calls, 0x8011c2bc, json!([]));
    }
    /// +0xAA50
    pub fn apply(&self, calls: &mut Vec<Value>, p: &FogParams) -> Result<()> {
        // fcmpo with cror gt|eq: disabled when far >= 100000.
        if p.far >= self.c.disabled_far {
            self.disable(calls, p);
            return Ok(());
        }
        let (kind, near) = if p.flags & 2 != 0 {
            (self.fog_type(p.flags)?, p.near)
        } else {
            (5, self.c.default_near)
        };
        call(calls, 0x8011c308, json!([kind]));
        call(calls, 0x8011c2fc, json!([near.to_bits(), p.far.to_bits()]));
        call(calls, 0x8011c2c8, json!([rgba(p.color)]));
        call(calls, 0x8011c2b0, json!([]));
        call(calls, 0x8011c310, json!([]));
        Ok(())
    }
    /// +0xAB5C
    pub fn blend(
        &self,
        calls: &mut Vec<Value>,
        a: &FogParams,
        b: &FogParams,
        t: f32,
        i: &FogInputs,
    ) -> Result<()> {
        if a.far >= self.c.disabled_far && b.far >= self.c.disabled_far {
            self.disable(calls, a);
            return Ok(());
        }
        let mut t = t;
        if t > self.c.one {
            t = self.c.one;
        }
        if t < self.c.zero {
            t = self.c.zero;
        }
        call(
            calls,
            0x800334b0,
            json!([a.color, b.color, t.to_bits(), i.lerp_color]),
        );
        let far = a.far + t * (b.far - a.far);
        call(calls, 0x8010af3c, json!([far.to_bits(), i.far_int]));
        let far_f = i.far_int as f64 as f32;
        let (kind, near) = if a.flags & 2 != 0 {
            (self.fog_type(a.flags)?, a.near + t * (b.near - a.near))
        } else {
            (5, self.c.blend_near)
        };
        call(calls, 0x8011c308, json!([kind]));
        call(calls, 0x8011c2fc, json!([near.to_bits(), far_f.to_bits()]));
        call(calls, 0x8011c2c8, json!([rgba(i.lerp_color)]));
        call(calls, 0x8011c2b0, json!([]));
        call(calls, 0x8011c310, json!([]));
        Ok(())
    }
    fn params(&self, i: &FogInputs, at: u32) -> Result<FogParams> {
        i.params
            .get(&format!("0x{at:08x}"))
            .copied()
            .ok_or_else(|| Error::invalid("missing fog parameter block"))
    }
    /// Manager update +0xAE04.
    pub fn manager_update(&self, st: &mut FogState, i: &FogInputs) -> Vec<Value> {
        let mut calls = Vec::new();
        if st.worker == 0 {
            call(
                &mut calls,
                0x80071e0c,
                json!([0, 1, s(1, 0xad48), self.c.worker_name, i.spawned]),
            );
            st.worker = i.spawned;
            if i.spawned != 0 {
                calls.push(json!({ "write": "worker+0x18", "value": s(1, 0xade4) }));
            }
            st.busy = 0;
        }
        calls
    }
    /// Manager delete +0xAE94 for the task `me`.
    pub fn manager_delete(&self, st: &mut FogState, me: u32) -> Vec<Value> {
        let mut calls = Vec::new();
        if st.worker != 0 {
            call(&mut calls, 0x80071af0, json!([st.worker]));
            st.worker = 0;
        }
        if st.manager == me {
            call(&mut calls, 0x8011c308, json!([0]));
            call(
                &mut calls,
                0x8011c2fc,
                json!([self.c.default_near.to_bits(), self.c.delete_far.to_bits()]),
            );
            call(&mut calls, 0x8011c2c8, json!([-1]));
            call(&mut calls, 0x8011c2bc, json!([]));
            call(&mut calls, 0x8011c2bc, json!([]));
            call(&mut calls, 0x8011c310, json!([]));
            st.engine_fog = 0;
            st.manager = 0;
        }
        st.busy = 0;
        calls
    }
    /// Worker update +0xAD48 for the task `me`.
    pub fn worker_update(&self, st: &mut FogState, me: u32, i: &FogInputs) -> Result<Vec<Value>> {
        let mut calls = Vec::new();
        if st.worker != me || st.busy != 0 {
            return Ok(calls);
        }
        if st.current != 0 {
            let a = self.params(i, st.current)?;
            if st.target != 0 {
                let b = self.params(i, st.target)?;
                self.blend(&mut calls, &a, &b, st.blend, i)?;
            } else {
                self.apply(&mut calls, &a)?;
            }
        }
        st.busy = 0;
        Ok(calls)
    }
    /// Worker delete +0xADE4 for the task `me`.
    pub fn worker_delete(&self, st: &mut FogState, me: u32) {
        if st.worker == me {
            st.worker = 0;
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    call: String,
    task: u32,
    state: FogState,
    inputs: FogInputs,
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let fog = Fog::new(rel_bytes)?;
    let mut r: Request =
        serde_json::from_slice(request).map_err(|e| Error::invalid(format!("fog request: {e}")))?;
    let calls = match r.call.as_str() {
        "managerUpdate" => fog.manager_update(&mut r.state, &r.inputs),
        "managerDelete" => fog.manager_delete(&mut r.state, r.task),
        "workerUpdate" => fog.worker_update(&mut r.state, r.task, &r.inputs)?,
        "workerDelete" => {
            fog.worker_delete(&mut r.state, r.task);
            vec![]
        }
        _ => return Err(Error::invalid("unknown fog callback")),
    };
    Ok(json!({ "calls": calls, "state": r.state }))
}
