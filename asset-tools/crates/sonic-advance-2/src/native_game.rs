//! Local execution harness for the first normal-gravity Sonic movement slice.
//!
//! Native arithmetic, sensors, animation scripts and pixels come from the
//! verified source. Launch/countdown bypass and frame transport are host
//! bindings. Some objects, enemies, stage events, audio mixing and the complete
//! retail callback graph are unfinished. This has no MMO service connection.
use crate::{
    Error, VerifiedRom, gba,
    native_animation::{AffineTransform, AnimationPlayer, SpriteFrame},
    native_camera::NativeCamera,
    native_bouncy_bar::{self as bar,StageBouncyBars,BarEvent},
    native_campaign_save::{CampaignSaveSession, MemoryFlashFaults, SECTOR_BYTES},
    native_checkpoints::StageCheckpoints,
    native_collision,
    native_combat::{self as combat, HomingTarget},
    native_enemies::{EnemyContext, EnemyEvent, StageEnemies},
    native_entities::StageEntities,
    native_goal::{
        GoalController, GoalPhase, ResultsContext, ResultsDestination, ResultsSound, ScoreState,
        StageGoals, StageResults,
    },
    native_hud::StageHud,
    native_lifecycle::{self, DeathTick, GameOverCause, NativeLifecycle, Phase},
    native_player::{self as player, Callback, Character, PlayerState},
    native_results_ui::ResultsUi,
    native_rings::{RingPosition, StageRings, increment_single_player, stage_ring_palette},
    native_runtime::{A_BUTTON, InputFrame, InputState, NativeRng},
    native_scatter::{ScatterFrame, StageScatter},
    native_sonic::{self, AirAction, SonicInputBuffer},
    native_special_rings::StageSpecialRings,
    native_springs::StageSprings,
    native_stage::{self, NativeStage},
};
use crate::native_spin::{self as spin, DownAction,SpinDash};
use crate::native_runtime::DPAD_DOWN;
use serde::Serialize;

pub const VIEW_WIDTH: usize = 240;
pub const VIEW_HEIGHT: usize = 160;
pub const MAX_REPLAY_FRAMES: usize = 36_000;
pub const SUPPORTED_INPUT: u16 = A_BUTTON | player::LEFT | player::RIGHT | DPAD_DOWN;
const SINE_SHA256: &str = "78caf4fc1cb643a1f24d8e9d9efb1822cda68335adc1799d517582939d9c5c99";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum HarnessCallback {
    Ground,
    InitJump,
    Jumping,
    Uncurl,
    InitUncurl,
    Spring,
    Hurt,
    Homing,
    InitCrouch,
    Crouch,
    Spindash,
    InitRoll,
    Rolling,
    InitSkid,
    Turning,
    TouchGround,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessSnapshot {
    pub frame: u64,
    pub callback: HarnessCallback,
    pub player: PlayerState,
    pub input: InputFrame,
    pub animation: u16,
    pub variant: u8,
    pub animation_frame: Option<u16>,
    pub level_id: u8,
    pub lifecycle: NativeLifecycle,
    pub game_over: Option<GameOverCause>,
    pub camera: [i32; 2],
    pub rings: u16,
    pub special_rings: u8,
    pub layer_contacts: usize,
    pub lives: u8,
    pub goal_phase: Option<GoalPhase>,
    pub results: Option<StageResults>,
    pub last_destination: Option<ResultsDestination>,
    pub unlocked_level: u8,
    pub score: u32,
    pub enemy_events: Vec<EnemyEvent>,
    pub ring_sound_pans: Vec<i8>,
    pub animation_sound_ids: Vec<u16>,
    pub native_effect_requests: Vec<&'static str>,
    pub halted: Option<String>,
}

struct Pickup<'a> {
    position: RingPosition,
    sprite: AnimationPlayer<'a>,
    finished: bool,
}

pub struct NativeHarness<'a> {
    rom: &'a VerifiedRom<'a>,
    stage: NativeStage<'a>,
    camera: NativeCamera,
    hud: StageHud,
    lifecycle: NativeLifecycle,
    game_over: Option<GameOverCause>,
    displayed_time: (u32, u64),
    stage_ticks: u64,
    rings: StageRings,
    entities: StageEntities,
    bars:StageBouncyBars<'a>,
    springs: StageSprings<'a>,
    enemies: StageEnemies<'a>,
    special: StageSpecialRings<'a>,
    goals: StageGoals<'a>,
    goal: Option<GoalController>,
    results: Option<StageResults>,
    results_ui: Option<ResultsUi>,
    last_destination: Option<ResultsDestination>,
    campaign: CampaignSaveSession,
    save_rng: NativeRng,
    scatter: StageScatter<'a>,
    pseudo_rng: u32,
    score: u32,
    enemy_events: Vec<EnemyEvent>,
    queued_transition: Option<u8>,
    target: HomingTarget,
    homing_frames: i16,
    show_player: bool,
    displayed_counts: (u16, u8, u32),
    checkpoints: StageCheckpoints<'a>,
    layer_contacts: usize,
    ring_count: u16,
    lives: u8,
    ring_sprite: AnimationPlayer<'a>,
    ring_palette: [u16; 16],
    pickups: Vec<Pickup<'a>>,
    ring_sound_pans: Vec<i8>,
    player: PlayerState,
    input: InputState,
    sonic_inputs: SonicInputBuffer,
    spin:SpinDash,
    animation: AnimationPlayer<'a>,
    sine: &'a [u8],
    callback: HarnessCallback,
    previous_char_state: i16,
    variant: u8,
    frame: u64,
    sound_ids: Vec<u16>,
    animation_frame: Option<u16>,
    effect_requests: Vec<&'static str>,
    halted: Option<String>,
}

impl<'a> NativeHarness<'a> {
    pub fn new(rom: &'a VerifiedRom<'_>) -> Result<Self, Error> {
        Self::for_level(rom, 0)
    }

