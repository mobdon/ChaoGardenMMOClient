//! Native A2NE player arithmetic and callback-state primitives.
//!
//! Behavior evidence: SAT-R/sa2 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
//! `src/game/shared/stage/player.c` and `include/game/shared/parameters/characters.h`.
//! Collision, camera, object callbacks, sound and sprite animation are separate
//! host services. These primitives do not pretend to execute those services.

use serde::{Deserialize, Serialize};

pub const RIGHT: u16 = 0x10;
pub const LEFT: u16 = 0x20;
pub const UP: u16 = 0x40;
pub const DOWN: u16 = 0x80;
pub const FACING_LEFT: u32 = 1;
pub const IN_AIR: u32 = 2;
pub const SPIN_ATTACK: u32 = 4;
pub const STOOD_ON_OBJECT: u32 = 8;
pub const FLIP_WITH_MOVE_DIR: u32 = 0x10;
pub const IN_WATER: u32 = 0x40;
pub const JUMP_HELD_STATE: u32 = 0x100;
pub const IGNORE_INPUT: u32 = 0x200000;
pub const ANIMATION_OVER: u32 = 0x4000;

pub const IDLE: i16 = 0;
pub const WALK: i16 = 9;
pub const JUMP_SLOW: i16 = 10;
pub const JUMP_FAST: i16 = 11;
pub const HIT_AIR: i16 = 20;
pub const BOUNCE: i16 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Character {
    Sonic = 0,
    Cream = 1,
    Tails = 2,
    Knuckles = 3,
    Amy = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Callback {
    Ground,
    InitJump,
    Airborne,
    Rolling,
    Skidding,
}

/// Movement fields used by this native slice. This is not a native save layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub character: Character,
    pub q_world_x: i32,
    pub q_world_y: i32,
    pub q_speed_air_x: i16,
    pub q_speed_air_y: i16,
    pub q_speed_ground: i16,
    pub move_state: u32,
    pub rotation: u8,
    pub held_input: u16,
    pub frame_input: u16,
    pub callback: Callback,
    pub top_speed: i32,
    pub max_speed: i32,
    pub acceleration: i32,
    pub deceleration: i32,
    pub boost_threshold: u8,
    pub boost_speed: i16,
    pub is_boosting: bool,
    pub walk_anim: u8,
    pub control_lock: i16,
    pub char_state: i16,
    pub sprite_offset_x: i8,
    pub sprite_offset_y: i8,
    pub sprite_frame_flags: u32,
    pub layer: u8,
    pub unk28: u8,
    pub unk29: u8,
    pub defeat_score_index: i8,
    pub unk61: i8,
    pub unk62: i8,
    pub unk63: i8,
    pub unk70: bool,
    pub unk71: u8,
}

impl PlayerState {
    /// Native initialization values for the represented movement fields.
    /// Stage start explicitly releases IGNORE_INPUT; no timer is fabricated here.
    pub fn new_at(character: Character, q_x: i32, q_y: i32) -> Self {
        Self {
            character,
            q_world_x: q_x,
            q_world_y: q_y,
            q_speed_air_x: 0,
            q_speed_air_y: 0,
            q_speed_ground: 0,
            move_state: IGNORE_INPUT,
            rotation: 0,
            held_input: 0x103,
            frame_input: 0x103,
            callback: Callback::Ground,
            top_speed: 6 * 256,
            max_speed: 9 * 256,
            acceleration: 8,
            deceleration: 64,
            boost_threshold: 0,
            boost_speed: 0,
            is_boosting: false,
            walk_anim: 0,
            control_lock: 0,
            char_state: IDLE,
            sprite_offset_x: 6,
            sprite_offset_y: 14,
            sprite_frame_flags: 0,
            layer: 1,
            unk28: 0,
            unk29: 0,
            defeat_score_index: 0,
            unk61: 0,
            unk62: 0,
            unk63: 0,
            unk70: false,
            unk71: 0,
        }
    }

    /// Single-player Player_HandleInputs. Multiplayer confusion is separate.
    pub fn handle_input(&mut self, current: u16) {
        let previous = self.held_input;
        if self.move_state & IGNORE_INPUT == 0 {
            self.held_input = current;
        }
        self.frame_input = (previous ^ self.held_input) & self.held_input;
    }

