//! The stage 42 module's shared debris manager (task name "EfCrash3DExec").
//! stg42D.rel code: spawn +0x8308 (wrapper of +0x8348), update +0x888C,
//! delete +0x8820, wind query +0x7558 and trail effects +0x1DFE0/+0x1E194.
//! The display callback (+0x91A8) is in `duel_display`. Every engine result
//! (allocations, matrix transform, rand, wind callbacks, vector length) is an
//! input; every engine call is recorded with its arguments.
use crate::duel_platform::{self, invalid};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Constants read through the instruction or relocation that loads each one.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebrisConstants {
    pub one: f32,
    pub rand_scale: f32,
    pub degrees_to_bams: f32,
    pub spin_range: [f32; 3],
    pub half: f32,
    pub timer_base: u32,
    pub timer_range: f32,
    pub damping: f32,
    pub gravity: f32,
    pub rest_speed_scale: f32,
    pub zero: f32,
    pub rest_damping: f32,
    pub bounce: f32,
    pub fade: f32,
    pub wind_pull: f32,
    pub trail_chance: f32,
    pub trail_velocity: f32,
    pub large_scale: f32,
    pub small_scale: f32,
    /// Trail helper constants: BAMS per degree, yaw range, rand scale, yaw
    /// offset and roll offset.
    pub trail_bams: f32,
    pub trail_yaw_range: f32,
    pub trail_rand_scale: f32,
    pub trail_yaw_offset: f32,
    pub trail_roll_offset: f32,
    /// Signed axis indices, eight rows, used when flag 0x4000 is set.
    pub axes: [[i32; 3]; 8],
    /// Section 5 offsets of the two trail effect descriptors.
    pub trail_descriptors: [u32; 2],
}
pub fn constants(rel_bytes: &[u8]) -> Result<DebrisConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f4 = |pc| duel_platform::float(&rel, pc);
    let s5 = |pc| duel_platform::target(&rel, pc, 5);
    let word5 = |at: u32| -> Result<u32> {
        let at = at as usize;
        let b = rel.data(5)?.get(at..at + 4).ok_or_else(invalid)?;
        Ok(u32::from_be_bytes(b.try_into().unwrap()))
    };
    for (pc, entry) in [(0x83a2, 0x888c), (0x83be, 0x91a8), (0x83ca, 0x8820)] {
        if duel_platform::target(&rel, pc, 1)? != entry {
            return Err(invalid());
        }
    }
    let axes_at = s5(0x8972)?;
    let mut axes = [[0i32; 3]; 8];
    for (i, row) in axes.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = word5(axes_at + 12 * i as u32 + 4 * j as u32)? as i32;
        }
    }
    Ok(DebrisConstants {
        one: f4(0x849a)?,
        rand_scale: f4(0x84d2)?,
        degrees_to_bams: f4(0x85f6)?,
        spin_range: [f4(0x85e6)?, f4(0x8692)?, f4(0x873e)?],
        half: f4(0x85d6)?,
        timer_base: word5(s5(0x84fe)?)?,
        timer_range: f32::from_bits(word5(s5(0x84e2)?)?),
        damping: f32::from_bits(word5(s5(0x8942)?)?),
        gravity: f32::from_bits(word5(s5(0x894a)?)?),
        rest_speed_scale: f4(0x8952)?,
        zero: f4(0x8906)?,
        rest_damping: f4(0x895e)?,
        bounce: f4(0x8996)?,
        fade: f4(0x8966)?,
        wind_pull: f4(0x897e)?,
        trail_chance: f4(0x88fe)?,
        trail_velocity: f4(0x8926)?,
        large_scale: f4(0x890e)?,
        small_scale: f4(0x8916)?,
        trail_bams: f4(0x1e0be)?,
        trail_yaw_range: f4(0x1e09e)?,
        trail_rand_scale: f4(0x1e08e)?,
        trail_yaw_offset: f4(0x1e0ae)?,
        trail_roll_offset: f4(0x1e10a)?,
        axes,
        trail_descriptors: [s5(0x1e01a)?, s5(0x1e1ce)?],
    })
}