    pub fn for_level(rom: &'a VerifiedRom<'a>, level_id: u8) -> Result<Self, Error> {
        if !native_stage::is_course_act(level_id) {
            return Err(Error(
                "The movement runner supports course acts 1 and 2 in zones 1 through 7".into(),
            ));
        }
        let stage = NativeStage::for_level(rom, level_id)?;
        let sine = rom.range("rom", 0x97aa4, 0xa00)?;
        if gba::sha256(sine) != SINE_SHA256 {
            return Err(Error("Native sine table fingerprint mismatch".into()));
        }
        let mut p = PlayerState::new_at(Character::Sonic, stage.spawn.0 * 256, stage.spawn.1 * 256);
        // Explicit host launch: the retail intro/countdown task is not present.
        p.move_state &= !player::IGNORE_INPUT;
        p.handle_input(0);
        let mut animation = AnimationPlayer::new(rom, 0, 0)?;
        animation.set_flip(true, false);
        let initial_frame = animation.advance()?.frame_index;
        Ok(Self {
            rom,
            hud: StageHud::new(rom)?,
            lifecycle: NativeLifecycle::new([stage.spawn.0 as i16, stage.spawn.1 as i16], false),
            game_over: None,
            displayed_time: (0, 0),
            stage_ticks: 0,
            camera: NativeCamera::normal_stage(&p, stage.dimensions().0, stage.dimensions().1)?,
            stage,
            rings: StageRings::load(rom, level_id)?,
            checkpoints: StageCheckpoints::new(rom, level_id)?,
            special: StageSpecialRings::new(rom),
            goals: StageGoals::new(rom),
            goal: None,
            results: None,
            results_ui: None,
            last_destination: None,
            campaign: CampaignSaveSession::new(&vec![0xff; 16 * SECTOR_BYTES], true)?,
            save_rng: NativeRng::seeded(0, 0),
            scatter: StageScatter::new(rom)?,
            pseudo_rng: 0,
            enemies: StageEnemies::load(rom, level_id)?,
            enemy_events: Vec::new(),
            queued_transition: None,
            score: 0,
            target: HomingTarget::default(),
            homing_frames: 0,
            show_player: true,
            displayed_counts: (0, 3, 0),
            bars:StageBouncyBars::new(rom),
            springs: StageSprings::new(rom, level_id),
            entities: StageEntities::load(rom, level_id)?,
            layer_contacts: 0,
            ring_count: 0,
            lives: 3,
            ring_sprite: AnimationPlayer::new(rom, 726, 0)?,
            ring_palette: stage_ring_palette(rom)?,
            pickups: Vec::new(),
            ring_sound_pans: Vec::new(),
            player: p,
            input: InputState::default(),
            sonic_inputs: SonicInputBuffer::from_rom(rom)?,
            spin:SpinDash::new(rom)?,
            animation,
            sine,
            callback: HarnessCallback::Ground,
            previous_char_state: player::IDLE,
            variant: 0,
            frame: 0,
            sound_ids: Vec::new(),
            animation_frame: initial_frame,
            effect_requests: Vec::new(),
            halted: None,
        })
    }

    fn trig(&self, rotation: u8) -> (i32, i32) {
        self.trig1024(usize::from(rotation) * 4)
    }

    fn trig1024(&self, index: usize) -> (i32, i32) {
        let word = |index: usize| {
            i32::from(i16::from_le_bytes([
                self.sine[index * 2],
                self.sine[index * 2 + 1],
            ])) >> 6
        };
        (word(index), word(index + 256))
    }

