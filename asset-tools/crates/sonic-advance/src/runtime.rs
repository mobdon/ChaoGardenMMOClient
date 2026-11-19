//! First playable solo slice. Native source data and fixed-point reference
//! locomotion are combined with explicitly adapted pixel-ray terrain settlement.
//! This is not connected to canonical MMO profiles, rewards or saves.
use crate::{
    binary,
    collision::CollisionStage,
    entities::{PracticeRings, RingMap},
    hud::Hud,
    movement::{Input, Movement, MAX_ACCELERATED_SPEED, ONE},
    sprite,
    stage::{Stage, VIEW_HEIGHT, VIEW_WIDTH},
    Error, Result,
};

mod terrain;

pub const SUPPORTED_INPUT_MASK: u32 = 0xb1;
pub const BODY_HALF_WIDTH: i32 = 6;
pub const BODY_HALF_HEIGHT: i32 = 14;
const MAX_PICKUP_EFFECTS: usize = 8;

#[derive(Clone, Copy)]
struct PickupEffect {
    x: i32,
    y: i32,
    age: u32,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub frame: u64,
    pub x_fixed: i32,
    pub y_fixed: i32,
    pub vx_fixed: i32,
    pub vy_fixed: i32,
    pub grounded: bool,
    pub facing_left: bool,
    pub camera_x: i32,
    pub camera_y: i32,
    pub stage: usize,
    pub source_sha256: &'static str,
    pub resets: u32,
    pub rings: usize,
    pub animation: u16,
    pub variant: u8,
    pub rolling: bool,
    pub spindashing: bool,
    pub charge: i32,
    pub ground_speed: i32,
    pub body_half_height: i32,
    pub active_pickups: usize,
    pub dust_animation: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pose {
    Idle,
    Walk,
    Run,
    Spin,
    Crouch,
    Charge,
    Rev,
}

impl Pose {
    fn id(self) -> u16 {
        match self {
            Self::Idle => 0,
            Self::Walk => 4,
            Self::Run => 5,
            Self::Spin => 8,
            Self::Crouch => 3,
            Self::Charge | Self::Rev => 18,
        }
    }
}

pub struct Runtime {
    rom: Vec<u8>,
    stage: Stage,
    collision_header: usize,
    movement: Movement,
    idle: sprite::AnimationClip,
    walk: sprite::AnimationClip,
    run: sprite::AnimationClip,
    spin: sprite::AnimationClip,
    crouch: sprite::AnimationClip,
    charge_clip: sprite::AnimationClip,
    rev_clip: sprite::AnimationClip,
    rev_duration: u64,
    dust_small: sprite::AnimationClip,
    dust_big: sprite::AnimationClip,
    dust_id: Option<u16>,
    dust_tick: u64,
    pickup_clip: sprite::AnimationClip,
    pickup_duration: u32,
    effects: Vec<PickupEffect>,
    hud: Hud,
    ring_clip: sprite::AnimationClip,
    rings: PracticeRings,
    pose: Pose,
    pose_tick: u64,
    pixels: Vec<u8>,
    resets: u32,
}

impl Runtime {
    pub fn load(rom: Vec<u8>) -> Result<Self> {
        binary::verify_asoe(&rom)?;
        let stage = Stage::load_asoe(&rom, 0)?;
        let collision_header = Self::collision_header(&rom, 0)?;
        let idle = sprite::load_asoe_animation(&rom, 0, 0, 256, 1024 * 1024)?;
        // Walk/run use the source OBJ pixels with an explicit identity affine
        // transform. Native slope/matrix animation is a separate port gate.
        let walk = sprite::load_asoe_animation_identity(&rom, 4, 0, 256, 1024 * 1024)?;
        let run = sprite::load_asoe_animation_identity(&rom, 5, 0, 256, 1024 * 1024)?;
        let spin = sprite::load_asoe_animation(&rom, 8, 0, 256, 1024 * 1024)?;
        let ring_clip = sprite::load_asoe_animation(&rom, 707, 0, 256, 1024 * 1024)?;
        let crouch = sprite::load_asoe_animation(&rom, 3, 0, 256, 1024 * 1024)?;
        let charge_clip = sprite::load_asoe_animation(&rom, 18, 0, 256, 1024 * 1024)?;
        let rev_clip = sprite::load_asoe_animation(&rom, 18, 1, 256, 1024 * 1024)?;
        let rev_duration = rev_clip.frames.iter().map(|f| u64::from(f.duration.max(1))).sum();
        if rev_clip.loop_start.is_some() || rev_duration != 8 {
            return Err(Error("unexpected ASOE rev clip timeline".into()));
        }
        let pickup_clip = sprite::load_asoe_animation(&rom, 708, 0, 4, 32 * 32 * 4)?;
        let pickup_duration = pickup_clip.frames.iter().map(|f| f.duration).sum();
        if pickup_clip.loop_start.is_some() || pickup_duration != 32 {
            return Err(Error("unexpected ASOE pickup effect timeline".into()));
        }
        let dust_small = sprite::load_asoe_animation(&rom, 746, 0, 8, 32768)?;
        let dust_big = sprite::load_asoe_animation(&rom, 747, 0, 4, 32768)?;
        let hud = Hud::load_asoe(&rom)?;
        let rings = PracticeRings::new(RingMap::load_asoe(&rom, 0)?)?;
        let mut movement = Movement::new(stage.spawn().0, stage.spawn().1);
        movement.configure_spindash(&rom)?;
        let mut runtime = Self {
            rom,
            stage,
            collision_header,
            movement,
            idle,
            walk,
            run,
            spin,
            crouch,
            charge_clip,
            rev_clip,
            rev_duration,
            dust_small,
            dust_big,
            dust_id: None,
            dust_tick: 0,
            pickup_clip,
            pickup_duration,
            effects: Vec::with_capacity(MAX_PICKUP_EFFECTS),
            hud,
            ring_clip,
            rings,
            pose: Pose::Idle,
            pose_tick: 0,
            pixels: vec![0; VIEW_WIDTH * VIEW_HEIGHT * 4],
            resets: 0,
        };
        runtime.render()?;
        Ok(runtime)
    }