    /// Task_PlayerMain's boost/physics/walk-animation order. Returns the native
    /// boost activation event; the host owns its sound, particles and camera.
    pub fn update_frame_physics(&mut self, rings: u16) -> bool {
        self.boost_threshold = match rings {
            150.. => 4,
            100..=149 => 3,
            50..=99 => 2,
            11..=49 => 1,
            _ => 0,
        };
        let threshold = [2048, 2040, 1680, 1440, 1080][usize::from(self.boost_threshold)];
        let mut activated = false;
        if self.is_boosting {
            if self.move_state & IN_AIR == 0 {
                self.boost_speed = threshold;
                if i32::from(self.q_speed_ground).abs() < 1152 {
                    self.is_boosting = false;
                    self.boost_speed = 0;
                }
            }
        } else if self.move_state & IN_AIR == 0
            && i32::from(self.q_speed_ground).abs() >= self.top_speed
        {
            if self.boost_speed >= threshold {
                self.is_boosting = true;
                activated = true;
            }
        } else {
            self.boost_speed = 0;
        }
        (self.top_speed, self.max_speed) = if self.is_boosting {
            (3072, 3840)
        } else if self.move_state & SPIN_ATTACK != 0 {
            (1536, 3840)
        } else {
            (1536, 2304)
        };
        self.acceleration = [8, 12, 14, 16, 18][usize::from(self.boost_threshold)];
        self.deceleration = 64;
        self.walk_anim = match i32::from(self.q_speed_ground).abs() {
            0..=320 => 0,
            321..=640 => 1,
            641..=1024 => 2,
            1025..=2304 => 3,
            2305..=2560 => 4,
            _ => 5,
        };
        activated
    }

    /// Scalar portion of Player_HandleGroundMovement, before sub_8023128's
    /// required wall probe. Returns whether native braking effects are requested.
    pub fn ground_input(&mut self, inverted: bool) -> bool {
        let mut next_state = -1;
        let mut braking = false;
        let speed = i32::from(self.q_speed_ground);
        if self.control_lock == 0 && self.held_input & (LEFT | RIGHT) != 0 {
            let same_direction = if speed > 0 {
                self.held_input & RIGHT != 0
            } else {
                self.held_input & LEFT != 0
            };
            if speed != 0 && same_direction {
                let direction = speed.signum();
                if speed.abs() < self.top_speed {
                    let next = speed.wrapping_add(direction.wrapping_mul(self.acceleration)) as i16;
                    let next = i32::from(next);
                    self.q_speed_ground = if direction > 0 {
                        next.min(self.top_speed) as i16
                    } else {
                        next.max(-self.top_speed) as i16
                    };
                } else {
                    self.boost_speed =
                        i32::from(self.boost_speed).wrapping_add(self.acceleration) as i16;
                }
                if direction > 0 {
                    self.move_state &= !FACING_LEFT;
                } else {
                    self.move_state |= FACING_LEFT;
                }
            } else if speed != 0 {
                self.q_speed_ground =
                    speed.wrapping_sub(speed.signum().wrapping_mul(self.deceleration)) as i16;
                if speed.abs() >= 512 {
                    next_state = if matches!(self.char_state, 7 | 8) {
                        self.char_state
                    } else if self.walk_anim > 3 {
                        8
                    } else {
                        7
                    };
                    braking = true;
                } else {
                    let facing_left = self.move_state & FACING_LEFT != 0;
                    if (speed > 0 && self.q_speed_ground > 0 && !facing_left)
                        || (speed < 0 && self.q_speed_ground < 0 && facing_left)
                    {
                        if matches!(self.char_state, 7 | 8) {
                            next_state = 7;
                        }
                        self.callback = Callback::Skidding;
                    }
                }
            } else if (self.move_state & FACING_LEFT) != u32::from((self.held_input & RIGHT) >> 4) {
                self.q_speed_ground = if self.move_state & FACING_LEFT != 0 {
                    self.acceleration.wrapping_neg() as i16
                } else {
                    self.acceleration as i16
                };
                next_state = WALK;
                self.change_sprite_offsets(6, 14, inverted);
            } else {
                self.callback = Callback::Skidding;
            }
        } else if speed != 0 {
            let next = speed.wrapping_sub(speed.signum() * 8) as i16;
            self.q_speed_ground = if (speed > 0 && next <= 0) || (speed < 0 && next >= 0) {
                next_state = IDLE;
                0
            } else {
                next_state = WALK;
                next
            };
        } else {
            next_state = IDLE;
        }
        if self.move_state & 0x8000 != 0 {
            if matches!(next_state, 7 | 8) {
                self.char_state = next_state;
            }
        } else if next_state != -1 {
            self.char_state = next_state;
        } else if matches!(self.char_state, 7 | 8) {
            self.char_state = WALK;
        }
        braking
    }

