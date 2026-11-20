//! MINIMAL, the small-animal capsule (main program: GSNE8P main.dol
//! 0x80025C7C set-up, 0x80026AFC exec; PC stage-module row 0x80025A34).
//!
//! Set-up: a 0x20-byte work block. `mode` (w0) is (Rotation.x >> 8) & 0xF
//! when 1 or 2, else 0. The animal slot (w1) comes from (Rotation.x >> 4) &
//! 0xF and Rotation.x & 0xF: group 0 is slot 4; group 1 is slot 2 or 3 (sub
//! 0 or 1, else MSL rand < 0.5 picks 2); group 2 is slot 0 or 1 likewise;
//! otherwise slot trunc(4 * u). The animal (w2) is the level's entry for
//! that slot in the table at 0x80195B54 (filled at run time; the host
//! supplies it). w3 is the spin angle (Rotation.y), w4 the vertical speed,
//! w5 a hold counter, w6 the motion frame, w7 the last gravity mode
//! (0x80005500).
//!
//! Exec, each frame (the object range check is not run):
//! * The hold counter counts down. Action 0 sets Scale.z to 1, copies the
//!   byte at r13-0x7978 into Rotation.x and becomes action 1.
//! * Action 1, unless mode 2, holding, or mode 3 within its first 10
//!   frames: the capsule (orbiting its centre by Scale.x at the spin angle in
//!   mode 0) is collected by the player in reach: within 12 horizontally and
//!   -5 < dy < 18 for character 7, 11 / -5..15 for character 6, 7 / -5..10
//!   otherwise. Collecting gives the animal (0x800199DC), marks the SET
//!   entry, spawns the pickup effect, counts toward the player's animal
//!   mission (unless Status has 0x100), and goes to action 2 (deleted on the
//!   next frame). Missing it, modes 2 and up register collision.
//! * Unless the byte at 0x801CC168 + 0x37 is set, by mode: 0 spins by
//!   Rotation.z a frame; 2 asks a walker within Scale.x for action 0x2A and,
//!   once the walker is doing it, goes to mode 5 with a 60-frame hold and
//!   speed +-2.5; 3 flies off (bob 0.1 * njSin(timer * 0x222) - 0.008 up,
//!   0.1 forward along the spin angle, which turns 0xDA a frame) for 600
//!   frames, then mode 4; 4 settles on the land below (or above, in gravity
//!   mode 3) through fn_80023808 / fn_800D05D0 and goes to mode 5; 5 hops
//!   toward the nearest player, 0.2 a frame (in 3-D for animals 9 to 11).
//! * The motion frame advances by the motion table at 0x801E5B88 (host).
use crate::land_probe::Probe;
use crate::msl_math::{atan2f, gekko_frsqrte};
use crate::ninja_trig::{fmadds, nj_cos, nj_sin};
use crate::play_world::V3;
use crate::prison_visuals::MslRand;
use serde::Serialize;

const UNIT: f32 = f32::from_bits(0x3800_0000); // r2-0x79C0
const HOP: f32 = 2.5; // r2-0x79A8
const REACH: f32 = 10.0; // r2-0x7978
const FLY_BOB: f32 = f32::from_bits(0xbc03_126f); // r2-0x7974, -0.008
const FLY_STEP: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x7970, 0.1
const GRAVITY: f32 = f32::from_bits(0xbdcc_cccd); // r2-0x796C, -0.1
const FAR: f32 = f32::from_bits(0x5015_02f9); // r2-0x7968, 1e10
const FLOAT: f32 = 2.0; // r2-0x7964
const HOP_STEP: f32 = f32::from_bits(0x3e4c_cccd); // r2-0x7948, 0.2
const HOP_LIFT: f32 = 5.0; // r2-0x7938
const BAMS_PER_RADIAN: f64 = 10430.38043493439; // r2-0x7940

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MinimalEvent {
    /// 0x800199DC(slot, animal), the pickup effect, and (unless Status has
    /// 0x100) one more toward the player's animal mission.
    Collected {
        slot: u8,
        animal: u32,
        counts: bool,
    },
    /// Byte +0xD of the player's 0x801E7728 entry set to 0x2A.
    RequestAction {
        slot: u8,
    },
    Gone,
}

