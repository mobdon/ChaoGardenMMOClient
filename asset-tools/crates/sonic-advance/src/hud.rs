//! Single-player Sonic stage HUD, from the retail UI graphics table.
use crate::{binary, graphics, tables, Error, Result};

const WIDTH: usize = 240;
const HEIGHT: usize = 160;
const MILLIS: usize = 0x657ef0;
const SECONDS: usize = 0x657f68;
const ZERO_FLASH: usize = 0x658070;

#[derive(Clone, Copy, Debug, Default)]
pub struct HudState {
    pub rings: u32,
    pub score: u32,
    /// Includes the current life; the cartridge displays the reserve count.
    pub lives: u8,
    pub course_frames: u32,
    pub stage_frames: u32,
    /// Eight frames per entry in the four-entry ring flash table.
    pub zero_ring_phase: u8,
    pub timer_reversed: bool,
    pub time_limit_disabled: bool,
}

struct ObjImage {
    width: usize,
    height: usize,
    pixels: Vec<Option<u32>>,
}

impl ObjImage {
    fn decode(
        rom: &[u8],
        tile_offset: usize,
        palette_offset: usize,
        width: usize,
        height: usize,
    ) -> Result<Self> {
        let packed = binary::range(rom, tile_offset, width * height / 2)?;
        let palette = graphics::decode_palette(binary::range(rom, palette_offset, 32)?, 16)?;
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let tile = (y / 8) * (width / 8) + x / 8;
                let byte = packed[tile * 32 + (y % 8) * 4 + x % 8 / 2];
                let index = (byte >> ((x % 2) * 4)) & 15;
                let [r, g, b, _] = palette[usize::from(index)];
                let color = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
                pixels.push((index != 0).then_some(color));
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    fn draw(&self, target: &mut [u32], left: i32, top: i32) {
        for y in 0..self.height {
            let dy = top + y as i32;
            if !(0..HEIGHT as i32).contains(&dy) {
                continue;
            }
            for x in 0..self.width {
                let dx = left + x as i32;
                if (0..WIDTH as i32).contains(&dx) {
                    if let Some(color) = self.pixels[y * self.width + x] {
                        target[dy as usize * WIDTH + dx as usize] = color;
                    }
                }
            }
        }
    }
}

pub struct Hud {
    glyphs: Vec<ObjImage>,
    ring: ObjImage,
    sonic: ObjImage,
    millis: [u8; 120],
    seconds: [u8; 120],
    zero_flash: [u8; 4],
}

impl Hud {
    pub fn from_asoe(rom: &[u8]) -> Result<Self> {
        let records = tables::asoe_ui_graphics(rom)?;
        let ascii = &records[0];
        let face = &records[1];
        let ring = &records[5];
        let mut glyphs = Vec::with_capacity(22);
        for base_tile in [32, 224] {
            for i in 0..11 {
                glyphs.push(ObjImage::decode(
                    rom,
                    ascii.tile_offset + (base_tile + i * 2) * 32,
                    ring.palette_offset,
                    8,
                    16,
                )?);
            }
        }
        let millis = binary::range(rom, MILLIS, 120)?.try_into().unwrap();
        let seconds = binary::range(rom, SECONDS, 120)?.try_into().unwrap();
        let zero_flash = binary::range(rom, ZERO_FLASH, 4)?.try_into().unwrap();
        Ok(Self {
            glyphs,
            ring: ObjImage::decode(rom, ring.tile_offset, ring.palette_offset, 32, 32)?,
            sonic: ObjImage::decode(rom, face.tile_offset, face.palette_offset, 16, 16)?,
            millis,
            seconds,
            zero_flash,
        })
    }

    fn digit(&self, target: &mut [u32], glyph: u8, x: i32, y: i32) {
        self.glyphs[usize::from(glyph)].draw(target, x, y);
    }
    fn number(&self, target: &mut [u32], number: u32, right_x: i32, y: i32) {
        let mut value = number;
        let mut x = right_x;
        loop {
            self.digit(target, (value % 10) as u8, x, y);
            value /= 10;
            if value == 0 {
                break;
            }
            x -= 8;
        }
    }

    pub fn draw(&self, target: &mut [u32], s: HudState) -> Result<()> {
        if target.len() != WIDTH * HEIGHT {
            return Err(Error("HUD requires a 240 x 160 framebuffer".into()));
        }
        // The icon objects have later OAM indexes than the digits.
        self.sonic.draw(target, 2, 144);
        self.ring.draw(target, -2, -1);
        self.digit(target, s.lives.saturating_sub(1).min(9), 22, 143);
        let clamped = s.course_frames.min(35999);
        let timer = if s.course_frames >= 36000 {
            [9, 10, 5, 9, 10, 9, 9]
        } else {
            let min = (clamped / 3600) as usize;
            let sec = ((clamped / 60) % 60) as usize * 2;
            let sub = (clamped % 60) as usize * 2;
            [
                self.seconds[min * 2 + 1] - 32,
                10,
                self.seconds[sec] - 32,
                self.seconds[sec + 1] - 32,
                10,
                self.millis[sub] - 32,
                self.millis[sub + 1] - 32,
            ]
        };
        let warning = if s.timer_reversed {
            s.course_frames < 3600
        } else {
            s.course_frames > 32400
        };
        let offset = if warning && !s.time_limit_disabled && s.stage_frames & 16 != 0 {
            11
        } else {
            0
        };
        for (i, digit) in timer.into_iter().enumerate() {
            self.digit(target, digit + offset, 25 + i as i32 * 8, 12);
        }
        // Score has nine native slots. The source suppresses every leading zero,
        // including all slots when a nonzero score wraps its displayed digits.
        let score = s.score % 1_000_000_000;
        if s.score == 0 || score != 0 {
            self.number(target, score, 73, -2);
        }
        let rings = s.rings.min(999);
        if rings == 0 {
            self.digit(
                target,
                self.zero_flash[usize::from(s.zero_ring_phase % 32) / 8] - 32,
                9,
                5,
            );
        } else {
            let right = if rings < 10 {
                9
            } else if rings < 100 {
                13
            } else {
                17
            };
            self.number(target, rings, right, 5);
        }
        Ok(())
    }
}
