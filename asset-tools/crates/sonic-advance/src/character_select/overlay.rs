use crate::{
    binary,
    graphics::{decode_palette, decode_tile_indices, BitsPerPixel},
    Result,
};

pub(super) struct Overlay {
    sine: [i16; 384],
    tiles: Vec<Vec<u8>>,
    common: Vec<[u8; 4]>,
    bands: Vec<Vec<[u8; 4]>>,
    pub(super) scroll: i32,
    pub(super) base: usize,
    pub(super) name: usize,
    label_anchor: usize,
    pub(super) previous_gap: i32,
}
impl Overlay {
    pub(super) fn load(rom: &[u8]) -> Result<Self> {
        let mut tiles = Vec::new();
        for (a, n) in [
            (0x670b78, 0x400),
            (0x670f98, 0x400),
            (0x6713b8, 0x3000),
            (0x6743d8, 0x800),
            (0x674bf8, 0x100),
            (0x674d18, 0x200),
        ] {
            tiles.push(decode_tile_indices(
                binary::range(rom, a, n)?,
                BitsPerPixel::Four,
                n * 2,
            )?);
        }
        let common = decode_palette(binary::range(rom, 0x671398, 32)?, 16)?;
        let mut bands = Vec::new();
        for i in 0..4 {
            bands.push(decode_palette(binary::range(rom, 0x670f78 + 6 * i, 8)?, 4)?);
        }
        let mut sine = [0; 384];
        for (i, v) in sine.iter_mut().enumerate() {
            *v = binary::u16_at(rom, crate::player::SINE_TABLE_OFFSET + 2 * i)? as i16;
        }
        Ok(Self {
            sine,
            tiles,
            common,
            bands,
            scroll: 0x4600 - 1600,
            base: 0,
            name: 0,
            label_anchor: 0,
            previous_gap: 0,
        })
    }
    pub(super) fn tick(&mut self, frame: u32, angle: u16) {
        self.scroll -= if frame <= 9 { 1600 } else { 180 };
        if self.scroll < -1984 {
            self.scroll += 2048;
            self.base = (self.base + 1) % 8;
        }
        let frac = i32::from(angle & 255);
        self.label_anchor = usize::from(angle >> 8);
        if frac != 0 {
            if self.previous_gap == 0 {
                self.name = (usize::from(angle >> 8) + usize::from(frac > 128)) % 4;
                self.label_anchor = self.name;
            }
            self.previous_gap = (frac - 127).abs() / 2;
        }
    }
    pub(super) fn confirm_tick(&mut self, age: u8) {
        self.scroll -= 180 - 3 * i32::from(age - 1);
        if self.scroll < -1984 {
            self.scroll += 2048;
            self.base = (self.base + 1) % 8;
        }
    }
    fn obj(
        &self,
        out: &mut [u8],
        bank: usize,
        tile: usize,
        w: usize,
        h: usize,
        x: i32,
        y: i32,
        pal: &[[u8; 4]],
    ) {
        for j in 0..h {
            let py = (y + j as i32).rem_euclid(256);
            if !(0..160).contains(&py) {
                continue;
            }
            for i in 0..w {
                let px = (x + i as i32).rem_euclid(512);
                if !(0..240).contains(&px) {
                    continue;
                }
                let t = tile + j / 8 * (w / 8) + i / 8;
                let color = self.tiles[bank][t * 64 + j % 8 * 8 + i % 8] as usize;
                if color != 0 {
                    let off = (py as usize * 240 + px as usize) * 4;
                    out[off..off + 4].copy_from_slice(&pal[color]);
                }
            }
        }
    }
    pub(super) fn draw(&self, out: &mut [u8], frame: u32, angle: u16, confirmed: bool) {
        let t = frame.saturating_add(1);
        let sin = |index: i32| i32::from(self.sine[index as usize]) >> 8;
        let banner_y = match frame {
            0..=12 => 56,
            13..=32 => {
                let n = frame as i32 - 13;
                56 - sin(n * (n / 10 + 17))
            }
            _ => 6,
        };
        let banner_x = if frame < 10 {
            256 - (frame as i32) * 256 / 10
        } else {
            0
        };
        let band_x = if t <= 10 {
            256 - (t as i32) * 256 / 10
        } else {
            0
        };
        let dy = if (14..34).contains(&t) {
            sin(((t as i32 * 103 / 110) - 10) * 17)
        } else {
            0
        };
        for i in 0..8 {
            self.obj(out, 4, 0, 32, 16, banner_x + i * 32, banner_y, &self.common);
        }
        let count = if self.scroll > 0x3c00 {
            0
        } else if self.scroll > 0 {
            9 - (self.scroll >> 11) as usize
        } else {
            9
        };
        for i in 0..count {
            self.obj(
                out,
                if confirmed { 5 } else { 3 },
                ((self.base + i) % if confirmed { 2 } else { 8 }) * 8,
                32,
                16,
                (self.scroll >> 6) + 32 * i as i32,
                banner_y,
                &self.common,
            );
        }
        let frac = usize::from(angle & 255);
        let bottom = if t < 34 {
            64 + dy
        } else if frac == 0 {
            112
        } else {
            (168 - self.previous_gap).max(112)
        };
        for i in 0..8 {
            for (tile, y) in [(16, bottom), (0, bottom + 32)] {
                self.obj(
                    out,
                    0,
                    tile,
                    32,
                    32,
                    band_x + i * 32,
                    y,
                    &self.bands[self.name],
                );
            }
        }
        let index = self.label_anchor;
        let shift = if frac == 0 {
            0
        } else {
            [
                -31, 27, -23, 19, -15, 11, -7, 3, -3, 7, -11, 15, -19, 23, -27, 31,
            ][frac >> 4]
        };
        let x = [25, 25, 24, 20][index] + shift + band_x;
        for i in 0..6 {
            self.obj(
                out,
                2,
                self.name * 96 + 16 * i,
                32,
                32,
                x + i as i32 * 32,
                if t < 34 { 84 + dy } else { 132 },
                &self.common,
            );
        }
        if confirmed {
            return;
        }
        let frame = t;
        if frame <= 30 {
            return;
        }
        let (l, r) = if frame < 39 {
            (
                ((frame as i32 - 30) * 6) - 36,
                240 - (frame as i32 - 30) * 6,
            )
        } else {
            let i = (frame % 60 / 10) as usize;
            ([16, 12, 8, 4, 8, 12][i], [194, 198, 202, 206, 202, 198][i])
        };
        self.obj(out, 1, 0, 32, 32, l, 40, &self.common);
        self.obj(out, 1, 16, 32, 32, r, 40, &self.common);
    }
}