/// A player as the capsule reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimalPlayer {
    pub position: V3,
    /// The byte at +1 of the 0x801E7728 entry.
    pub character: i8,
    /// The task's Action byte.
    pub action: u8,
}

pub struct MinimalInputs<'a> {
    /// The eight player slots (0x801E7768 / 0x801E7728 / 0x801E7788).
    pub players: [Option<MinimalPlayer>; 8],
    /// 0x80037C84: the player slot nearest a point, used when the byte at
    /// 0x801CC168 + 0x1E is set (else slot 0) and by mode 5.
    pub nearest: &'a dyn Fn(V3) -> usize,
    pub use_nearest: bool,
    pub suppressed: bool,
    /// 0x80005500 (r13-0x7B10).
    pub gravity_mode: u32,
    /// r13-0x7978.
    pub rotation_x_byte: i8,
    /// fn_80023808 then fn_800D05D0 on each listed entry, at a point with
    /// radius 10.
    pub land: &'a mut dyn FnMut(V3) -> Vec<Probe>,
    /// The motion table at 0x801E5B88: (speed, frame count), if any.
    pub motion: Option<(f32, i32)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Minimal {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [i32; 3],
    pub scale: V3,
    pub action: u8,
    pub timer: u16,
    pub status: u16,
    pub mode: u32,
    pub slot: u32,
    pub animal: u32,
    pub spin: i32,
    pub speed: f32,
    pub hold: i32,
    pub frame: f32,
    pub gravity: u32,
}

fn unit(rand: &mut MslRand) -> f32 {
    UNIT * (rand.rand() as i32 as f32)
}

/// The inline square root at 0x8002724C (as 0x8002F15C: Gekko `frsqrte`,
/// three fused double Newton steps); a non-positive input is kept.
fn inline_sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        return x;
    }
    let x64 = f64::from(x);
    let mut g = gekko_frsqrte(x64);
    for _ in 0..3 {
        let sq = g * g;
        let half = 0.5 * g;
        g = half * -x64.mul_add(sq, -3.0);
    }
    (x64 * g) as f32
}

/// The animal slot set-up picks (0x80025CF0..0x80025E74).
pub fn animal_slot(rotation_x: i32, rand: &mut MslRand) -> u32 {
    let group = (rotation_x >> 4) & 0xf;
    let sub = rotation_x & 0xf;
    let pick = |a: u32, b: u32, rand: &mut MslRand| if unit(rand) < 0.5 { a } else { b };
    match group {
        0 => 4,
        1 => match sub {
            0 => 2,
            1 => 3,
            _ => pick(2, 3, rand),
        },
        2 => match sub {
            0 => 0,
            1 => 1,
            _ => pick(0, 1, rand),
        },
        _ => (4.0 * unit(rand)) as i32 as u32,
    }
}

