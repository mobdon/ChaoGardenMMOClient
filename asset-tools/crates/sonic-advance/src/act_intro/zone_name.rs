//! Scrolling English zone-name objects for Sonic's first act title card.
use crate::{
    binary,
    graphics::{decode_palette, decode_tile_indices, BitsPerPixel},
    Error, Result,
};

pub struct ZoneName {
    x: i16,
    first: u8,
    count: u8,
    tiles: Vec<u8>,
    palette: Vec<[u8; 4]>,
}
impl ZoneName {
    pub fn from_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        Ok(Self {
            x: 256,
            first: 0,
            count: 0,
            tiles: decode_tile_indices(
                binary::range(rom, 0x660558, 0x700)?,
                BitsPerPixel::Four,
                56 * 64,
            )?,
            palette: decode_palette(binary::range(rom, 0x660c58, 32)?, 16)?,
        })
    }
    /// One call per graphics task update, using the title-card clock (also
    /// when the control task fast-forwards its clock on a fresh button press).
    pub fn step(&mut self, tick: u16) {
        if tick <= 15 || tick > 225 {
            self.count = 0;
            return;
        }
        if tick > 215 {
            self.x -= 28;
            if self.x < -31 {
                self.count = self.count.saturating_sub(1);
                self.wrap();
            }
        } else if tick > 25 {
            self.x -= 2;
            self.count = 9;
            self.wrap();
        } else {
            self.x -= 28;
            if self.x < 240 {
                self.count = (9 - (self.x >> 5)) as u8;
                self.wrap();
            }
        }
    }
    fn wrap(&mut self) {
        if self.x < -31 {
            self.x += 32;
            self.first = (self.first + 1) % 7;
        }
    }
    pub fn objects(&self) -> impl Iterator<Item = (i16, u8)> + '_ {
        (0..self.count).map(|i| (self.x + i16::from(i) * 32, (self.first + i) % 7))
    }
    /// Draw onto a caller-owned 240x160 RGBA layer. Index zero is transparent.
    pub fn draw(&self, out: &mut [u8]) -> Result<()> {
        if out.len() != 240 * 160 * 4 {
            return Err(Error("invalid title card RGBA surface".into()));
        }
        for (x, glyph) in self.objects() {
            for j in 0..16 {
                for i in 0..32 {
                    let px = (x + i as i16).rem_euclid(512) as usize;
                    if px >= 240 {
                        continue;
                    }
                    let tile = usize::from(glyph) * 8 + (j / 8) * 4 + i / 8;
                    let color = self.tiles[tile * 64 + (j % 8) * 8 + i % 8] as usize;
                    if color != 0 {
                        let at = ((128 + j) * 240 + px) * 4;
                        out[at..at + 4].copy_from_slice(&self.palette[color]);
                    }
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positions_follow_retail_oam_through_entry_scroll_and_exit() {
        let mut z = ZoneName {
            x: 256,
            first: 0,
            count: 0,
            tiles: Vec::new(),
            palette: Vec::new(),
        };
        for frame in 1657u16..=1884 {
            z.step(frame.saturating_sub(1659));
            let expected = match frame {
                1674 => Some((0, 256, 0)),
                1675 => Some((2, 228, 0)),
                1684 => Some((10, -24, 0)),
                1720 => Some((9, 0, 3)),
                1770 => Some((9, -4, 6)),
                1876 => Some((7, -12, 0)),
                1882 => Some((2, -20, 5)),
                _ => None,
            };
            if let Some((count, x, first)) = expected {
                assert_eq!((z.count, z.x, z.first), (count, x, first), "frame {frame}");
            }
        }
    }
}
