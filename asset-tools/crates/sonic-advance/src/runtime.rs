//! Local deterministic stage runtime, without ROM instruction execution,
//! profiles, network endpoints or transferable progress.
use crate::{
    binary, entities, hud, items, player, posts, rings, springs, sprite, stage, Error, Result,
};
mod camera;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const MAX_REPLAY_FRAMES: u32 = 36_000;
/// Two ordinary acts per zone. Final, extra and multiplayer slots use other actors or tasks.
pub const PRACTICE_ACTS: usize = 12;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub stage_index: usize,
    pub frame: u64,
    pub x_q8: i32,
    pub y_q8: i32,
    pub vx_q8: i32,
    pub vy_q8: i32,
    pub grounded: bool,
    pub animation: u16,
    pub local_rings: usize,
    pub camera_x: i32,
    pub camera_y: i32,
    pub paused_after_fall: bool,
    pub collision_layer: usize,
    pub checkpoint: Option<posts::Checkpoint>,
    pub goal: Option<posts::GoalState>,
    pub lives: u8,
    pub death: Option<Death>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Death {
    Fall,
    Spikes,
}

/// A request to start an ASOE song-table entry during the last accepted frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SoundCue {
    pub song: u16,
    pub pan: Option<i8>,
    pub pitch: Option<u16>,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Equipment {
    pub shield: Option<Shield>,
    pub invincibility_frames: u16,
    pub speed_frames: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Shield {
    Normal,
    Magnetic,
}

impl SoundCue {
    fn start(song: u16) -> Self {
        Self {
            song,
            pan: None,
            pitch: None,
        }
    }
}

pub struct NativeRun<'a> {
    rom: &'a [u8],
    stage: stage::Stage<'a>,
    sprites: sprite::SpriteBank<'a>,
    player: player::Player,
    player_sprite: sprite::SpritePlayer,
    ring_sprite: sprite::SpritePlayer,
    dust: Option<sprite::SpritePlayer>,
    ring_layout: rings::RingLayout,
    collected: rings::LocalCollection,
    entities: entities::StageEntities,
    layer_triggers: entities::LayerTriggers,
    posts: posts::StagePosts,
    springs: springs::StageSprings,
    items: items::ItemBoxes,
    equipment: Equipment,
    rng: u32,
    spikes: player::UprightSpikes,
    camera: camera::Camera,
    ring_effects: Vec<(i32, i32, sprite::SpritePlayer)>,
    lives: u8,
    held_rings: usize,
    hud: hud::Hud,
    zero_ring_phase: u8,
    checkpoint_clock: u32,
    death: Option<Death>,
    sounds: Vec<SoundCue>,
}

impl<'a> NativeRun<'a> {
    pub fn load(rom: &'a [u8]) -> Result<Self> {
        Self::load_stage(rom, 0)
    }

    pub fn load_stage(rom: &'a [u8], index: usize) -> Result<Self> {
        if index >= PRACTICE_ACTS {
            return Err(Error("practice stage index must be 0 through 11".into()));
        }
        let mut player = player::Player::sonic_stage(rom, index)?;
        // InitializePlayer starts normal single-player Sonic without MOVESTATE_IN_AIR.
        player.grounded = true;
        let entities = entities::StageEntities::load(rom, index)?;
        let sprites = sprite::SpriteBank::from_asoe(rom)?;
        let mut run = Self {
            rom,
            stage: stage::Stage::load(rom, index)?,
            spikes: player::UprightSpikes::from_bank(&sprites)?,
            items: items::ItemBoxes::new(rom, &entities, &sprites)?,
            equipment: Equipment::default(),
            rng: 0,
            sprites,
            camera: camera::Camera::initial(&player),
            player,
            player_sprite: sprite::SpritePlayer::new(0, 0),
            ring_sprite: sprite::SpritePlayer::new(707, 0),
            dust: None,
            ring_layout: rings::RingLayout::load(rom, index)?,
            posts: posts::StagePosts::new(index, &entities),
            springs: springs::StageSprings::new(&entities),
            entities,
            layer_triggers: entities::LayerTriggers::default(),
            ring_effects: Vec::new(),
            collected: rings::LocalCollection::default(),
            death: None,
            lives: 3,
            held_rings: 0,
            hud: hud::Hud::from_asoe(rom)?,
            zero_ring_phase: 0,
            checkpoint_clock: 0,
            sounds: vec![SoundCue::start(12 + index as u16)],
        };
        run.player_sprite.tick(&run.sprites)?;
        run.ring_sprite.tick(&run.sprites)?;
        Ok(run)
    }

