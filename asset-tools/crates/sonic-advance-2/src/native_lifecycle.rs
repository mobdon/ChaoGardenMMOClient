//! Normal single-player A2NE bounds, clock, death and stage-restart decisions.
//!
//! Native task recreation, intro/game-over screens and checkpoint entity
//! activation are host boundaries. This module does not auto-release the intro.

use crate::{
    Error, VerifiedRom,
    native_camera::NativeCamera,
    native_player::{IN_WATER, PlayerState},
};
use serde::Serialize;

pub const DEAD: u32 = 0x80;
pub const DEAD_STATE: i16 = 22;
pub const HIDDEN: u32 = 0x10_0000;
pub const ACT_START: u16 = 1;
pub const TIMER_REVERSED: u16 = 4;
pub const DISABLE_PAUSE: u16 = 0x20;
pub const GRAVITY_INVERTED: u16 = 0x80;
pub const MAX_COURSE_TIME: u32 = 36_000;

fn q(pixel: i32) -> Result<i32, Error> {
    pixel
        .checked_mul(256)
        .ok_or_else(|| Error("Native bound exceeds signed Q24.8".into()))
}

/// Normal-stage sub_80232D0, before position advancement. Bounds are the live
/// camera limits, including native narrowing, rather than terrain dimensions.
/// Course runner with fewer than seven special rings: a reached ordinary goal
/// still uses these bounds (sub_80232D0). The special-region wrap and boss
/// camera adjustments require their own paths.
pub fn normal_bounds(
    p: &mut PlayerState,
    camera: &NativeCamera,
    inverted: bool,
) -> Result<bool, Error> {
    let min_x = q(camera.min_x)?;
    let min_y = q(camera.min_y)?;
    let max_x = q(camera.max_x)?
        .checked_sub(1)
        .ok_or_else(|| Error("Native X bound underflow".into()))?;
    let max_y = q(camera.max_y)?
        .checked_sub(1)
        .ok_or_else(|| Error("Native Y bound underflow".into()))?;
    if min_x > max_x || min_y > max_y {
        return Err(Error("Invalid native world bounds".into()));
    }
    if p.move_state & (0x8000_0000 | DEAD) == DEAD {
        return Ok(false);
    }
    let mut y = p.q_world_y;
    let died = p.move_state & 0x8000_0000 == 0
        && if inverted {
            p.q_world_y <= min_y
        } else {
            p.q_world_y >= max_y
        };
    if died {
        p.move_state |= DEAD;
        p.q_speed_air_y = if p.move_state & IN_WATER != 0 {
            -672
        } else {
            -1248
        };
        y = if inverted { min_y } else { max_y };
    }
    let new_x = p.q_world_x.clamp(min_x, max_x);
    let new_y = y.clamp(min_y, max_y);
    if new_x != p.q_world_x {
        p.q_speed_air_x = 0;
        p.q_speed_ground = 0;
    }
    if new_y != y {
        p.q_speed_air_y = 0;
        p.q_speed_ground = 0;
    }
    p.q_world_x = new_x;
    p.q_world_y = new_y;
    Ok(died)
}

