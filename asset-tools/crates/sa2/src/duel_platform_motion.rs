//! Per-frame update of one Sonic vs Shadow 2 platform (DODAI, stg42D.rel
//! code +0x1D68). The task's engine queries are inputs and its engine calls
//! are recorded, so the module's own state machine runs without the engine:
//! creation, activation by a standing player, the timed fall, the random
//! spin once below the floor, and removal. Rendering, collision response and
//! the debris effect body are not executed.
use crate::duel_platform::{self, Generator};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// fcmpo/bge skips unless y < floor (an unordered compare also skips).
fn below(y: f32, floor: f32) -> bool {
    y < floor
}

/// Floats read through the instruction that loads each one.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionConstants {
    pub visibility_pad: f32,
    pub debris: [f32; 5],
    pub rumble_strength: f32,
    pub fall_velocity: f32,
    pub floor_y: f32,
    pub terminal_velocity: f32,
    pub acceleration: f32,
    pub rand_scale: f32,
    pub spin_scale: f32,
    pub roll_scale: f32,
    pub roll_offset: f32,
}
pub fn constants(rel_bytes: &[u8]) -> Result<MotionConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    let c = MotionConstants {
        visibility_pad: f(0x1de6)?,
        debris: [f(0x1e5a)?, f(0x1e66)?, f(0x1e76)?, f(0x1e82)?, f(0x1e8e)?],
        rumble_strength: f(0x1ed6)?,
        fall_velocity: f(0x1eea)?,
        floor_y: f(0x1fe6)?,
        terminal_velocity: f(0x2026)?,
        acceleration: f(0x203a)?,
        rand_scale: f(0x2086)?,
        spin_scale: f(0x2096)?,
        roll_scale: f(0x213e)?,
        roll_offset: f(0x214e)?,
    };
    // The state-2 and second floor checks load the same constants.
    if f(0x1f2a)? != c.visibility_pad || f(0x21b2)? != c.floor_y {
        return Err(duel_platform::invalid());
    }
    Ok(c)
}

/// DODAI work fields plus the private model copy and motion block.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformWork {
    /// Work +0.
    pub mode: i8,
    /// Work +3.
    pub activated: u8,
    /// Work +4 (bit 0x100: last visibility result).
    pub flags: u16,
    /// Work +6.
    pub timer: u16,
    /// Work +8..+0x10 (BAMS).
    pub angles: [i32; 3],
    /// Work +0x14..+0x1C.
    pub position: [f32; 3],
    /// Collision object copy +0xC (Y) after the last write.
    pub object_y: Option<f32>,
    /// Collision object copy +0x14..+0x1C.
    pub object_angles: Option<[i32; 3]>,
    /// Task +0x3C motion block, 14 raw words: +4/+8/+0xC spin, +0x14 Y
    /// velocity, and a copy of +0..+0x18 at +0x1C once spinning.
    pub motion: [u32; 14],
}
/// Work as the engine hands it to DODAI's first update. The generator
/// writes only X, the timer and the activation byte; every other field is the
/// engine allocator's and must be supplied.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatformStart {
    pub mode: i8,
    pub activated: u8,
    pub flags: u16,
    pub timer: u16,
    pub angles: [i32; 3],
    pub position: [f32; 3],
    pub motion: [u32; 14],
}
impl PlatformWork {
    /// The object copy does not exist until the constructor succeeds.
    pub fn start(s: &PlatformStart) -> Self {
        Self {
            mode: s.mode,
            activated: s.activated,
            flags: s.flags,
            timer: s.timer,
            angles: s.angles,
            position: s.position,
            object_y: None,
            object_angles: None,
            motion: s.motion,
        }
    }
    pub fn velocity(&self) -> f32 {
        f32::from_bits(self.motion[5])
    }
}

/// Engine results consumed by one update.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatformFrame {
    /// Return of the engine task check (0x8006538C) whenever it is called.
    #[serde(default)]
    pub task_check: bool,
    /// Return of the visibility query (0x80039670).
    #[serde(default)]
    pub visible: bool,
    /// Player data slot 0 field +0xA0 refers to this task.
    #[serde(default)]
    pub standing: bool,
    /// Player 0 work flags bit 1 (gates the rumble request).
    #[serde(default)]
    pub player_flag: bool,
    /// Game byte +0x37.
    #[serde(default)]
    pub paused: bool,
    /// Game byte +0x42.
    #[serde(default)]
    pub game_byte_42: i8,
    /// Zero-based indices of this update's allocator calls that return null.
    #[serde(default)]
    pub failed_allocations: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "call", rename_all = "camelCase")]
pub enum Call {
    /// Engine allocator (0x80119488) and its result.
    Alloc {
        size: u32,
        ok: bool,
    },
    /// Engine free (0x801194E0) of the object copy after a failed model copy.
    FreeObject,
    /// Dynamic-collision registration (0x8002305C).
    #[serde(rename_all = "camelCase")]
    Register {
        collision_flags: u32,
        model_radius: f32,
    },
    TaskCheck,
    Visibility {
        radius: f32,
    },
    /// Local effect helper (+0x1E8DC) at the work position with the angles.
    Debris {
        angles: [i32; 3],
        args: [f32; 5],
    },
    Rumble {
        strength: f32,
        frames: u32,
    },
    Sound {
        args: [u32; 4],
    },
    Rand {
        value: i32,
    },
    FallEffect {
        args: [u32; 4],
    },
    Free,
}