    /// One 60 Hz input frame; no wall clock or offline time grants progress.
    pub fn step(&mut self, input: player::Input) -> Result<player::FrameOutcome> {
        self.step_controls(input, false)
    }

    pub fn step_controls(
        &mut self,
        input: player::Input,
        down: bool,
    ) -> Result<player::FrameOutcome> {
        if self.death.is_some() || self.posts.goal.is_some_and(|g| g.results_ready) {
            return Err(Error(
                "stage stopped; continue or restart the local run".into(),
            ));
        }
        // Camera (0x0f00) and map tasks (0x2000) precede the player (0x3000).
        let mut camera = self.camera;
        if self.player.frame > 0 {
            camera.update(&self.player, &self.stage);
        }
        let mut player = self.player.clone();
        let px = player.x >> 8;
        let py = player.y >> 8;
        let mut triggers = self.layer_triggers.clone();
        player.collision_layer = usize::from(triggers.update(
            &self.entities.interactables,
            (camera.x, camera.y),
            (px, py),
            player.collision_layer as u8,
        ));
        let mut posts = self.posts.clone();
        posts.tick(
            &self.sprites,
            posts::PostInput {
                x: px,
                y: py,
                half_height: player.half_height,
                camera_x: camera.x,
                camera_y: camera.y,
                clock: u32::try_from(player.frame)
                    .ok()
                    .and_then(|f| f.checked_add(1))
                    .and_then(|f| f.checked_add(self.checkpoint_clock))
                    .ok_or_else(|| Error("stage clock overflow".into()))?,
                ia_override: false,
                body: self.player_sprite.hitboxes[0],
                grounded: player.grounded,
                ground_speed_q8: player.ground_speed,
            },
        )?;
        let input = if posts.goal.is_some() && self.stage.evidence().stage_index != 10 {
            camera.start_goal_pan();
            player.ground_speed = player.ground_speed.min(player::GROUND_TOP_SPEED);
            player::Input {
                right: true,
                ..Default::default()
            }
        } else {
            input
        };
        let mut damage = false;
        let mut boxes = self.items.clone();
        let awards = boxes.tick(&mut player, &self.player_sprite, &self.stage, &self.sprites)?;
        let mut equipment = self.equipment;
        let mut lives = self.lives;
        let mut bonus_rings = 0;
        let mut rng = self.rng;
        let mut sounds = Vec::new();
        for event in awards {
            use items::{ItemEvent, ItemGrant};
            match event {
                ItemEvent::Break => sounds.push(SoundCue::start(167)),
                ItemEvent::Grant(ItemGrant::Life) => lives = lives.wrapping_add(1),
                ItemEvent::Grant(ItemGrant::Shield) => equipment.shield = Some(Shield::Normal),
                ItemEvent::Grant(ItemGrant::MagneticShield) => {
                    equipment.shield = Some(Shield::Magnetic)
                }
                ItemEvent::Grant(ItemGrant::Invincibility) => equipment.invincibility_frames = 1200,
                ItemEvent::Grant(ItemGrant::SpeedUp) => equipment.speed_frames = 1200,
                ItemEvent::Grant(ItemGrant::Rings(n)) => {
                    bonus_rings += usize::from(n);
                    sounds.push(SoundCue::start(117));
                }
                ItemEvent::Grant(ItemGrant::RandomRings) => {
                    rng = rng.wrapping_mul(0x196225).wrapping_add(0x3c6ef35f);
                    bonus_rings += usize::from(ItemGrant::random_ring_count(rng));
                    sounds.push(SoundCue::start(117));
                }
            }
        }
        player.speed_up = equipment.speed_frames > 0;
        player.invincible = equipment.invincibility_frames > 0;
        for spike in self
            .entities
            .interactables
            .records
            .iter()
            .filter(|p| p.kind == 1 && (px - p.x).abs() < 64 && (py - p.y).abs() < 64)
        {
            damage |= self
                .spikes
                .contact(&mut player, &self.stage, spike.x, spike.y)?
                == player::SpikeContact::Damage;
        }
        let mut springs = self.springs.clone();
        sounds.extend(
            springs
                .tick(
                    &self.sprites,
                    &mut player,
                    springs::SpringContext {
                        camera: (camera.x, camera.y),
                        min_y: 0,
                        max_y: self.stage.height_px() as i32,
                        ia_override: false,
                    },
                )?
                .into_iter()
                .map(|e| SoundCue::start(e.sound)),
        );
        let outcome = player.step_controls(input, down && posts.goal.is_none(), &self.stage)?;
        if outcome.jumped {
            sounds.push(SoundCue::start(116));
        }
        if let Some(song) = outcome.spin_sound {
            sounds.push(SoundCue {
                song,
                pitch: outcome.spin_pitch,
                pan: None,
            });
        }
        if posts.goal.is_some()
            && self.posts.goal.is_none()
            && self.stage.evidence().stage_index != 10
        {
            sounds.push(SoundCue::start(135));
        }
        let choice = player.animation();
        let mut actor = self.player_sprite.clone();
        if (actor.animation, u16::from(actor.variant)) != (choice.animation, choice.variant) {
            actor.hitboxes[..2].fill(None);
        }
        actor.animation = choice.animation;
        actor.variant = u8::try_from(choice.variant)
            .map_err(|_| Error("animation variant exceeds native byte".into()))?;
        actor.speed_q4_4 = choice.speed_q4_4;
        actor.flip_x = choice.flip_x;
        if player.take_animation_restart() {
            actor.restart_variant(actor.variant);
        }
        let mut ring = self.ring_sprite.clone();
        let tick = actor.tick(&self.sprites)?;
        for event in tick.events {
            if let sprite::SpriteEvent::Sound(song) = event {
                sounds.push(SoundCue::start(song));
            }
        }
        if tick.ended {
            player.animation_finished();
        }
        ring.tick(&self.sprites)?;
        let mut dust = self.dust.clone();
        if choice.animation == 18 {
            let large = player.spindash_charge() >= 512
                || dust.as_ref().is_some_and(|d| d.animation == 747);
            let id = if large { 747 } else { 746 };
            if dust.as_ref().is_none_or(|d| d.animation != id) {
                dust = Some(sprite::SpritePlayer::new(id, 0));
            }
            if let Some(d) = dust.as_mut() {
                d.flip_x = !player.facing_left;
                d.tick(&self.sprites)?;
            }
        } else {
            dust = None;
        }
        let mut collected = self.collected.clone();
        let mut effects = self.ring_effects.clone();
        effects.retain(|(_, _, sprite)| !sprite.ended);
        let mut ring_number = self.held_rings;
        let gained = if self.player.can_collect_rings() {
            collected.collect_with(&self.ring_layout, px, py, self.player.half_height, |ring| {
                ring_number += 1;
                if effects.len() < 8 {
                    effects.push((ring.x, ring.y, sprite::SpritePlayer::new(708, 0)));
                    sounds.push(SoundCue {
                        song: 117,
                        pan: Some(if ring_number % 2 == 1 { -64 } else { 64 }),
                        pitch: None,
                    });
                }
            })?
        } else {
            0
        };
        for (_, _, effect) in &mut effects {
            effect.tick(&self.sprites)?;
        }
        let before = self.held_rings;
        self.held_rings += gained + bonus_rings;
        // SA1 stores the hundred-ring award back into an unsigned byte.
        self.lives = lives.wrapping_add((self.held_rings / 100 - before / 100) as u8);
        let lose_rings = damage && equipment.shield.take().is_none();
        self.death = if outcome.fell_out {
            Some(Death::Fall)
        } else if lose_rings && self.held_rings == 0 {
            Some(Death::Spikes)
        } else {
            None
        };
        if lose_rings {
            self.held_rings = 0;
        }
        if self.death.is_some() {
            self.lives = self.lives.wrapping_sub(1);
        }
        if self.held_rings == 0 {
            self.zero_ring_phase = (self.zero_ring_phase + 1) % 32;
        }
        self.camera = camera;
        self.sounds = sounds;
        self.layer_triggers = triggers;
        self.posts = posts;
        self.springs = springs;
        self.items = boxes;
        equipment.invincibility_frames = equipment.invincibility_frames.saturating_sub(1);
        equipment.speed_frames = equipment.speed_frames.saturating_sub(1);
        self.equipment = equipment;
        self.rng = rng;
        self.collected = collected;
        self.ring_effects = effects;
        self.player = player;
        self.player_sprite = actor;
        self.ring_sprite = ring;
        self.dust = dust;
        Ok(outcome)
    }