pub fn dead_player_left_screen(p: &PlayerState, camera: &NativeCamera, inverted: bool) -> bool {
    if p.move_state & 0x8000_0000 != 0 {
        return false;
    }
    if inverted {
        p.q_world_y <= camera.y.wrapping_sub(80).wrapping_mul(256)
    } else {
        p.q_world_y
            >= camera
                .y
                .wrapping_mul(256)
                .wrapping_add(240 * 256)
                .wrapping_sub(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Checkpoint {
    pub x: i16,
    pub y: i16,
    pub time: u32,
}

pub fn checkpoint_position(rom: &VerifiedRom<'_>, level: u8) -> Result<[i16; 2], Error> {
    if !crate::native_stage::is_course_act(level) {
        return Err(Error(
            "Native checkpoint lookup requires a course act".into(),
        ));
    }
    let bytes = rom.range("rom", 0xd63fc + usize::from(level) * 8, 8)?;
    let mut result = [0; 2];
    for (out, raw) in result.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *out = i16::try_from(i32::from_le_bytes(*raw))
            .map_err(|_| Error("Native checkpoint coordinate exceeds player field".into()))?;
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    Alive,
    Dying,
    Waiting { frames: u16 },
    AwaitingStageReset,
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GameOverCause {
    ZeroLives,
    TimeUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathTick {
    /// Gravity/position executed; host updates native death animation.
    Animate,
    /// Native player task performs no animation while waiting/offscreen.
    Wait,
    /// Host clears rings/special rings and recreates the original stage tasks.
    RestartStage,
    /// Time-up does not clear rings through the ordinary life-lost branch.
    GameOver {
        cause: GameOverCause,
        clear_rings: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeLifecycle {
    pub phase: Phase,
    pub stage_time: u32,
    pub stage_flags: u16,
    pub course_time: u32,
    pub checkpoint_time: u32,
    pub checkpoint: Checkpoint,
    pub checkpoint_reached: bool,
    pub time_limit_disabled: bool,
    pub timer_invulnerability: u16,
    pub item_effect: u32,
    pub seconds_until_drown: i8,
    pub q_scale_x: i16,
    pub q_scale_y: i16,
}

impl NativeLifecycle {
    /// Caller chooses whether the native intro is still active. The movement
    /// harness's immediate intro bypass must remain an explicit host policy.
    pub fn new(spawn: [i16; 2], intro_active: bool) -> Self {
        Self {
            phase: Phase::Alive,
            stage_time: 0,
            stage_flags: if intro_active {
                ACT_START | DISABLE_PAUSE
            } else {
                0
            },
            course_time: 0,
            checkpoint_time: 0,
            checkpoint: Checkpoint {
                x: spawn[0],
                y: spawn[1],
                time: 0,
            },
            checkpoint_reached: false,
            time_limit_disabled: false,
            timer_invulnerability: 0,
            item_effect: 0,
            seconds_until_drown: 30,
            q_scale_x: 256,
            q_scale_y: 256,
        }
    }

    /// Call only for an active, onscreen native checkpoint map-entity task at
    /// priority 0x2010, before player 0x3000. Native activation is X-only.
    pub fn cross_checkpoint(
        &mut self,
        p: &PlayerState,
        entity_x: i32,
        destination: [i16; 2],
    ) -> bool {
        if self.checkpoint_reached
            || p.move_state & (0x0040_0000 | DEAD) != 0
            || entity_x > p.q_world_x >> 8
        {
            return false;
        }
        self.checkpoint = Checkpoint {
            x: destination[0],
            y: destination[1],
            time: self.checkpoint_time,
        };
        self.checkpoint_reached = true;
        true
    }

    /// End of Task_PlayerMain, after this frame's presentation pass. Native
    /// death pose is selected here and displayed on the next dying-task tick.
    /// Returns the native sound request: life lost 119 or drowned 157.
    pub fn finish_player_main(
        &mut self,
        p: &mut PlayerState,
        camera: &mut NativeCamera,
    ) -> Option<u16> {
        if self.phase != Phase::Alive || p.move_state & DEAD == 0 {
            return None;
        }
        self.phase = Phase::Dying;
        p.char_state = DEAD_STATE;
        p.q_speed_air_x = 0;
        if p.q_speed_air_y < -1024 {
            p.q_speed_air_y = -512;
        } else if p.q_speed_air_y > 0 {
            p.q_speed_air_y = 0;
        }
        self.timer_invulnerability = 2;
        self.item_effect = 0;
        p.move_state &= !(0x20 | 8);
        camera.axes_locked |= 3;
        self.stage_flags |= ACT_START;
        p.sprite_frame_flags = p.sprite_frame_flags & !0x3000 | 0x1000;
        self.q_scale_x = 256;
        self.q_scale_y = 256;
        Some(if self.seconds_until_drown < 0 {
            157
        } else {
            119
        })
    }

    /// Original dying task and 60-count wait; life decrement occurs only on the
    /// invocation after the wait reaches zero. Lives retain their native u8.
    pub fn death_task(
        &mut self,
        p: &mut PlayerState,
        camera: &NativeCamera,
        previously_inverted: bool,
        lives: &mut u8,
    ) -> Result<DeathTick, Error> {
        match self.phase {
            Phase::Dying => {
                let inverted = self.stage_flags & GRAVITY_INVERTED != 0;
                if dead_player_left_screen(p, camera, inverted) {
                    p.move_state |= HIDDEN;
                    self.phase = Phase::Waiting { frames: 60 };
                    return Ok(DeathTick::Wait);
                }
                p.apply_gravity(false);
                p.advance_position(inverted, previously_inverted);
                Ok(DeathTick::Animate)
            }
            Phase::Waiting { frames } if frames != 0 => {
                self.phase = Phase::Waiting { frames: frames - 1 };
                Ok(DeathTick::Wait)
            }
            Phase::Waiting { .. } => {
                self.stage_flags |= DISABLE_PAUSE;
                let timeout = !self.time_limit_disabled
                    && (self.course_time > MAX_COURSE_TIME
                        || self.stage_flags & TIMER_REVERSED != 0 && self.course_time == 0);
                *lives = lives.wrapping_sub(1);
                if *lives == 0 || timeout {
                    self.phase = Phase::GameOver;
                    Ok(DeathTick::GameOver {
                        cause: if *lives == 0 {
                            GameOverCause::ZeroLives
                        } else {
                            GameOverCause::TimeUp
                        },
                        clear_rings: !timeout,
                    })
                } else {
                    self.phase = Phase::AwaitingStageReset;
                    Ok(DeathTick::RestartStage)
                }
            }
            _ => Err(Error("Native death task is not active".into())),
        }
    }

    /// Scalar single-player Task_GameStage at priority 0xFF00, after player.
    /// Returns true when native timeout starts (sound SE_TIME_UP = 149).
    pub fn tick_stage_clock(&mut self, p: &mut PlayerState) -> bool {
        self.stage_time = self.stage_time.wrapping_add(1);
        if self.stage_flags & ACT_START != 0 {
            return false;
        }
        self.checkpoint_time = self.checkpoint_time.wrapping_add(1);
        let expired = if self.stage_flags & TIMER_REVERSED != 0 {
            self.course_time = self.course_time.wrapping_sub(1);
            self.course_time as i32 <= 0
        } else {
            self.course_time = self.course_time.wrapping_add(1);
            self.course_time > MAX_COURSE_TIME
        };
        if !expired {
            return false;
        }
        self.stage_flags |= ACT_START;
        if self.time_limit_disabled {
            return false;
        }
        self.item_effect = 0;
        p.q_speed_air_y = if p.move_state & IN_WATER != 0 {
            -672
        } else {
            -1248
        };
        p.move_state |= DEAD;
        true
    }

    /// Represented InitializePlayer/CreateGameStage state. The host must first
    /// recreate terrain/entity/ring/intro tasks; this is not their replacement.
    /// Returns the new camera with its original first-task arming delay.
    pub fn reinitialize_player(
        &mut self,
        p: &mut PlayerState,
        width: i32,
        height: i32,
    ) -> Result<NativeCamera, Error> {
        if self.phase != Phase::AwaitingStageReset {
            return Err(Error("Native stage restart is not pending".into()));
        }
        let initialized = PlayerState::new_at(
            p.character,
            i32::from(self.checkpoint.x) * 256,
            i32::from(self.checkpoint.y) * 256,
        );
        let camera = NativeCamera::normal_stage(&initialized, width, height)?;
        if self.course_time >= MAX_COURSE_TIME {
            self.checkpoint.time = 0;
        }
        self.course_time = self.checkpoint.time;
        self.checkpoint_time = self.checkpoint.time;
        self.stage_flags = (self.stage_flags | ACT_START | DISABLE_PAUSE) & !GRAVITY_INVERTED;
        self.phase = Phase::Alive;
        self.timer_invulnerability = 0;
        self.item_effect = 0;
        self.seconds_until_drown = 30;
        *p = initialized;
        Ok(camera)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_player::Character;

    fn setup() -> (PlayerState, NativeCamera, NativeLifecycle) {
        let p = PlayerState::new_at(Character::Sonic, 96 * 256, 400 * 256);
        let c = NativeCamera::normal_stage(&p, 2048, 1024).unwrap();
        (p, c, NativeLifecycle::new([96, 400], false))
    }

    #[test]
    fn horizontal_clamps_and_narrowed_vertical_bounds_drive_death() {
        let (mut p, mut c, _) = setup();
        p.move_state = 0;
        p.q_world_x = -1;
        p.q_speed_air_x = -200;
        p.q_speed_ground = -200;
        assert!(!normal_bounds(&mut p, &c, false).unwrap());
        assert_eq!((p.q_world_x, p.q_speed_air_x, p.q_speed_ground), (0, 0, 0));
        c.max_y = 400;
        p.q_world_y = 400 * 256 - 1;
        assert!(normal_bounds(&mut p, &c, false).unwrap());
        assert_eq!(p.q_speed_air_y, -1248);
        let before = p;
        assert!(!normal_bounds(&mut p, &c, false).unwrap());
        assert_eq!(p, before);
    }

    #[test]
    fn death_setup_wait_and_ordinary_life_restart_are_separate_ticks() {
        let (mut p, mut c, mut life) = setup();
        p.move_state = DEAD | 8 | 0x20;
        p.q_speed_air_y = -1248;
        assert_eq!(life.finish_player_main(&mut p, &mut c), Some(119));
        assert_eq!(p.q_speed_air_y, -512);
        assert_eq!(p.char_state, DEAD_STATE);
        assert_eq!(c.axes_locked & 3, 3);
        p.q_world_y = (c.y + 240) * 256 - 1;
        let mut lives = 3;
        assert_eq!(
            life.death_task(&mut p, &c, false, &mut lives).unwrap(),
            DeathTick::Wait
        );
        for _ in 0..60 {
            assert_eq!(
                life.death_task(&mut p, &c, false, &mut lives).unwrap(),
                DeathTick::Wait
            );
        }
        assert_eq!(lives, 3);
        assert_eq!(
            life.death_task(&mut p, &c, false, &mut lives).unwrap(),
            DeathTick::RestartStage
        );
        assert_eq!(lives, 2);
        life.checkpoint = Checkpoint {
            x: 1200,
            y: 500,
            time: 800,
        };
        let camera = life.reinitialize_player(&mut p, 2048, 1024).unwrap();
        assert_eq!((p.q_world_x, p.q_world_y), (1200 * 256, 500 * 256));
        assert_eq!(life.course_time, 800);
        assert_eq!(camera.x, 1080);
        assert_eq!(life.stage_flags & ACT_START, ACT_START);
    }

    #[test]
    fn timeout_is_strict_and_ends_run_even_with_spare_lives() {
        let (mut p, c, mut life) = setup();
        p.move_state = 0;
        life.course_time = MAX_COURSE_TIME - 1;
        assert!(!life.tick_stage_clock(&mut p));
        assert!(life.tick_stage_clock(&mut p));
        life.phase = Phase::Waiting { frames: 0 };
        let mut lives = 3;
        assert_eq!(
            life.death_task(&mut p, &c, false, &mut lives).unwrap(),
            DeathTick::GameOver {
                cause: GameOverCause::TimeUp,
                clear_rings: false
            }
        );
        assert_eq!(lives, 2);
    }

    #[test]
    fn checkpoint_requires_native_entity_crossing_and_invalid_bounds_do_not_mutate() {
        let (mut p, mut c, mut life) = setup();
        p.move_state = 0;
        life.checkpoint_time = 99;
        assert!(!life.cross_checkpoint(&p, 97, [1200, 500]));
        assert!(life.cross_checkpoint(&p, 96, [1200, 500]));
        assert_eq!(life.checkpoint.time, 99);
        c.min_x = c.max_x;
        let before = p;
        assert!(normal_bounds(&mut p, &c, false).is_err());
        assert_eq!(p, before);
    }
}