impl Minimal {
    /// `animal` is the level table's entry for the slot this returns
    /// through `animal_slot` (the host looks it up).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layer: usize,
        index: usize,
        position: V3,
        rotation: [u16; 3],
        scale: V3,
        rand: &mut MslRand,
        animal_for_slot: impl Fn(u32) -> u32,
        gravity_mode: u32,
    ) -> Self {
        let r = rotation.map(|r| i32::from(r as i16));
        let mode = match (r[0] >> 8) & 0xf {
            1 => 1,
            2 => 2,
            _ => 0,
        };
        let slot = animal_slot(r[0], rand);
        Self {
            layer,
            index,
            position,
            rotation: r,
            scale,
            action: 0,
            timer: 0,
            status: 0,
            mode,
            slot,
            animal: animal_for_slot(slot),
            spin: r[1],
            speed: 0.0,
            hold: 0,
            frame: 0.0,
            gravity: gravity_mode,
        }
    }

    pub fn step(&mut self, input: &mut MinimalInputs<'_>) -> Vec<MinimalEvent> {
        let mut out = vec![];
        if self.hold > 0 {
            self.hold -= 1;
        }
        match self.action {
            0 | 1 => {
                if self.action == 0 {
                    self.scale[2] = 1.0;
                    self.action = 1;
                    self.rotation[0] = i32::from(input.rotation_x_byte);
                }
                let skip = self.mode == 2 || self.hold != 0 || (self.mode == 3 && self.timer < 10);
                if !skip && self.collect(input, &mut out) {
                    return out;
                }
            }
            2 => {
                // 0x80071AE0 deletes the task; the rest of this frame still runs.
                out.push(MinimalEvent::Gone);
                self.action = 3;
            }
            3 => return out,
            _ => {}
        }
        if input.suppressed {
            return out;
        }
        match self.mode {
            0 => self.spin = self.spin.wrapping_add(self.rotation[2]),
            2 => self.request(input, &mut out),
            3 => self.fly_off(),
            4 => self.settle(input),
            5 => self.hop(input),
            _ => {}
        }
        if let Some((speed, frames)) = input.motion {
            self.frame += speed;
            if self.frame >= frames as f32 {
                self.frame -= frames as f32;
            }
        }
        self.gravity = input.gravity_mode;
        out
    }

    fn collect(&mut self, input: &mut MinimalInputs<'_>, out: &mut Vec<MinimalEvent>) -> bool {
        let mut p = self.position;
        if self.mode == 0 {
            p[2] = fmadds(self.scale[0], nj_cos(self.spin), p[2]);
            p[0] = fmadds(self.scale[0], nj_sin(self.spin), p[0]);
        }
        let slot = if input.use_nearest {
            (input.nearest)(p)
        } else {
            0
        };
        let Some(pl) = input.players.get(slot).copied().flatten() else {
            return false;
        };
        let dx = p[0] - pl.position[0];
        let dy = p[1] - pl.position[1];
        let dz = p[2] - pl.position[2];
        let (r2, top) = match pl.character {
            7 => (144.0, 18.0),
            6 => (121.0, 15.0),
            _ => (49.0, REACH),
        };
        let flat = fmadds(dx, dx, dz * dz);
        let hit = flat < r2 && dy > -5.0 && dy < top;
        if !hit {
            // Modes 2 and up then register collision (0x8000F4EC).
            return false;
        }
        out.push(MinimalEvent::Collected {
            slot: slot as u8,
            animal: self.animal,
            counts: self.status & 0x100 == 0,
        });
        self.action = 2;
        true
    }

    fn request(&mut self, input: &MinimalInputs<'_>, out: &mut Vec<MinimalEvent>) {
        for (k, p) in input.players.iter().enumerate() {
            let Some(p) = p else { continue };
            let r = self.scale[0] * self.scale[0];
            let dy = self.position[1] - p.position[1];
            let dx = self.position[0] - p.position[0];
            let dz = self.position[2] - p.position[2];
            let d = fmadds(dz, dz, fmadds(dx, dx, dy * dy));
            if d >= r {
                continue;
            }
            out.push(MinimalEvent::RequestAction { slot: k as u8 });
            if p.action == 0x2a {
                self.mode = 5;
                self.hold = 60;
                self.speed = if input.gravity_mode == 3 { HOP } else { -HOP };
            }
        }
    }

    fn fly_off(&mut self) {
        self.timer = self.timer.wrapping_add(1);
        if self.timer >= 600 {
            self.timer = 0;
            self.mode = 4;
            return;
        }
        let t = i32::from(self.timer);
        self.position[1] += fmadds(FLY_STEP, nj_sin(t * 0x222), FLY_BOB);
        self.position[2] = fmadds(FLY_STEP, nj_cos(self.rotation[1]), self.position[2]);
        self.position[0] = fmadds(FLY_STEP, nj_sin(self.rotation[1]), self.position[0]);
        self.rotation[1] = self.rotation[1].wrapping_add(0xda);
    }

    fn settle(&mut self, input: &mut MinimalInputs<'_>) {
        if input.gravity_mode == 3 {
            self.speed += GRAVITY;
            if self.speed < -HOP {
                self.speed = -HOP;
            }
            self.position[1] += self.speed;
            let mut best = FAR;
            for pr in (input.land)(self.position) {
                if pr.above.is_some() && best > pr.above_y {
                    best = pr.above_y;
                }
            }
            if best < FLOAT + self.position[1] {
                self.position[1] = best - FLOAT;
            }
            if self.speed < 0.0 {
                self.mode = 5;
            }
        } else {
            self.speed -= GRAVITY;
            if self.speed > HOP {
                self.speed = HOP;
            }
            self.position[1] += self.speed;
            let mut best = -FAR;
            for pr in (input.land)(self.position) {
                if pr.below.is_some() && best < pr.below_y {
                    best = pr.below_y;
                }
            }
            if best > self.position[1] - FLOAT {
                self.position[1] = FLOAT + best;
            }
            if self.speed > 0.0 {
                self.mode = 5;
            }
        }
    }

    fn hop(&mut self, input: &mut MinimalInputs<'_>) {
        let slot = (input.nearest)(self.position);
        let Some(pl) = input.players.get(slot).copied().flatten() else {
            return;
        };
        let p = pl.position;
        if (9..12).contains(&self.animal) {
            let dz = p[2] - self.position[2];
            let dx = p[0] - self.position[0];
            let base = if input.gravity_mode == 3 {
                self.position[1] - HOP_LIFT
            } else {
                HOP_LIFT + self.position[1]
            };
            let dy = p[1] - base;
            let len = inline_sqrtf(fmadds(dy, dy, fmadds(dz, dz, dx * dx)));
            let k = HOP_STEP / len;
            self.rotation[1] = (BAMS_PER_RADIAN * f64::from(atan2f(dx, dz))) as i32;
            self.position[2] = fmadds(k, dz, self.position[2]);
            self.position[0] = fmadds(k, dx, self.position[0]);
            self.position[1] = fmadds(k, dy, self.position[1]);
            return;
        }
        let dx = p[0] - self.position[0];
        let dz = p[2] - self.position[2];
        let len = inline_sqrtf(fmadds(dz, dz, dx * dx));
        let k = HOP_STEP / len;
        self.rotation[1] = (BAMS_PER_RADIAN * f64::from(atan2f(dx, dz))) as i32;
        let nz = fmadds(k, dz, self.position[2]);
        let nx = fmadds(k, dx, self.position[0]);
        if input.gravity_mode == 3 {
            if self.gravity != input.gravity_mode {
                self.position[1] -= FLOAT;
            }
            self.speed += GRAVITY;
            if self.speed < -HOP {
                self.speed = -HOP;
            }
            let at = [nx, HOP_STEP + self.position[1], nz];
            let mut best = -FAR;
            for pr in (input.land)(at) {
                if pr.below.is_some() && best < pr.below_y {
                    best = pr.below_y;
                }
            }
            // fcmpo best, y: move while the land is at or below the capsule.
            if best <= self.position[1] {
                self.position[2] = nz;
                self.position[0] = nx;
                self.position[1] += self.speed;
                if self.position[1] < best {
                    self.speed = 0.0;
                    self.position[1] = best;
                }
            }
        } else {
            if self.gravity != input.gravity_mode {
                self.position[1] += FLOAT;
            }
            self.speed -= GRAVITY;
            if self.speed > -HOP {
                self.speed = HOP;
            }
            let at = [nx, self.position[1] - HOP_STEP, nz];
            let mut best = FAR;
            for pr in (input.land)(at) {
                if pr.above.is_some() && best > pr.above_y {
                    best = pr.above_y;
                }
            }
            // fcmpo best, y: move while the land is at or above the capsule.
            if best >= self.position[1] {
                self.position[2] = nz;
                self.position[0] = nx;
                self.position[1] += self.speed;
                if self.position[1] > best {
                    self.speed = 0.0;
                    self.position[1] = best;
                }
            }
        }
    }
}
