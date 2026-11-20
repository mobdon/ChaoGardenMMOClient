//! Sonic vs Shadow 2 meteor (object list row 26 `FIREBALL`, stg42D.rel code
//! +0x24BC constructor, +0x2670 update, +0x2590 destructor). The module's own
//! logic runs here; every engine query is an input and every engine call is
//! recorded with its arguments. Ninja matrix results used by the explosion are
//! inputs, never computed. The display callback (+0x25BC) and the shared
//! debris task (+0x8308) are recorded, not executed.
use crate::duel_platform::{self, invalid};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const PIECES: usize = 20;

/// Every constant, read through the instruction that loads it.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FireballConstants {
    pub light_radius: f32,
    pub light_radius_squared: f32,
    pub zero: f32,
    pub rand_scale: f32,
    pub one: f32,
    pub two: f32,
    pub half: f32,
    pub degrees_to_bams: f32,
    pub piece_turn_degrees: f32,
    pub piece_floor_y: f32,
    pub visibility_pad: f32,
    pub smoke_chance: f32,
    pub spin_degrees: [f32; 2],
    pub smoke_scale_base: f32,
    pub smoke_scale_range: f32,
    pub smoke_color: u32,
    /// Section 5 offsets of the collision description, debris models and texlists.
    pub collision_data: u32,
    pub debris_models: [u32; 2],
    pub debris_texlists: [u32; 2],
}
pub fn constants(rel_bytes: &[u8]) -> Result<FireballConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f4 = |pc| duel_platform::float(&rel, pc);
    let word5 = |pc| -> Result<u32> {
        let at = duel_platform::target(&rel, pc, 5)? as usize;
        let b = rel.data(5)?.get(at..at + 4).ok_or_else(invalid)?;
        Ok(u32::from_be_bytes(b.try_into().unwrap()))
    };
    let f5 = |pc| word5(pc).map(f32::from_bits);
    let off5 = |pc| duel_platform::target(&rel, pc, 5);
    // The debris model is the attach pointer (+4) of each local object.
    let attach = |pc| -> Result<u32> {
        match rel.local_pointer(5, off5(pc)? + 4)? {
            Some((5, n)) => Ok(n),
            _ => Err(invalid()),
        }
    };
    for (pc, entry) in [(0x24e6, 0x2670), (0x24f2, 0x25bc), (0x24fe, 0x2590)] {
        if duel_platform::target(&rel, pc, 1)? != entry {
            return Err(invalid());
        }
    }
    let c = FireballConstants {
        light_radius: f4(0x2556)?,
        light_radius_squared: f4(0x2566)?,
        zero: f4(0x26d6)?,
        rand_scale: f4(0x2722)?,
        one: f4(0x272a)?,
        two: f4(0x2732)?,
        half: f4(0x273a)?,
        piece_turn_degrees: f4(0x2742)?,
        degrees_to_bams: f4(0x274a)?,
        piece_floor_y: f4(0x2772)?,
        visibility_pad: f4(0x294a)?,
        smoke_chance: f4(0x2a0a)?,
        spin_degrees: [f5(0x2982)?, f5(0x29ae)?],
        smoke_scale_base: f5(0x2b3a)?,
        smoke_scale_range: f5(0x2b2a)?,
        smoke_color: word5(0x2b6a)?,
        collision_data: off5(0x250e)?,
        debris_models: [attach(0x2752)?, attach(0x2762)?],
        debris_texlists: [off5(0x275a)?, off5(0x276a)?],
    };
    // Repeated loads of the same constants elsewhere in the update.
    let same = [
        (0x29fa, c.rand_scale),
        (0x2976, c.degrees_to_bams),
        (0x2a4a, c.half),
        (0x2a5a, c.two),
        (0x2b52, c.zero),
    ];
    for (pc, v) in same {
        if f4(pc)?.to_bits() != v.to_bits() {
            return Err(invalid());
        }
    }
    Ok(c)
}

