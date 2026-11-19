//! First playable solo slice. Native source data and fixed-point reference
//! locomotion are combined with explicitly adapted pixel-ray terrain settlement.
//! This is not connected to canonical MMO profiles, rewards or saves.
use crate::{
    binary,
    collision::CollisionStage,
    entities::{PracticeRings, RingMap},
    movement::{Input, Movement, MAX_ACCELERATED_SPEED, ONE},
    sprite,
    stage::{Stage, VIEW_HEIGHT, VIEW_WIDTH},
    Error, Result,
};

pub const SUPPORTED_INPUT_MASK: u32 = 0x31;
pub const BODY_HALF_WIDTH: i32 = 6;
pub const BODY_HALF_HEIGHT: i32 = 14;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pose {
    Idle,
    Walk,
    Run,
    Spin,
}

impl Pose {
    fn id(self) -> u16 {
        match self {
            Self::Idle => 0,
            Self::Walk => 4,
            Self::Run => 5,
            Self::Spin => 8,
        }
    }
}

pub struct Runtime {
    rom: Vec<u8>,
    stage: Stage,
    movement: Movement,
    idle: sprite::AnimationClip,
    walk: sprite::AnimationClip,
    run: sprite::AnimationClip,
    spin: sprite::AnimationClip,
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
        CollisionStage::load_asoe(&rom, 0)?;
        let idle = sprite::load_asoe_animation(&rom, 0, 0, 256, 1024 * 1024)?;
        // Walk/run use the source OBJ pixels with an explicit identity affine
        // transform. Native slope/matrix animation is a separate port gate.
        let walk = sprite::load_asoe_animation_identity(&rom, 4, 0, 256, 1024 * 1024)?;
        let run = sprite::load_asoe_animation_identity(&rom, 5, 0, 256, 1024 * 1024)?;
        let spin = sprite::load_asoe_animation(&rom, 8, 0, 256, 1024 * 1024)?;
        let ring_clip = sprite::load_asoe_animation(&rom, 707, 0, 256, 1024 * 1024)?;
        let rings = PracticeRings::new(RingMap::load_asoe(&rom, 0)?)?;
        let movement = Movement::new(stage.spawn().0, stage.spawn().1);
        let mut runtime = Self {
            rom,
            stage,
            movement,
            idle,
            walk,
            run,
            spin,
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

    pub fn reset(&mut self) -> Result<()> {
        self.movement = Movement::new(self.stage.spawn().0, self.stage.spawn().1);
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
            self.advance(input)?;
        }
        self.render()
    }

    fn advance(&mut self, input: Input) -> Result<()> {
        let geometry = CollisionStage::parse(&self.rom, 0x4bf1f8)?;
        let before = self.movement.player;
        let old_x = before.x.div_euclid(ONE);
        let old_y = before.y.div_euclid(ONE);
        let clearance = geometry
            .ceiling(old_x, old_y - BODY_HALF_HEIGHT, 0x81, 4)?
            .map_or(4, |h| h.distance);
        self.movement
            .advance_with_jump_clearance(input, i32::from(clearance));
        let mut x = self.movement.player.x.div_euclid(ONE);
        let y = self.movement.player.y.div_euclid(ONE);
        // Adapted sweep for this first slice: native directional occupancy,
        // bounded body samples, no teleport through a wall between ticks.
        let direction = (x - old_x).signum();
        if direction != 0 {
            let distance = (x - old_x).unsigned_abs().min(64);
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
        if y < old_y {
            let travel = (old_y - y).min(64) as u16;
            let mut ceiling = None;
            for sensor in [x - 5, x + 5] {
                if let Some(hit) =
                    geometry.ceiling(sensor, old_y - BODY_HALF_HEIGHT, 0x81, travel)?
                {
                    if ceiling.is_none_or(|best: i32| hit.y > best) {
                        ceiling = Some(hit.y);
                    }
                }
            }
            if let Some(surface) = ceiling {
                self.movement.player.y = (surface + BODY_HALF_HEIGHT + 1) * ONE;
                self.movement.player.vy = 0;
            }
        }
        if self.movement.player.vy >= 0 {
            let y_now = self.movement.player.y.div_euclid(ONE);
            let old_foot = old_y + BODY_HALF_HEIGHT;
            let new_foot = y_now + BODY_HALF_HEIGHT;
            let start = if before.grounded {
                old_foot - 16
            } else {
                old_foot.min(new_foot)
            };
            let search_end = if before.grounded {
                old_foot + 16
            } else {
                new_foot
            };
            let distance = (search_end - start).clamp(0, 96) as u16;
            let mut floor = None;
            for sensor in [x - 5, x + 5] {
                if let Some(hit) = geometry.floor(sensor, start, 1, distance)? {
                    if floor.is_none_or(|best: i32| hit.y < best) {
                        floor = Some(hit.y);
                    }
                }
            }
            if let Some(surface) = floor {
                self.movement.land_at(surface - BODY_HALF_HEIGHT);
            } else {
                self.movement.player.grounded = false;
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
        self.rings.collect_at(
            player.x.div_euclid(ONE),
            player.y.div_euclid(ONE),
            BODY_HALF_WIDTH,
            BODY_HALF_HEIGHT,
        )?;
        // The run threshold follows pinned SA1 source; clip timing uses the
        // original script duration at unit speed (native speed scaling pending).
        // All airborne states use spin in this slice, including ledge falls.
        let pose = if !player.grounded {
            Pose::Spin
        } else if player.ground_speed.abs() >= MAX_ACCELERATED_SPEED {
            Pose::Run
        } else if player.ground_speed != 0 {
            Pose::Walk
        } else {
            Pose::Idle
        };
        if pose == self.pose {
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
            stage: 0,
            source_sha256: binary::ROM_SHA256,
            resets: self.resets,
            rings: self.rings.collected_count(),
            animation: self.pose.id(),
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    fn render(&mut self) -> Result<()> {
        let state = self.snapshot();
        self.stage.render_into(
            &mut self.pixels,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            state.camera_x,
            state.camera_y,
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
        let clip = match self.pose {
            Pose::Idle => &self.idle,
            Pose::Walk => &self.walk,
            Pose::Run => &self.run,
            Pose::Spin => &self.spin,
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
