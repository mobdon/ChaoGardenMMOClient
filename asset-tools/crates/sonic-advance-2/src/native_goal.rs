//! Native goal-toggle decisions, original lever presentation and results logic.
//! Physical player integration, fade/music completion and destination scenes
//! remain explicit host bindings. No progression here is an MMO reward.

use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_campaign_save::CampaignProfile,
    native_entities::StageEntities,
    native_player::{self as player, PlayerState},
};
use serde::Serialize;

pub const GOAL_REACHED: u32 = 0x0800_0000;
pub const GOAL_TRANSITION: u8 = 10;
pub const GOAL_LEVER_ANIMATION: u16 = 532;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreState {
    pub level_score: i32,
    pub lives: u8,
    pub music_request: u8,
}
impl ScoreState {
    /// INCREMENT_SCORE[_A]: native signed division and u16 life narrowing.
    pub fn award(&mut self, points: u32, game_mode: u8, restart_music: bool) {
        let old = self.level_score;
        self.level_score = old.wrapping_add(points as i32);
        let before = old / 50_000;
        let after = self.level_score / 50_000;
        if before != after && game_mode == 0 {
            let lives = (after.wrapping_sub(before) as u16).wrapping_add(u16::from(self.lives));
            self.lives = lives.min(255) as u8;
            if restart_music {
                self.music_request = 0x10;
            }
        }
    }
}

pub fn crossing_speed_bonus(q_ground_speed: i16, in_air: bool, game_mode: u8) -> u32 {
    if game_mode != 0 || in_air || q_ground_speed <= 640 {
        return 0;
    }
    if q_ground_speed <= 1024 {
        200
    } else if q_ground_speed <= 2304 {
        300
    } else if q_ground_speed <= 2560 {
        500
    } else {
        800
    }
}
pub fn braking_distance_bonus(player_x: i32, goal_x: i32) -> u32 {
    if goal_x == 0 {
        return 0;
    }
    let distance = player_x.wrapping_sub(goal_x) as u16;
    if distance <= 730 {
        800
    } else if distance <= 1114 {
        500
    } else if distance <= 1401 {
        100
    } else {
        0
    }
}

