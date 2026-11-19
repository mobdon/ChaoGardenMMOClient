//! A2NE's normal-stage camera state and integer follow algorithm.
//!
//! Evidence: SAT-R/sa2 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
//! `src/game/shared/stage/camera.c` InitCamera/UpdateCamera and camera tasks.
//! This module does not execute boss/spectator cameras, parallax callbacks,
//! display-register writes or the stage renderer.

use crate::{
    Error,
    native_player::{Character, IN_AIR, PlayerState},
};
use serde::{Deserialize, Serialize};

pub const DISPLAY_WIDTH: i32 = 240;
pub const DISPLAY_HEIGHT: i32 = 160;

/// Normal-stage fields of the native Camera. Names describe only verified uses;
/// numeric unknown labels remain in documentation for source correspondence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCamera {
    pub x: i32,
    pub y: i32,
    /// Native dx/dy are previous position minus new position, in whole pixels.
    pub dx: i32,
    pub dy: i32,
    /// unk10 / unk14.
    pub target_x: i32,
    pub target_y: i32,
    /// unk8 is Q24.8 in A2NE, unlike the SA1 whole-pixel field.
    pub q_max_step_x: i32,
    /// unkC is a whole-pixel vertical step limit.
    pub max_step_y: i32,
    pub shift_x: i32,
    pub shift_y: i32,
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    /// unk40: nonzero delays target recomputation, not camera movement.
    pub target_delay: i16,
    /// unk44 / unk48.
    pub deadzone_x: i32,
    pub deadzone_y: i32,
    /// unk4C: supplied by native looking/camera-shift behavior.
    pub look_y: i32,
    /// unk50 bit 0 locks target X; bit 1 locks target Y.
    pub axes_locked: u16,
    /// unk56: signed Q8.8 filtered horizontal player velocity.
    pub q_velocity_lead_x: i16,
    /// unk64: spriteOffsetY - 4, approached by five pixels each update.
    pub body_offset_y: i16,
    pub shake_x: i16,
    pub shake_y: i16,
    task_armed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CameraStep {
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    /// False for the initial Task_CallUpdateCamera arming invocation.
    pub updated: bool,
}

impl NativeCamera {
    /// The scalar normal-stage branch of InitCamera, for the retail 240x160 view.
    /// Bounds come from the verified Collision header, not the current actor.
    pub fn normal_stage(player: &PlayerState, width: i32, height: i32) -> Result<Self, Error> {
        if width <= DISPLAY_WIDTH || height <= DISPLAY_HEIGHT {
            return Err(Error(
                "Native camera stage must exceed the retail viewport".into(),
            ));
        }
        let x = (player.q_world_x >> 8).wrapping_sub(120).max(0);
        let y = (player.q_world_y >> 8).wrapping_sub(84).max(0);
        Ok(Self {
            x,
            y,
            dx: 0,
            dy: 0,
            target_x: x,
            target_y: y,
            q_max_step_x: 4096,
            max_step_y: 12,
            shift_x: 0,
            shift_y: 0,
            min_x: 0,
            max_x: width,
            min_y: 0,
            max_y: height,
            target_delay: 0,
            deadzone_x: 8,
            deadzone_y: 0,
            look_y: 0,
            axes_locked: 0,
            q_velocity_lead_x: 0,
            body_offset_y: i16::from(player.sprite_offset_y) - 4,
            shake_x: 0,
            shake_y: 0,
            task_armed: false,
        })
    }

    pub fn position(&self) -> [i32; 2] {
        [self.x, self.y]
    }

    /// Native Player_HandleBoostState sets gCamera.unk8 to Q(4) on activation.
    pub fn on_boost_activation(&mut self) {
        self.q_max_step_x = 1024;
    }

    /// Invoke at native camera task priority 0xF00, before player priority0x3000.
    /// The initial task only arms Task_UpdateCamera; it performs no follow step.
    pub fn tick_task(&mut self, player: &PlayerState, inverted: bool) -> Result<CameraStep, Error> {
        if !self.task_armed {
            self.validate()?;
            self.task_armed = true;
            return Ok(self.step(false));
        }
        self.update(player, inverted)
    }

