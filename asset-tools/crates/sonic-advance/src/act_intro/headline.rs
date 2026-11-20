use crate::{
    binary,
    graphics::{decode_palette, decode_tile_indices, BitsPerPixel},
    Error, Result,
};

/// Repeated native 32x32 headline backing. Composite before both lettering layers.
pub struct HeadlineBacking {
    pixels: Vec<u8>,
    palette: Vec<[u8; 4]>,
}
impl HeadlineBacking {
    pub fn from_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        Ok(Self {
            pixels: decode_tile_indices(
                binary::range(rom, 0x65f078, 512)?,
                BitsPerPixel::Four,
                1024,
            )?,
            palette: decode_palette(binary::range(rom, 0x660c58, 32)?, 16)?,
        })
    }
    /// Signed first-object X and the number of repeated objects. GBA wraps X to nine bits.
    pub fn layout(tick: u16) -> (i16, u8) {
        match tick {
            0..=15 => (240, 8),
            16..=25 => ((240 - 26 * (tick as i16 - 15)).max(-16), 8),
            26..=215 => (-16, 8),
            216..=225 => {
                let (mut x, mut count) = (-16, 8u8);
                for _ in 215..tick {
                    if x < -32 {
                        x += 32;
                        count = count.saturating_sub(1);
                    }
                    x -= 26;
                }
                (x, count)
            }
            _ => (0, 0),
        }
    }
    pub fn draw(&self, tick: u16, out: &mut [u8]) -> Result<()> {
        if out.len() != 240 * 160 * 4 {
            return Err(Error("invalid headline backing surface".into()));
        }
        let (left, count) = Self::layout(tick);
        for obj in 0..count {
            for y in 0..32 {
                for x in 0..32 {
                    let px = (left + i16::from(obj) * 32 + x as i16).rem_euclid(512) as usize;
                    if px >= 240 {
                        continue;
                    }
                    let index =
                        self.pixels[((y / 8) * 4 + x / 8) * 64 + (y % 8) * 8 + x % 8] as usize;
                    if index != 0 {
                        let at = ((126 + y) * 240 + px) * 4;
                        out[at..at + 4].copy_from_slice(&self.palette[index]);
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
    fn matches_first_object_and_count_from_cold_boot() {
        for (frame, x, count) in [
            (1675, 214, 8),
            (1680, 84, 8),
            (1684, -16, 8),
            (1720, -16, 8),
            (1876, -36, 7),
            (1882, -32, 2),
        ] {
            assert_eq!(
                HeadlineBacking::layout(frame - 1659),
                (x, count),
                "frame {frame}"
            );
        }
    }
}