/// Native sub_8029FA4 normal-gravity effect request. The caller must render the
/// original braking-dust asset; this function does not replace that effect.
pub fn braking_dust_position(p: &PlayerState, stage_time: u32) -> Option<(i32, i32)> {
    let mask = if p.move_state & GOAL_REACHED != 0 {
        7
    } else {
        3
    };
    (stage_time & mask == 0).then(|| {
        (
            p.q_world_x >> 8,
            (p.q_world_y >> 8).wrapping_add(i32::from(p.sprite_offset_y)),
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalTrigger {
    pub x: i32,
    pub transition: u8,
    pub stage_flags_or: u16,
    pub bonus: u32,
}

/// Called only for an already spawned native toggle task. Its Y coordinate and
/// alive state are intentionally not additional finish conditions.
pub fn toggle_crossing(
    goal_x: i32,
    p: &PlayerState,
    game_mode: u8,
) -> Result<Option<GoalTrigger>, Error> {
    if game_mode > 2 {
        return Err(Error(
            "Multiplayer goal synchronization is not ported".into(),
        ));
    }
    if goal_x > p.q_world_x >> 8 || p.move_state & GOAL_REACHED != 0 {
        return Ok(None);
    }
    Ok(Some(GoalTrigger {
        x: goal_x,
        transition: GOAL_TRANSITION,
        stage_flags_or: 0x21,
        bonus: crossing_speed_bonus(
            p.q_speed_ground,
            p.move_state & player::IN_AIR != 0,
            game_mode,
        ),
    }))
}

struct GoalTask<'a> {
    index: usize,
    x: i32,
    y: i32,
    lever: Option<AnimationPlayer<'a>>,
    animating: bool,
}
pub struct StageGoals<'a> {
    rom: &'a VerifiedRom<'a>,
    tasks: Vec<GoalTask<'a>>,
}
impl<'a> StageGoals<'a> {
    pub fn new(rom: &'a VerifiedRom<'a>) -> Self {
        Self {
            rom,
            tasks: Vec::new(),
        }
    }
    /// Dispatch priority0x2010 after the entity scan, before player motion.
    /// The host establishes whether newly admitted tasks are eligible this tick.
    #[allow(clippy::too_many_arguments)] // Stable host boundary mirrors the separately owned native services.
    pub fn tick(
        &mut self,
        entities: &mut StageEntities,
        p: &PlayerState,
        player_hitbox: Hitbox,
        camera_x: i32,
        camera_y: i32,
        game_mode: u8,
        dispatch_new: bool,
        score: &mut ScoreState,
    ) -> Result<Vec<GoalTrigger>, Error> {
        if game_mode > 2 {
            return Err(Error(
                "Multiplayer goal synchronization is not ported".into(),
            ));
        }
        let previous = self.tasks.len();
        for index in entities.take_new_goals() {
            if self.tasks.len() == 128 {
                return Err(Error("Native goal task budget exhausted".into()));
            }
            let e = entities.placements()[index];
            let lever = if e.kind == 42 {
                let mut s = AnimationPlayer::new(self.rom, GOAL_LEVER_ANIMATION, 1)?;
                s.advance()?;
                Some(s)
            } else {
                None
            };
            self.tasks.push(GoalTask {
                index,
                x: e.x,
                y: e.y,
                lever,
                animating: false,
            });
        }
        let dispatch_count = if dispatch_new {
            self.tasks.len()
        } else {
            previous
        };
        let mut events = Vec::new();
        let mut remove = Vec::new();
        for (index, task) in self.tasks.iter_mut().take(dispatch_count).enumerate() {
            let sx = task.x.wrapping_sub(camera_x);
            let sy = task.y.wrapping_sub(camera_y);
            if let Some(lever) = &mut task.lever {
                let sx = sx as i16;
                let sy = sy as i16;
                if (i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy) {
                    remove.push(index);
                    continue;
                }
                if task.animating {
                    lever.advance()?;
                } else if p.move_state & (0x80 | 0x00400000) == 0 {
                    let hitbox = lever.hitboxes()[0];
                    let [left, top, right, bottom] = hitbox.rectangle.map(i32::from);
                    let px = p.q_world_x >> 8;
                    let py = p.q_world_y >> 8;
                    let [pl, pt, pr, pb] = player_hitbox.rectangle.map(i32::from);
                    if hitbox.index != -1
                        && player_hitbox.index != -1
                        && px + pr >= task.x + left
                        && px + pl <= task.x + right
                        && py + pb >= task.y + top
                        && py + pt <= task.y + bottom
                    {
                        lever.set_animation(GOAL_LEVER_ANIMATION, 0)?;
                        task.animating = true;
                    }
                }
            } else {
                if let Some(trigger) = toggle_crossing(task.x, p, game_mode)? {
                    score.award(trigger.bonus, game_mode, true);
                    events.push(trigger);
                }
                // Unlike the lever, the toggle checks crossing before despawn.
                if sx.wrapping_add(128) as u32 > 496 || !(-128..=288).contains(&sy) {
                    remove.push(index);
                }
            }
        }
        for index in remove.into_iter().rev() {
            let task = self.tasks.remove(index);
            entities.restore_goal(task.index);
        }
        Ok(events)
    }
    /// Native lever OAM order4/BG priority1; the host draws these above ordinary
    /// priority2 actors. A newly selected variant shows its old frame until the
    /// next native animation update, matching Task_StageGoalMain.
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.tasks
            .iter()
            .filter_map(|task| task.lever.as_ref().map(|s| (task.x, task.y, s)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GoalPhase {
    Init,
    WaitingForGround,
    Slowdown,
    Brake,
    Victory,
    CameraShift,
    SpecialTransition,
    SpecialExit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GoalController {
    pub phase: GoalPhase,
    pub victory_timer: i16,
    pub goal_x: i32,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalPlayerStep {
    pub continue_existing_callback: bool,
    pub advance_ground: bool,
    pub begin_results: bool,
    pub special_transition: bool,
    pub bonus: u32,
    pub start_long_brake: bool,
    pub stop_long_brake: bool,
    pub stop_water_sound: bool,
    pub check_braking_dust: bool,
}

fn shift_camera(shift_y: &mut i32) {
    if *shift_y > -56 {
        *shift_y -= 1;
    }
}
impl GoalController {
    pub fn reached_transition(p: &mut PlayerState, goal_x: i32) -> Self {
        let deferred = p.move_state & 0x3000200a != 0;
        p.move_state |= GOAL_REACHED;
        if deferred {
            p.move_state |= player::IGNORE_INPUT;
            p.held_input = 0;
            p.frame_input = 0;
        }
        Self {
            phase: if deferred {
                GoalPhase::WaitingForGround
            } else {
                GoalPhase::Init
            },
            victory_timer: 0,
            goal_x,
        }
    }
    fn victory(&mut self, p: &mut PlayerState, level: u8, shift_y: &mut i32) {
        shift_camera(shift_y);
        self.victory_timer = 90;
        p.char_state = if level & 3 == 1 { 29 } else { 28 };
        self.phase = GoalPhase::Victory;
    }
    /// Normal-gravity Sonic ordinary-course callbacks through the ground-motion
    /// service boundary. AdvanceGround requests native convert/position/follow/
    /// control-lock processing, without ordinary input or slope acceleration.
    pub fn step(
        &mut self,
        p: &mut PlayerState,
        level: u8,
        special_rings: u8,
        animation: u16,
        variant: u8,
        shift_y: &mut i32,
    ) -> Result<GoalPlayerStep, Error> {
        if p.character != player::Character::Sonic || level >= 28 || level & 3 > 1 {
            return Err(Error(
                "Goal player controller supports Sonic ordinary course acts".into(),
            ));
        }
        let mut output = GoalPlayerStep::default();
        loop {
            match self.phase {
                GoalPhase::WaitingForGround => {
                    if p.move_state & (player::IN_AIR | player::STOOD_ON_OBJECT) != 0 {
                        output.continue_existing_callback = true;
                        return Ok(output);
                    }
                    self.phase = GoalPhase::Init;
                }
                GoalPhase::Init => {
                    output.stop_water_sound = p.move_state & 0x20000 != 0;
                    p.move_state &= !0x330ee730;
                    p.unk61 = 0;
                    p.unk62 = 0;
                    p.unk63 = 0;
                    p.unk70 = false;
                    p.unk71 = 0;
                    p.move_state &= !(player::SPIN_ATTACK | player::FACING_LEFT);
                    p.change_sprite_offsets(6, 14, false);
                    if p.move_state & GOAL_REACHED != 0 && special_rings >= 7 {
                        p.is_boosting = false;
                        p.char_state = player::WALK;
                        self.victory_timer = 0;
                        self.phase = GoalPhase::SpecialTransition;
                        continue;
                    }
                    if p.q_speed_ground <= 0 {
                        self.victory(p, level, shift_y);
                        return Ok(output);
                    }
                    p.char_state = player::WALK;
                    p.sprite_frame_flags &= !player::ANIMATION_OVER;
                    p.move_state &= !player::IGNORE_INPUT;
                    self.phase = GoalPhase::Slowdown;
                }
                GoalPhase::Slowdown => {
                    let distance = (p.q_world_x >> 8).wrapping_sub(self.goal_x) as u16;
                    if (p.q_speed_ground >= 512 && p.frame_input & player::LEFT != 0)
                        || distance > 1401
                    {
                        p.char_state = 25;
                        p.sprite_frame_flags &= !player::ANIMATION_OVER;
                        output.start_long_brake = true;
                        self.phase = GoalPhase::Brake;
                    } else {
                        p.q_speed_ground = if p.q_speed_ground > 0 {
                            p.q_speed_ground.wrapping_sub(8).max(0)
                        } else {
                            0
                        };
                        if p.q_speed_ground <= 0 {
                            self.victory(p, level, shift_y);
                            return Ok(output);
                        }
                        shift_camera(shift_y);
                        output.advance_ground = true;
                        return Ok(output);
                    }
                }
                GoalPhase::Brake => {
                    shift_camera(shift_y);
                    if animation == 31 {
                        if variant == 0 && p.sprite_frame_flags & player::ANIMATION_OVER != 0 {
                            p.char_state = 26;
                        }
                        if variant == 1 && p.q_speed_ground <= 0 {
                            p.char_state = 27;
                            p.q_speed_ground = 0;
                            output.stop_long_brake = true;
                            output.bonus = braking_distance_bonus(p.q_world_x >> 8, self.goal_x);
                        }
                        if variant == 2 && p.sprite_frame_flags & player::ANIMATION_OVER != 0 {
                            self.victory(p, level, shift_y);
                            return Ok(output);
                        }
                    }
                    p.q_speed_ground = p.q_speed_ground.wrapping_sub(32).max(0);
                    output.check_braking_dust = true;
                    output.advance_ground = true;
                    return Ok(output);
                }
                GoalPhase::Victory => {
                    shift_camera(shift_y);
                    self.victory_timer = self.victory_timer.wrapping_sub(1);
                    if self.victory_timer == 0 {
                        output.begin_results = true;
                        self.phase = GoalPhase::CameraShift;
                    }
                    output.advance_ground = true;
                    return Ok(output);
                }
                GoalPhase::CameraShift => {
                    shift_camera(shift_y);
                    return Ok(output);
                }
                GoalPhase::SpecialTransition => {
                    p.q_speed_ground = if p.q_speed_ground > 1408 {
                        p.q_speed_ground.wrapping_sub(128)
                    } else if p.q_speed_air_x < 1216 {
                        p.q_speed_ground.wrapping_add(64)
                    } else {
                        1280
                    };
                    self.victory_timer = self.victory_timer.saturating_add(1);
                    if self.victory_timer == 120 {
                        p.char_state = 32;
                    }
                    output.begin_results = self.victory_timer == 180;
                    output.special_transition = true;
                    output.advance_ground = true;
                    return Ok(output);
                }
                GoalPhase::SpecialExit => {
                    output.special_transition = true;
                    output.advance_ground = true;
                    return Ok(output);
                }
            }
        }
    }
}

/// The special-exit callbacks perform their camera/boost writes after ground motion.
impl GoalController {
    pub fn finish_special_motion(
        &mut self,
        p: &mut PlayerState,
        camera: &mut crate::native_camera::NativeCamera,
    ) -> bool {
        match self.phase {
            GoalPhase::SpecialTransition => {
                shift_camera(&mut camera.shift_y);
                if p.move_state & 0x04000000 != 0 {
                    p.is_boosting = true;
                    p.held_input = player::RIGHT;
                    p.q_speed_ground = 2560;
                    p.char_state = player::WALK;
                    self.phase = GoalPhase::SpecialExit;
                    return true;
                }
            }
            GoalPhase::SpecialExit => {
                camera.q_max_step_x = camera.q_max_step_x.wrapping_sub(56);
            }
            _ => {}
        }
        false
    }
}
/// Ordinary single-player sub_80232D0 special gate, before native bounds.
pub fn special_course_wrap(
    rom: &VerifiedRom,
    level: u8,
    p: &mut PlayerState,
    c: &mut crate::native_camera::NativeCamera,
) -> Result<(), Error> {
    if !crate::native_stage::is_course_act(level) {
        return Err(Error("Special course wrap requires an ordinary act".into()));
    }
    let pair = |table| -> Result<[i32; 2], Error> {
        let b = rom.range("rom", table + usize::from(level) * 8, 8)?;
        Ok([
            i32::from_le_bytes(b[..4].try_into().unwrap()),
            i32::from_le_bytes(b[4..].try_into().unwrap()),
        ])
    };
    let [tx, ty] = pair(0xd650c)?;
    let [dx, dy] = pair(0xd661c)?;
    if ty != -1 || dy != 0 {
        return Err(Error("Unexpected vertical wrap in ordinary course".into()));
    }
    if tx >= 0
        && p.q_world_x >= tx.wrapping_mul(256)
        && c.q_max_step_x != 0
        && c.axes_locked & 1 == 0
    {
        p.q_world_x = p.q_world_x.wrapping_add(dx.wrapping_mul(256));
        c.x = c.x.wrapping_add(dx);
        c.target_x = c.target_x.wrapping_add(dx);
    }
    Ok(())
}

pub fn time_bonus(course_time: u32) -> u32 {
    for (limit, bonus) in [
        (1800, 80000),
        (3000, 50000),
        (3600, 10000),
        (5400, 5000),
        (7200, 4000),
        (10800, 3000),
        (14400, 2000),
        (18000, 1000),
        (21600, 500),
    ] {
        if course_time < limit {
            return bonus;
        }
    }
    0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsContext {
    pub level: u8,
    pub character: u8,
    pub game_mode: u8,
    pub move_state: u32,
    pub special_rings: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ResultsDestination {
    NextStage(u8),
    SpecialStage,
    CourseSelect { level: u8, unlocked: u8, mode: u8 },
    CharacterUnlock(u8),
    Ending,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ResultsSound {
    Counter,
    CounterDone,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsStep {
    pub sound: Option<ResultsSound>,
    pub hide_graphics: bool,
    pub destination: Option<ResultsDestination>,
    pub request_save: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageResults {
    pub time_bonus: u32,
    pub ring_bonus: u32,
    pub special_bonus: u32,
    pub counter: u32,
    pub count_frames: u32,
    pub counting_done: bool,
    pub graphics_hidden: bool,
    pub complete: bool,
    level: u8,
    character: u8,
}
impl StageResults {
    pub fn begin(
        context: ResultsContext,
        course_time: u32,
        ring_count: u16,
        global_ring_count: u16,
        profile: &mut CampaignProfile,
    ) -> Result<Self, Error> {
        if context.character >= 5
            || context.level > 29
            || context.level & 3 == 3
            || context.game_mode > 2
        {
            return Err(Error("Unsupported native results context".into()));
        }
        profile.score = profile
            .score
            .wrapping_add(i32::from(global_ring_count as i16) as u32);
        let time_bonus = time_bonus(course_time);
        let ring_bonus = u32::from(ring_count) * 100;
        let special_bonus = if context.special_rings == 7 {
            10000
        } else {
            u32::from(context.special_rings) * 1000
        };
        Ok(Self {
            time_bonus,
            ring_bonus,
            special_bonus,
            counter: 0,
            count_frames: time_bonus.max(ring_bonus).max(special_bonus) / 100,
            counting_done: false,
            graphics_hidden: false,
            complete: false,
            level: context.level,
            character: context.character,
        })
    }
    /// Advance the original score/countdown decisions. Fade and music readiness
    /// are explicit service results; passing true does not implement that UI.
    #[allow(clippy::too_many_arguments)] // Fade/audio readiness remain explicit native service results.
    pub fn tick(
        &mut self,
        context: &mut ResultsContext,
        profile: &mut CampaignProfile,
        score: &mut ScoreState,
        pressed_keys: u16,
        stage_time: u32,
        fade_complete: bool,
        bgm_finished: bool,
    ) -> Result<ResultsStep, Error> {
        let mut output = ResultsStep::default();
        if self.complete {
            return Ok(output);
        }
        if context.level != self.level
            || context.character != self.character
            || context.game_mode > 2
        {
            return Err(Error(
                "Native results context changed before task completion".into(),
            ));
        }
        self.counter = self.counter.wrapping_add(1).min(self.count_frames + 310);
        if self.counter >= 150 {
            for bonus in [
                &mut self.ring_bonus,
                &mut self.special_bonus,
                &mut self.time_bonus,
            ] {
                if *bonus != 0 {
                    *bonus = bonus.wrapping_sub(100);
                    score.award(100, context.game_mode, false);
                }
            }
            if !matches!(context.level, 28 | 29) && pressed_keys & 1 != 0 {
                for bonus in [
                    &mut self.ring_bonus,
                    &mut self.special_bonus,
                    &mut self.time_bonus,
                ] {
                    score.award(*bonus, context.game_mode, false);
                    *bonus = 0;
                }
                self.counter = self.counter.max(self.count_frames + 149);
            }
            if stage_time.is_multiple_of(4) {
                if self.ring_bonus | self.special_bonus | self.time_bonus != 0 {
                    output.sound = Some(ResultsSound::Counter);
                } else if !self.counting_done {
                    self.counting_done = true;
                    output.sound = Some(ResultsSound::CounterDone);
                }
            }
        }
        if self.counter > self.count_frames + 309 {
            if context.level == 28 && bgm_finished {
                profile.unlocked_levels[usize::from(context.character)] = 29;
                output.destination = Some(ResultsDestination::Ending);
            } else if context.level == 29 && bgm_finished {
                context.level += 1;
                profile.unlocked_levels[usize::from(context.character)] = context.level;
                output.destination = Some(ResultsDestination::Ending);
            } else if !matches!(context.level, 28 | 29) && fade_complete {
                context.move_state |= 0x100000;
                let old = context.level;
                context.level += 1;
                let unlocked = &mut profile.unlocked_levels[usize::from(context.character)];
                let new = context.level > *unlocked;
                if new {
                    *unlocked = context.level;
                }
                output.destination = Some(if old & 3 == 2 {
                    if new && context.character == 0 {
                        match old >> 2 {
                            0 => {
                                profile.unlocked_characters |= 2;
                                ResultsDestination::CharacterUnlock(0)
                            }
                            2 => {
                                profile.unlocked_characters |= 4;
                                ResultsDestination::CharacterUnlock(2)
                            }
                            4 => {
                                profile.unlocked_characters |= 8;
                                ResultsDestination::CharacterUnlock(1)
                            }
                            _ => ResultsDestination::CourseSelect {
                                level: context.level,
                                unlocked: *unlocked,
                                mode: 1,
                            },
                        }
                    } else {
                        ResultsDestination::CourseSelect {
                            level: context.level,
                            unlocked: *unlocked,
                            mode: if new { 1 } else { 4 },
                        }
                    }
                } else if context.move_state & GOAL_REACHED != 0 && context.special_rings >= 7 {
                    ResultsDestination::SpecialStage
                } else {
                    ResultsDestination::NextStage(context.level)
                });
            }
            if output.destination.is_some() {
                self.complete = true;
                output.request_save = true;
                return Ok(output);
            }
            if context.move_state & GOAL_REACHED != 0 && context.special_rings >= 7 {
                output.hide_graphics = !self.graphics_hidden;
                self.graphics_hidden = true;
                context.move_state |= 0x4000000;
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_bonus_edges_and_signed_score_narrowing_are_preserved() {
        for (time, bonus) in [(1799, 80000), (1800, 50000), (3000, 10000), (21600, 0)] {
            assert_eq!(time_bonus(time), bonus);
        }
        for (speed, bonus) in [
            (640, 0),
            (641, 200),
            (1024, 200),
            (1025, 300),
            (2304, 300),
            (2305, 500),
            (2561, 800),
        ] {
            assert_eq!(crossing_speed_bonus(speed, false, 0), bonus);
        }
        let mut score = ScoreState {
            level_score: 49999,
            lives: 254,
            music_request: 0,
        };
        score.award(100001, 0, true);
        assert_eq!(
            (score.level_score, score.lives, score.music_request),
            (150000, 255, 16)
        );
    }
    #[test]
    fn results_skip_waits_for_native_countdown_and_progresses_once() {
        let mut p = CampaignProfile::new_usa();
        let mut score = ScoreState::default();
        let mut context = ResultsContext {
            level: 0,
            character: 0,
            game_mode: 0,
            move_state: GOAL_REACHED,
            special_rings: 0,
        };
        let mut result = StageResults::begin(context, 10000, 10, 10, &mut p).unwrap();
        assert_eq!(p.score, 10);
        for frame in 0..149 {
            assert!(
                result
                    .tick(&mut context, &mut p, &mut score, 1, frame, true, true)
                    .unwrap()
                    .destination
                    .is_none()
            );
        }
        result
            .tick(&mut context, &mut p, &mut score, 1, 149, true, true)
            .unwrap();
        assert_eq!(
            (result.time_bonus, result.ring_bonus, result.counter),
            (0, 0, result.count_frames + 149)
        );
        let mut destination = None;
        for frame in 150..400 {
            let step = result
                .tick(&mut context, &mut p, &mut score, 0, frame, true, true)
                .unwrap();
            if step.destination.is_some() {
                destination = step.destination;
                break;
            }
        }
        assert_eq!(destination, Some(ResultsDestination::NextStage(1)));
        assert_eq!(p.unlocked_levels[0], 1);
        assert!(
            result
                .tick(&mut context, &mut p, &mut score, 0, 400, true, true)
                .unwrap()
                .destination
                .is_none()
        );
    }
}
