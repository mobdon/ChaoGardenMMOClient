//! First Sonic locomotion slice, derived from the pinned SA1 reference rules.
//!
//! This is a flat-ground and free-air kernel. Terrain settlement belongs to the
//! stage reader. Slope projection, water, powerups, scripted
//! motion, attacks, camera constraints and other characters remain unsupported.
//! Source-rule conformance does not establish ASOE execution/replay parity.

use crate::{binary, Error, Result};

pub const ONE: i32 = 256;
pub const GROUND_ACCELERATION: i32 = 8;
pub const GROUND_DECELERATION: i32 = 96;
pub const GROUND_FRICTION: i32 = 8;
pub const MAX_ACCELERATED_SPEED: i32 = 1152;
pub const AIR_ACCELERATION: i32 = 16;
pub const GRAVITY: i32 = 42;
pub const JUMP_IMPULSE: i32 = 1248;
pub const RELEASED_JUMP_LIMIT: i32 = -768;

pub const BUTTON_JUMP: u16 = 0x0001;
pub const BUTTON_RIGHT: u16 = 0x0010;
pub const BUTTON_LEFT: u16 = 0x0020;
pub const BUTTON_DOWN: u16 = 0x0080;
pub const SPINDASH_SPEEDS_OFFSET: usize = 0x4a7084;
pub const SPINDASH_SPEEDS_LEN: usize = 66;
pub const SPINDASH_SPEEDS_SHA256: &str =
    "5297e53ffab0d05d677f379f2f0d7a09b3886c7c0e5c7f931bf1685a7631a1d0";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum GroundAction {
    #[default]
    Standing,
    Crouched,
    Charging,
    Rolling,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

impl Input {
    pub fn bits(self) -> u16 {
        (u16::from(self.jump) * BUTTON_JUMP)
            | (u16::from(self.right) * BUTTON_RIGHT)
            | (u16::from(self.left) * BUTTON_LEFT)
    }
}

/// Positions and speeds use eight fractional bits. Native position words are
/// signed 32-bit; native velocity words are signed 16-bit, widened in this API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Player {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub ground_speed: i32,
    pub grounded: bool,
    pub facing_left: bool,
    pub frame: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movement {
    pub player: Player,
    held_input: u16,
    frame_input: u16,
    jump_active: bool,
    action: GroundAction,
    body_height: i32,
    air_control_locked: bool,
    charge: i32,
    rev_animation: bool,
    release_speeds: Option<[i16; 33]>,
    ground_angle: u8,
    sine: Option<Box<[i16; 1280]>>,
}

fn velocity(value: i32) -> i32 {
    i32::from(value as i16)
}

fn accelerate(speed: i32, direction: i32, acceleration: i32) -> i32 {
    let proposed = speed + direction * acceleration;
    if direction * proposed > MAX_ACCELERATED_SPEED {
        // Native input acceleration preserves speed gained from other causes.
        if direction * speed < MAX_ACCELERATED_SPEED {
            direction * MAX_ACCELERATED_SPEED
        } else {
            speed
        }
    } else {
        proposed
    }
}

impl Movement {
    /// Start at pixel center coordinates, airborne until the stage settles it.
    pub fn new(spawn_x: i32, spawn_y: i32) -> Self {
        Self {
            player: Player {
                x: spawn_x.wrapping_mul(ONE),
                y: spawn_y.wrapping_mul(ONE),
                vx: 0,
                vy: 0,
                ground_speed: 0,
                grounded: false,
                facing_left: false,
                frame: 0,
            },
            held_input: 0,
            frame_input: 0,
            jump_active: false,
            action: GroundAction::Standing,
            body_height: 14,
            air_control_locked: false,
            charge: 0,
            rev_animation: false,
            release_speeds: None,
            ground_angle: 0,
            sine: None,
        }
    }

    pub fn held_input(&self) -> u16 {
        self.held_input
    }

    pub fn frame_input(&self) -> u16 {
        self.frame_input
    }

    /// Source-backed release lookup, including adjacent native words reached
    /// by the original MAX-versus-MIN overcharge behavior. No ROM bytes are
    /// embedded in the executable. The first nine words are the normal table.
    pub fn configure_spindash(&mut self, rom: &[u8]) -> Result<()> {
        binary::verify_asoe(rom)?;
        let raw = binary::range(rom, SPINDASH_SPEEDS_OFFSET, SPINDASH_SPEEDS_LEN)?;
        if binary::sha256(raw) != SPINDASH_SPEEDS_SHA256 {
            return Err(Error("ASOE spindash release lookup mismatch".into()));
        }
        let mut speeds = [0; 33];
        for (speed, pair) in speeds.iter_mut().zip(raw.as_chunks::<2>().0) {
            *speed = i16::from_le_bytes([pair[0], pair[1]]);
        }
        let trig = binary::range(rom, 0x9b3f0, 2560)?;
        if binary::sha256(trig)
            != "78caf4fc1cb643a1f24d8e9d9efb1822cda68335adc1799d517582939d9c5c99"
        {
            return Err(Error("ASOE sine table mismatch".into()));
        }
        let mut sine = Box::new([0; 1280]);
        for (value, pair) in sine.iter_mut().zip(trig.as_chunks::<2>().0) {
            *value = i16::from_le_bytes(*pair);
        }
        self.sine = Some(sine);
        self.release_speeds = Some(speeds);
        Ok(())
    }

    pub fn is_rolling(&self) -> bool {
        self.action == GroundAction::Rolling
    }

    pub fn is_spindashing(&self) -> bool {
        self.player.grounded && self.action == GroundAction::Charging
    }

    pub fn is_crouched(&self) -> bool {
        self.player.grounded && self.action == GroundAction::Crouched
    }

    pub fn spindash_charge(&self) -> i32 {
        self.charge
    }

    pub fn body_half_height(&self) -> i32 {
        self.body_height
    }

    /// Actual ASOE sCharStateAnimInfo rows: crouch 2, roll 5, charge 6, rev 7.
    /// Other locomotion animations remain the runtime's responsibility.
    pub fn native_animation(&self) -> Option<(u16, u8)> {
        match self.action {
            GroundAction::Standing => None,
            GroundAction::Crouched if self.player.grounded => Some((3, 0)),
            GroundAction::Crouched => None,
            GroundAction::Rolling => Some((8, 0)),
            GroundAction::Charging => Some((18, u8::from(self.rev_animation))),
        }
    }

    /// Native sprite completion changes charState 7 back to charState 6.
    /// Call only when the actual rev clip reports completion, not on a timer
    /// chosen by this movement kernel or by the browser adapter.
    pub fn notify_animation_finished(&mut self) {
        if self.action == GroundAction::Charging {
            self.rev_animation = false;
        }
    }

    fn set_body_height(&mut self, height: i32) {
        let change = (self.body_height - height) * ONE;
        let quadrant = if self.ground_angle == 0 {
            32
        } else {
            self.ground_angle.wrapping_add(31)
        } >> 6;
        match quadrant {
            0 => self.player.y = self.player.y.wrapping_add(change),
            1 => self.player.x = self.player.x.wrapping_sub(change),
            2 => self.player.y = self.player.y.wrapping_sub(change),
            _ => self.player.x = self.player.x.wrapping_add(change),
        }
        self.body_height = height;
    }

    /// One logical game frame; wall-clock accumulation belongs to the adapter.
    /// A successful jump uses this entire tick for the native state transition:
    /// position integration and gravity begin on the following airborne tick.
    pub fn advance(&mut self, input: Input) {
        self.advance_with_jump_clearance(input, 4);
    }

    /// Stage-aware variant matching the native four-pixel jump clearance gate.
    /// The input edge is consumed even when blocked; holding jump under a low
    /// ceiling must not create a fresh press when clearance later increases.
    pub fn advance_with_jump_clearance(&mut self, input: Input, clearance_pixels: i32) {
        // This legacy walk/jump entry point keeps its existing standing-height
        // jump contract. New native action callers use the fallible method.
        let checkpoint = self.clone();
        if self
            .advance_inner(input, false, clearance_pixels, false)
            .is_err()
        {
            *self = checkpoint;
        }
    }

    /// Native DOWN/crouch/roll/spindash path without changing Input literals.
    /// Configure the exact source first to enable charging. A rejected action
    /// leaves input edges, position, charge and the frame counter unchanged.
    pub fn advance_with_actions(
        &mut self,
        input: Input,
        down: bool,
        clearance_pixels: i32,
    ) -> Result<()> {
        let checkpoint = self.clone();
        match self.advance_inner(input, down, clearance_pixels, true) {
            Ok(()) => Ok(()),
            Err(error) => {
                *self = checkpoint;
                Err(error)
            }
        }
    }

    fn advance_inner(
        &mut self,
        input: Input,
        down: bool,
        clearance_pixels: i32,
        native_actions: bool,
    ) -> Result<()> {
        // Native SA1 input handling suppresses vertical keys whenever either
        // horizontal key is held. Spindash release uses that filtered DOWN.
        let down = down && !input.left && !input.right;
        let held = input.bits() | if down { BUTTON_DOWN } else { 0 };
        self.frame_input = held & !self.held_input;
        self.held_input = held;
        self.player.frame = self.player.frame.wrapping_add(1);
        self.player.vx = velocity(self.player.vx);
        self.player.vy = velocity(self.player.vy);
        self.player.ground_speed = velocity(self.player.ground_speed);

        if self.player.grounded {
            if self.action == GroundAction::Charging {
                self.advance_charge(down)?;
                return Ok(());
            }
            if native_actions
                && self.action == GroundAction::Crouched
                && self.frame_input & BUTTON_JUMP != 0
            {
                if self.release_speeds.is_none() {
                    return Err(Error("configure the ASOE spindash source first".into()));
                }
                self.action = GroundAction::Charging;
                self.charge = 0;
                self.rev_animation = false;
                self.set_body_height(9);
                return Ok(());
            }
            if self.frame_input & BUTTON_JUMP != 0 && clearance_pixels >= 4 {
                let was_rolling = self.action == GroundAction::Rolling;
                self.player.grounded = false;
                let angle = self.ground_angle.wrapping_sub(64);
                let (sin, cos) = self.sin_cos(angle);
                self.player.vx = velocity(self.player.vx + ((JUMP_IMPULSE * cos) >> 8));
                self.player.vy = velocity(self.player.vy + ((JUMP_IMPULSE * sin) >> 8));
                self.jump_active = true;
                self.air_control_locked = was_rolling;
                if native_actions || was_rolling {
                    self.action = GroundAction::Rolling;
                    self.set_body_height(9);
                }
                return Ok(());
            }
            if self.action == GroundAction::Rolling {
                self.advance_roll(input, down);
            } else {
                self.advance_ground(input, down);
            }
        } else {
            self.advance_air(input);
        }
        Ok(())
    }

    fn advance_charge(&mut self, down: bool) -> Result<()> {
        if !down {
            let index = usize::try_from(self.charge >> 8)
                .ok()
                .filter(|index| *index < 33)
                .ok_or_else(|| Error("spindash charge exceeds bounded native lookup".into()))?;
            let source = self
                .release_speeds
                .as_ref()
                .ok_or_else(|| Error("ASOE spindash release lookup missing".into()))?;
            let speed = i32::from(source[index]);
            self.player.ground_speed = velocity(if self.player.facing_left {
                -speed
            } else {
                speed
            });
            self.action = GroundAction::Rolling;
            // Native release changes ground speed but does not project it to
            // air X or integrate position until the next rolling frame.
            return Ok(());
        }
        if self.charge != 0 {
            self.charge = (self.charge - (self.charge >> 5)).max(0);
        }
        if self.frame_input & BUTTON_JUMP != 0 {
            self.rev_animation = true;
            // This is deliberately MAX, confirmed in actual ASOE Thumb at
            // 0x441da..0x441e0. Do not silently repair the retail overcharge.
            self.charge = (self.charge + 512).max(2048);
        }
        if !(0..33 * ONE).contains(&self.charge) {
            return Err(Error("spindash charge exceeds physical edge bound".into()));
        }
        Ok(())
    }

    fn advance_ground(&mut self, input: Input, down: bool) {
        if self.ground_angle.wrapping_add(96) < 192 && self.player.ground_speed != 0 {
            self.player.ground_speed =
                velocity(self.player.ground_speed + ((self.sin_cos(self.ground_angle).0 * 3) >> 5));
        }
        let speed = self.player.ground_speed;
        let direction = match (input.left, input.right) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        self.player.ground_speed = if direction != 0 {
            if direction * speed >= 0 {
                self.player.facing_left = direction < 0;
                accelerate(speed, direction, GROUND_ACCELERATION)
            } else {
                let proposed = speed + direction * GROUND_DECELERATION;
                if direction * proposed > 0 {
                    direction * GROUND_DECELERATION
                } else {
                    proposed
                }
            }
        } else {
            speed
        };
        // DOWN is handled before this frame's ordinary ground friction. Even
        // a tiny nonzero speed enters roll, then stops on the next roll tick.
        self.action = if down {
            if self.player.ground_speed == 0 {
                GroundAction::Crouched
            } else {
                self.set_body_height(9);
                GroundAction::Rolling
            }
        } else {
            GroundAction::Standing
        };
        if !input.left && !input.right {
            let speed = self.player.ground_speed;
            self.player.ground_speed = if speed > 0 {
                (speed - GROUND_FRICTION).max(0)
            } else {
                (speed + GROUND_FRICTION).min(0)
            };
        }
        self.integrate_ground();
    }

    fn advance_roll(&mut self, input: Input, down: bool) {
        let mut speed = self.player.ground_speed;
        if self.ground_angle.wrapping_add(96) < 192 && speed != 0 {
            let mut acceleration = (self.sin_cos(self.ground_angle).0 * 60) >> 8;
            if (speed > 0 && acceleration <= 0) || (speed < 0 && acceleration >= 0) {
                acceleration >>= 2;
            }
            speed = velocity(speed + acceleration);
        }
        if input.left {
            if speed <= 0 {
                self.player.facing_left = true;
            } else {
                speed -= 24;
                if speed < 0 {
                    speed = -96;
                }
            }
        } else if input.right {
            if speed >= 0 {
                self.player.facing_left = false;
            } else {
                speed += 24;
                if speed > 0 {
                    speed = 96;
                }
            }
        }
        self.player.ground_speed = if speed > 0 {
            (speed - 4).max(0)
        } else {
            (speed + 4).min(0)
        };
        if self.player.ground_speed == 0 {
            self.action = if down {
                GroundAction::Crouched
            } else {
                GroundAction::Standing
            };
            self.set_body_height(14);
        }
        self.integrate_ground();
    }

    fn integrate_ground(&mut self) {
        self.player.ground_speed = velocity(self.player.ground_speed);
        let (sin, cos) = self.sin_cos(self.ground_angle);
        self.player.vx = velocity((self.player.ground_speed * cos) >> 8);
        self.player.vy = velocity((self.player.ground_speed * sin) >> 8);
        self.player.x = self.player.x.wrapping_add(self.player.vx);
        self.player.y = self.player.y.wrapping_add(self.player.vy);
    }

    fn advance_air(&mut self, input: Input) {
        if self.jump_active && !input.jump && self.player.vy < RELEASED_JUMP_LIMIT {
            self.player.vy = RELEASED_JUMP_LIMIT;
        }
        // Native airborne input uses if-left / else-if-right, unlike ground.
        let direction = if input.left {
            -1
        } else if input.right {
            1
        } else {
            0
        };
        if direction != 0 && !self.air_control_locked {
            self.player.facing_left = direction < 0;
            self.player.vx = velocity(accelerate(self.player.vx, direction, AIR_ACCELERATION));
        }
        // Preserve the reference's unsigned comparison and signed arithmetic
        // shift literally; changing it to a familiar apex threshold would
        // introduce an unverified rule. ASOE execution parity remains open.
        if (self.player.vy as u16) > ((-67 * ONE - 1) as u16) {
            self.player.vx -= self.player.vx >> 5;
        }
        self.player.x = self.player.x.wrapping_add(self.player.vx);
        self.player.y = self.player.y.wrapping_add(self.player.vy);
        // SA1 has no SA2-style 15 px/frame fall-speed cap in this path.
        self.player.vy = velocity(self.player.vy + GRAVITY);
    }

    fn sin_cos(&self, angle: u8) -> (i32, i32) {
        if let Some(sine) = &self.sine {
            let index = usize::from(angle) * 4;
            (
                i32::from(sine[index]) >> 6,
                i32::from(sine[index + 256]) >> 6,
            )
        } else {
            // Unconfigured legacy movement has only the original flat plane.
            match angle {
                64 => (256, 0),
                128 => (0, -256),
                192 => (-256, 0),
                _ => (0, 256),
            }
        }
    }

    /// Native upright floor follow: preserve subpixels, use the minimum sensor
    /// distance, and retain rotation when the boundary marks it as odd.
    pub(crate) fn follow_floor(&mut self, distance: i32, angle: u8) {
        if distance < -11 {
            return;
        }
        if distance > (self.player.vx.abs().div_euclid(ONE) + 3).min(11) {
            self.player.grounded = false;
            return;
        }
        self.player.y = self.player.y.wrapping_add(distance * ONE);
        if angle & 1 == 0 {
            self.ground_angle = angle;
        }
    }

    /// Downward native landing, after both sensors and penetration limits have
    /// been checked by the terrain adapter. Landing uncurls before projection.
    pub(crate) fn land_from_floor(&mut self, distance: i32, angle: u8) {
        let angle = if angle & 1 != 0 { 0 } else { angle };
        self.ground_angle = angle;
        self.player.y = self.player.y.wrapping_add(distance * ONE);
        self.player.grounded = true;
        self.jump_active = false;
        self.air_control_locked = false;
        self.action = GroundAction::Standing;
        self.set_body_height(14);
        if angle.wrapping_add(32) & 64 != 0 {
            self.player.vx = 0;
            self.player.vy = self.player.vy.min(3024);
        } else if angle.wrapping_add(16) & 32 == 0 {
            self.player.vy = 0;
            self.player.ground_speed = self.player.vx;
            return;
        } else {
            self.player.vy >>= 1;
        }
        let speed = self.player.vy.abs();
        self.player.ground_speed = if (angle as i8) < 0 { -speed } else { speed };
    }

    /// Settle at a pixel CENTER Y supplied by stage collision. The stage owns
    /// sensor/body offsets; this helper never subtracts a sprite height.
    pub fn land_at(&mut self, y_pixels: i32) {
        let was_grounded = self.player.grounded;
        self.ground_angle = 0;
        self.player.y = y_pixels.wrapping_mul(ONE);
        self.player.vy = 0;
        self.player.grounded = true;
        self.jump_active = false;
        self.air_control_locked = false;
        if !was_grounded {
            self.player.ground_speed = velocity(self.player.vx);
            self.action = GroundAction::Standing;
            self.set_body_height(14);
        }
    }
}