    /// Practice adapter for continuing a stopped run. All changes commit together.
    pub fn continue_stage(&mut self) -> Result<()> {
        let index = self.stage.evidence().stage_index;
        let mut next = if self.death.is_some() {
            if self.lives == 0 {
                return Err(Error("no lives remaining".into()));
            }
            let mut next = Self::load_stage(self.rom, index)?;
            if let Some(checkpoint) = self.posts.checkpoint {
                next.player.x = checkpoint.x * player::ONE;
                next.player.y = checkpoint.y * player::ONE;
                next.camera = camera::Camera::initial(&next.player);
                next.posts.checkpoint = Some(checkpoint);
                next.checkpoint_clock = checkpoint.clock;
            }
            next
        } else if self.posts.goal.is_some_and(|goal| goal.results_ready) {
            Self::load_stage(self.rom, index + 1)?
        } else {
            return Err(Error("still playing this act".into()));
        };
        next.lives = self.lives;
        *self = next;
        Ok(())
    }

    pub fn stage_evidence(&self) -> &stage::StageEvidence {
        self.stage.evidence()
    }
    pub fn sounds(&self) -> &[SoundCue] {
        &self.sounds
    }
    pub fn equipment(&self) -> Equipment {
        self.equipment
    }
    pub fn ring_layout(&self) -> &rings::RingLayout {
        &self.ring_layout
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            stage_index: self.stage.evidence().stage_index,
            frame: self.player.frame,
            x_q8: self.player.x,
            y_q8: self.player.y,
            vx_q8: self.player.vx,
            vy_q8: self.player.vy,
            grounded: self.player.grounded,
            animation: self.player_sprite.animation,
            local_rings: self.held_rings,
            camera_x: self.camera.x,
            camera_y: self.camera.y,
            paused_after_fall: self.death == Some(Death::Fall),
            collision_layer: self.player.collision_layer,
            checkpoint: self.posts.checkpoint,
            goal: self.posts.goal,
            lives: self.lives,
            death: self.death,
        }
    }

    pub fn render(&self, output: &mut [u32]) -> Result<()> {
        if output.len() != WIDTH * HEIGHT {
            return Err(Error("native framebuffer must be 240 by 160".into()));
        }
        let (cx, cy) = (self.camera.x, self.camera.y);
        self.stage
            .render_view(cx, cy, WIDTH, HEIGHT, self.player.frame, output)?;
        let ring = self
            .sprites
            .render(&self.ring_sprite)?
            .ok_or_else(|| Error("native ring animation has no frame".into()))?;
        for position in &self.ring_layout.rings {
            if !self.collected.contains(position) {
                composite(output, &ring, position.x - cx, position.y - cy)?;
            }
        }
        for spike in self
            .entities
            .interactables
            .records
            .iter()
            .filter(|p| p.kind == 1)
        {
            composite(output, self.spikes.rendered(), spike.x - cx, spike.y - cy)?;
        }
        for (x, y, object) in self
            .posts
            .visible_sprites()
            .chain(self.springs.visible_sprites())
            .chain(self.items.visible_sprites())
        {
            if x - cx < -128
                || x - cx > WIDTH as i32 + 128
                || y - cy < -128
                || y - cy > HEIGHT as i32 + 128
            {
                continue;
            }
            if let Some(rendered) = self.sprites.render(object)? {
                composite(output, &rendered, x - cx, y - cy)?;
            }
        }
        if let Some(dust) = &self.dust {
            if let Some(frame) = self.sprites.render(dust)? {
                composite(
                    output,
                    &frame,
                    (self.player.x >> 8) - cx,
                    (self.player.y >> 8) - cy + 4,
                )?;
            }
        }
        let actor = self
            .sprites
            .render(&self.player_sprite)?
            .ok_or_else(|| Error("native player animation has no frame".into()))?;
        composite(
            output,
            &actor,
            (self.player.x >> 8) - cx,
            (self.player.y >> 8) - cy,
        )?;
        for (x, y, effect) in &self.ring_effects {
            if let Some(rendered) = self.sprites.render(effect)? {
                composite(output, &rendered, x - cx, y - cy)?;
            }
        }
        self.stage
            .overlay_front(cx, cy, WIDTH, HEIGHT, self.player.frame, output)?;
        self.hud.draw(
            output,
            hud::HudState {
                rings: self.held_rings as u32,
                score: 0,
                lives: self.lives,
                course_frames: (self.player.frame as u32).saturating_add(self.checkpoint_clock),
                stage_frames: self.player.frame as u32,
                zero_ring_phase: self.zero_ring_phase,
                timer_reversed: false,
                time_limit_disabled: false,
            },
        )
    }
}

