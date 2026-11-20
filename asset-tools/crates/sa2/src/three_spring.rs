//! The triple spring (3SPRING), ported from the installed stage module
//! (City Escape: stg13D.rel section 1, 0x225CC to 0x22A68, o_3spring in the
//! GSNE8P split, plus its launch helper just before it).
//!
//! Init allocates a 12-byte work block (two floats for the bounce animation
//! and a per-slot cooldown byte at +8 and +9) and registers collision. Each
//! update, a player hit launches that player unless its cooldown is running,
//! then sets the cooldown to 10. Both cooldowns count down, and the bounce
//! animation advances.
//!
//! The launch moves the player into the spring's frame (through the inverse
//! of the placement matrix), pulls local Z toward 8 by 70%, lifts local Y by
//! 5, moves it back to world space and sets it as the player position. It
//! then sets the player's velocity from local (0, 5 + Scale.x, 0) and the
//! placement angles with Y turned by 0x8000, with 30 frames without control.
//! It plays sound 0x1000 and rumbles the slot.
//!
//! Engine services are boundaries the caller provides: the clip gate,
//! allocation, the hit test and the hit player's slot, the player's
//! position, and the Ninja matrix calls (`calc_point`).
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Serialize;

pub const CODE_START: u32 = 0x225cc;
pub const CODE_LEN: u32 = 0x49c;
/// SHA-256 of that range with relocated words zeroed.
pub const CODE_HASH: &str = "73126151e17f68fa727278a40f1d7174e1a778108d7db1196b0a4ec19e8baa4f";

/// Engine imports at offsets from `CODE_START`: (offset, address).
const IMPORTS: &[(u32, u32)] = &[
    (0x4c, 0x8003_7da0),  // player position
    (0x50, 0x8011_5d50),  // push matrix
    (0x58, 0x8011_509c),  // unit matrix
    (0x60, 0x8011_5e24),  // translate
    (0x6c, 0x8011_5560),  // rotate (angle Z)
    (0x78, 0x8011_5918),  // rotate (angle X)
    (0x84, 0x8011_54e0),  // rotate (angle Y)
    (0x88, 0x8011_5d50),  // push matrix
    (0x90, 0x8011_4cb0),  // invert matrix
    (0xa0, 0x8011_4bc4),  // calc point
    (0xe8, 0x8011_5d9c),  // pop matrix
    (0xf8, 0x8011_4bc4),  // calc point
    (0xfc, 0x8011_5d9c),  // pop matrix
    (0x110, 0x8003_9754), // set player position
    (0x154, 0x8003_9ab8), // set player velocity, angles and no-control frames
    (0x168, 0x8006_b710), // sound
    (0x17c, 0x8002_f8c4), // rumble
    (0x1b8, 0x8006_538c), // init: clip gate
    (0x1cc, 0x8011_94b4), // init: allocate
    (0x220, 0x8000_f8b8), // init: register collision
    (0x284, 0x8011_94e0), // destructor: free work
    (0x2c0, 0x8006_538c), // update: clip gate
    (0x2d0, 0x8000_66f4), // update: CCL_IsHitPlayer
    (0x2dc, 0x8003_7c74), // update: hit player's slot
    (0x384, 0x8000_f4ec), // update: CCL_Entry
];

fn invalid() -> Error {
    Error::invalid("unsupported triple spring module")
}

fn word(d: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        d.get(at..at + 4).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}

fn imm(d: &[u8], at: usize) -> Result<i32> {
    Ok(word(d, at)? as u16 as i16 as i32)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub module: u32,
    pub init: u32,
    /// Local launch velocity before Scale.x is added to Y.
    pub velocity: [f32; 3],
    /// Local Z target and pull: z' = target + pull * (z - target).
    pub z_target: f32,
    pub z_pull: f32,
    pub lift: f32,
    pub nocon_frames: i32,
    pub sound: u16,
    pub rumble: [i32; 3],
    pub cooldown: u8,
    pub work_size: u32,
    pub collision_flag: u16,
    /// Bounce: on launch a = kick, b = 0; each update a = damp * (a - b * spring), b += a.
    pub kick: f32,
    pub damp: f32,
    pub spring: f32,
}

