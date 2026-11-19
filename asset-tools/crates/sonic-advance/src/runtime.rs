//! Local deterministic stage runtime, without ROM instruction execution,
//! profiles, network endpoints or transferable progress.
use crate::{binary, player, rings, sprite, stage, Error, Result};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const MAX_REPLAY_FRAMES: u32 = 36_000;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
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
}

pub struct NativeRun<'a> {
    stage: stage::Stage<'a>,
    sprites: sprite::SpriteBank<'a>,
    player: player::Player,
    player_sprite: sprite::SpritePlayer,
    ring_sprite: sprite::SpritePlayer,
    ring_layout: rings::RingLayout,
    collected: rings::LocalCollection,
    paused_after_fall: bool,
}

impl<'a> NativeRun<'a> {
    pub fn load(rom: &'a [u8]) -> Result<Self> {
        let mut run = Self {
            stage: stage::Stage::load(rom, 0)?,
            sprites: sprite::SpriteBank::from_asoe(rom)?,
            player: player::Player::sonic_first_stage(rom)?,
            player_sprite: sprite::SpritePlayer::new(0, 0),
            ring_sprite: sprite::SpritePlayer::new(707, 0),
            ring_layout: rings::RingLayout::load(rom, 0)?,
            collected: rings::LocalCollection::default(),
            paused_after_fall: false,
        };
        run.player_sprite.tick(&run.sprites)?;
        run.ring_sprite.tick(&run.sprites)?;
        Ok(run)
    }

    /// One 60 Hz input frame; no wall clock or offline time grants progress.
    pub fn step(&mut self, input: player::Input) -> Result<player::FrameOutcome> {
        if self.paused_after_fall {
            return Err(Error("run fell out of the stage; reset explicitly".into()));
        }
        let mut player = self.player.clone();
        let outcome = player.step(input, &self.stage)?;
        let choice = player.animation();
        let mut actor = self.player_sprite.clone();
        actor.animation = choice.animation;
        actor.variant = u8::try_from(choice.variant)
            .map_err(|_| Error("animation variant exceeds native byte".into()))?;
        actor.speed_q4_4 = choice.speed_q4_4;
        actor.flip_x = choice.flip_x;
        let mut ring = self.ring_sprite.clone();
        actor.tick(&self.sprites)?;
        ring.tick(&self.sprites)?;
        self.collected.collect(
            &self.ring_layout,
            player.x >> 8,
            player.y >> 8,
            player.half_height,
        )?;
        self.player = player;
        self.player_sprite = actor;
        self.ring_sprite = ring;
        self.paused_after_fall = outcome.fell_out;
        Ok(outcome)
    }

    pub fn stage_evidence(&self) -> &stage::StageEvidence {
        self.stage.evidence()
    }
    pub fn ring_layout(&self) -> &rings::RingLayout {
        &self.ring_layout
    }

    pub fn snapshot(&self) -> Snapshot {
        let (camera_x, camera_y) = self.camera();
        Snapshot {
            frame: self.player.frame,
            x_q8: self.player.x,
            y_q8: self.player.y,
            vx_q8: self.player.vx,
            vy_q8: self.player.vy,
            grounded: self.player.grounded,
            animation: self.player_sprite.animation,
            local_rings: self.collected.count(),
            camera_x,
            camera_y,
            paused_after_fall: self.paused_after_fall,
        }
    }

    /// Center/clamp is a testbed adaptation; native camera triggers are open.
    fn camera(&self) -> (i32, i32) {
        let x = ((self.player.x >> 8) - WIDTH as i32 / 2)
            .clamp(0, self.stage.width_px().saturating_sub(WIDTH) as i32);
        let y = ((self.player.y >> 8) - HEIGHT as i32 / 2)
            .clamp(0, self.stage.height_px().saturating_sub(HEIGHT) as i32);
        (x, y)
    }

    pub fn render(&self, output: &mut [u32]) -> Result<()> {
        if output.len() != WIDTH * HEIGHT {
            return Err(Error("native framebuffer must be 240 by 160".into()));
        }
        let (cx, cy) = self.camera();
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
        self.stage
            .overlay_front(cx, cy, WIDTH, HEIGHT, self.player.frame, output)
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