fn composite(output: &mut [u32], sprite: &sprite::RenderedSprite, x: i32, y: i32) -> Result<()> {
    let image = &sprite.image;
    if image.width.checked_mul(image.height) != Some(image.pixels.len())
        || output.len() != WIDTH * HEIGHT
    {
        return Err(Error("sprite/framebuffer dimensions disagree".into()));
    }
    let left = i64::from(x) - i64::from(sprite.anchor_x);
    let top = i64::from(y) - i64::from(sprite.anchor_y);
    if left >= WIDTH as i64
        || top >= HEIGHT as i64
        || left + image.width as i64 <= 0
        || top + image.height as i64 <= 0
    {
        return Ok(());
    }
    for sy in 0..image.height {
        let dy = top + sy as i64;
        if !(0..HEIGHT as i64).contains(&dy) {
            continue;
        }
        for sx in 0..image.width {
            let dx = left + sx as i64;
            if !(0..WIDTH as i64).contains(&dx) {
                continue;
            }
            let c = image.pixels[sy * image.width + sx];
            match c[3] {
                0 => {}
                255 => {
                    output[dy as usize * WIDTH + dx as usize] =
                        u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2])
                }
                _ => return Err(Error("unsupported native sprite blend alpha".into())),
            }
        }
    }
    Ok(())
}