/// The fields of the SET work area the meteor reads and writes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FireballWork {
    /// Work +1 (set when the meteor explodes).
    pub exploded: u8,
    /// Work +6: 0 until first seen, then frames since.
    pub timer: u16,
    /// Work +8 and +0xC (BAMS).
    pub angles: [i32; 2],
    /// Work +0x14.
    pub position: [f32; 3],
    /// Work +0x20 (SET parameter X).
    pub scale_x: f32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transform {
    /// Result of the engine point transform (0x80114BC4) for one piece.
    pub point: [f32; 3],
    /// Result of the engine vector transform (0x801151A4) for one piece.
    pub vector: [f32; 3],
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FireballFrame {
    /// Which callback the engine runs: "init", "update" (default) or "delete".
    #[serde(default)]
    pub call: Option<String>,
    /// Return of the engine task check (0x8006538C).
    #[serde(default)]
    pub task_check: bool,
    /// Return of the player hit query (0x800066F4) is nonzero.
    #[serde(default)]
    pub hit: bool,
    /// Return of the visibility query (0x80039670).
    #[serde(default)]
    pub visible: bool,
    /// Return of the smoke particle allocator (0x800E23B8) is nonzero.
    #[serde(default)]
    pub smoke_allocated: bool,
    /// Twenty engine transform results, consumed only by an explosion.
    #[serde(default)]
    pub transforms: Vec<Transform>,
}

pub struct Fireball<'a> {
    pub constants: &'a FireballConstants,
    /// Collision description radius the engine exposes at work +0x2C -> +0xC -> +0x14.
    pub collision_radius: f32,
    /// Handle the engine light allocator (0x800C5868) returns.
    pub light_handle: u32,
}
fn engine(address: u32, args: Value) -> Value {
    json!({ "engine": format!("0x{address:08x}"), "args": args })
}
fn unit(c: &FireballConstants, r: i32) -> f32 {
    c.rand_scale * r as f64 as f32
}
fn bams(c: &FireballConstants, degrees: f32) -> i32 {
    (c.degrees_to_bams * degrees) as i32
}
impl Fireball<'_> {
    /// Constructor: three callbacks, collision and the light source.
    pub fn init(&self, w: &FireballWork, f: &FireballFrame) -> Vec<Value> {
        let c = self.constants;
        let mut calls = vec![engine(0x8006538c, json!([]))];
        if f.task_check {
            return calls;
        }
        calls.push(
            json!({ "callbacks": { "update": "0x2670", "display": "0x25bc", "delete": "0x2590" } }),
        );
        calls.push(engine(0x8000f8b8, json!({ "collisionData": format!("s5+0x{:x}", c.collision_data), "count": 1, "list": 4 })));
        calls.push(engine(
            0x800c5868,
            json!({ "kind": 1, "position": w.position, "vector": [0.0, 0.0, 0.0],
                    "radius": c.light_radius, "radiusSquared": c.light_radius_squared,
                    "returned": self.light_handle }),
        ));
        calls
    }
    pub fn delete(&self) -> Vec<Value> {
        if self.light_handle == 0 {
            return vec![];
        }
        vec![engine(0x800c5854, json!([self.light_handle]))]
    }
    /// One update. Returns the calls and whether the task destroyed itself.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // matches fcmpo/bge on NaN
    pub fn update(
        &self,
        w: &mut FireballWork,
        f: &FireballFrame,
        rand: &mut dyn Iterator<Item = i32>,
    ) -> Result<(Vec<Value>, bool)> {
        let c = self.constants;
        let mut calls = vec![engine(0x8006538c, json!([]))];
        if f.task_check {
            return Ok((calls, false));
        }
        let mut draw = |calls: &mut Vec<Value>| -> Result<f32> {
            let r = rand
                .next()
                .ok_or_else(|| Error::invalid("meteor rand values exhausted"))?;
            calls.push(engine(0x8010d940, json!({ "returned": r })));
            Ok(unit(c, r))
        };
        calls.push(engine(0x800066f4, json!({ "returnedNonzero": f.hit })));
        if f.hit || w.timer > 60 {
            w.exploded = 1;
            if f.transforms.len() != PIECES {
                return Err(Error::invalid("meteor explosion needs 20 transforms"));
            }
            calls.push(engine(0x80115d50, json!([])));
            calls.push(engine(0x8011509c, json!([0])));
            calls.push(engine(0x80115e24, json!({ "translate": w.position })));
            calls.push(engine(0x80115d50, json!([])));
            for (i, t) in f.transforms.iter().enumerate() {
                let u = draw(&mut calls)?;
                let up = c.one + c.two * u;
                let height = c.half * (self.collision_radius * (c.one + u));
                let yaw = bams(c, c.piece_turn_degrees * draw(&mut calls)?);
                calls.push(engine(0x80115560, json!([0, yaw])));
                let roll = bams(c, c.piece_turn_degrees * draw(&mut calls)?);
                calls.push(engine(0x80115918, json!([0, roll])));
                calls.push(engine(
                    0x80114bc4,
                    json!({ "point": [c.zero, height, c.zero], "returned": t.point }),
                ));
                calls.push(engine(
                    0x801151a4,
                    json!({ "vector": [c.zero, up, c.zero], "returned": t.vector }),
                ));
                let k = usize::from(i >= 5);
                let mut velocity = t.vector;
                velocity[1] += c.half;
                calls.push(json!({
                    "local": "0x8308",
                    "position": t.point, "angles": [roll, 0, yaw], "velocity": velocity,
                    "floorY": c.piece_floor_y,
                    "model": format!("s5+0x{:x}", c.debris_models[k]), "flags": 0,
                    "texlist": format!("s5+0x{:x}", c.debris_texlists[k]), "mode": 0x2000,
                }));
                calls.push(engine(0x80115d9c, json!([])));
                calls.push(engine(0x80115d50, json!([])));
            }
            calls.push(engine(0x80115a80, json!([2])));
            calls.push(engine(0x80065148, json!([])));
            return Ok((calls, true));
        }
        calls.push(engine(0x8000f4ec, json!([])));
        if w.timer != 0 {
            w.timer = w.timer.wrapping_add(1);
        } else {
            calls.push(engine(
                0x80039670,
                json!({ "position": w.position, "radius": c.visibility_pad + w.scale_x, "returned": f.visible }),
            ));
            if f.visible {
                w.timer = 1;
            }
        }
        for i in 0..2 {
            w.angles[i] = w.angles[i].wrapping_add(bams(c, c.spin_degrees[i]));
        }
        // fcmpo/bge: smoke only when u < 0.2.
        if !(draw(&mut calls)? < c.smoke_chance) {
            return Ok((calls, false));
        }
        let vx = c.two * (draw(&mut calls)? - c.half);
        let vy = draw(&mut calls)? - c.smoke_chance;
        let vz = c.two * (draw(&mut calls)? - c.half);
        let scale = c.smoke_scale_base + c.smoke_scale_range * draw(&mut calls)?;
        calls.push(engine(
            0x800e23b8,
            json!({ "position": w.position, "velocity": [vx, vy, vz], "scale": scale,
                    "rotation": c.zero, "returnedNonzero": f.smoke_allocated }),
        ));
        if f.smoke_allocated {
            calls.push(json!({ "particleColor": format!("0x{:08x}", c.smoke_color) }));
        }
        Ok((calls, false))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    work: FireballWork,
    collision_radius: f32,
    light_handle: u32,
    #[serde(default)]
    rand: Vec<i32>,
    frames: Vec<FireballFrame>,
}
/// Replay one meteor through the requested engine callbacks.
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let constants = constants(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("meteor request: {e}")))?;
    let m = Fireball {
        constants: &constants,
        collision_radius: req.collision_radius,
        light_handle: req.light_handle,
    };
    let mut w = req.work;
    let mut rand = req.rand.into_iter();
    let mut frames = Vec::with_capacity(req.frames.len());
    let mut destroyed = false;
    for f in &req.frames {
        if destroyed {
            return Err(Error::invalid("meteor task already destroyed"));
        }
        let calls = match f.call.as_deref().unwrap_or("update") {
            "init" => m.init(&w, f),
            "delete" => m.delete(),
            "update" => {
                let (calls, gone) = m.update(&mut w, f, &mut rand)?;
                destroyed = gone;
                calls
            }
            _ => return Err(Error::invalid("meteor call must be init, update or delete")),
        };
        frames.push(json!({ "work": w, "calls": calls }));
    }
    Ok(json!({ "constants": constants, "frames": frames }))
}
