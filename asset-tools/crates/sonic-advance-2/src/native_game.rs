//! Local execution harness for the first normal-gravity Sonic movement slice.
//!
//! Native arithmetic, sensors, animation scripts and pixels come from the
//! verified source. Launch/countdown, camera tracking and frame transport are
//! host bindings. Objects, enemies, stage events, audio mixing and the complete
//! retail callback graph are unfinished. This has no MMO service connection.
use crate::{
    Error, VerifiedRom, gba,
    native_animation::AnimationPlayer,
    native_collision,
    native_player::{self as player, Callback, Character, PlayerState},
    native_runtime::{A_BUTTON, InputFrame, InputState},
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
    pub camera: [i32; 2],
    pub animation_sound_ids: Vec<u16>,
    pub native_effect_requests: Vec<&'static str>,
    pub halted: Option<String>,
}

pub struct NativeHarness<'a> {
    stage: NativeStage<'a>,
    player: PlayerState,
    input: InputState,
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
        let sine = rom.range("rom", 0x97aa4, 0xa00)?;
        if gba::sha256(sine) != SINE_SHA256 {
            return Err(Error("Native sine table fingerprint mismatch".into()));
        }
        let mut p = PlayerState::new_at(
            Character::Sonic,
            native_stage::SPAWN.0 * 256,
            native_stage::SPAWN.1 * 256,
        );
        // Explicit host launch: the retail intro/countdown task is not present.
        p.move_state &= !player::IGNORE_INPUT;
        p.handle_input(0);
        let mut animation = AnimationPlayer::new(rom, 0, 0)?;
        animation.set_flip(true, false);
        let initial_frame = animation.advance()?.frame_index;
        Ok(Self {
            stage: NativeStage::leaf_forest(rom)?,
            player: p,
            input: InputState::default(),
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
        if self.halted.is_some() {
            return Ok(self.snapshot());
        }
        self.frame += 1;
        self.sound_ids.clear();
        self.effect_requests.clear();
        self.input.poll_held(held);
        if self.player.update_frame_physics(0) {
            self.effect_requests.push("SE_221 / native boost particles");
        }
        self.player.handle_input(self.input.current);
        if let Err(error) = self
            .execute_callback()
            .and_then(|()| self.update_animation())
        {
            self.halted = Some(error.to_string());
        }
        Ok(self.snapshot())
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
            HarnessCallback::Jumping => {
                if self.player.frame_input & A_BUTTON != 0 {
                    return Err(Error(
                        "Sonic midair actions are not ported in this slice".into(),
                    ));
                }
                self.airborne()?;
            }
        }
        let x = self.player.q_world_x >> 8;
        let y = self.player.q_world_y >> 8;
        if !(0..native_stage::WIDTH).contains(&x) || !(0..native_stage::HEIGHT).contains(&y) {
            return Err(Error(
                "Stage edge/death callback is not ported in this slice".into(),
            ));
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

    fn airborne(&mut self) -> Result<(), Error> {
        self.player.release_jump(A_BUTTON);
        // Native sub_80246DC runs before air controls and changes the collision
        // offsets with the original jump-script variants, not a replacement art.
        if matches!(
            self.player.char_state,
            player::JUMP_SLOW | player::JUMP_FAST
        ) {
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
        self.player.air_input();
        self.player.decay_air_x(false);
        self.player.apply_gravity(false);
        self.player.advance_position(false, false);
        self.player.relax_rotation();
        native_collision::air_collision(&self.stage, &mut self.player)?;
        if self.player.move_state & player::IN_AIR == 0 {
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
            _ => {
                return Err(Error(format!(
                    "Native character state {state} is not integrated in this slice"
                )));
            }
        };
        self.animation.set_animation(animation, variant)?;
        self.animation.set_speed(speed);
        self.animation
            .set_flip(self.player.move_state & player::FACING_LEFT == 0, false);
        let step = self.animation.advance()?;
        self.animation_frame = step.frame_index;
        if step.ended {
            self.player.sprite_frame_flags |= player::ANIMATION_OVER;
        } else {
            self.player.sprite_frame_flags &= !player::ANIMATION_OVER;
        }
        self.sound_ids = step.sounds;
        self.previous_char_state = state;
        Ok(())
    }

    /// Host tracking camera. Native camera modes/parallax are still separate.
    fn camera(&self) -> [i32; 2] {
        [
            ((self.player.q_world_x >> 8) - 96).clamp(0, native_stage::WIDTH - VIEW_WIDTH as i32),
            ((self.player.q_world_y >> 8) - 80).clamp(0, native_stage::HEIGHT - VIEW_HEIGHT as i32),
        ]
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
            camera: self.camera(),
            animation_sound_ids: self.sound_ids.clone(),
            native_effect_requests: self.effect_requests.clone(),
            halted: self.halted.clone(),
        }
    }

    /// Original terrain/OBJ pixels. The inspection host places OBJ after both
    /// terrain layers; native affine transforms and full BG priority are pending.
    pub fn render(&self) -> Result<Vec<u8>, Error> {
        let [cx, cy] = self.camera();
        let mut rgba = self.stage.render_view(cx, cy, VIEW_WIDTH, VIEW_HEIGHT)?;
        let sprite = self.animation.render()?;
        let native_nonaffine_x = i32::from(
            self.player.char_state != player::WALK
                && self.player.move_state & player::FACING_LEFT != 0,
        );
        let left = (self.player.q_world_x >> 8) - cx - i32::from(sprite.origin_x)
            + i32::from(sprite.translation_x)
            + native_nonaffine_x;
        let top = (self.player.q_world_y >> 8) - cy - i32::from(sprite.origin_y)
            + i32::from(sprite.translation_y);
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
        Ok(rgba)
    }
}
