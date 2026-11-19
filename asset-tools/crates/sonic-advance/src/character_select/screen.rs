//! Native carousel portraits (animation 722) and tiled BG1 (animation 723).
use super::{Carousel, Character, Selection};
use crate::{
    binary,
    graphics::{decode_palette, decode_tile_indices, BitsPerPixel},
    sprite::{RenderedSprite, SpriteBank, SpritePlayer},
    Error, Result,
};
const W: usize = 240;
const H: usize = 160;
pub struct Screen<'a> {
    bank: SpriteBank<'a>,
    confirmed_player: Option<SpritePlayer>,
    confirmed_image: Option<RenderedSprite>,
    confirm_age: u8,
    pub(crate) carousel: Carousel,
    backgrounds: Vec<Vec<u8>>,
    portraits: Vec<RenderedSprite>,
    overlay: super::overlay::Overlay,
    bg: usize,
    xs: [i32; 3],
    ids: [usize; 3],
    held: u16,
    repeat: [u8; 10],
    frame: u32,
    opening: u8,
}
impl<'a> Screen<'a> {
    pub fn load(rom: &'a [u8], held: u16) -> Result<Self> {
        if held & !1023 != 0 {
            return Err(Error("invalid initial buttons".into()));
        }
        let bank = SpriteBank::from_asoe(rom)?;
        let mut portraits = Vec::new();
        let mut backgrounds = Vec::new();
        for i in 0..4 {
            let mut player = SpritePlayer::new(722, 2 * i);
            player.tick(&bank)?;
            portraits.push(
                bank.render(&player)?
                    .ok_or_else(|| Error("missing character portrait".into()))?,
            );
            let at = [0x3e14b4, 0x3d33b4, 0x3dc9b4, 0x3d7eb4][i as usize];
            let pal = [0x11b8d4, 0x11b8b4, 0x11b8f4, 0x11b8d4][i as usize];
            let indices =
                decode_tile_indices(binary::range(rom, at, 600 * 32)?, BitsPerPixel::Four, W * H)?;
            let colors = decode_palette(binary::range(rom, pal, 32)?, 16)?;
            let mut bg = Vec::with_capacity(W * H * 4);
            for y in 0..H {
                for x in 0..W {
                    let tx = x / 8;
                    let col = [0, 8, 16, 24, 28]
                        .into_iter()
                        .rev()
                        .find(|&c| c <= tx)
                        .unwrap_or(0);
                    let width = match col {
                        28 => 2,
                        24 => 4,
                        _ => 8,
                    };
                    let tile = col * 20 + (y / 8) * width + tx - col;
                    bg.extend_from_slice(
                        &colors[indices[tile * 64 + (y % 8) * 8 + x % 8] as usize],
                    );
                }
            }
            backgrounds.push(bg);
        }
        Ok(Self {
            carousel: Carousel::new(Character::Sonic),
            bank,
            confirmed_player: None,
            confirmed_image: None,
            confirm_age: 0,
            backgrounds,
            portraits,
            overlay: super::overlay::Overlay::load(rom)?,
            bg: 0,
            xs: [-50, 280, 290],
            ids: [3, 0, 1],
            held,
            repeat: [20; 10],
            frame: 0,
            opening: 15,
        })
    }
    pub fn character(&self) -> Character {
        self.carousel.character()
    }
    pub fn selection(&self) -> Option<Selection> {
        self.carousel.selection()
    }
    pub fn confirmation_finished(&self) -> bool {
        self.confirm_age >= 57
    }
    fn confirm_tick(&mut self) -> Result<()> {
        if self.confirmation_finished() {
            return Ok(());
        }
        self.frame = self.frame.saturating_add(1);
        self.confirm_age += 1;
        let character = self.character() as u8;
        let p = self
            .confirmed_player
            .get_or_insert_with(|| SpritePlayer::new(722, 2 * character + 1));
        p.tick(&self.bank)?;
        self.confirmed_image = self.bank.render(p)?;
        if self.confirm_age <= 5 {
            self.xs[0] -= 20;
            self.xs[2] += 20;
        }
        self.overlay.confirm_tick(self.confirm_age);
        Ok(())
    }
    pub fn frame(&self) -> u32 {
        self.frame
    }
    pub fn step(&mut self, held: u16) -> Result<()> {
        if held & !1023 != 0 {
            return Err(Error("invalid carousel input".into()));
        }
        if matches!(self.selection(), Some(Selection::Confirm { .. })) {
            return self.confirm_tick();
        }
        if self.selection().is_some() {
            return Ok(());
        }
        let pressed = held & !self.held;
        self.held = held;
        let mut repeated = pressed;
        for (i, count) in self.repeat.iter_mut().enumerate() {
            let bit = 1 << i;
            if held & bit == 0 {
                *count = 20;
            } else if *count == 0 {
                repeated |= bit;
                *count = 8;
            } else {
                *count -= 1;
            }
        }
        if self.opening != 0 {
            self.opening -= 1;
            return Ok(());
        }
        self.carousel.step(pressed, repeated)?;
        if matches!(self.selection(), Some(Selection::Confirm { .. })) {
            return self.confirm_tick();
        }
        self.frame = self.frame.saturating_add(1);
        self.overlay.tick(self.frame, self.carousel.angle_q8());
        self.bg = usize::from((self.carousel.angle_q8() & 255) >> 6);

        if self.frame < 29 {
            if self.frame >= 17 {
                let n = self.frame as i32 - 16;
                self.xs = [n * 32 / 11 - 32, 256 - n * 128 / 11, 284 - n * 32 / 11];
            }
        } else if self.carousel.direction == 0 {
            self.xs = [-8, 120, 248];
        } else {
            let right = self.carousel.direction == 1;
            let angle = self.carousel.angle_q8();
            let slot = usize::from(angle >> 8);
            let phase = i32::from((angle & 255) >> 4);
            let phase = if right { phase } else { 16 - phase };
            let sign = if right { -1 } else { 1 };
            if phase == 4 {
                let middle = (slot + usize::from(right)) % 4;
                self.ids = [(middle + 3) % 4, middle, (middle + 1) % 4];
                for x in &mut self.xs {
                    *x -= sign * 128;
                }
            }
            if phase <= 8 {
                for x in &mut self.xs {
                    *x += sign * 16;
                }
            }
        }

        Ok(())
    }
    pub fn render_into(&self, out: &mut [u8]) -> Result<()> {
        if out.len() != W * H * 4 {
            return Err(Error("character screen needs 240x160 RGBA".into()));
        }
        out.copy_from_slice(&self.backgrounds[self.bg]);
        if self.opening != 0 {
            let k = u16::from(self.opening + 1);
            for pixel in out.as_chunks_mut::<4>().0 {
                for (i, c) in pixel[..3].iter_mut().enumerate() {
                    let v = u16::from(*c >> 3);
                    let v = if i == 2 {
                        v - v * k / 16
                    } else {
                        v * (16 - k) / 16
                    };
                    *c = ((v << 3) | (v >> 2)) as u8;
                }
            }
            return Ok(());
        }
        for (i, (&x, &id)) in self.xs.iter().zip(&self.ids).enumerate().rev() {
            let s = if i == 1 {
                self.confirmed_image.as_ref().unwrap_or(&self.portraits[id])
            } else {
                &self.portraits[id]
            };
            let w = s.image.width;
            for (n, p) in s.image.pixels.iter().enumerate() {
                let px = x - i32::from(s.anchor_x) + (n % w) as i32;
                let py = 110 - i32::from(s.anchor_y) + (n / w) as i32;
                if p[3] != 0 && (0..240).contains(&px) && (0..160).contains(&py) {
                    let at = (py as usize * W + px as usize) * 4;
                    out[at..at + 4].copy_from_slice(p);
                    let right = match self.frame {
                        0..=16 => 200,
                        17..=20 => 240,
                        21..=27 => 240 - (self.frame as i32 - 21) * 5,
                        _ => 200,
                    };
                    if self.confirm_age == 0 && !(40..right).contains(&px) {
                        for c in &mut out[at..at + 3] {
                            let v = *c >> 3;
                            let v = v + (31 - v) / 2;
                            *c = v << 3 | v >> 2;
                        }
                    }
                }
            }
        }
        self.overlay.draw(
            out,
            self.frame,
            self.carousel.angle_q8(),
            self.confirm_age > 0,
        );
        let k = u16::from(self.confirm_age.saturating_sub(39).min(16));
        if k > 0 {
            for pixel in out.as_chunks_mut::<4>().0 {
                for (i, c) in pixel[..3].iter_mut().enumerate() {
                    let v = u16::from(*c >> 3);
                    let v = if i == 2 {
                        v - v * k / 16
                    } else {
                        v * (16 - k) / 16
                    };
                    *c = ((v << 3) | (v >> 2)) as u8;
                }
            }
        }
        Ok(())
    }
}