    /// Advance exactly one 1/60-second simulation frame. The caller supplies
    /// only a physical input word; no positions, rewards or elapsed gains.
    pub fn tick(&mut self, held: u16) -> Result<HarnessSnapshot, Error> {
        if held & !SUPPORTED_INPUT != 0 {
            return Err(Error(
                "Supported buttons are left, right, down and A/jump".into(),
            ));
        }
        if self.halted.is_some() || self.game_over.is_some() {
            return Ok(self.snapshot());
        }
        self.frame += 1;
        self.stage_ticks += 1;
        self.sound_ids.clear();
        self.effect_requests.clear();
        self.enemy_events.clear();
        self.input.poll_held(held);
        self.camera.tick_task(&self.player, false)?;
        self.tick_rings()?;
        let [cx, cy] = self.camera.position();
        self.enemies.admit(&self.stage, cx, cy)?;
        self.layer_contacts = self.entities.update_layers(&mut self.player, cx, cy);
        let scatter = self.scatter.tick(
            &self.stage,
            &self.player,
            self.animation.hitboxes()[0],
            [cx, cy],
            ScatterFrame {
                stage_time: self.lifecycle.stage_time,
                timer_invulnerability: self.lifecycle.timer_invulnerability,
                ..Default::default()
            },
        )?;
        for pos in scatter {
            self.pickup_effect(pos)?;
            self.increment_rings();
        }
        self.sound_ids.extend(self.checkpoints.tick(
            &mut self.entities,
            &self.player,
            cx,
            cy,
            &mut self.lifecycle,
        )?);
        if let Some((bank, colors)) = self.checkpoints.background_palette() {
            self.stage.set_background_bank(bank, colors)?;
        }
        let launches = self
            .springs
            .tick(&mut self.entities, &mut self.player, cx, cy)?;
        self.sound_ids.extend(launches.iter().map(|s| s.sound_id));
        for index in self.entities.take_new_bars(){self.bars.admit(index,self.entities.placements()[index])?;}
        let mut bar_uncurl=false;
        for event in self.bars.tick(&mut self.player,self.animation.hitboxes()[0],[cx,cy])?{
            match event{
                BarEvent::Transition(bar::UNCURL_TRANSITION)=>bar_uncurl=true,
                BarEvent::Transition(id)=>return Err(Error(format!("Unsupported bar transition {id}"))),
                BarEvent::Sound(id)=>self.sound_ids.push(id),
                BarEvent::Restore(index)=>self.entities.restore_bar(index),
            }
        }
        let mut score = self.score_state();
        let triggers = self.goals.tick(
            &mut self.entities,
            &self.player,
            self.animation.hitboxes()[0],
            cx,
            cy,
            0,
            true,
            &mut score,
        )?;
        self.accept_score(score);
        for t in &triggers {
            self.lifecycle.stage_flags |= t.stage_flags_or;
        }
        self.displayed_counts = (self.ring_count, self.lives, self.score);
        self.displayed_time = (
            self.lifecycle.course_time,
            u64::from(self.lifecycle.stage_time),
        );
        self.hud.set_special_count(self.special.count);
        self.hud.tick(
            self.player.q_speed_air_x,
            self.displayed_time.0,
            self.displayed_time.1,
        );
        if !self.lifecycle.time_limit_disabled
            && self.lifecycle.course_time >= 34800
            && self.lifecycle.course_time.is_multiple_of(60)
        {
            self.sound_ids.push(139);
        }
        if self.lifecycle.phase != Phase::Alive {
            match self.lifecycle.death_task(
                &mut self.player,
                &self.camera,
                false,
                &mut self.lives,
            )? {
                DeathTick::Animate => self.update_animation()?,
                DeathTick::Wait => {}
                DeathTick::RestartStage => {
                    self.restart_stage()?;
                    return Ok(self.snapshot());
                }
                DeathTick::GameOver { cause, clear_rings } => {
                    self.game_over = Some(cause);
                    if clear_rings {
                        self.ring_count = 0;
                    }
                    return Ok(self.snapshot());
                }
            }
            self.run_enemies(cx, cy)?;
            self.sound_ids.extend(self.special.tick(
                &mut self.entities,
                &self.player,
                cx,
                cy,
                None,
            )?);
            self.lifecycle.tick_stage_clock(&mut self.player);
            return Ok(self.snapshot());
        }
        if self.player.update_frame_physics(self.ring_count) {
            self.sound_ids.push(221);
            self.camera.on_boost_activation();
            self.effect_requests.push("SE_221 / native boost particles");
        }
        self.player.handle_input(self.input.current);
        self.sonic_inputs.handle_frame(&mut self.player)?;
        let transition = self.queued_transition.take();
        if let Some(trigger) = triggers.last() {
            self.goal = Some(GoalController::reached_transition(
                &mut self.player,
                trigger.x,
            ));
        } else if bar_uncurl {
            self.player.move_state|=0x100;
            self.callback=HarnessCallback::InitUncurl;
        } else if let Some(launch) = launches.last() {
            launch.apply(self.stage.level, &mut self.player);
            self.previous_char_state = -1;
            self.callback = HarnessCallback::Spring;
        } else if let Some(transition) = transition {
            match transition {
                1 => {
                    self.callback = HarnessCallback::TouchGround;
                }
                9 => {
                    combat::init_hurt(&mut self.player);
                    self.lifecycle.timer_invulnerability = 120;
                    self.callback = HarnessCallback::Hurt;
                }
                8 => {
                    combat::init_recoil(&mut self.player);
                    self.callback = HarnessCallback::Uncurl;
                }
                _ => return Err(Error("Unsupported native enemy transition".into())),
            }
        }
        if let Err(error) = self.execute_player().and_then(|()| self.update_animation()) {
            self.halted = Some(error.to_string());
        }
        self.show_player = self.player.move_state & native_lifecycle::DEAD != 0
            || (self.lifecycle.timer_invulnerability == 0 || self.lifecycle.stage_time & 2 == 0);
        combat::tick_hurt_immunity(
            self.player.char_state,
            &mut self.lifecycle.timer_invulnerability,
        );
        self.player.disable_trick_timer = self.player.disable_trick_timer.saturating_sub(1);
        if let Some(id) = self
            .lifecycle
            .finish_player_main(&mut self.player, &mut self.camera)
        {
            self.sound_ids.push(id);
        }
        self.run_enemies(cx, cy)?;
        self.sound_ids.extend(
            self.special
                .tick(&mut self.entities, &self.player, cx, cy, None)?,
        );
        if self.tick_results()? {
            return Ok(self.snapshot());
        }
        if self.lifecycle.tick_stage_clock(&mut self.player) {
            self.sound_ids.push(149);
        }
        Ok(self.snapshot())
    }

    fn run_enemies(&mut self, cx: i32, cy: i32) -> Result<(), Error> {
        self.target = HomingTarget::default();
        let hit = self.animation.hitboxes();
        let mut context = EnemyContext {
            player: &mut self.player,
            body: [hit[0], hit[1]],
            lifecycle: &mut self.lifecycle,
            rings: &mut self.ring_count,
            score: &mut self.score,
            lives: &mut self.lives,
            timer_invincibility: 0,
        };
        self.enemy_events = self.enemies.tick(&mut context, cx, cy)?;
        for event in &self.enemy_events {
            match *event {
                EnemyEvent::Transition(n) => self.queued_transition = Some(n),
                EnemyEvent::Sound(n) => self.sound_ids.push(n),
                EnemyEvent::ExtraLifeMusic => self.sound_ids.push(407),
                EnemyEvent::ScatterRings { x, y, count } => self.sound_ids.extend(
                    self.scatter
                        .emit(x, y, count, self.player.layer, false, &mut self.pseudo_rng),
                ),
                EnemyEvent::HomingTarget { x_q8, y_q8 } => {
                    self.target.consider(&self.player, x_q8, y_q8)
                }
                EnemyEvent::Defeated { .. } => {
                    self.effect_requests.push("enemy dust, animal and score")
                }
            }
        }
        Ok(())
    }

    fn restart_stage(&mut self) -> Result<(), Error> {
        let mut next = Self::for_level(self.rom, self.stage.level)?;
        next.lifecycle = self.lifecycle.clone();
        let (width, height) = next.stage.dimensions();
        next.camera = next
            .lifecycle
            .reinitialize_player(&mut next.player, width, height)?;
        // This runner explicitly bypasses the countdown on both launch and retry.
        next.lifecycle.stage_flags &=
            !(native_lifecycle::ACT_START | native_lifecycle::DISABLE_PAUSE);
        next.player.move_state &= !player::IGNORE_INPUT;
        next.player.handle_input(0);
        next.displayed_time = (
            next.lifecycle.course_time,
            u64::from(next.lifecycle.stage_time),
        );
        self.carry_campaign(&mut next);
        next.input = self.input.clone();
        next.lives = self.lives;
        next.score = self.score;
        next.pseudo_rng = self.pseudo_rng;
        next.displayed_counts = (0, next.lives, next.score);
        next.frame = self.frame;
        *self = next;
        Ok(())
    }

