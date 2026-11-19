//! Deterministic SA1 dry-land Sonic movement, with an explicitly upright
//! collision adapter. The arithmetic/update order comes from the SA1 paths,
//! not the SA2 boost/top-speed defaults. Loops, water, rolling/spindash,
//! interactables and damage are not implemented by this module.
use crate::{binary, Error, Result};

pub const ONE: i32 = 256;
pub const GROUND_ACCELERATION: i32 = 8;
pub const GROUND_DECELERATION: i32 = 96;
pub const GROUND_FRICTION: i32 = 8;
pub const GROUND_TOP_SPEED: i32 = 0x480;
pub const AIR_ACCELERATION: i32 = 16;
pub const GRAVITY: i32 = 42;
pub const JUMP_IMPULSE: i32 = 0x4e0;
pub const RELEASE_JUMP_SPEED: i32 = 0x300;
pub const STANDING_HALF_HEIGHT: i32 = 14;
pub const SPIN_HALF_HEIGHT: i32 = 9;
pub const HALF_WIDTH: i32 = 6;
pub const DEFAULT_LAYER: usize = 1;
pub const SPAWN_TABLE_OFFSET: usize = 0x4a6dec;
pub const SPAWN_TABLE_SHA256: &str =
    "2319766677333c64570fe5478adbe0118e7fd2bc078ebb4532f2462c011d81fb";
pub const SINE_TABLE_OFFSET: usize = 0x9b3f0;
pub const SINE_TABLE_SHA256: &str =
    "78caf4fc1cb643a1f24d8e9d9efb1822cda68335adc1799d517582939d9c5c99";
pub const STATE_ANIMATION_OFFSET: usize = 0x4a6f0c;
pub const STATE_ANIMATION_SHA256: &str =
    "985682f047869070d741d18c73eed96b65830e4d341336b37eca0dc9e1818cdf";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundContact {
    /// First occupied pixel at the floor boundary, in world pixels.
    pub surface_y: i32,
    /// Native 256-units-per-turn angle; zero is a horizontal floor.
    pub angle: u8,
}

/// The terrain adapter is bounded, fallible, and read-only. It is intentionally
/// distinct from the retail four-direction sensor/loop collision routines.
pub trait SurfaceQuery {
    fn bounds(&self) -> (i32, i32);
    fn ground_below(&self, x: i32, y: i32, max_distance: i32) -> Result<Option<GroundContact>>;
    fn solid_at(&self, x: i32, y: i32) -> Result<bool>;
}

