//! Neo Green Hill Act 1 label. Six consecutive 8x16 OBJ records share OBJ palette 6.
use crate::{
    binary,
    graphics::{decode_palette, decode_tile_indices, BitsPerPixel},
    Error, Result,
};

pub struct ActLabel {
    pixels: Vec<u8>,
    palette: Vec<[u8; 4]>,
}
impl ActLabel {
    pub fn from_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let pixels = decode_tile_indices(
            binary::range(rom, 0x65ee58, 0x180)?,
            BitsPerPixel::Four,
            48 * 16,
        )?;
        let palette = decode_palette(binary::range(rom, 0x660c58, 32)?, 16)?;
        Ok(Self { pixels, palette })
    }
    /// Position after the stage UI graphics task runs. The final descent is
    /// entered from the settled phase for both natural and skipped routes.
    pub fn y(tick: u16) -> usize {
        match tick {
            0..=30 => 160,
            31..=35 => 160 - 3 * usize::from(tick - 30),
            36..=215 => 144,
            _ => 144 + 2 * usize::from((tick - 215).min(8)),
        }
    }
    /// Unblended object layer. The GBA compositor applies any window/blend state.
    pub fn draw(&self, tick: u16, out: &mut [u8]) -> Result<()> {
        if out.len() != 240 * 160 * 4 {
            return Err(Error("invalid act-label surface".into()));
        }
        let y = Self::y(tick);
        for j in 0..(160 - y).min(16) {
            for x in 0..48 {
                let tile = (x / 8) * 2 + j / 8;
                let color = self.pixels[tile * 64 + (j % 8) * 8 + x % 8] as usize;
                if color != 0 {
                    let off = ((y + j) * 240 + 180 + x) * 4;
                    out[off..off + 4].copy_from_slice(&self.palette[color]);
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
    fn matches_retail_object_height_on_entry_hold_and_exit() {
        for (frame, y) in [
            (1689, 160),
            (1690, 157),
            (1691, 154),
            (1692, 151),
            (1695, 144),
            (1720, 144),
            (1876, 148),
            (1882, 160),
        ] {
            assert_eq!(ActLabel::y(frame - 1659), y, "frame {frame}");
        }
    }
}