    /// Player_AirInputControls preserves momentum already outside top speed.
    /// It is deliberately not a general clamp; LEFT wins simultaneous inputs.
    pub fn air_input(&mut self) {
        if self.char_state == HIT_AIR || self.move_state & FLIP_WITH_MOVE_DIR != 0 {
            return;
        }
        let delta = self.acceleration.wrapping_mul(2);
        let old = i32::from(self.q_speed_air_x);
        let direction: i32 = if self.held_input & LEFT != 0 {
            -1
        } else if self.held_input & RIGHT != 0 {
            1
        } else {
            0
        };
        if direction == 0 {
            return;
        }
        if self.char_state != BOUNCE && self.move_state & 0x2000 == 0 {
            if direction < 0 {
                self.move_state |= FACING_LEFT;
            } else {
                self.move_state &= !FACING_LEFT;
            }
        }
        let candidate = old.wrapping_add(direction.wrapping_mul(delta)) as i16;
        self.q_speed_air_x = candidate;
        if (direction < 0 && i32::from(candidate) < -self.top_speed)
            || (direction > 0 && i32::from(candidate) > self.top_speed)
        {
            let restored = i32::from(candidate).wrapping_sub(direction.wrapping_mul(delta)) as i16;
            self.q_speed_air_x = restored;
            if (direction < 0 && i32::from(restored) > -self.top_speed)
                || (direction > 0 && i32::from(restored) < self.top_speed)
            {
                self.q_speed_air_x = direction.wrapping_mul(self.top_speed) as i16;
            }
        }
    }

    /// sub_80236C8 and sub_8023708. The unusual unsigned Y condition is kept.
    pub fn decay_air_x(&mut self, slower: bool) {
        if (self.q_speed_air_y as u16) < 48_384 {
            return;
        }
        let speed = self.q_speed_air_x;
        let fraction = speed >> if slower { 6 } else { 5 };
        let next = speed.wrapping_sub(fraction);
        self.q_speed_air_x = if (fraction < 0 && next > 0) || (fraction > 0 && next < 0) {
            0
        } else {
            next
        };
    }

    pub fn apply_gravity(&mut self, half_when_rising: bool) {
        let mut gravity = if self.move_state & IN_WATER != 0 {
            12
        } else {
            42
        };
        if half_when_rising && self.q_speed_air_y < 0 {
            gravity /= 2;
        }
        self.q_speed_air_y = self.q_speed_air_y.wrapping_add(gravity);
    }

    /// PLAYERFN_UPDATE_POSITION: inversion changes velocity before fall capping.
    pub fn advance_position(&mut self, inverted: bool, previously_inverted: bool) {
        self.q_world_x = self.q_world_x.wrapping_add(i32::from(self.q_speed_air_x));
        if inverted != previously_inverted {
            self.q_speed_air_y = self.q_speed_air_y.wrapping_neg();
        }
        self.q_speed_air_y = self.q_speed_air_y.min(3840);
        let dy = i32::from(self.q_speed_air_y);
        self.q_world_y = if inverted {
            self.q_world_y.wrapping_sub(dy)
        } else {
            self.q_world_y.wrapping_add(dy)
        };
    }