/// One 0x50-byte piece. Fields are named by their native offsets.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    /// +0..+8
    pub position: [f32; 3],
    /// +0xC..+0x14
    pub velocity: [f32; 3],
    /// +0x18..+0x20 (BAMS)
    pub angles: [i32; 3],
    /// +0x24..+0x2C (degrees per update)
    pub spin: [f32; 3],
    /// +0x30
    pub texlist: u32,
    /// +0x34 (section 5 offset of a model whose centre is at +8, radius +0x14)
    pub object: Option<u32>,
    /// +0x38 (flag 0x8000: section 5 offset, centre at +0x14, radius +0x20)
    pub model: Option<u32>,
    /// +0x3C
    pub floor_y: f32,
    /// +0x40
    pub timer: i32,
    /// +0x44
    pub flags: u32,
    /// +0x48
    pub life: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SpawnArgs {
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub velocity: [f32; 3],
    pub floor_y: f32,
    /// Spin in BAMS; `null` draws it from `rand()` (the +0x8308 wrapper passes null).
    pub spin: Option<[i32; 3]>,
    /// Section 5 offset of the model or object.
    pub model: u32,
    pub texlist: u32,
    pub flags: u32,
    /// Game byte +0x37.
    pub paused: bool,
    /// Indices of this call's allocator calls that return null.
    pub failed_allocations: Vec<u32>,
    /// Engine njCalcPoint result for the model centre.
    pub point: [f32; 3],
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WindResult {
    /// The query buffer after the force callbacks (+0 flags, +4 vector, +0x10 direction).
    pub flags: u32,
    pub vector: [f32; 3],
    pub direction: [f32; 3],
    /// njScalor (0x80114EAC) of the vector, used when flag 1 is set.
    pub length: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateArgs {
    pub paused: bool,
    /// Consumed in list order by pieces with flag 0x20000.
    pub wind: Vec<WindResult>,
    /// Indices of this update's trail-effect allocations that return null.
    pub failed_allocations: Vec<u32>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Debris {
    /// Section 6 +0x638: the manager task exists.
    pub manager: bool,
    /// Section 6 +0x63C list, head first.
    pub pieces: Vec<Piece>,
    /// Manager work +6 after the last update.
    pub visited: u16,
}

fn engine(address: u32, args: Value) -> Value {
    json!({ "engine": format!("0x{address:08x}"), "args": args })
}
fn floats(rel: &crate::city_rel::CityRel<'_>, at: u32, n: usize) -> Result<Vec<f32>> {
    let d = rel.data(5)?;
    (0..n)
        .map(|i| {
            let o = at as usize + 4 * i;
            d.get(o..o + 4)
                .map(|b| f32::from_be_bytes(b.try_into().unwrap()))
                .ok_or_else(invalid)
        })
        .collect()
}
fn read(v: &[f32; 3], axes: &[i32; 3]) -> [f32; 3] {
    axes.map(|i| {
        if i < 0 {
            -v[(-i) as usize]
        } else {
            v[i as usize]
        }
    })
}
fn write(v: &mut [f32; 3], axes: &[i32; 3], x: [f32; 3]) {
    for (&i, value) in axes.iter().zip(x) {
        if i < 0 {
            v[(-i) as usize] = -value;
        } else {
            v[i as usize] = value;
        }
    }
}

pub struct System<'a> {
    pub rel: crate::city_rel::CityRel<'a>,
    pub c: DebrisConstants,
}
impl<'a> System<'a> {
    pub fn new(rel_bytes: &'a [u8]) -> Result<Self> {
        Ok(Self {
            rel: duel_platform::module(rel_bytes)?,
            c: constants(rel_bytes)?,
        })
    }
    /// One engine rand() as the single the code builds from it.
    fn draw(&self, rand: &mut dyn Iterator<Item = i32>, calls: &mut Vec<Value>) -> Result<f32> {
        let r = rand
            .next()
            .ok_or_else(|| Error::invalid("debris rand values exhausted"))?;
        calls.push(engine(0x8010d940, json!({ "returned": r })));
        Ok(r as f64 as f32)
    }
    fn unit(&self, rand: &mut dyn Iterator<Item = i32>, calls: &mut Vec<Value>) -> Result<f32> {
        Ok(self.c.rand_scale * self.draw(rand, calls)?)
    }
    /// Spawn through +0x8308 / +0x8348.
    pub fn spawn(
        &self,
        d: &mut Debris,
        a: &SpawnArgs,
        rand: &mut dyn Iterator<Item = i32>,
    ) -> Result<Vec<Value>> {
        let c = &self.c;
        let mut calls = Vec::new();
        let mut alloc = 0u32;
        let mut ok = || {
            alloc += 1;
            !a.failed_allocations.contains(&(alloc - 1))
        };
        if !d.manager {
            let made = ok();
            calls.push(engine(0x80071ba4, json!({ "list": 2, "size": 3, "update": "0x888c", "name": "EfCrash3DExec", "returnedNonzero": made })));
            if made {
                calls.push(json!({ "callbacks": { "display": "0x91a8", "delete": "0x8820" } }));
                d.manager = true;
            }
        }
        if !d.manager {
            return Ok(calls);
        }
        let mut flags = a.flags;
        if a.paused {
            flags |= 0x8000_0000;
        }
        let got = ok();
        calls.push(engine(
            0x801194b4,
            json!({ "list": 1, "size": 0x50, "returnedNonzero": got }),
        ));
        if !got {
            return Ok(calls);
        }
        calls.push(engine(0x80115d50, json!([])));
        calls.push(engine(0x8011509c, json!([0])));
        calls.push(engine(0x80115044, json!({ "translate": a.position })));
        calls.push(engine(0x80115560, json!([0, a.angles[2]])));
        calls.push(engine(0x801154e0, json!([0, a.angles[1]])));
        calls.push(engine(0x80115918, json!([0, a.angles[0]])));
        let large = flags & 0x8000 != 0;
        let centre = floats(&self.rel, a.model + if large { 0x14 } else { 8 }, 3)?;
        calls.push(engine(
            0x80114bc4,
            json!({ "point": centre, "returned": a.point }),
        ));
        calls.push(engine(0x80115a80, json!([1])));
        let timer = (c.timer_base as i32)
            .wrapping_add((c.timer_range * self.unit(rand, &mut calls)?) as i32);
        let spin = match a.spin {
            Some(s) => s.map(|v| v as f64 as f32),
            None => {
                let mut s = [0.0f32; 3];
                for (i, bit) in [0x80u32, 0x100, 0x200].into_iter().enumerate() {
                    let v = if flags & bit != 0 {
                        0
                    } else {
                        let u = self.unit(rand, &mut calls)?;
                        (c.degrees_to_bams * (c.spin_range[i] * (u - c.half))) as i32
                    };
                    s[i] = v as f64 as f32;
                }
                s
            }
        };
        d.pieces.insert(
            0,
            Piece {
                position: a.point,
                velocity: a.velocity,
                angles: a.angles,
                spin,
                texlist: a.texlist,
                object: (!large).then_some(a.model),
                model: large.then_some(a.model),
                floor_y: a.floor_y,
                timer,
                flags,
                life: c.one,
            },
        );
        Ok(calls)
    }
    /// Delete callback +0x8820 for the manager task.
    pub fn delete(&self, d: &mut Debris) -> Vec<Value> {
        d.manager = false;
        d.pieces
            .drain(..)
            .map(|_| engine(0x801194e0, json!([])))
            .collect()
    }
    fn radius(&self, p: &Piece) -> Result<f32> {
        Ok(match (p.model, p.object) {
            (Some(m), _) if p.flags & 0x8000 != 0 => floats(&self.rel, m + 0x20, 1)?[0],
            (_, Some(o)) => floats(&self.rel, o + 0x14, 1)?[0],
            _ => return Err(invalid()),
        })
    }
    /// Update callback +0x888C.
    pub fn update(
        &self,
        d: &mut Debris,
        a: &UpdateArgs,
        rand: &mut dyn Iterator<Item = i32>,
    ) -> Result<Vec<Value>> {
        let c = &self.c;
        let mut calls = Vec::new();
        let running = !a.paused;
        let mut wind = a.wind.iter();
        let mut alloc = 0u32;
        let mut i = 0;
        let mut visited = 0u32;
        while i < d.pieces.len() {
            visited += 1;
            let mut p = d.pieces[i];
            let mut r27 = p.flags;
            let process = r27 & 0x8000_0000 != 0 || running;
            if process {
                if running {
                    r27 &= 0x7fff_ffff;
                    p.flags = r27;
                }
                let permuted = r27 & 0x4000 != 0;
                let axes = c.axes[(r27 & 7) as usize];
                let (mut pos, mut vel) = if permuted {
                    (read(&p.position, &axes), read(&p.velocity, &axes))
                } else {
                    (p.position, p.velocity)
                };
                if !permuted && r27 & 0x20000 != 0 {
                    let w = wind
                        .next()
                        .ok_or_else(|| Error::invalid("debris wind results exhausted"))?;
                    calls.push(json!({ "windQuery": { "flags": w.flags, "vector": w.vector, "direction": w.direction } }));
                    if w.flags & 1 != 0 {
                        calls.push(engine(
                            0x80114eac,
                            json!({ "vector": w.vector, "returned": w.length }),
                        ));
                    }
                    if w.flags & 2 != 0 {
                        for k in 0..3 {
                            vel[k] += c.wind_pull * (w.direction[k] - vel[k]);
                        }
                    }
                }
                for k in 0..3 {
                    pos[k] += vel[k];
                }
                // fcmpo with cror gt|eq: on the ground when floor >= y.
                let ground = p.floor_y >= pos[1];
                let radius_factor = |flags: u32| {
                    if flags & 0x400 != 0 {
                        c.large_scale
                    } else {
                        c.small_scale
                    }
                };
                if ground {
                    vel[0] *= c.damping;
                    vel[2] *= c.damping;
                    if r27 & 0x40800 != 0 {
                        vel[1] *= c.damping;
                    } else {
                        pos[1] = p.floor_y;
                        // fcmpo/bge: rest only when |vy| < 5 * gravity.
                        if vel[1].abs() < c.rest_speed_scale * c.gravity {
                            vel[1] = c.zero;
                            vel[0] *= c.rest_damping;
                            vel[2] *= c.rest_damping;
                            p.spin = [c.zero; 3];
                            p.life = if p.life < c.fade {
                                c.zero
                            } else {
                                p.life - c.fade
                            };
                        } else {
                            vel[1] *= c.bounce;
                            for s in &mut p.spin {
                                *s *= c.rest_damping;
                            }
                        }
                    }
                    if r27 & 0x20 != 0 {
                        p.flags &= !0x20;
                        let radius = self.radius(&p)?;
                        calls.push(engine(
                            0x800e254c,
                            json!({ "position": p.position, "vector": [c.zero, c.zero, c.zero],
                                    "scale": radius * radius_factor(p.flags), "size": c.one + radius }),
                        ));
                    }
                    if r27 & 0x40 != 0 {
                        p.life = c.zero;
                    }
                } else {
                    if r27 & 0x10 != 0 {
                        let u = self.unit(rand, &mut calls)?;
                        if u < c.trail_chance {
                            let v = p.velocity.map(|x| c.trail_velocity * x);
                            let radius = self.radius(&p)?;
                            let scale = radius * radius_factor(p.flags);
                            let size = c.one + radius;
                            let k = usize::from(permuted);
                            let made = !a.failed_allocations.contains(&alloc);
                            alloc += 1;
                            calls.push(engine(0x80032910, json!({ "descriptor": format!("s5+0x{:x}", c.trail_descriptors[k]), "returnedNonzero": made })));
                            if made {
                                let mut unit = |calls: &mut Vec<Value>| -> Result<f32> {
                                    Ok(c.trail_rand_scale * self.draw(rand, calls)?)
                                };
                                let yaw = (c.trail_bams
                                    * (c.trail_yaw_range * unit(&mut calls)? - c.trail_yaw_offset))
                                    as i32;
                                let roll = (c.trail_bams
                                    * (unit(&mut calls)? - c.trail_roll_offset))
                                    as i32;
                                let add = unit(&mut calls)?;
                                calls.push(json!({ "trail": { "position": p.position, "velocity": v, "scale": scale,
                                    "angle0": yaw, "angle2C": roll, "size": size, "add18": add } }));
                            }
                        }
                    }
                    vel[0] *= c.damping;
                    vel[1] = if r27 & 0x40000 != 0 {
                        vel[1] * c.damping
                    } else {
                        vel[1] * c.damping - c.gravity
                    };
                    vel[2] *= c.damping;
                }
                if permuted {
                    write(&mut p.position, &axes, pos);
                    write(&mut p.velocity, &axes, vel);
                } else {
                    p.position = pos;
                    p.velocity = vel;
                }
                for k in 0..3 {
                    p.angles[k] = p.angles[k].wrapping_add((c.degrees_to_bams * p.spin[k]) as i32);
                    p.spin[k] *= c.damping;
                }
                p.timer = p.timer.wrapping_sub(1);
            }
            // fcmpo with cror lt|eq: remove when life <= 0.
            if p.life <= c.zero {
                calls.push(engine(0x801194e0, json!([])));
                d.pieces.remove(i);
                continue;
            }
            if p.timer < 10 {
                p.life = if p.life < c.trail_chance {
                    c.zero
                } else {
                    p.life - c.trail_chance
                };
            }
            d.pieces[i] = p;
            i += 1;
        }
        d.visited = visited as u16;
        if d.pieces.is_empty() {
            // The engine later runs the delete callback; see delete.
            calls.push(engine(0x80071888, json!([])));
        }
        Ok(calls)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "call")]
enum Op {
    Spawn(SpawnArgs),
    Update(UpdateArgs),
    Delete,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    rand: Vec<i32>,
    ops: Vec<Op>,
}
/// Replay a sequence of spawn, update and delete callbacks.
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let sys = System::new(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("debris request: {e}")))?;
    let mut d = Debris::default();
    let mut rand = req.rand.into_iter();
    let mut out = Vec::with_capacity(req.ops.len());
    for op in &req.ops {
        let calls = match op {
            Op::Spawn(a) => sys.spawn(&mut d, a, &mut rand)?,
            Op::Update(a) => sys.update(&mut d, a, &mut rand)?,
            Op::Delete => sys.delete(&mut d),
        };
        out.push(json!({ "calls": calls, "manager": d.manager, "visited": d.visited, "pieces": d.pieces }));
    }
    Ok(json!({ "constants": sys.c, "ops": out }))
}