pub fn frame_sha256(frame: &[u32]) -> Result<String> {
    if frame.len() != WIDTH * HEIGHT {
        return Err(Error("wrong frame size".into()));
    }
    let bytes: Vec<u8> = frame
        .iter()
        .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8])
        .collect();
    Ok(binary::sha256(&bytes))
}

/// Repeatable exercise inputs, not a retail gameplay recording.
pub fn exercise_input(frame: u32) -> player::Input {
    player::Input {
        right: frame >= 30,
        jump: matches!(frame % 180, 100..=119),
        left: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires local ASOE ROM"]
    fn handoff_original_spin_digest_and_both_pickups() {
        let bytes = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
        let mut run = NativeRun::load(&bytes).unwrap();
        for _ in 0..20 {
            run.step(Default::default()).unwrap();
        }
        assert_eq!(run.player.y, 113920);
        let mut packed = Vec::new();
        for (i, mask) in std::iter::once(None)
            .chain(
                [128, 129, 128, 129, 128, 129, 128, 128, 128]
                    .into_iter()
                    .map(Some),
            )
            .chain(std::iter::repeat_n(Some(0), 14))
            .enumerate()
        {
            if let Some(mask) = mask {
                run.step_controls(
                    player::Input {
                        jump: mask & 1 != 0,
                        ..Default::default()
                    },
                    mask & 128 != 0,
                )
                .unwrap();
            }
            let p = &run.player;
            for n in [
                p.x,
                p.y,
                p.vx,
                p.vy,
                p.ground_speed,
                p.half_height,
                i32::from(run.player_sprite.animation),
                i32::from(run.player_sprite.variant),
            ] {
                packed.extend(n.to_le_bytes());
            }
            assert_eq!(
                run.dust.as_ref().map(|d| d.animation),
                match i {
                    2..=3 => Some(746),
                    4..=9 => Some(747),
                    _ => None,
                }
            );
        }
        assert_eq!(
            binary::sha256(&packed),
            "3cc0a91c9d8c4d7f4e501f77ae43e6ba7bb0ec3a03cc63220c4689d069c56b63"
        );
        let mut run = NativeRun::load(&bytes).unwrap();
        let mut collected = Vec::new();
        for tick in 1..=270 {
            let count = run.held_rings;
            run.step(player::Input {
                right: (21..=170).contains(&tick),
                jump: (141..=170).contains(&tick),
                left: false,
            })
            .unwrap();
            if count != run.held_rings {
                collected.push(tick);
            }
        }
        assert_eq!(collected, [198, 222]);
        for i in [14, 15] {
            assert!(run.collected.contains(&run.ring_layout.rings[i]));
        }
    }

    #[test]
    #[ignore = "requires local ASOE ROM"]
    fn practice_retry_preserves_checkpoint_clock_and_lives_and_advancing_changes_act() {
        let bytes = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
        let mut run = NativeRun::load(&bytes).unwrap();
        let cp = posts::Checkpoint {
            index: 1,
            x: 1000,
            y: 360,
            clock: 1492,
        };
        run.posts.checkpoint = Some(cp);
        run.lives = 2;
        run.death = Some(Death::Fall);
        run.continue_stage().unwrap();
        let s = run.snapshot();
        assert_eq!(
            (
                s.stage_index,
                s.lives,
                s.checkpoint,
                s.x_q8,
                s.y_q8,
                run.checkpoint_clock
            ),
            (0, 2, Some(cp), cp.x * 256, cp.y * 256, cp.clock)
        );
        assert!(run.continue_stage().is_err());
        assert_eq!(run.snapshot(), s);
        run.posts.goal = Some(posts::GoalState {
            results_ready: true,
            ..Default::default()
        });
        assert!(run.step(Default::default()).is_err());
        run.continue_stage().unwrap();
        let s = run.snapshot();
        assert_eq!(
            (
                s.stage_index,
                s.lives,
                s.checkpoint,
                s.goal,
                run.checkpoint_clock
            ),
            (1, 2, None, None, 0)
        );
        run.death = Some(Death::Spikes);
        run.lives = 0;
        let stopped = run.snapshot();
        assert!(run.continue_stage().is_err());
        assert_eq!(run.snapshot(), stopped);
        run.death = None;
        run.checkpoint_clock = u32::MAX;
        let before = run.snapshot();
        assert!(run.step(Default::default()).is_err());
        assert_eq!(run.snapshot(), before);
    }

    #[test]
    fn anchored_sprites_clip_and_preserve_transparent_background() {
        let image = crate::graphics::RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![
                [1, 2, 3, 255],
                [4, 5, 6, 0],
                [7, 8, 9, 255],
                [10, 11, 12, 255],
            ],
        };
        let sprite = sprite::RenderedSprite {
            image,
            anchor_x: 1,
            anchor_y: 1,
        };
        let mut frame = vec![0x998877; WIDTH * HEIGHT];
        composite(&mut frame, &sprite, 0, 1).unwrap();
        assert_eq!(frame[0], 0x998877);
        assert_eq!(frame[WIDTH], 0x0a0b0c);
        assert_eq!(frame.iter().filter(|&&p| p != 0x998877).count(), 1);
        composite(&mut frame, &sprite, WIDTH as i32, HEIGHT as i32).unwrap();
        assert_eq!(frame.last(), Some(&0x010203));
        let before = frame.clone();
        composite(&mut frame, &sprite, i32::MIN, i32::MAX).unwrap();
        assert_eq!(frame, before);
    }
}
