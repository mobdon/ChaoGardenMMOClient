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
    native_checkpoints::StageCheckpoints,
    native_collision,
    native_entities::StageEntities,
    native_hud::StageHud,
    native_lifecycle::{self, DeathTick, GameOverCause, NativeLifecycle, Phase},
    native_player::{self as player, Callback, Character, PlayerState},
    native_rings::{RingPosition, StageRings, increment_single_player, stage_ring_palette},
    native_runtime::{A_BUTTON, InputFrame, InputState},
    native_sonic::{self, AirAction, SonicInputBuffer},
    native_springs::StageSprings,
    native_stage::{self, NativeStage},
};
use serde::Serialize;

pub const VIEW_WIDTH: usize = 240;
pub const VIEW_HEIGHT: usize = 160;
pub const MAX_REPLAY_FRAMES: usize = 36_000;
pub const SUPPORTED_INPUT: u16 = A_BUTTON | player::LEFT | player::RIGHT;
const SINE_SHA256: &str = "78caf4fc1cb643a1f24d8e9d9efb1822cda68335adc1799d517582939d9c5c99";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum HarnessCallback {
    Ground,
    InitJump,
    Jumping,
    Uncurl,
    Spring,
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
    pub layer_contacts: usize,
    pub lives: u8,
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
    springs: StageSprings<'a>,
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
        let word = |index: usize| {
            i32::from(i16::from_le_bytes([
                self.sine[index * 2],
                self.sine[index * 2 + 1],
            ])) >> 6
        };
        let index = usize::from(rotation) * 4;
        (word(index), word(index + 256))
    }

    /// Advance exactly one 1/60-second simulation frame. The caller supplies
    /// only a physical input word; no positions, rewards or elapsed gains.
    pub fn tick(&mut self, held: u16) -> Result<HarnessSnapshot, Error> {
        if held & !SUPPORTED_INPUT != 0 {
            return Err(Error(
                "This slice accepts only left, right and A/jump".into(),
            ));
        }
        if self.halted.is_some() || self.game_over.is_some() {
            return Ok(self.snapshot());
        }
        self.frame += 1;
        self.stage_ticks += 1;
        self.sound_ids.clear();
        self.effect_requests.clear();
        self.input.poll_held(held);
        self.camera.tick_task(&self.player, false)?;
        self.tick_rings()?;
        let [cx, cy] = self.camera.position();
        self.layer_contacts = self.entities.update_layers(&mut self.player, cx, cy);
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
        self.displayed_time = (
            self.lifecycle.course_time,
            u64::from(self.lifecycle.stage_time),
        );
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
            self.lifecycle.tick_stage_clock(&mut self.player);
            return Ok(self.snapshot());
        }
        if self.player.update_frame_physics(self.ring_count) {
            self.camera.on_boost_activation();
            self.effect_requests.push("SE_221 / native boost particles");
        }
        self.player.handle_input(self.input.current);
        self.sonic_inputs.handle_frame(&mut self.player)?;
        if let Some(launch) = launches.last() {
            launch.apply(self.stage.level, &mut self.player);
            self.previous_char_state = -1;
            self.callback = HarnessCallback::Spring;
        }
        if let Err(error) = self
            .execute_callback()
            .and_then(|()| self.update_animation())
        {
            self.halted = Some(error.to_string());
        }
        self.player.disable_trick_timer = self.player.disable_trick_timer.saturating_sub(1);
        if let Some(id) = self
            .lifecycle
            .finish_player_main(&mut self.player, &mut self.camera)
        {
            self.sound_ids.push(id);
        }
        if self.lifecycle.tick_stage_clock(&mut self.player) {
            self.sound_ids.push(149);
        }
        Ok(self.snapshot())
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
        next.input = self.input.clone();
        next.lives = self.lives;
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
            if increment_single_player(&mut self.ring_count, &mut self.lives) {
                self.effect_requests.push("MUS_1_UP");
            }
            if self.pickups.len() < 8 {
                self.ring_sound_pans
                    .push(if self.ring_count & 1 == 1 { -64 } else { 64 });
                self.pickups.push(Pickup {
                    position: pos,
                    sprite: AnimationPlayer::new(self.rom, 727, 0)?,
                    finished: false,
                });
            }
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
                self.airborne()?;
            }
            HarnessCallback::Jumping => self.airborne()?,
            HarnessCallback::Uncurl => self.uncurl()?,
            HarnessCallback::Spring => {
                self.air_animation_variants()?;
                self.player.air_input();
                self.air_step()?;
            }
        }
        Ok(())
    }

    fn ground(&mut self, controls: bool) -> Result<(), Error> {
        if self.player.frame_input & A_BUTTON != 0 {
            let clearance = native_collision::jump_clearance(&self.stage, &mut self.player)?;
            if self.player.request_jump(A_BUTTON, clearance) {
                self.callback = HarnessCallback::InitJump;
                return Ok(());
            }
        }
        if controls {
            if self.player.ground_input(false) {
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
        let (sin, cos) = self.trig(self.player.rotation);
        self.player.ground_slope_acceleration(sin);
        native_lifecycle::normal_bounds(&mut self.player, &self.camera, false)?;
        self.player.ground_speed_to_air(sin, cos);
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
        if self.player.move_state & player::IN_AIR != 0 {
            self.player.callback = Callback::Airborne;
            self.callback = HarnessCallback::Jumping;
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
            if self.dispatch_air_action().returns_early() {
                // Initializing the move clears its input latch before re-entry.
                return self.uncurl();
            }
        }
        self.air_step()
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
        if self.player.move_state & 0x20 != 0 {
            self.player.unk62 = 0;
            self.player.unk63 = 0;
            self.player.char_state = 14;
        }
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
            0 | 5 | 6 | 7 | 8 | 10 | 11 | 13 => (state as u16, self.variant, 16),
            14 => (13, 0, 16),
            native_lifecycle::DEAD_STATE => (29, self.variant, 16),
            37 => (65, self.variant, 16),
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
        if self.player.move_state & native_lifecycle::HIDDEN == 0 {
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
            layer_contacts: self.layer_contacts,
            lives: self.lives,
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
        self.hud.draw(
            self.displayed_time.0,
            self.displayed_time.1,
            self.ring_count,
            self.lives,
            0,
            |sprite, x, y| blit(&mut rgba, sprite, x, y),
        );
        Ok(rgba)
    }

    fn draw_objects(&self, rgba: &mut [u8], cx: i32, cy: i32) -> Result<(), Error> {
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