    fn tick_rings(&mut self) -> Result<(), Error> {
        self.ring_sound_pans.clear();
        self.pickups.retain(|p| !p.finished);
        self.ring_sprite.advance()?;
        // TasksExec caches the manager's successor before dispatch. A newly
        // appended effect is visited this pass only if a prior effect links to it.
        let had_active_effect = !self.pickups.is_empty();
        let first_new = self.pickups.len();
        for pos in self.rings.collect(&self.player) {
            self.increment_rings();
            self.pickup_effect(pos)?;
        }
        let dispatch = if had_active_effect {
            self.pickups.len()
        } else {
            first_new
        };
        // Preserve the terminating animation frame through this display pass.
        for pickup in &mut self.pickups[..dispatch] {
            let step = pickup.sprite.advance()?;
            self.sound_ids.extend(step.sounds);
            pickup.finished = step.ended;
        }
        // The post-intro HUD refreshes shared OBJ bank 6 after the ring tasks.
        self.ring_sprite.set_palette_bank(6, &self.ring_palette)?;
        for pickup in &mut self.pickups {
            pickup.sprite.set_palette_bank(6, &self.ring_palette)?;
        }
        Ok(())
    }

    fn increment_rings(&mut self) {
        if increment_single_player(&mut self.ring_count, &mut self.lives) {
            self.effect_requests.push("MUS_1_UP");
            self.sound_ids.push(407);
        }
    }
    fn pickup_effect(&mut self, pos: RingPosition) -> Result<(), Error> {
        if self.pickups.iter().filter(|p| !p.finished).count() < 8 {
            self.ring_sound_pans
                .push(if self.ring_count & 1 != 0 { -64 } else { 64 });
            self.pickups.push(Pickup {
                position: pos,
                sprite: AnimationPlayer::new(self.rom, 727, 0)?,
                finished: false,
            });
        }
        Ok(())
    }

    fn score_state(&self) -> ScoreState {
        ScoreState {
            level_score: self.score as i32,
            lives: self.lives,
            music_request: 0,
        }
    }

    fn accept_score(&mut self, state: ScoreState) {
        self.score = state.level_score as u32;
        self.lives = state.lives;
        if state.music_request != 0 {
            self.sound_ids.push(407);
        }
    }

    fn carry_campaign(&mut self, next: &mut Self) {
        std::mem::swap(&mut self.campaign, &mut next.campaign);
        next.save_rng = self.save_rng;
        next.enemies
            .set_easy(next.campaign.loaded.difficulty_level == 1);
        next.last_destination = self.last_destination;
    }

    fn results_context(&self) -> ResultsContext {
        ResultsContext {
            level: self.stage.level,
            character: 0,
            game_mode: 0,
            move_state: self.player.move_state,
            special_rings: self.special.count,
        }
    }

    fn execute_player(&mut self) -> Result<(), Error> {
        if self.player.move_state&bar::IA_OVERRIDE!=0{return Ok(());}
        let Some(goal) = self.goal.as_mut() else {
            return self.execute_callback();
        };
        let step = goal.step(
            &mut self.player,
            self.stage.level,
            self.special.count,
            self.animation.animation,
            self.animation.variant,
            &mut self.camera.shift_y,
        )?;
        if step.continue_existing_callback {
            return self.execute_callback();
        }
        if step.start_long_brake {
            self.sound_ids.push(232);
        }
        if step.stop_long_brake {
            self.effect_requests.push("stop SE_LONG_BRAKE");
        }
        if step.bonus != 0 {
            let mut s = self.score_state();
            s.award(step.bonus, 0, true);
            self.accept_score(s);
        }
        if step.begin_results {
            self.results_ui = Some(ResultsUi::new(self.rom, &self.results_context())?);
            self.results = Some(StageResults::begin(
                self.results_context(),
                self.lifecycle.course_time,
                self.ring_count,
                self.ring_count,
                &mut self.campaign.loaded,
            )?);
            self.sound_ids.push(401);
        }
        if step.advance_ground {
            if step.special_transition {crate::native_goal::special_course_wrap(self.rom,self.stage.level,&mut self.player,&mut self.camera)?;}
            let (sin, cos) = self.trig(self.player.rotation);
            native_lifecycle::normal_bounds(&mut self.player, &self.camera, false)?;
            self.advance_ground(sin, cos)?;
            if step.special_transition && self.goal.as_mut().is_some_and(|g|g.finish_special_motion(&mut self.player,&mut self.camera)){
                self.sound_ids.push(273);self.effect_requests.push("goal boost effect");
            }
        }
        Ok(())
    }

