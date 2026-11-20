//! The dash panel (KASOKU) task, ported from the installed stage module.
//!
//! The update callback lives in each stage REL's section 1 (City Escape:
//! stg13D.rel at 1:0x22C8C). `Program::read` locates it through the stage's
//! object list, checks the instruction words and every engine import, and
//! reads its constants. `DashPanel::step` replays one task update and
//! returns the engine calls it makes. Engine services are boundaries: the
//! caller observes the clip gate, the collision hit, the players' ground
//! flags and the float the timer conversion returns.
//!
//! Native behaviour, in order:
//! - Init (mode 0): mode 1, last slot 13, collision registered (1, 4). A
//!   Scale.x of 0 or less becomes 14.0 and a Scale.y of 0 or less 60.0.
//! - Update (mode 1): a nonzero timer counts down. A player hit fires only if
//!   that player's status bit 0 is set, and not again for the same slot while
//!   the timer runs. Firing passes local velocity (Scale.x, 0, 0) and the
//!   panel's three angles to the engine with the converted Scale.y as the
//!   timer, plays sound 0x1003 and rumbles the slot (4, 15, 0).
//! - Every update of mode 1 ends by entering collision.
use crate::{city_rel::CityRel, manifest, Error, Result};
use serde::Serialize;

/// SHA-256 of the 0x264-byte update callback with relocated words zeroed.
pub const CODE_HASH: &str = "1036dc6e1d91fb6c909601b763d4b9020a6db9b0d0d1133832da881498bd597d";
pub const CODE_LEN: u32 = 0x264;

/// Engine imports at offsets from the callback start: (offset, address).
const IMPORTS: [(u32, u32); 8] = [
    (0x24, 0x8006_55f4),  // clip gate
    (0xc0, 0x8000_f7d8),  // CCL_InitShare
    (0x138, 0x8000_66f4), // CCL_IsHitPlayer
    (0x1ec, 0x8011_12e8), // timer conversion
    (0x210, 0x8003_9eb0), // player velocity, angles and timer
    (0x224, 0x8006_b978), // sound
    (0x238, 0x8002_fb2c), // rumble
    (0x244, 0x8000_f4ec), // CCL_Entry
];

fn invalid() -> Error {
    Error::invalid("unsupported dash panel module")
}

fn word(d: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        d.get(at..at + 4).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}

fn imm(d: &[u8], at: usize) -> Result<i16> {
    Ok(word(d, at)? as u16 as i16)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub module: u32,
    /// Callback offset in section 1.
    pub update: u32,
    pub default_speed: f32,
    pub default_frames: f32,
    /// Threshold both Scale fields are compared with (less or equal).
    pub floor: f32,
    /// Y and Z of the local velocity (X is Scale.x).
    pub velocity_yz: [f32; 2],
    pub initial_slot: i8,
    pub sound: u16,
    pub rumble: [i32; 3],
}