impl Program {
    pub fn read(filename: &str, bytes: &[u8]) -> Result<Self> {
        let info = stage_land::catalog()
            .iter()
            .find(|i| i.filename == filename)
            .ok_or_else(invalid)?;
        if bytes.len() != info.size || manifest::sha256(bytes) != info.sha256 {
            return Err(invalid());
        }
        let rel = CityRel::read_identified(bytes, info.module)?;
        let d5 = rel.data(5)?;
        let mut init = None;
        for row in (4..d5.len() as u32).step_by(4) {
            let Ok(Some((s, n))) = rel.local_pointer(5, row) else {
                continue;
            };
            let name = rel.data(s)?.get(n as usize..).unwrap_or_default();
            let name = name.split(|&c| c == 0).next().unwrap_or_default();
            if name.trim_ascii_end() == b"3SPRING" {
                if let Ok(Some((1, at))) = rel.local_pointer(5, row - 4) {
                    init = Some(at);
                    break;
                }
            }
        }
        let init = init.ok_or_else(invalid)?;
        if init != CODE_START + 0x198 {
            return Err(invalid());
        }
        let code = rel
            .data(1)?
            .get(CODE_START as usize..(CODE_START + CODE_LEN) as usize)
            .ok_or_else(invalid)?;
        let mut masked = code.to_vec();
        for off in (0..CODE_LEN).step_by(2) {
            if rel.reference(1, CODE_START + off).is_some() {
                let w = (off & !3) as usize;
                masked[w..w + 4].fill(0);
            }
        }
        if manifest::sha256(&masked) != CODE_HASH {
            return Err(invalid());
        }
        for &(off, addr) in IMPORTS {
            let r = rel.reference(1, CODE_START + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        // A HA/LO pair into this module's section 4 or 5.
        let target = |at: u32| -> Result<(u8, u32)> {
            let hi = rel.reference(1, CODE_START + at + 2).ok_or_else(invalid)?;
            let lo = rel.reference(1, CODE_START + at + 6).ok_or_else(invalid)?;
            if hi.module != info.module
                || lo.module != info.module
                || (hi.kind, lo.kind) != (6, 4)
                || hi.section != lo.section
                || !(4..=5).contains(&hi.section)
                || hi.addend != lo.addend
            {
                return Err(invalid());
            }
            Ok((hi.section, hi.addend))
        };
        let f = |at: u32, k: u32| -> Result<f32> {
            let (s, a) = target(at)?;
            Ok(f32::from_bits(word(rel.data(s)?, (a + 4 * k) as usize)?))
        };
        let zero = f(0x234, 0)?;
        if f(0x310, 0)? != zero {
            return Err(invalid());
        }
        Ok(Self {
            module: info.module,
            init,
            velocity: [f(0x18, 0)?, f(0x18, 1)?, f(0x18, 2)?],
            z_target: f(0xa4, 0)?,
            z_pull: f(0xb0, 0)?,
            lift: f(0xd4, 0)?,
            nocon_frames: imm(code, 0x150)?,
            sound: imm(code, 0x158)? as u16,
            rumble: [imm(code, 0x170)?, imm(code, 0x174)?, imm(code, 0x178)?],
            cooldown: imm(code, 0x330)? as u8,
            work_size: imm(code, 0x1c8)? as u32,
            collision_flag: imm(code, 0x22c)? as u16,
            kick: f(0x300, 0)?,
            damp: f(0x38c, 0)?,
            spring: f(0x3a0, 0)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "call", rename_all = "camelCase")]
pub enum Call {
    Allocate {
        size: u32,
    },
    /// Collision registered (import 0x8000F8B8, 1, 4), then `flag` ORed into
    /// the collision info's halfword at +4.
    InitCollision {
        flag: u16,
    },
    FreeWork,
    EntryCollision,
    PlayerPosition {
        slot: i8,
    },
    PushMatrix,
    UnitMatrix,
    Translate {
        position: [f32; 3],
    },
    Rotate {
        import: u32,
        angle: i32,
    },
    InvertMatrix,
    CalcPoint {
        local: [f32; 3],
    },
    PopMatrix,
    SetPosition {
        slot: i8,
        position: [f32; 3],
    },
    SetVelocity {
        slot: i8,
        velocity: [f32; 3],
        angles: [i32; 3],
        frames: i32,
    },
    Sound(u16),
    Rumble {
        slot: i8,
        args: [i32; 3],
    },
}

pub trait Engine {
    fn clipped(&mut self) -> bool;
    fn allocate(&mut self) -> bool;
    fn hit(&mut self) -> bool;
    /// The hit player's slot (signed; negative means none). Only 0 and 1
    /// are modeled.
    fn hit_slot(&mut self) -> i8;
    fn player_position(&mut self, slot: i8) -> [f32; 3];
    fn calc_point(&mut self, p: [f32; 3]) -> [f32; 3];
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    pub bounce: [f32; 2],
    pub cooldown: [u8; 2],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeSpring {
    pub position: [f32; 3],
    /// Rotation words (X, Y, Z) as the task stores them.
    pub angles: [i32; 3],
    pub scale: [f32; 3],
    pub work: Option<Work>,
}

impl ThreeSpring {
    pub fn new(position: [f32; 3], angles: [i32; 3], scale: [f32; 3]) -> Self {
        Self {
            position,
            angles,
            scale,
            work: None,
        }
    }

    pub fn step(&mut self, p: &Program, e: &mut impl Engine) -> Vec<Call> {
        let mut calls = vec![];
        if e.clipped() {
            return calls;
        }
        if self.work.is_none() {
            calls.push(Call::Allocate { size: p.work_size });
            if e.allocate() {
                calls.push(Call::InitCollision {
                    flag: p.collision_flag,
                });
                self.work = Some(Work::default());
            }
            return calls;
        }
        if e.hit() {
            let slot = e.hit_slot();
            // The native code indexes its two cooldown bytes with the slot
            // unchecked; only slots 0 and 1 are modeled.
            if (0..=1).contains(&slot) {
                let i = slot as usize;
                if self.work.as_ref().unwrap().cooldown[i] == 0 {
                    let w = self.work.as_mut().unwrap();
                    w.bounce = [p.kick, 0.0];
                    self.launch(p, e, slot, &mut calls);
                }
                self.work.as_mut().unwrap().cooldown[i] = p.cooldown;
            }
        }
        let w = self.work.as_mut().unwrap();
        for c in &mut w.cooldown {
            *c = c.saturating_sub(1);
        }
        calls.push(Call::EntryCollision);
        let [a, b] = w.bounce;
        let a = p.damp * (a - b * p.spring);
        w.bounce = [a, b + a];
        calls
    }

    /// The destructor.
    pub fn destroy(&mut self) -> Vec<Call> {
        self.work = None;
        vec![Call::FreeWork]
    }

    fn launch(&self, p: &Program, e: &mut impl Engine, slot: i8, calls: &mut Vec<Call>) {
        calls.push(Call::PlayerPosition { slot });
        let player = e.player_position(slot);
        calls.extend([
            Call::PushMatrix,
            Call::UnitMatrix,
            Call::Translate {
                position: self.position,
            },
            Call::Rotate {
                import: 0x8011_5560,
                angle: self.angles[2],
            },
            Call::Rotate {
                import: 0x8011_5918,
                angle: self.angles[0],
            },
            Call::Rotate {
                import: 0x8011_54e0,
                angle: self.angles[1],
            },
            Call::PushMatrix,
            Call::InvertMatrix,
            Call::CalcPoint { local: player },
        ]);
        let mut l = e.calc_point(player);
        l[2] = p.z_target + p.z_pull * (l[2] - p.z_target);
        l[1] += p.lift;
        calls.extend([Call::PopMatrix, Call::CalcPoint { local: l }]);
        let world = e.calc_point(l);
        calls.extend([
            Call::PopMatrix,
            Call::SetPosition {
                slot,
                position: world,
            },
            Call::SetVelocity {
                slot,
                velocity: [p.velocity[0], p.velocity[1] + self.scale[0], p.velocity[2]],
                angles: [
                    self.angles[0],
                    self.angles[1].wrapping_add(0x8000),
                    self.angles[2],
                ],
                frames: p.nocon_frames,
            },
            Call::Sound(p.sound),
            Call::Rumble {
                slot,
                args: p.rumble,
            },
        ]);
    }
}