    // Task_UpdateStageResults runs at 0xC100, between enemy tasks and the stage clock.
    fn tick_results(&mut self) -> Result<bool, Error> {
        let mut ctx = self.results_context();
        let mut score = self.score_state();
        let Some(results) = self.results.as_mut() else {
            return Ok(false);
        };
        // UpdateScreenFade applies the previous coefficient, then increments by Q(1).
        // Only the ordinary act path is reachable from this runner.
        let fade_complete = self
            .results_ui
            .as_mut()
            .ok_or_else(|| Error("Missing native results graphics".into()))?
            .advance_fade_for_next_tick(results);
        let step = results.tick(
            &mut ctx,
            &mut self.campaign.loaded,
            &mut score,
            self.input.pressed,
            self.lifecycle.stage_time,
            fade_complete,
            false,
        )?;
        self.accept_score(score);
        self.player.move_state = ctx.move_state;
        if let Some(s) = step.sound {
            self.sound_ids.push(match s {
                ResultsSound::Counter => 140,
                ResultsSound::CounterDone => 141,
            });
        }
        if step.request_save {
            let rng = &mut self.save_rng;
            let report = self.campaign.write(
                &mut || (u32::from(rng.next_u16()) << 16) | u32::from(rng.next_u16()),
                &MemoryFlashFaults::default(),
            );
            if !report.success {
                return Err(Error("In-memory native campaign write failed".into()));
            }
        }
        if let Some(dest) = step.destination {
            self.last_destination = Some(dest);
            if let ResultsDestination::NextStage(id) = dest
                && native_stage::is_course_act(id)
            {
                let mut next = Self::for_level(self.rom, id)?;
                self.carry_campaign(&mut next);
                next.input = self.input.clone();
                next.lives = self.lives;
                next.score = self.score;
                next.displayed_counts = (0, next.lives, next.score);
                next.pseudo_rng = self.pseudo_rng;
                next.frame = self.frame;
                *self = next;
            } else {
                self.halted = Some(format!(
                    "Native results destination {dest:?} is not implemented"
                ));
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn execute_callback(&mut self) -> Result<(), Error> {
        match self.callback {
            HarnessCallback::TouchGround => {
                self.player.move_state &= !(0x20000000
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
                    | 0x10
                    | player::SPIN_ATTACK
                    | player::IN_AIR
                    | 0x2000000);
                self.player.unk61 = 0;
                self.player.unk62 = 0;
                self.player.unk63 = 0;
                self.player.unk71 = 0;
                self.player.unk70 = false;
                self.player.change_sprite_offsets(6, 14, false);
                self.player.char_state = if self.player.q_speed_ground == 0 {
                    player::IDLE
                } else {
                    player::WALK
                };
                self.player.callback = Callback::Ground;
                self.callback = HarnessCallback::Ground;
                self.ground(true)?;
            }
            HarnessCallback::InitCrouch => {
                self.player.move_state &= !0x20;
                self.player.char_state=spin::CROUCH;
                self.player.q_speed_ground=0;
                self.callback=HarnessCallback::Crouch;
                self.crouch()?;
            }
            HarnessCallback::Crouch=>self.crouch()?,
            HarnessCallback::Spindash=>self.spindash()?,
            HarnessCallback::InitRoll=>{
                spin::init_roll(&mut self.player);
                self.callback=HarnessCallback::Rolling;
                self.rolling()?;
            }
            HarnessCallback::Rolling=>self.rolling()?,
            HarnessCallback::Ground => self.ground(true)?,
            HarnessCallback::Turning => self.ground(false)?,
            HarnessCallback::InitSkid => {
                self.player.move_state &= !0x20;
                self.player.char_state = if matches!(self.player.char_state, 7 | 8) {
                    6
                } else {
                    5
                };
                self.player.sprite_frame_flags &= !player::ANIMATION_OVER;
                self.player.q_speed_ground = 0;
                self.player.move_state ^= player::FACING_LEFT;
                self.callback = HarnessCallback::Turning;
                self.ground(false)?;
            }
            HarnessCallback::InitJump => {
                let (sin, cos) = self.trig(self.player.rotation.wrapping_sub(64));
                self.player.initialize_jump(sin, cos, None);
                self.callback = HarnessCallback::Jumping;
                self.effect_requests.push("SE_JUMP");
                self.sound_ids.push(116);
                self.airborne()?;
            }
            HarnessCallback::Jumping => self.airborne()?,
            HarnessCallback::Uncurl => self.uncurl()?,
            HarnessCallback::InitUncurl=>{bar::init_uncurl(&mut self.player,self.animation.animation,false);self.callback=HarnessCallback::Uncurl;self.uncurl()?;}
            HarnessCallback::Hurt => {
                native_lifecycle::normal_bounds(&mut self.player, &self.camera, false)?;
                self.player.apply_gravity(false);
                self.player.advance_position(false, false);
                self.air_contacts()?;
            }
            HarnessCallback::Homing => self.homing()?,
            HarnessCallback::Spring => {
                self.air_animation_variants()?;
                self.player.air_input();
                self.air_step()?;
            }
        }
        Ok(())
    }

    fn requested_jump(&mut self)->Result<bool,Error> {
        if self.player.frame_input&A_BUTTON==0 {return Ok(false);}
        let clearance=native_collision::jump_clearance(&self.stage,&mut self.player)?;
        if self.player.request_jump(A_BUTTON,clearance) {
            self.callback=HarnessCallback::InitJump;return Ok(true);
        }
        Ok(false)
    }

    fn crouch(&mut self)->Result<(),Error>{
        if self.player.char_state==spin::CROUCH && self.player.frame_input&A_BUTTON!=0 {
            self.spin.start(&mut self.player);
            self.sound_ids.push(109);
            self.effect_requests.push("spindash dust");
            self.callback=HarnessCallback::Spindash;
            return self.spindash();
        }
        if self.requested_jump()? {return Ok(());}
        match self.player.held_input&0xf0 {
            0 if self.animation.animation==2 && self.variant==0=>{
                self.variant=1;self.player.sprite_frame_flags&=!player::ANIMATION_OVER;
            }
            x if x!=0 && x!=DPAD_DOWN=>self.callback=HarnessCallback::TouchGround,
            _=>{}
        }
        if self.animation.animation==2 && self.variant==1 && self.player.sprite_frame_flags&player::ANIMATION_OVER!=0 {
            self.callback=HarnessCallback::TouchGround;
        }
        self.walk_slope()
    }

    fn spindash(&mut self)->Result<(),Error>{
        if self.player.held_input&DPAD_DOWN==0 {
            self.spin.release(&mut self.player);
            self.callback=HarnessCallback::InitRoll;
            self.sound_ids.push(110);
        } else {
            if self.spin.tap_or_decay(self.player.frame_input&A_BUTTON!=0).is_some(){
                self.sound_ids.push(109);self.variant=1;self.player.unk6c=true;
            }
            if self.animation.animation==3 && self.variant==1 && self.player.sprite_frame_flags&player::ANIMATION_OVER!=0 {
                self.variant=0;
            }
        }
        if self.player.move_state&player::IN_AIR!=0 {return self.air_step();}
        let (sin,cos)=self.trig(self.player.rotation);
        spin::rolling_slope(&mut self.player,sin);
        self.spin_ground_motion(sin,cos,false)
    }

    fn rolling(&mut self)->Result<(),Error>{
        if self.requested_jump()? {return Ok(())}
        let (sin,cos)=self.trig(self.player.rotation);
        if !spin::roll_speed(&mut self.player,sin) {
            self.callback=HarnessCallback::TouchGround;
            return self.execute_callback();
        }
        self.spin_ground_motion(sin,cos,true)?;
        if self.player.move_state & player::IN_AIR!=0 {self.callback=HarnessCallback::Jumping;}
        Ok(())
    }

    fn spin_ground_motion(&mut self,sin:i32,cos:i32,rolling:bool)->Result<(),Error>{
        native_lifecycle::normal_bounds(&mut self.player,&self.camera,false)?;
        self.player.ground_speed_to_air(sin,cos);
        native_collision::ground_wall(&self.stage,&mut self.player)?;
        if rolling && self.player.move_state&player::IN_AIR!=0 {self.player.apply_gravity(false);}
        self.move_and_follow_ground()
    }

    fn ground(&mut self, controls: bool) -> Result<(), Error> {
        if let Some(action)=spin::down_action(&self.player){
            self.callback=match action {DownAction::Crouch=>HarnessCallback::InitCrouch,
                DownAction::Roll=>{self.sound_ids.push(109);HarnessCallback::InitRoll}};
            return Ok(());
        }
        if self.requested_jump()?{return Ok(());}
        if controls {
            if self.player.ground_input(false) {
                self.sound_ids.push(115);
                self.effect_requests
                    .push("SE_BRAKE / native braking particles");
            }
            native_collision::ground_wall(&self.stage, &mut self.player)?;
            if self.player.callback == Callback::Skidding {
                self.callback = HarnessCallback::InitSkid;
            }
        } else if self.player.sprite_frame_flags & player::ANIMATION_OVER != 0 {
            self.callback = HarnessCallback::TouchGround;
        }
        self.walk_slope()
    }

    fn walk_slope(&mut self)->Result<(),Error>{
        let (sin, cos) = self.trig(self.player.rotation);
        self.player.ground_slope_acceleration(sin);
        native_lifecycle::normal_bounds(&mut self.player, &self.camera, false)?;
        self.advance_ground(sin, cos)?;
        if self.player.move_state & player::IN_AIR != 0 {
            self.player.callback = Callback::Airborne;
            self.callback = HarnessCallback::Jumping;
        }
        Ok(())
    }

    fn advance_ground(&mut self, sin: i32, cos: i32) -> Result<(), Error> {
        self.player.ground_speed_to_air(sin, cos);
        self.move_and_follow_ground()
    }

    fn move_and_follow_ground(&mut self)->Result<(),Error>{
        self.player.advance_position(false, false);
        native_collision::ground_follow(&self.stage, &mut self.player)?;
        if self.player.control_lock != 0 {
            self.player.control_lock = self.player.control_lock.wrapping_sub(1);
        } else if self.player.rotation.wrapping_add(32) & 0xc0 != 0
            && i32::from(self.player.q_speed_ground).abs() < 480
        {
            self.player.q_speed_ground = 0;
            self.player.move_state |= player::IN_AIR;
            self.player.control_lock = 30;
        }
        Ok(())
    }

    fn dispatch_air_action(&mut self) -> AirAction {
        let action = native_sonic::try_no_target_air_action(&mut self.player, A_BUTTON, false);
        if let Some(id) = action.sound() {
            self.sound_ids.push(id);
        }
        match action {
            AirAction::InstaShield => self.effect_requests.push("Insta Shield"),
            AirAction::ForwardThrust { stop_sound_281 } => {
                self.callback = HarnessCallback::Uncurl;
                if stop_sound_281 {
                    self.effect_requests.push("stop sound 281");
                }
            }
            AirAction::None => {}
        }
        action
    }

    fn airborne(&mut self) -> Result<(), Error> {
        if self.try_homing() {
            return self.homing();
        }
        if self.dispatch_air_action().returns_early() {
            return self.uncurl();
        }
        self.player.release_jump(A_BUTTON);
        self.air_animation_variants()?;
        self.player.air_input();
        self.air_step()
    }

    fn uncurl(&mut self) -> Result<(), Error> {
        self.air_animation_variants()?;
        if native_sonic::uncurl_accepts_air_input(&self.player) {
            self.player.air_input();
            if self.try_homing() {
                return self.homing();
            }
            if self.dispatch_air_action().returns_early() {
                // Initializing the move clears its input latch before re-entry.
                return self.uncurl();
            }
        }
        self.air_step()
    }

    fn try_homing(&mut self) -> bool {
        if !self.target.can_launch(&self.player, A_BUTTON) {
            return false;
        }
        let (sin, cos) = self.trig1024(usize::from(self.target.angle));
        combat::init_homing(&mut self.player, sin, cos);
        self.homing_frames = 60;
        self.callback = HarnessCallback::Homing;
        self.sound_ids.push(112);
        true
    }

    fn homing(&mut self) -> Result<(), Error> {
        combat::homing_tick_before_physics(&mut self.player, &mut self.homing_frames);
        self.player.air_input();
        self.air_step()?;
        self.queued_transition = combat::homing_transition_after_physics(&self.player);
        Ok(())
    }

    fn air_animation_variants(&mut self) -> Result<(), Error> {
        // Native sub_80246DC runs before air controls and changes the collision
        // offsets with the original jump-script variants, not a replacement art.
        if matches!(
            self.player.char_state,
            player::JUMP_SLOW | player::JUMP_FAST
        ) {
            if !matches!(self.animation.animation, 10 | 11) {
                return Ok(());
            }
            if self.variant == 0 && self.player.sprite_frame_flags & player::ANIMATION_OVER != 0 {
                self.variant = 1;
                self.player.move_state |= player::SPIN_ATTACK;
                self.player.change_sprite_offsets(6, 9, false);
            } else if self.variant == 1 && self.player.q_speed_air_y > 0 {
                let floor = self.stage.long_probe(
                    self.player.q_world_x >> 8,
                    (self.player.q_world_y >> 8) + i32::from(self.player.sprite_offset_y),
                    self.player.layer,
                    8,
                    true,
                )?;
                if floor.distance <= 32 {
                    self.variant = 2;
                }
            }
        }
        let anim = self.animation.animation;
        match self.player.char_state {
            37 | 38
                if (self.player.char_state == 37 && anim == 65)
                    || (self.player.char_state == 38 && anim == 52) =>
            {
                if self.variant == 0 && self.player.q_speed_air_y > 0 {
                    self.variant = 1;
                } else if self.variant == 1
                    && self.player.sprite_frame_flags & player::ANIMATION_OVER != 0
                {
                    self.variant = 2;
                }
            }
            39 if anim == 53
                && self.variant == 0
                && self.player.sprite_frame_flags & player::ANIMATION_OVER != 0 =>
            {
                self.variant = 2
            }
            _ => {}
        }
        Ok(())
    }

    fn air_step(&mut self) -> Result<(), Error> {
        self.player.decay_air_x(false);
        native_lifecycle::normal_bounds(&mut self.player, &self.camera, false)?;
        self.player.apply_gravity(false);
        self.player.advance_position(false, false);
        self.player.relax_rotation();
        self.air_contacts()
    }

    fn air_contacts(&mut self) -> Result<(), Error> {
        let collision = native_collision::air_collision(&self.stage, &mut self.player)?;
        let stood = self.player.move_state & (player::STOOD_ON_OBJECT | player::IN_AIR)
            == player::STOOD_ON_OBJECT;
        if stood {
            self.player.q_speed_ground = self.player.q_speed_air_x;
            self.player.rotation = 0;
        }
        if collision.landed || stood {
            self.callback = HarnessCallback::TouchGround;
        }
        Ok(())
    }

    fn update_animation(&mut self) -> Result<(), Error> {
        let state = self.player.char_state;
        if state != self.previous_char_state {
            self.variant = 0;
        }
        let (animation, variant, speed) = match state {
            player::WALK => {
                let speed = i32::from(self.player.q_speed_ground);
                (
                    9,
                    self.player.walk_anim,
                    ((speed >> 5) + (speed >> 6)).abs().clamp(8, 128) as u8,
                )
            }
            0 | 2..=8 | 10 | 11 | 13 => (state as u16, self.variant, 16),
            14 => (13, 0, 16),
            19 => (18, 0, 16),
            25..=27 => (31, (state - 25) as u8, 16),
            28 | 29 => ((state + 4) as u16, 0, 16),
            32 => (35,0,16),
            20 | 21 => (28, (state - 20) as u8, 16),
            native_lifecycle::DEAD_STATE => (29, self.variant, 16),
            37 => (65, self.variant, 16),
            50=>(70,self.variant,16),
            38 => (52, self.variant, 16),
            39 => (if self.variant == 2 { 52 } else { 53 }, self.variant, 16),
            native_sonic::INSTA_SHIELD_STATE => (17, self.variant, 16),
            native_sonic::FORWARD_THRUST_STATE => (19, self.variant, 16),
            _ => {
                return Err(Error(format!(
                    "Native character state {state} is not integrated in this slice"
                )));
            }
        };
        if self.player.unk6c
            || self.animation.animation != animation
            || self.animation.variant != variant
        {
            self.animation.restart_body_script();
            self.player.unk6c = false;
        }
        self.animation.set_animation(animation, variant)?;
        self.animation.set_speed(speed);
        self.animation.set_flip(
            state != player::WALK && self.player.move_state & player::FACING_LEFT == 0,
            false,
        );
        let step = self.animation.advance()?;
        self.animation_frame = step.frame_index;
        if step.ended {
            self.player.sprite_frame_flags |= player::ANIMATION_OVER;
        } else {
            self.player.sprite_frame_flags &= !player::ANIMATION_OVER;
        }
        self.sound_ids.extend(step.sounds);
        self.previous_char_state = state;
        Ok(())
    }

    fn draw_player(&self, rgba: &mut [u8], cx: i32, cy: i32) -> Result<(), Error> {
        if self.player.move_state & native_lifecycle::HIDDEN == 0
            && (self.show_player || self.lifecycle.phase == Phase::Dying)
        {
            let facing_left = self.player.move_state & player::FACING_LEFT != 0;
            let (sprite, adjust_x) = if self.animation.animation == 9 {
                (
                    self.animation.render_affine(
                        AffineTransform {
                            rotation: u16::from(self.player.rotation) << 2,
                            q_scale_x: if facing_left { 256 } else { -256 },
                            q_scale_y: 256,
                        },
                        false,
                    )?,
                    if facing_left { 0 } else { -1 },
                )
            } else {
                (self.animation.render()?, i32::from(facing_left))
            };
            blit(
                rgba,
                &sprite,
                (self.player.q_world_x >> 8) - cx + adjust_x,
                (self.player.q_world_y >> 8) - cy,
            );
        }
        Ok(())
    }

    pub fn snapshot(&self) -> HarnessSnapshot {
        HarnessSnapshot {
            frame: self.frame,
            callback: self.callback,
            player: self.player,
            input: self.input.frame(),
            animation: self.animation.animation,
            variant: self.animation.variant,
            animation_frame: self.animation_frame,
            level_id: self.stage.level,
            lifecycle: self.lifecycle.clone(),
            game_over: self.game_over,
            camera: self.camera.position(),
            rings: self.ring_count,
            special_rings: self.special.count,
            layer_contacts: self.layer_contacts,
            lives: self.lives,
            goal_phase: self.goal.as_ref().map(|g| g.phase),
            results: self.results.clone(),
            last_destination: self.last_destination,
            unlocked_level: self.campaign.loaded.unlocked_levels[0],
            score: self.score,
            enemy_events: self.enemy_events.clone(),
            ring_sound_pans: self.ring_sound_pans.clone(),
            animation_sound_ids: self.sound_ids.clone(),
            native_effect_requests: self.effect_requests.clone(),
            halted: self.halted.clone(),
        }
    }

    pub fn render(&self) -> Result<Vec<u8>, Error> {
        let [cx, cy] = self.camera.position();
        let mut rgba = self.stage.render_layers(
            [cx, cy],
            [VIEW_WIDTH, VIEW_HEIGHT],
            self.stage_ticks.saturating_sub(1),
            true,
            |rgba| self.draw_objects(rgba, cx, cy),
        )?;
        if self.animation.animation == 29 {
            self.draw_player(&mut rgba, cx, cy)?;
        }
        for (x, y, lever) in self.goals.sprites() {
            blit(&mut rgba, &lever.render()?, x - cx, y - cy);
        }
        self.hud.draw(
            self.displayed_time.0,
            self.displayed_time.1,
            self.displayed_counts.0,
            self.displayed_counts.1,
            self.displayed_counts.2,
            |sprite, x, y| blit(&mut rgba, sprite, x, y),
        );
        if let (Some(results), Some(ui)) = (&self.results, &self.results_ui) {
            ui.draw(results, |sprite, x, y| blit(&mut rgba, sprite, x, y));
            ui.apply_fade(&mut rgba);
        }
        Ok(rgba)
    }

    fn draw_objects(&self, rgba: &mut [u8], cx: i32, cy: i32) -> Result<(), Error> {
        let scattered: Vec<_> = self.scatter.sprites().collect();
        for (x, y, sprite) in scattered.into_iter().rev() {
            blit(rgba, &sprite.render()?, x - cx, y - cy);
        }
        if self.stage_ticks > 0 {
            let ring = self.ring_sprite.render()?;
            let visible: Vec<_> = self.rings.visible(cx, cy).collect();
            for pos in visible.iter().rev() {
                blit(rgba, &ring, pos.x - cx, pos.y - cy);
            }
        }
        let springs: Vec<_> = self
            .springs
            .sprites()
            .chain(self.checkpoints.sprites())
            .chain(self.bars.sprites())
            .chain(self.enemies.sprites())
            .chain(self.special.sprites())
            .collect();
        for (x, y, sprite) in springs.into_iter().rev() {
            blit(rgba, &sprite.render()?, x - cx, y - cy);
        }
        if self.animation.animation != 29 {
            self.draw_player(rgba, cx, cy)?;
        }
        for pickup in self.pickups.iter().rev() {
            if pickup.sprite.dimensions().is_some() {
                blit(
                    rgba,
                    &pickup.sprite.render()?,
                    pickup.position.x - cx,
                    pickup.position.y - cy,
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn blit(rgba: &mut [u8], sprite: &SpriteFrame, x: i32, y: i32) {
    let left = x - i32::from(sprite.origin_x) + i32::from(sprite.translation_x);
    let top = y - i32::from(sprite.origin_y) + i32::from(sprite.translation_y);
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            let dx = left + x as i32;
            let dy = top + y as i32;
            if dx < 0 || dy < 0 || dx >= VIEW_WIDTH as i32 || dy >= VIEW_HEIGHT as i32 {
                continue;
            }
            let source = (y * sprite.width + x) * 4;
            if sprite.rgba[source + 3] == 0 {
                continue;
            }
            let dest = (dy as usize * VIEW_WIDTH + dx as usize) * 4;
            rgba[dest..dest + 4].copy_from_slice(&sprite.rgba[source..source + 4]);
        }
    }
}

#[cfg(test)]
mod course_completion {
    use super::*;

    #[test]
    #[ignore="requires verified A2NE_ROM"]
    fn seven_special_rings_run_native_qualifying_finish_and_select_special_stage() {
        let raw=std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom=VerifiedRom::parse(&raw).unwrap();
        let mut g=NativeHarness::new(&rom).unwrap();
        let e=*g.entities.placements().iter().find(|e|e.kind==46).unwrap();
        g.player.q_world_x=e.x*256;g.player.q_world_y=e.y*256;g.player.move_state=0;
        g.camera=NativeCamera::normal_stage(&g.player,g.stage.dimensions().0,g.stage.dimensions().1).unwrap();
        g.special.count=7;g.lifecycle.course_time=3600;
        let mut tally=false;let mut pose=false;let mut boost=false;
        for i in 0..1900 {
            let s=g.tick(if i==400 {A_BUTTON}else{0}).unwrap();
            pose|=s.animation==35;
            tally|=s.results.as_ref().is_some_and(|r|r.special_bonus==10_000);
            boost|=s.animation_sound_ids.contains(&273);
            if s.halted.is_some(){assert_eq!(s.last_destination,Some(ResultsDestination::SpecialStage),"{i}: {:?}",s.halted);break;}
        }
        assert!(pose && tally && boost);assert_eq!(g.last_destination,Some(ResultsDestination::SpecialStage));
        assert_eq!(g.score,15_000);
        let mut reloaded=CampaignSaveSession::new(g.campaign.flash_bytes(),true).unwrap();
        assert!(reloaded.load_latest(&MemoryFlashFaults::default()).success);
        assert_eq!(reloaded.loaded.unlocked_levels[0],1);
    }

    // Controlled starting state at the actual finish placements. This tests
    // engine integration, separately from any input-only run from stage start.
    #[test]
    #[ignore = "requires local verified A2NE_ROM"]
    fn cartridge_finish_counts_once_and_journals_destination_in_memory() {
        let raw = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&raw).unwrap();
        for level in (0..26).filter(|&id| native_stage::is_course_act(id)) {
            let mut game = NativeHarness::for_level(&rom, level).unwrap();
            let goal = *game
                .entities
                .placements()
                .iter()
                .find(|e| e.kind == 46)
                .unwrap();
            game.player.q_world_x = goal.x * 256;
            game.player.q_world_y = goal.y * 256;
            game.player.move_state = 0;
            game.player.q_speed_ground = 0;
            game.ring_count = 37;
            game.lifecycle.course_time = 3600;
            game.camera = NativeCamera::normal_stage(
                &game.player,
                game.stage.dimensions().0,
                game.stage.dimensions().1,
            )
            .unwrap();
            let mut counted = false;
            let mut saw_victory = false;
            let mut received_clear_sound = false;
            let mut freeze = None;
            for i in 0..1400 {
                let s = game.tick(if i == 260 { A_BUTTON } else { 0 }).unwrap();
                if s.goal_phase.is_some() {
                    assert_eq!(
                        s.lifecycle.course_time,
                        *freeze.get_or_insert(s.lifecycle.course_time)
                    );
                }
                saw_victory |= s.animation == if level & 3 == 0 { 32 } else { 33 };
                received_clear_sound |= s.animation_sound_ids.contains(&401);
                if let Some(r) = s.results.as_ref() {
                    counted |= r.counting_done;
                }
                if s.last_destination.is_some() {
                    break;
                }
                assert!(
                    s.halted.is_none(),
                    "level {level}, frame {i}: {:?}",
                    s.halted
                );
            }
            let state = game.snapshot();
            assert!(
                saw_victory && received_clear_sound && counted,
                "level {level} {:?}",
                state.goal_phase
            );
            assert_eq!(
                state.last_destination,
                Some(ResultsDestination::NextStage(level + 1))
            );
            assert_eq!(state.unlocked_level, level + 1);
            assert_eq!(
                state.score, 8700,
                "5000 time + 3700 rings for level {level}"
            );
            if level & 3 == 0 {
                assert_eq!(state.level_id, level + 1);
                assert!(state.halted.is_none());
            } else {
                assert_eq!(state.level_id, level);
                assert!(state.halted.is_some());
            }
            let mut readback = CampaignSaveSession::new(game.campaign.flash_bytes(), true).unwrap();
            assert!(readback.load_latest(&MemoryFlashFaults::default()).success);
            assert_eq!(readback.loaded.score, 37);
            assert_eq!(readback.loaded.unlocked_levels[0], level + 1);
        }
    }
}