    /// Scalar nonboss UpdateCamera. Boss, spectator and background callbacks are
    /// outside this API; callers must choose this normal-stage path explicitly.
    pub fn update(&mut self, player: &PlayerState, inverted: bool) -> Result<CameraStep, Error> {
        self.validate()?;
        let previous_x = self.x;
        let previous_y = self.y;
        // The preliminary clamp intentionally reserves one more pixel than the
        // final clamp. Collapsing these bounds changes native edge behavior.
        let mut x = self.x.clamp(self.min_x, self.max_x - DISPLAY_WIDTH - 1);
        let mut y = self.y.clamp(self.min_y, self.max_y - DISPLAY_HEIGHT - 1);
        if self.target_delay != 0 {
            self.target_delay = self.target_delay.wrapping_sub(1);
        } else {
            if self.axes_locked & 1 == 0 {
                self.target_x = (player.q_world_x >> 8)
                    .wrapping_add(self.shift_x)
                    .wrapping_sub(120);
                self.q_velocity_lead_x = ((i32::from(player.q_speed_air_x)
                    + i32::from(self.q_velocity_lead_x) * 15)
                    >> 4) as i16;
                self.target_x = self
                    .target_x
                    .wrapping_add(i32::from(self.q_velocity_lead_x) >> 5);
            }
            if self.axes_locked & 2 == 0 {
                let mut target_offset = i32::from(player.sprite_offset_y) - 4;
                if inverted {
                    target_offset = -target_offset;
                }
                let offset = i32::from(self.body_offset_y);
                self.body_offset_y = if offset < target_offset {
                    (offset + 5).min(target_offset) as i16
                } else if offset > target_offset {
                    (offset - 5).max(target_offset) as i16
                } else {
                    self.body_offset_y
                };
                self.target_y = (player.q_world_y >> 8)
                    .wrapping_add(self.shift_y)
                    .wrapping_sub(80)
                    .wrapping_add(self.look_y)
                    .wrapping_add(i32::from(self.body_offset_y));
            }
        }
        let delta_x = self.target_x.wrapping_sub(x);
        if delta_x > self.deadzone_x {
            x = x.wrapping_add((self.q_max_step_x >> 8).min(delta_x.wrapping_sub(self.deadzone_x)));
        } else if delta_x < -self.deadzone_x {
            x = x.wrapping_add(
                (-(self.q_max_step_x >> 8)).max(delta_x.wrapping_add(self.deadzone_x)),
            );
        }
        x = x.clamp(self.min_x, self.max_x - DISPLAY_WIDTH);
        if self.q_max_step_x < 4096 {
            self.q_max_step_x = self.q_max_step_x.wrapping_add(32);
        }
        if player.move_state & IN_AIR != 0
            && (player.character != Character::Knuckles || player.unk61 != 9)
        {
            self.deadzone_y = self.deadzone_y.wrapping_add(4).min(24);
        } else {
            self.deadzone_y = self.deadzone_y.wrapping_sub(4).max(0);
        }
        let delta_y = self.target_y.wrapping_sub(y);
        if delta_y > self.deadzone_y {
            y = y.wrapping_add(self.max_step_y.min(delta_y.wrapping_sub(self.deadzone_y)));
        } else if delta_y < -self.deadzone_y {
            y = y.wrapping_add((-self.max_step_y).max(delta_y.wrapping_add(self.deadzone_y)));
        }
        y = y.clamp(self.min_y, self.max_y - DISPLAY_HEIGHT);
        x = x
            .clamp(self.min_x, self.max_x - DISPLAY_WIDTH)
            .wrapping_add(i32::from(self.shake_x));
        y = y
            .clamp(self.min_y, self.max_y - DISPLAY_HEIGHT)
            .wrapping_add(i32::from(self.shake_y));
        self.x = x;
        self.y = y;
        self.dx = previous_x.wrapping_sub(x);
        self.dy = previous_y.wrapping_sub(y);
        Ok(self.step(true))
    }

    fn validate(&self) -> Result<(), Error> {
        let width = self.max_x.checked_sub(self.min_x);
        let height = self.max_y.checked_sub(self.min_y);
        if width.is_none_or(|v| v <= DISPLAY_WIDTH)
            || height.is_none_or(|v| v <= DISPLAY_HEIGHT)
            || self.q_max_step_x < 0
            || self.max_step_y < 0
            || self.deadzone_x < 0
            || !(0..=24).contains(&self.deadzone_y)
        {
            return Err(Error(
                "Invalid native normal-stage camera bounds or step limits".into(),
            ));
        }
        Ok(())
    }

    fn step(&self, updated: bool) -> CameraStep {
        CameraStep {
            x: self.x,
            y: self.y,
            dx: self.dx,
            dy: self.dy,
            updated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(x: i32, y: i32) -> PlayerState {
        PlayerState::new_at(Character::Sonic, x * 256, y * 256)
    }

    #[test]
    fn native_spawn_anchor_and_first_task_delay_are_preserved() {
        let p = player(96, 655);
        let mut c = NativeCamera::normal_stage(&p, 19776, 3072).unwrap();
        assert_eq!(c.position(), [0, 571]);
        assert!(!c.tick_task(&p, false).unwrap().updated);
        assert_eq!(c.position(), [0, 571]);
        assert!(c.tick_task(&p, false).unwrap().updated);
        assert_eq!(c.position(), [0, 583]);
        assert_eq!((c.dx, c.dy), (0, -12));
        c.update(&p, false).unwrap();
        assert_eq!(c.position(), [0, 585]);
    }

    #[test]
    fn target_delay_does_not_freeze_camera_and_velocity_filter_is_signed() {
        let mut p = player(500, 500);
        let mut c = NativeCamera::normal_stage(&p, 19776, 3072).unwrap();
        p.q_speed_air_x = -1;
        c.update(&p, false).unwrap();
        assert_eq!(c.q_velocity_lead_x, -1);
        assert_eq!(c.target_x, 379);
        c.target_delay = 1;
        c.target_x = c.x + 100;
        let previous = c.x;
        c.update(&p, false).unwrap();
        assert_eq!(c.target_delay, 0);
        assert_eq!(c.x, previous + 16);
    }

    #[test]
    fn airborne_deadzone_and_boost_step_recover_incrementally() {
        let mut p = player(500, 500);
        let mut c = NativeCamera::normal_stage(&p, 19776, 3072).unwrap();
        c.on_boost_activation();
        p.move_state |= IN_AIR;
        for _ in 0..6 {
            c.update(&p, false).unwrap();
        }
        assert_eq!(c.deadzone_y, 24);
        assert_eq!(c.q_max_step_x, 1024 + 6 * 32);
        p.character = Character::Knuckles;
        p.unk61 = 9;
        c.update(&p, false).unwrap();
        assert_eq!(c.deadzone_y, 20);
    }

    #[test]
    fn invalid_bounds_fail_before_mutating_camera() {
        let p = player(96, 655);
        assert!(NativeCamera::normal_stage(&p, 240, 160).is_err());
        let mut c = NativeCamera::normal_stage(&p, 19776, 3072).unwrap();
        c.max_x = c.min_x;
        let before = c;
        assert!(c.update(&p, false).is_err());
        assert_eq!(c, before);
    }
}