impl SurfaceQuery for crate::stage::Stage<'_> {
    fn bounds(&self) -> (i32, i32) {
        (self.width_px() as i32, self.height_px() as i32)
    }

    fn ground_below(&self, x: i32, y: i32, max_distance: i32) -> Result<Option<GroundContact>> {
        Ok(
            crate::stage::Stage::ground_below(self, x, y, max_distance, DEFAULT_LAYER)?.map(
                |contact| GroundContact {
                    surface_y: contact.surface_y,
                    angle: contact.angle,
                },
            ),
        )
    }

    fn solid_at(&self, x: i32, y: i32) -> Result<bool> {
        crate::stage::Stage::solid_at(self, x, y, DEFAULT_LAYER)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SineTable {
    samples: [i16; 256],
}

impl SineTable {
    /// Generic arithmetic fixture/input. This does not label a caller's table
    /// native; `from_asoe` requires the actual fingerprinted ROM table.
    pub fn from_samples(samples: [i16; 256]) -> Result<Self> {
        if samples.iter().any(|&v| !(-256..=256).contains(&v))
            || samples[0] != 0
            || samples[64] != 256
            || samples[128] != 0
            || samples[192] != -256
        {
            return Err(Error("invalid Q8 sine table".into()));
        }
        Ok(Self { samples })
    }

    pub fn from_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let raw = binary::range(rom, SINE_TABLE_OFFSET, 0xa00)?;
        if binary::sha256(raw) != SINE_TABLE_SHA256 {
            return Err(Error("ASOE trigonometry table mismatch".into()));
        }
        let mut samples = [0; 256];
        for (index, out) in samples.iter_mut().enumerate() {
            let raw_value = binary::u16_at(raw, index * 8)? as i16;
            *out = raw_value >> 6;
        }
        Self::from_samples(samples)
    }

    pub fn sin(&self, angle: u8) -> i32 {
        i32::from(self.samples[usize::from(angle)])
    }

    pub fn cos(&self, angle: u8) -> i32 {
        self.sin(angle.wrapping_add(64))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationChoice {
    pub animation: u16,
    pub variant: u16,
    /// Native sprite playback speed uses Q4.4, with 16 = one frame/tick.
    pub speed_q4_4: u8,
    pub flip_x: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameOutcome {
    pub jumped: bool,
    pub landed: bool,
    pub left_ground: bool,
    pub blocked_x: bool,
    pub blocked_y: bool,
    /// Runtime handles this explicitly; the core never fabricates a respawn.
    pub fell_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// Signed Q24.8, world Y increases downwards. These are the native anchor,
    /// not the top-left corner of a sprite frame or a feet coordinate.
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub ground_speed: i32,
    pub angle: u8,
    pub grounded: bool,
    pub facing_left: bool,
    pub half_height: i32,
    pub frame: u64,
    previous_jump: bool,
    jumping: bool,
    braking: bool,
    trig: SineTable,
}

impl Player {
    /// Generic deterministic state for synthetic stages. Only the ASOE
    /// constructor below assigns a verified retail spawn and table provenance.
    pub fn at_pixels(x: i32, y: i32, trig: SineTable) -> Result<Self> {
        let x = x
            .checked_mul(ONE)
            .ok_or_else(|| Error("spawn X overflow".into()))?;
        let y = y
            .checked_mul(ONE)
            .ok_or_else(|| Error("spawn Y overflow".into()))?;
        Ok(Self {
            x,
            y,
            vx: 0,
            vy: 0,
            ground_speed: 0,
            angle: 0,
            grounded: false,
            facing_left: false,
            half_height: STANDING_HALF_HEIGHT,
            frame: 0,
            previous_jump: false,
            jumping: false,
            braking: false,
            trig,
        })
    }

    pub fn sonic_first_stage(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        for (at, length, expected) in [
            (SPAWN_TABLE_OFFSET, 72, SPAWN_TABLE_SHA256),
            (STATE_ANIMATION_OFFSET, 376, STATE_ANIMATION_SHA256),
        ] {
            if binary::sha256(binary::range(rom, at, length)?) != expected {
                return Err(Error("ASOE player data mismatch".into()));
            }
        }
        let x = binary::u16_at(rom, SPAWN_TABLE_OFFSET)? as i16;
        let y = binary::u16_at(rom, SPAWN_TABLE_OFFSET + 2)? as i16;
        Self::at_pixels(i32::from(x), i32::from(y), SineTable::from_asoe(rom)?)
    }

    pub fn animation(&self) -> AnimationChoice {
        let speed = self.ground_speed.unsigned_abs();
        let (animation, speed_q4_4) = if !self.grounded && self.jumping {
            (8, 16)
        } else if !self.grounded {
            // Initial/falling animation selection is a presentation adaptation;
            // the movement states for springs/hurt/scripted falls are absent.
            (0, 16)
        } else if self.braking {
            (if speed < 384 { 7 } else { 9 }, 16)
        } else if self.ground_speed == 0 {
            (0, 16)
        } else if speed >= GROUND_TOP_SPEED as u32 {
            (5, 16)
        } else {
            (4, (speed >> 4).clamp(8, 16) as u8)
        };
        AnimationChoice {
            animation,
            variant: 0,
            speed_q4_4,
            flip_x: self.facing_left,
        }
    }

    /// Exactly one 60 Hz input frame. A failing terrain query or invalid state
    /// leaves the previous state intact. There is no caller-supplied dt/gain.
    pub fn step(&mut self, input: Input, world: &impl SurfaceQuery) -> Result<FrameOutcome> {
        let (width, height) = world.bounds();
        if !(32..=1_000_000).contains(&width)
            || !(32..=1_000_000).contains(&height)
            || ![self.vx, self.vy, self.ground_speed]
                .iter()
                .all(|v| (-32767..=32767).contains(v))
            || ![STANDING_HALF_HEIGHT, SPIN_HALF_HEIGHT].contains(&self.half_height)
            || !(0..=width * ONE).contains(&self.x)
            || !(-128 * ONE..=(height + 128) * ONE).contains(&self.y)
        {
            return Err(Error(
                "player state or world exceeds supported bounds".into(),
            ));
        }
        let mut next = self.clone();
        let mut outcome = FrameOutcome::default();
        let pressed_jump = input.jump && !self.previous_jump;
        next.previous_jump = input.jump;
        next.frame = next
            .frame
            .checked_add(1)
            .ok_or_else(|| Error("frame counter overflow".into()))?;

        if next.grounded {
            if !upright(next.angle) {
                return Err(Error(
                    "non-upright player contact requires native loop sensors".into(),
                ));
            }
            if pressed_jump && next.head_clear(world, 4)? {
                let normal = next.angle.wrapping_sub(64);
                next.vx += qmul(JUMP_IMPULSE, next.trig.cos(normal));
                next.vy += qmul(JUMP_IMPULSE, next.trig.sin(normal));
                // Native shrink keeps the same foot contact (+5px on floors).
                next.y += (STANDING_HALF_HEIGHT - SPIN_HALF_HEIGHT) * ONE;
                next.half_height = SPIN_HALF_HEIGHT;
                next.grounded = false;
                next.jumping = true;
                next.braking = false;
                outcome.jumped = true;
                // The SA1 grounded jump branch does not integrate this frame.
            } else {
                if next.ground_speed != 0 {
                    next.ground_speed += (next.trig.sin(next.angle) * 3) >> 5;
                }
                next.braking = false;
                match (input.left, input.right) {
                    (true, false) => next.accelerate_ground(-1),
                    (false, true) => next.accelerate_ground(1),
                    (false, false) => {
                        next.ground_speed = toward_zero(next.ground_speed, GROUND_FRICTION);
                    }
                    (true, true) => {} // Both directions suppress friction in SA1.
                }
                next.vx = qmul(next.ground_speed, next.trig.cos(next.angle));
                next.vy = qmul(next.ground_speed, next.trig.sin(next.angle));
                next.move_upright(world, true, &mut outcome)?;
            }
        } else {
            if next.jumping && !input.jump && next.vy < -RELEASE_JUMP_SPEED {
                next.vy = -RELEASE_JUMP_SPEED;
            }
            next.vx = air_horizontal(next.vx, next.vy, input)?;
            if input.left {
                next.facing_left = true;
            } else if input.right {
                next.facing_left = false;
            }
            next.move_upright(world, false, &mut outcome)?;
            // Native gravity is applied after integrating the old vertical speed.
            if !next.grounded {
                next.vy = next
                    .vy
                    .checked_add(GRAVITY)
                    .ok_or_else(|| Error("fall speed overflow".into()))?;
                let angle = i32::from(next.angle as i8);
                next.angle = toward_zero(angle, 2) as u8;
            }
        }
        outcome.fell_out = next.y >= (height + 64) * ONE;
        if ![next.vx, next.vy, next.ground_speed]
            .iter()
            .all(|v| (-32767..=32767).contains(v))
        {
            return Err(Error("native velocity range exceeded".into()));
        }
        *self = next;
        Ok(outcome)
    }

    fn accelerate_ground(&mut self, direction: i32) {
        let aligned = self.ground_speed * direction;
        if aligned >= 0 {
            self.facing_left = direction < 0;
            self.ground_speed =
                directional_acceleration(self.ground_speed, direction, GROUND_ACCELERATION);
        } else {
            self.ground_speed += direction * GROUND_DECELERATION;
            if self.ground_speed * direction > 0 {
                self.ground_speed = direction * GROUND_DECELERATION;
            }
            self.braking = self.ground_speed * direction <= -3 * ONE;
        }
    }

    fn head_clear(&self, world: &impl SurfaceQuery, distance: i32) -> Result<bool> {
        let x = self.x >> 8;
        let top = (self.y >> 8) - self.half_height;
        for d in 1..=distance {
            for dx in [-HALF_WIDTH, HALF_WIDTH] {
                if world.solid_at(x + dx, top - d)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn move_upright(
        &mut self,
        world: &impl SurfaceQuery,
        was_grounded: bool,
        out: &mut FrameOutcome,
    ) -> Result<()> {
        let (width, _) = world.bounds();
        let old_x = self.x;
        let old_y = self.y;
        let target_x = self
            .x
            .checked_add(self.vx)
            .ok_or_else(|| Error("player X overflow".into()))?;
        // Bounded pixel sweep is an upright adapter, not retail directional
        // sensors. It prevents crossing a thin wall at high speed.
        let direction = self.vx.signum();
        let pixels = ((target_x >> 8) - (old_x >> 8)).unsigned_abs();
        if pixels > 128 {
            return Err(Error("horizontal sweep budget exceeded".into()));
        }
        for d in 1..=pixels as i32 {
            let x = (old_x >> 8) + direction * d;
            let edge = x + direction * HALF_WIDTH;
            let center_y = self.y >> 8;
            let mut blocked = edge < 0 || edge >= width;
            for dy in [-(self.half_height - 4), 0, self.half_height - 4] {
                blocked |= world.solid_at(edge, center_y + dy)?;
            }
            if blocked {
                self.x = ((old_x >> 8) + direction * (d - 1)) * ONE;
                self.vx = 0;
                self.ground_speed = 0;
                if was_grounded {
                    self.vy = 0;
                }
                out.blocked_x = true;
                break;
            }
        }
        if !out.blocked_x {
            self.x = target_x;
        }
        let target_y = self
            .y
            .checked_add(self.vy)
            .ok_or_else(|| Error("player Y overflow".into()))?;
        if self.vy < 0 && !was_grounded {
            let pixels = ((target_y >> 8) - (old_y >> 8)).unsigned_abs();
            if pixels > 128 {
                return Err(Error("vertical sweep budget exceeded".into()));
            }
            for d in 1..=pixels as i32 {
                let top = (old_y >> 8) - self.half_height - d;
                let x = self.x >> 8;
                if world.solid_at(x - HALF_WIDTH, top)? || world.solid_at(x + HALF_WIDTH, top)? {
                    self.y = ((old_y >> 8) - d + 1) * ONE;
                    self.vy = 0;
                    out.blocked_y = true;
                    break;
                }
            }
        }
        if !out.blocked_y {
            self.y = target_y;
        }
        if was_grounded || self.vy >= 0 {
            let feet_before = (old_y >> 8) + self.half_height;
            let feet_after = (self.y >> 8) + self.half_height;
            let scan_start = if was_grounded {
                feet_after - 8
            } else {
                feet_before - 1
            };
            let scan_length = if was_grounded {
                24
            } else {
                (feet_after - scan_start + 1).clamp(1, 130)
            };
            let x = self.x >> 8;
            let mut best = None;
            for dx in [-HALF_WIDTH, HALF_WIDTH] {
                if let Some(contact) = world.ground_below(x + dx, scan_start, scan_length)? {
                    if !(scan_start - scan_length..=scan_start + scan_length)
                        .contains(&contact.surface_y)
                    {
                        return Err(Error("terrain contact outside probe budget".into()));
                    }
                    if !upright(contact.angle) {
                        continue;
                    }
                    if (was_grounded || contact.surface_y <= feet_after)
                        && best.is_none_or(|old: GroundContact| contact.surface_y < old.surface_y)
                    {
                        best = Some(contact);
                    }
                }
            }
            if let Some(contact) = best {
                let was_airborne = !self.grounded;
                self.angle = contact.angle;
                if was_airborne {
                    // Retail projects landing speed by angle bands. This slice
                    // only accepts upright slopes; preserve horizontal speed.
                    self.ground_speed = self.vx;
                    self.half_height = STANDING_HALF_HEIGHT;
                    self.jumping = false;
                    out.landed = true;
                    self.vy = 0;
                }
                self.y = (contact.surface_y - self.half_height) * ONE;
                self.grounded = true;
            } else {
                self.grounded = false;
                out.left_ground = was_grounded;
            }
        }
        Ok(())
    }
}

fn upright(angle: u8) -> bool {
    angle.wrapping_add(32) & 0xc0 == 0
}
fn qmul(a: i32, b: i32) -> i32 {
    (a * b) >> 8
}
fn toward_zero(value: i32, amount: i32) -> i32 {
    if value > 0 {
        (value - amount).max(0)
    } else {
        (value + amount).min(0)
    }
}

fn directional_acceleration(speed: i32, direction: i32, acceleration: i32) -> i32 {
    let attempted = speed + direction * acceleration;
    if attempted * direction > GROUND_TOP_SPEED {
        if speed * direction < GROUND_TOP_SPEED {
            direction * GROUND_TOP_SPEED
        } else {
            speed
        }
    } else {
        attempted
    }
}

/// The SA1 source's unsigned condition is preserved exactly: rising dry-air
/// velocities in the ordinary jump range receive signed arithmetic >>5 drag.
pub fn air_horizontal(speed: i32, vertical_speed: i32, input: Input) -> Result<i32> {
    if !(-32767..=32767).contains(&speed) || !(-32767..=32767).contains(&vertical_speed) {
        return Err(Error("air velocity outside native range".into()));
    }
    let direction = if input.left {
        -1
    } else if input.right {
        1
    } else {
        0
    };
    let mut speed = if direction == 0 {
        speed
    } else {
        directional_acceleration(speed, direction, AIR_ACCELERATION)
    };
    if (vertical_speed as i16 as u16) > ((-67 * ONE - 1) as i16 as u16) {
        speed -= speed >> 5;
    }
    Ok(speed)
}