    /// sub_8023260. sin/cos are the exact native Q2.14 table values shifted by 6.
    pub fn ground_speed_to_air(&mut self, sin_q8: i32, cos_q8: i32) {
        let maximum = i32::from(self.max_speed as i16);
        let speed = i32::from(self.q_speed_ground);
        self.q_speed_ground = if speed > maximum {
            maximum as i16
        } else if speed < -maximum {
            -maximum as i16
        } else {
            speed as i16
        };
        let speed = i32::from(self.q_speed_ground);
        self.q_speed_air_x = (cos_q8.wrapping_mul(speed) >> 8) as i16;
        if self.move_state & IN_AIR == 0 {
            self.q_speed_air_y = 0;
        }
        self.q_speed_air_y =
            i32::from(self.q_speed_air_y).wrapping_add(sin_q8.wrapping_mul(speed) >> 8) as i16;
    }

    /// Player_Idle's signed slope acceleration, before ground velocity rotation.
    pub fn ground_slope_acceleration(&mut self, sin_q8: i32) {
        if self.rotation.wrapping_add(96) < 192 && self.q_speed_ground != 0 {
            self.q_speed_ground =
                i32::from(self.q_speed_ground).wrapping_add(sin_q8.wrapping_mul(3) >> 5) as i16;
        }
    }

    pub fn relax_rotation(&mut self) {
        let rotation = self.rotation as i8;
        self.rotation = if rotation < 0 {
            rotation.saturating_add(2).min(0) as u8
        } else {
            rotation.saturating_sub(2).max(0) as u8
        };
    }

    /// SA2 Player_TryJump only selects InitJump for the next callback execution.
    /// `clearance` is sub_8022F58's native directional probe result, not a guess.
    pub fn request_jump(&mut self, jump_button: u16, clearance: i32) -> bool {
        if self.frame_input & jump_button != 0 && clearance > 3 {
            self.callback = Callback::InitJump;
            true
        } else {
            false
        }
    }

    /// Player_InitJump through the boundary immediately before Player_Jumping.
    /// sin/cos use `(rotation - 64) * 4` in the native sine table. Boss camera
    /// correction is supplied only by a host executing an actual boss context.
    pub fn initialize_jump(&mut self, sin_q8: i32, cos_q8: i32, boss_camera_dx: Option<i16>) {
        let cancel = 0x20000000
            | 0x10000000
            | 0x1000000
            | 0x80000
            | 0x40000
            | 0x20000
            | 0x8000
            | 0x4000
            | 0x2000
            | 0x400
            | 0x200
            | 0x100
            | 0x20
            | 0x10;
        self.move_state &= !cancel;
        self.unk61 = 0;
        self.unk62 = 0;
        self.unk63 = 0;
        self.unk71 = 0;
        self.unk70 = false;
        if self.character == Character::Sonic {
            self.move_state &= !0x2000000;
        }
        self.move_state |= JUMP_HELD_STATE | IN_AIR;
        self.move_state &= !(0x1000000 | 0x20);
        if self.move_state & SPIN_ATTACK != 0 {
            self.move_state |= FLIP_WITH_MOVE_DIR;
        }
        self.char_state = if i32::from(self.q_speed_air_x).abs() < 320 {
            JUMP_SLOW
        } else {
            JUMP_FAST
        };
        self.unk70 = true;
        let jump = if self.move_state & IN_WATER != 0 {
            672
        } else {
            1248
        };
        self.q_speed_air_x =
            i32::from(self.q_speed_air_x).wrapping_add(cos_q8.wrapping_mul(jump) >> 8) as i16;
        self.q_speed_air_y =
            i32::from(self.q_speed_air_y).wrapping_add(sin_q8.wrapping_mul(jump) >> 8) as i16;
        if self.move_state & STOOD_ON_OBJECT != 0
            && let Some(dx) = boss_camera_dx
        {
            self.q_speed_air_x =
                i32::from(self.q_speed_air_x).wrapping_sub(i32::from(dx) * 256) as i16;
        }
        self.sprite_frame_flags &= !ANIMATION_OVER;
        self.callback = Callback::Airborne;
    }

    /// The variable jump-height part of Player_Jumping. Special mid-air actions
    /// must be dispatched before this operation by a complete callback host.
    pub fn release_jump(&mut self, jump_button: u16) {
        let cap = if self.move_state & IN_WATER != 0 {
            -384
        } else {
            -768
        };
        if self.move_state & JUMP_HELD_STATE != 0
            && self.q_speed_air_y < cap
            && self.held_input & jump_button == 0
        {
            self.q_speed_air_y = cap;
        }
    }

