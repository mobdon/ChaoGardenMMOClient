//! First Sonic locomotion slice, derived from the pinned SA1 reference rules.
//!
//! This is a flat-ground and free-air kernel. Terrain settlement belongs to the
//! stage reader. Slope projection, rolling/spindash, water, powerups, scripted
//! motion, attacks, camera constraints and other characters remain unsupported.
//! Source-rule conformance does not establish ASOE execution/replay parity.

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
        }
    }

    pub fn held_input(&self) -> u16 {
        self.held_input
    }

    pub fn frame_input(&self) -> u16 {
        self.frame_input
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
        let held = input.bits();
        self.frame_input = held & !self.held_input;
        self.held_input = held;
        self.player.frame = self.player.frame.wrapping_add(1);
        self.player.vx = velocity(self.player.vx);
        self.player.vy = velocity(self.player.vy);
        self.player.ground_speed = velocity(self.player.ground_speed);

        if self.player.grounded {
            if self.frame_input & BUTTON_JUMP != 0 && clearance_pixels >= 4 {
                self.player.grounded = false;
                self.player.vy = velocity(self.player.vy - JUMP_IMPULSE);
                self.jump_active = true;
                return;
            }
            self.advance_ground(input);
        } else {
            self.advance_air(input);
        }
    }

    fn advance_ground(&mut self, input: Input) {
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
        } else if !input.left && !input.right {
            if speed > 0 {
                (speed - GROUND_FRICTION).max(0)
            } else {
                (speed + GROUND_FRICTION).min(0)
            }
        } else {
            // Both horizontal keys are held: native ground switch has no case.
            speed
        };
        self.player.ground_speed = velocity(self.player.ground_speed);
        self.player.vx = self.player.ground_speed;
        self.player.vy = 0;
        self.player.x = self.player.x.wrapping_add(self.player.vx);
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
        if direction != 0 {
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

    /// Settle at a pixel CENTER Y supplied by stage collision. The stage owns
    /// sensor/body offsets; this helper never subtracts a sprite height.
    pub fn land_at(&mut self, y_pixels: i32) {
        self.player.y = y_pixels.wrapping_mul(ONE);
        self.player.vy = 0;
        self.player.ground_speed = velocity(self.player.vx);
        self.player.grounded = true;
        self.jump_active = false;
    }
}