    fn collision_header(rom: &[u8], index: usize) -> Result<usize> {
        let pointer = binary::u32_at(rom, crate::collision::COLLISION_TABLE_OFFSET + index * 4)?;
        let header = binary::rom_offset(pointer, rom.len(), 4)?;
        CollisionStage::parse(rom, header)?;
        Ok(header)
    }

    /// Select a verified practice act from the existing local ROM. All source
    /// graphs are validated before changing the running act; no campaign flags
    /// or native save state are touched.
    pub fn select_act(&mut self, index: usize) -> Result<()> {
        let stage = Stage::load_asoe(&self.rom, index)?;
        let collision = Self::collision_header(&self.rom, index)?;
        let rings = PracticeRings::new(RingMap::load_asoe(&self.rom, index)?)?;
        let mut movement = Movement::new(stage.spawn().0, stage.spawn().1);
        movement.configure_spindash(&self.rom)?;
        self.stage = stage;
        self.collision_header = collision;
        self.rings = rings;
        self.movement = movement;
        self.effects.clear();
        self.dust_id = None;
        self.dust_tick = 0;
        self.pose = Pose::Idle;
        self.pose_tick = 0;
        self.resets = 0;
        self.render()
    }

    pub fn reset(&mut self) -> Result<()> {
        self.movement = Movement::new(self.stage.spawn().0, self.stage.spawn().1);
        self.movement.configure_spindash(&self.rom)?;
        self.effects.clear();
        self.dust_id = None;
        self.dust_tick = 0;
        self.rings = PracticeRings::new(self.rings.rings().to_vec())?;
        self.pose = Pose::Idle;
        self.pose_tick = 0;
        self.resets = self.resets.saturating_add(1);
        self.render()
    }

    pub fn step(&mut self, mask: u32, frames: u32) -> Result<()> {
        if mask & !SUPPORTED_INPUT_MASK != 0 || frames == 0 || frames > 4 {
            return Err(Error(
                "unsupported input bits or frame batch; maximum four ticks".into(),
            ));
        }
        let input = Input {
            left: mask & 0x20 != 0,
            right: mask & 0x10 != 0,
            jump: mask & 1 != 0,
        };
        for _ in 0..frames {
            self.advance(input, mask & 0x80 != 0)?;
        }
        self.render()
    }