    /// Player_HandleSpriteYOffsetChange plus the offset assignment macro.
    pub fn change_sprite_offsets(&mut self, x: i8, y: i8, inverted: bool) {
        if self.sprite_offset_y != y {
            let mut rotation = self.rotation;
            if inverted {
                rotation = rotation.wrapping_add(64).wrapping_neg().wrapping_sub(64);
            }
            // The source promotes u8 before this test, then truncates the sum.
            rotation = if rotation != 0 {
                rotation.wrapping_add(31)
            } else {
                32
            };
            let delta = (i32::from(y) - i32::from(self.sprite_offset_y)) * 256;
            match rotation >> 6 {
                0 => self.q_world_y = self.q_world_y.wrapping_sub(delta),
                1 => self.q_world_x = self.q_world_x.wrapping_add(delta),
                2 => self.q_world_y = self.q_world_y.wrapping_add(delta),
                _ => self.q_world_x = self.q_world_x.wrapping_sub(delta),
            }
        }
        self.sprite_offset_x = x;
        self.sprite_offset_y = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> PlayerState {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.move_state = 0;
        p.held_input = 0;
        p.frame_input = 0;
        p
    }

    #[test]
    fn native_air_acceleration_preserves_over_speed_momentum() {
        let mut p = player();
        p.held_input = RIGHT;
        p.q_speed_air_x = 1600;
        p.air_input();
        assert_eq!(p.q_speed_air_x, 1600);
        p.q_speed_air_x = 1530;
        p.air_input();
        assert_eq!(p.q_speed_air_x, 1536);
        p.held_input = LEFT | RIGHT;
        p.air_input();
        assert_eq!(p.q_speed_air_x, 1520);
        assert_ne!(p.move_state & FACING_LEFT, 0);
    }

    #[test]
    fn native_input_lock_keeps_held_state_without_new_edges() {
        let mut p = player();
        p.handle_input(1 | RIGHT);
        assert_eq!(p.frame_input, 1 | RIGHT);
        p.move_state |= IGNORE_INPUT;
        p.handle_input(0);
        assert_eq!(p.held_input, 1 | RIGHT);
        assert_eq!(p.frame_input, 0);
    }

    #[test]
    fn native_fall_cap_follows_inversion_and_gravity_is_asymmetric() {
        let mut p = player();
        p.q_speed_air_y = -4000;
        p.advance_position(true, false);
        assert_eq!((p.q_speed_air_y, p.q_world_y), (3840, -3840));
        p.q_speed_air_y = -100;
        p.apply_gravity(true);
        assert_eq!(p.q_speed_air_y, -79);
        p.q_speed_air_y = 100;
        p.apply_gravity(true);
        assert_eq!(p.q_speed_air_y, 142);
    }

    #[test]
    fn jump_request_obeys_clearance_and_callback_delay() {
        let mut p = player();
        p.frame_input = 1;
        assert!(!p.request_jump(1, 3));
        assert!(p.request_jump(1, 4));
        assert_eq!(p.callback, Callback::InitJump);
        assert_eq!(p.q_speed_air_y, 0);
        p.initialize_jump(-256, 0, None);
        assert_eq!(p.q_speed_air_y, -1248);
        assert_eq!(p.callback, Callback::Airborne);
        p.release_jump(1);
        assert_eq!(p.q_speed_air_y, -768);
    }

    #[test]
    fn ring_thresholds_are_native_and_boost_checks_previous_top_speed() {
        let mut p = player();
        for (rings, acceleration) in [(10, 8), (11, 12), (50, 14), (100, 16), (150, 18)] {
            p.update_frame_physics(rings);
            assert_eq!(p.acceleration, acceleration);
        }
        p.q_speed_ground = 1536;
        p.boost_speed = 1080;
        assert!(p.update_frame_physics(150));
        assert_eq!((p.top_speed, p.max_speed), (3072, 3840));
        p.q_speed_ground = 1151;
        p.update_frame_physics(150);
        assert!(!p.is_boosting);
    }
}