impl Program {
    /// Read and verify the KASOKU callback of an installed stage REL.
    pub fn read(filename: &str, bytes: &[u8]) -> Result<Self> {
        let rel = CityRel::read_stage(bytes, &filename).map_err(|_| invalid())?;
        let module = rel.module_id();
        let d5 = rel.data(5)?;
        let mut update = None;
        for row in (4..d5.len() as u32).step_by(4) {
            let Ok(Some((s, n))) = rel.local_pointer(5, row) else {
                continue;
            };
            // Object-list names are NUL-terminated and space-padded.
            let name = rel.data(s)?.get(n as usize..).unwrap_or_default();
            let name = name.split(|&c| c == 0).next().unwrap_or_default();
            if name.trim_ascii_end() != b"KASOKU" {
                continue;
            }
            if let Ok(Some((1, init))) = rel.local_pointer(5, row - 4) {
                update = Some(init);
                break;
            }
        }
        let update = update.ok_or_else(invalid)?;
        let code = rel
            .data(1)?
            .get(update as usize..(update + CODE_LEN) as usize)
            .ok_or_else(invalid)?;
        let mut masked = code.to_vec();
        for off in (0..CODE_LEN).step_by(2) {
            if rel.reference(1, update + off).is_some() {
                let w = (off & !3) as usize;
                masked[w..w + 4].fill(0);
            }
        }
        if manifest::sha256(&masked) != CODE_HASH {
            return Err(invalid());
        }
        for (off, addr) in IMPORTS {
            let r = rel.reference(1, update + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        // A HA/LO pair into this module's section 4 starting at `at`.
        let constant = |at: u32| -> Result<u32> {
            let hi = rel.reference(1, update + at + 2).ok_or_else(invalid)?;
            let lo = rel.reference(1, update + at + 6).ok_or_else(invalid)?;
            if (
                hi.module, hi.kind, hi.section, lo.module, lo.kind, lo.section,
            ) != (module, 6, 4, module, 4, 4)
                || hi.addend != lo.addend
            {
                return Err(invalid());
            }
            Ok(hi.addend)
        };
        let d4 = rel.data(4)?;
        let f = |a: u32| -> Result<f32> { Ok(f32::from_bits(word(d4, a as usize)?)) };
        let floor = constant(0xc8)?;
        if constant(0xf4)? != floor {
            return Err(invalid());
        }
        let velocity = constant(0x1a8)?;
        let c = code;
        Ok(Self {
            module,
            update,
            default_speed: f(constant(0xe0)?)?,
            default_frames: f(constant(0x10c)?)?,
            floor: f(floor)?,
            velocity_yz: [f(velocity + 4)?, f(velocity + 8)?],
            initial_slot: imm(c, 0x68)? as i8,
            sound: imm(c, 0x214)? as u16,
            rumble: [
                imm(c, 0x22c)? as i32,
                imm(c, 0x230)? as i32,
                imm(c, 0x234)? as i32,
            ],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "call", rename_all = "camelCase")]
pub enum Call {
    /// CCL_InitShare(task, collision, 1, 4).
    InitCollision,
    /// The engine sets the player's velocity from `velocity` turned by
    /// `angles`, with `frames` as its timer.
    Accelerate {
        slot: u8,
        velocity: [f32; 3],
        angles: [i32; 3],
        frames: u32,
    },
    Sound(u16),
    Rumble {
        slot: u8,
        args: [i32; 3],
    },
    /// CCL_Entry(task).
    EntryCollision,
}

/// What the engine reports to one update.
#[derive(Debug, Clone, Copy, Default)]
pub struct Observed {
    /// The clip gate returned nonzero: the update does nothing.
    pub clipped: bool,
    /// CCL_IsHitPlayer: the slot whose task was hit. The callback tells the
    /// two slots apart by comparing with slot 1, so any other hit is slot 0.
    pub hit: Option<u8>,
    /// Status bit 0 of each player's task work (+4).
    pub ground: [bool; 2],
    /// The value the timer conversion returns for Scale.y.
    pub frames: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashPanel {
    pub mode: i8,
    pub last_slot: i8,
    pub timer: u32,
    pub scale: [f32; 3],
    pub angles: [i32; 3],
}

impl DashPanel {
    pub fn new(angles: [i32; 3], scale: [f32; 3]) -> Self {
        Self {
            mode: 0,
            last_slot: 0,
            timer: 0,
            scale,
            angles,
        }
    }

    /// One update of the task.
    pub fn step(&mut self, p: &Program, o: Observed) -> Vec<Call> {
        let mut calls = vec![];
        if o.clipped {
            return calls;
        }
        match self.mode {
            0 => {
                self.mode = 1;
                self.last_slot = p.initial_slot;
                calls.push(Call::InitCollision);
                if self.scale[0] <= p.floor {
                    self.scale[0] = p.default_speed;
                }
                if self.scale[1] <= p.floor {
                    self.scale[1] = p.default_frames;
                }
            }
            1 => {
                if self.timer != 0 {
                    self.timer -= 1;
                }
                if let Some(hit) = o.hit {
                    let slot = u8::from(hit == 1);
                    let blocked = self.timer != 0 && self.last_slot as u8 == slot;
                    if o.ground[slot as usize] && !blocked {
                        // fctiwz (NaN converts to 0x80000000), stored unsigned.
                        self.timer = if o.frames.is_nan() {
                            0x8000_0000
                        } else {
                            o.frames as i32 as u32
                        };
                        calls.push(Call::Accelerate {
                            slot,
                            velocity: [self.scale[0], p.velocity_yz[0], p.velocity_yz[1]],
                            angles: self.angles,
                            frames: self.timer,
                        });
                        calls.push(Call::Sound(p.sound));
                        calls.push(Call::Rumble {
                            slot,
                            args: p.rumble,
                        });
                        self.last_slot = slot as i8;
                    }
                }
                calls.push(Call::EntryCollision);
            }
            _ => {}
        }
        calls
    }
}
