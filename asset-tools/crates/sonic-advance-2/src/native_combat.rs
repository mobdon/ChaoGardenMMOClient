//! Sonic's course combat transitions. The enemy tasks update next frame's
//! homing target after player input. A target must pass the original angle and distance tests.
use crate::native_player::{self as p, PlayerState};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HomingTarget {
    pub distance_squared: i32,
    pub angle: u16,
}
impl Default for HomingTarget {
    fn default() -> Self {
        Self {
            distance_squared: 128 * 128,
            angle: 0,
        }
    }
}
impl HomingTarget {
    pub fn consider(&mut self, player: &PlayerState, x_q8: i32, y_q8: i32) {
        if player.character != p::Character::Sonic {
            return;
        }
        let dx = player.q_world_x.wrapping_sub(x_q8) >> 8;
        let dy = player.q_world_y.wrapping_sub(y_q8) >> 8;
        if dx.unsigned_abs() >= 128 || dy.unsigned_abs() >= 128 {
            return;
        }
        let square = dx * dx + dy * dy;
        if square >= self.distance_squared {
            return;
        }
        let left = player.move_state & p::FACING_LEFT != 0;
        let angle = if left {
            (vector_angle(dx, dy).wrapping_sub(256) as u16) & 1023
        } else {
            vector_angle(-dy, -dx) as u16
        };
        if angle <= 312 {
            self.distance_squared = square;
            self.angle = if left {
                512u16.wrapping_sub(angle) & 1023
            } else {
                angle
            };
        }
    }
    pub fn can_launch(self, player: &PlayerState, jump: u16) -> bool {
        player.character == p::Character::Sonic
            && self.distance_squared < 128 * 128
            && player.unk71 != 1
            && player.move_state & (p::JUMP_HELD_STATE | crate::native_sonic::SOME_ATTACK)
                == p::JUMP_HELD_STATE
            && player.frame_input & jump != 0
    }
}
// sub_8004418 uses an octant ratio, not BIOS ArcTan2. Homing candidates
// are strictly inside a 128-pixel radius, so scaled ratios fit s16.
fn vector_angle(mut x: i32, mut y: i32) -> i16 {
    if x == 0 && y == 0 {
        return -1;
    }
    let mut quadrant = 0;
    if x <= 0 {
        x = -x;
        quadrant = 4;
    }
    if y <= 0 {
        y = -y;
        quadrant += 2;
    }
    let ratio = if x >= y {
        y * 128 / x
    } else {
        quadrant += 1;
        x * 128 / y
    };
    let sector = [1, 0, 2, 3, 6, 7, 5, 4][quadrant];
    ((sector * 128 + if sector & 1 != 0 { 128 - ratio } else { ratio }) & 1023) as i16
}
pub fn init_homing(p: &mut PlayerState, sin: i32, cos: i32) {
    p.cancel_flying_and_boost();
    p.move_state = (p.move_state | 0x22000000 | p::IN_AIR) & !(0x1000000 | 0x20);
    p.change_sprite_offsets(6, 9, false);
    p.char_state = 19;
    p.q_speed_air_x = (cos * 6) as i16;
    p.q_speed_air_y = (sin * 6 - 128) as i16;
    p.rotation = 0;
    p.unk70 = false;
    p.unk71 = 0;
    p.unk6e = 0;
    p.sprite_frame_flags &= !p::ANIMATION_OVER;
}
pub fn init_recoil(p: &mut PlayerState) {
    p.cancel_flying_and_boost();
    p.move_state = (p.move_state | 0x100 | p::IN_AIR) & !(0x20000000 | 0x1000000 | 0x20);
    p.change_sprite_offsets(6, 14, false);
    p.char_state = 14;
    p.q_speed_air_x = 0;
    p.q_speed_air_y = -1024;
    p.rotation = 0;
    p.unk70 = true;
    p.unk71 = 0;
    p.unk6e = 0;
    p.sprite_frame_flags &= !p::ANIMATION_OVER;
}
pub fn init_hurt(p: &mut PlayerState) {
    let vx = i32::from(p.q_speed_air_x);
    p.is_boosting = false;
    if vx.abs() <= 640 {
        p.q_speed_air_x = if vx < -160 || (vx <= 160 && p.move_state & p::FACING_LEFT != 0) {
            384
        } else {
            -384
        };
        p.char_state = 20;
    } else {
        p.q_speed_air_x = if vx >= 0 { 384 } else { -384 };
        p.char_state = 21;
    }
    p.q_speed_air_y = -768;
    if p.move_state & p::IN_WATER != 0 {
        p.q_speed_air_y >>= 1;
        p.q_speed_air_x >>= 1;
    }
    p.cancel_flying_and_boost();
    p.move_state = (p.move_state | p::IN_AIR) & !(0x200 | p::STOOD_ON_OBJECT | p::SPIN_ATTACK);
    p.change_sprite_offsets(6, 14, false);
}

/// The native signed homing timer is decremented before physics, including on
/// the launch frame. A timeout changes the pose and boost flag, not the callback.
pub fn homing_tick_before_physics(p: &mut PlayerState, frames: &mut i16) {
    *frames = frames.wrapping_sub(1);
    if *frames == -1 {
        p.char_state = 14;
        p.move_state &= !0x0200_0000;
    }
}
/// Native homing writes this transition after physics; later object tasks can replace it.
pub fn homing_transition_after_physics(p: &PlayerState) -> Option<u8> {
    (p.move_state & p::IN_AIR == 0).then_some(1)
}
/// End of Task_PlayerMain, after display and before death handling.
pub fn tick_hurt_immunity(char_state: i16, timer: &mut u16) {
    if char_state != 20 {
        *timer = timer.saturating_sub(1);
    }
}