pub struct Platform<'a> {
    pub generator: &'a Generator,
    pub constants: &'a MotionConstants,
    /// Task +0x30 is nonzero (the task check then runs first every update).
    pub has_task_field_30: bool,
}
impl Platform<'_> {
    /// One DODAI update. `rand` supplies successive engine `rand()` returns.
    pub fn step(
        &self,
        w: &mut PlatformWork,
        f: &PlatformFrame,
        rand: &mut dyn Iterator<Item = i32>,
    ) -> Result<Vec<Call>> {
        let c = self.constants;
        let mut calls = Vec::new();
        if self.has_task_field_30 {
            calls.push(Call::TaskCheck);
            if f.task_check {
                return Ok(calls);
            }
        }
        let radius = self.generator.collision_radius;
        let visibility = |w: &mut PlatformWork, calls: &mut Vec<Call>| {
            calls.push(Call::Visibility {
                radius: c.visibility_pad + radius,
            });
            if f.visible {
                w.flags |= 0x100;
            } else {
                w.flags &= !0x100;
            }
        };
        match w.mode {
            0 => 'init: {
                let ok = |n: u32| !f.failed_allocations.contains(&n);
                calls.push(Call::Alloc {
                    size: 0x38,
                    ok: ok(0),
                });
                if !ok(0) {
                    break 'init;
                }
                calls.push(Call::Alloc {
                    size: 0x28,
                    ok: ok(1),
                });
                if !ok(1) {
                    calls.push(Call::FreeObject);
                    break 'init;
                }
                calls.push(Call::Register {
                    collision_flags: self.generator.collision_flags,
                    model_radius: radius,
                });
                w.object_y = Some(w.position[1]);
                w.object_angles = Some(w.angles);
                w.mode = 1;
            }
            1 => {
                visibility(w, &mut calls);
                if w.timer as i16 > self.generator.fall_after {
                    w.mode = 2;
                    calls.push(Call::Debris {
                        angles: w.angles,
                        args: c.debris,
                    });
                    if f.player_flag && f.standing {
                        calls.push(Call::Rumble {
                            strength: c.rumble_strength,
                            frames: 60,
                        });
                    }
                    w.motion[5] = c.fall_velocity.to_bits();
                    w.motion[12] = c.fall_velocity.to_bits();
                    calls.push(Call::Sound {
                        args: [4, 0, 0, 0x50],
                    });
                }
            }
            2 => {
                visibility(w, &mut calls);
                w.position[1] += w.velocity();
                for i in 0..3 {
                    w.angles[i] = w.angles[i].wrapping_add(w.motion[1 + i] as i32);
                }
                w.object_y = Some(w.position[1]);
                w.object_angles = Some(w.angles);
                if below(w.position[1], c.floor_y) {
                    calls.push(Call::TaskCheck);
                    if f.task_check {
                        w.flags &= !0x100;
                        return Ok(calls);
                    }
                    // fcmpo/ble: accelerate only when velocity > terminal.
                    if w.velocity() > c.terminal_velocity {
                        w.motion[5] = (w.velocity() - c.acceleration).to_bits();
                    }
                    if w.motion[1] == 0 {
                        let mut draw = |calls: &mut Vec<Call>| -> Result<f32> {
                            let value = rand
                                .next()
                                .ok_or_else(|| Error::invalid("platform rand values exhausted"))?;
                            calls.push(Call::Rand { value });
                            Ok(c.rand_scale * value as f64 as f32)
                        };
                        let spin = |u: f32, base: i32| base - (((c.spin_scale * u) as i32) & !15);
                        let a = draw(&mut calls)?;
                        w.motion[1] = spin(a, 0x18) as u32;
                        let b = draw(&mut calls)?;
                        w.motion[2] = spin(b, 0x20) as u32;
                        let r = draw(&mut calls)?;
                        w.motion[3] = ((c.roll_scale * r - c.roll_offset) as i32) as u32;
                        w.motion.copy_within(0..7, 7);
                    }
                }
                if below(w.position[1], c.floor_y) {
                    calls.push(Call::FallEffect {
                        args: [5, 1, 0x28, 0x78],
                    });
                }
            }
            3 => {
                calls.push(Call::Free);
                return Ok(calls);
            }
            _ => {}
        }
        if w.activated == 0 && f.standing {
            w.activated = 1;
            w.timer = self.generator.stand_timer;
        }
        if !f.paused && f.game_byte_42 != 1 && w.activated != 0 {
            w.timer = w.timer.wrapping_add(1);
        }
        Ok(calls)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    work: PlatformStart,
    task_field_30: bool,
    #[serde(default)]
    rand: Vec<i32>,
    frames: Vec<PlatformFrame>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameOut {
    work: PlatformWork,
    calls: Vec<Call>,
}
/// Replay one platform from its spawn through the requested updates.
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    let generator = duel_platform::generator(rel_bytes)?;
    let constants = constants(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("duel platform request: {e}")))?;
    let p = Platform {
        generator: &generator,
        constants: &constants,
        has_task_field_30: req.task_field_30,
    };
    let mut w = PlatformWork::start(&req.work);
    let mut rand = req.rand.into_iter();
    let mut frames = Vec::with_capacity(req.frames.len());
    for f in &req.frames {
        let calls = p.step(&mut w, f, &mut rand)?;
        frames.push(FrameOut { work: w, calls });
    }
    Ok(serde_json::json!({ "constants": constants, "frames": frames }))
}
