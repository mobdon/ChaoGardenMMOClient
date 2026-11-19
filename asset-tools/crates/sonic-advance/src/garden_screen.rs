//! Tiny Garden BG3 artwork, reconstructed from the verified embedded program.
use crate::{binary, garden::GardenEvidence, Error, Result};

pub const WIDTH: usize = 176;
pub const HEIGHT: usize = 160;

/// Native BG3 rectangle, in little-endian GBA palette colors before compositing.
pub fn background(program: &GardenEvidence<'_>) -> Result<Vec<u16>> {
    let raw = program.opaque_program();
    let tiles = binary::decompress(binary::range(raw, 0x18fe4, 0x9f9)?, 0x1e00)?;
    if tiles.bytes.len() != 0x1e00 || tiles.consumed != 0x9f9 {
        return Err(Error("unexpected Garden BG3 tile stream".into()));
    }
    render(
        &tiles.bytes,
        binary::range(raw, 0x199e0, 880)?,
        binary::range(raw, 0x1b218, 512)?,
    )
}

fn render(tiles: &[u8], map: &[u8], palette: &[u8]) -> Result<Vec<u16>> {
    if tiles.len() % 64 != 0 || map.len() != 880 || palette.len() != 512 {
        return Err(Error("invalid Garden BG3 resource lengths".into()));
    }
    let mut pixels = vec![0; WIDTH * HEIGHT];
    for (cell, entry) in map.chunks_exact(2).enumerate() {
        let word = u16::from_le_bytes([entry[0], entry[1]]);
        let tile = binary::range(tiles, usize::from(word & 1023) * 64, 64)?;
        for y in 0..8 {
            for x in 0..8 {
                let tx = if word & 1024 == 0 { x } else { 7 - x };
                let ty = if word & 2048 == 0 { y } else { 7 - y };
                let color = usize::from(tile[ty * 8 + tx]) * 2;
                pixels[(cell / 22 * 8 + y) * WIDTH + cell % 22 * 8 + x] =
                    u16::from_le_bytes([palette[color], palette[color + 1]]);
            }
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_tile_and_short_map() {
        assert!(render(&[0; 64], &[1; 880], &[0; 512]).is_err());
        assert!(render(&[0; 64], &[0; 878], &[0; 512]).is_err());
    }
}