    fn advance(&mut self, input: Input, down: bool) -> Result<()> {
        let geometry = CollisionStage::parse(&self.rom, self.collision_header)?;
        let before = self.movement.player;
        let old_x = before.x.div_euclid(ONE);
        let old_y = before.y.div_euclid(ONE);
        let old_height = self.movement.body_half_height();
        let clearance = geometry
            .ceiling(old_x, old_y - old_height, 0x81, 4)?
            .map_or(4, |h| h.distance);
        self.movement
            .advance_with_actions(input, down, i32::from(clearance))?;
        if self.pose == Pose::Rev && self.pose_tick + 1 >= self.rev_duration
            && self.movement.frame_input() & 1 == 0 {
            self.movement.notify_animation_finished();
        }
        // The dust task starts at 746, upgrades to 747 on its first charge
        // >= Q(2), and stays large until release even if the charge decays.
        let next_dust = self.movement.is_spindashing().then(|| {
            if self.dust_id == Some(747) || self.movement.spindash_charge() >= 2 * ONE {
                747
            } else { 746 }
        });
        if next_dust == self.dust_id && next_dust.is_some() {
            self.dust_tick = self.dust_tick.wrapping_add(1);
        } else {
            self.dust_tick = 0;
        }
        self.dust_id = next_dust;
        let height = self.movement.body_half_height();
        let mut x = self.movement.player.x.div_euclid(ONE);
        let y = self.movement.player.y.div_euclid(ONE);
        // Adapted sweep for this first slice: native directional occupancy,
        // bounded body samples, no teleport through a wall between ticks.
        let direction = (x - old_x).signum();
        if direction != 0 {
            let distance = (x - old_x).unsigned_abs().min(128);
            for offset in 1..=distance {
                let center = old_x + direction * offset as i32;
                let leading = center + direction * BODY_HALF_WIDTH;
                let mut blocked = false;
                for sy in [y - 8, y, y + 8] {
                    if geometry
                        .sample(leading, sy, 0x81)?
                        .is_some_and(|p| p.solid_horizontal)
                    {
                        blocked = true;
                        break;
                    }
                }
                if blocked {
                    x = center - direction;
                    self.movement.player.x = x * ONE;
                    self.movement.player.vx = 0;
                    self.movement.player.ground_speed = 0;
                    break;
                }
            }
        }
        if y - height < old_y - old_height {
            let travel = (old_y - old_height - (y - height)).min(128) as u16;
            let mut ceiling = None;
            for sensor in [x - 5, x + 5] {
                if let Some(hit) =
                    geometry.ceiling(sensor, old_y - old_height, 0x81, travel)?
                {
                    if ceiling.is_none_or(|best: i32| hit.y > best) {
                        ceiling = Some(hit.y);
                    }
                }
            }
            if let Some(surface) = ceiling {
                self.movement.player.y = (surface + height + 1) * ONE;
                self.movement.player.vy = 0;
            }
        }
        if self.movement.player.grounded {
            let (floor, _) = terrain::floor_pair(&geometry, self.movement.player.x,
                self.movement.player.y, height, 1)?;
            self.movement.follow_floor(floor.distance, floor.angle);
        } else if !before.grounded && self.movement.player.vy >= 0 {
            let (floor, other) = terrain::floor_pair(&geometry, self.movement.player.x,
                self.movement.player.y, height, 1)?;
            let limit = -(self.movement.player.vy.div_euclid(ONE) + 6);
            if floor.distance < 0 && (floor.distance >= limit || other.distance >= limit) {
                self.movement.land_from_floor(floor.distance, floor.angle);
            }
        }
        let previous_x = self.movement.player.x;
        self.movement.player.x = previous_x.clamp(
            BODY_HALF_WIDTH * ONE,
            (self.stage.width() - BODY_HALF_WIDTH) * ONE,
        );
        if self.movement.player.x != previous_x {
            self.movement.player.vx = 0;
            self.movement.player.ground_speed = 0;
        }
        if self.movement.player.y > (self.stage.height() + 256) * ONE {
            // Explicit solo-practice recovery; native death/lives/checkpoints
            // remain unported. This awards nothing and never writes a save.
            return self.reset();
        }
        let player = self.movement.player;
        self.effects.retain_mut(|effect| {
            effect.age += 1;
            effect.age < self.pickup_duration
        });
        self.rings.collect_at_with(
            player.x.div_euclid(ONE),
            player.y.div_euclid(ONE),
            BODY_HALF_WIDTH,
            height,
            |ring| {
                if self.effects.len() < MAX_PICKUP_EFFECTS {
                    self.effects.push(PickupEffect { x: ring.x, y: ring.y, age: 0 });
                }
            },
        )?;
        // The run threshold follows pinned SA1 source; clip timing uses the
        // original script duration at unit speed (native speed scaling pending).
        // All airborne states use spin in this slice, including ledge falls.
        let pose = if let Some(action) = self.movement.native_animation() {
            match action {
                (3, 0) => Pose::Crouch,
                (8, 0) => Pose::Spin,
                (18, 0) => Pose::Charge,
                (18, 1) => Pose::Rev,
                _ => return Err(Error("unhandled native action animation".into())),
            }
        } else if !player.grounded {
            Pose::Spin
        } else if player.ground_speed.abs() >= MAX_ACCELERATED_SPEED {
            Pose::Run
        } else if player.ground_speed != 0 {
            Pose::Walk
        } else {
            Pose::Idle
        };
        if pose == self.pose && !(pose == Pose::Rev && self.movement.frame_input() & 1 != 0) {
            self.pose_tick = self.pose_tick.wrapping_add(1);
        } else {
            self.pose = pose;
            self.pose_tick = 0;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        let p = self.movement.player;
        let (camera_x, camera_y) = self.stage.camera(
            p.x.div_euclid(ONE),
            p.y.div_euclid(ONE),
            VIEW_WIDTH,
            VIEW_HEIGHT,
        );
        Snapshot {
            frame: p.frame,
            x_fixed: p.x,
            y_fixed: p.y,
            vx_fixed: p.vx,
            vy_fixed: p.vy,
            grounded: p.grounded,
            facing_left: p.facing_left,
            camera_x,
            camera_y,
            stage: self.stage.index(),
            source_sha256: binary::ROM_SHA256,
            resets: self.resets,
            rings: self.rings.collected_count(),
            animation: self.pose.id(),
            variant: u8::from(self.pose == Pose::Rev),
            rolling: self.movement.is_rolling(),
            spindashing: self.movement.is_spindashing(),
            charge: self.movement.spindash_charge(),
            ground_speed: p.ground_speed,
            body_half_height: self.movement.body_half_height(),
            active_pickups: self.effects.len(),
            dust_animation: self.dust_id,
        }
    }

    pub fn title_screen(&self) -> Result<crate::title::Title> {
        crate::title::Title::load_asoe(&self.rom)
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    fn render(&mut self) -> Result<()> {
        let state = self.snapshot();
        self.stage.render_at(
            &mut self.pixels,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            state.camera_x,
            state.camera_y,
            state.frame,
        )?;
        let ring_frame = animation_frame(&self.ring_clip, state.frame)?;
        for ring in self.rings.rings() {
            let x = ring.x - state.camera_x;
            let y = ring.y - state.camera_y;
            if !self.rings.is_collected(ring.id)
                && (-64..VIEW_WIDTH as i32 + 64).contains(&x)
                && (-64..VIEW_HEIGHT as i32 + 64).contains(&y)
            {
                sprite::composite_rgba(
                    &mut self.pixels,
                    VIEW_WIDTH,
                    VIEW_HEIGHT,
                    &ring_frame.sprite,
                    x,
                    y,
                    false,
                )?;
            }
        }
        if let Some(id) = self.dust_id {
            let clip = if id == 747 { &self.dust_big } else { &self.dust_small };
            let frame = animation_frame(clip, self.dust_tick)?;
            // ASOE spindash dust is anchored four pixels below the player.
            // Its OAM order 17 puts it behind the player at order 16.
            sprite::composite_rgba(&mut self.pixels, VIEW_WIDTH, VIEW_HEIGHT, &frame.sprite,
                state.x_fixed.div_euclid(ONE) - state.camera_x,
                state.y_fixed.div_euclid(ONE) - state.camera_y + 4, !state.facing_left)?;
        }
        let clip = match self.pose {
            Pose::Idle => &self.idle,
            Pose::Walk => &self.walk,
            Pose::Run => &self.run,
            Pose::Spin => &self.spin,
            Pose::Crouch => &self.crouch,
            Pose::Charge => &self.charge_clip,
            Pose::Rev => &self.rev_clip,
        };
        let frame = animation_frame(clip, self.pose_tick)?;
        // Unreflected native Sonic art faces left.
        sprite::composite_rgba(
            &mut self.pixels,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            &frame.sprite,
            state.x_fixed.div_euclid(ONE) - state.camera_x,
            state.y_fixed.div_euclid(ONE) - state.camera_y,
            !state.facing_left,
        )?;
        for effect in &self.effects {
            let frame = animation_frame(&self.pickup_clip, u64::from(effect.age))?;
            sprite::composite_rgba(&mut self.pixels, VIEW_WIDTH, VIEW_HEIGHT, &frame.sprite,
                effect.x - state.camera_x, effect.y - state.camera_y, false)?;
        }
        self.hud.render_into(&mut self.pixels, VIEW_WIDTH, VIEW_HEIGHT, state.rings as u32, (state.frame % 32) as u8)?;
        Ok(())
    }
}

fn animation_frame(clip: &sprite::AnimationClip, frame: u64) -> Result<&sprite::AnimationFrame> {
    if clip.frames.is_empty() {
        return Err(Error("empty native animation".into()));
    }
    let loop_start = clip.loop_start.unwrap_or(clip.frames.len() - 1);
    let intro: u64 = clip.frames[..loop_start]
        .iter()
        .map(|f| u64::from(f.duration.max(1)))
        .sum();
    let period: u64 = clip.frames[loop_start..]
        .iter()
        .map(|f| u64::from(f.duration.max(1)))
        .sum();
    let mut tick = if frame < intro {
        frame
    } else {
        intro + (frame - intro) % period
    };
    for item in &clip.frames {
        let duration = u64::from(item.duration.max(1));
        if tick < duration {
            return Ok(item);
        }
        tick -= duration;
    }
    Err(Error("invalid animation time".into()))
}
