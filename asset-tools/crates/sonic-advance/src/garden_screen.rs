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
    if !tiles.len().is_multiple_of(64) || map.len() != 880 || palette.len() != 512 {
        return Err(Error("invalid Garden BG3 resource lengths".into()));
    }
    let mut pixels = vec![0; WIDTH * HEIGHT];
    for (cell, entry) in map.as_chunks::<2>().0.iter().enumerate() {
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

/// Four active OBJ entries sampled on the cold-boot route at frame 1700.
/// Rows contain native BGR555; 0xffff is transparent.
pub struct GardenObject {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub colors: Vec<u16>,
}

pub fn entry_objects(program: &GardenEvidence<'_>) -> Result<Vec<GardenObject>> {
    let raw = program.opaque_program();
    let bank = binary::decompress(binary::range(raw, 0x1e6d0, 0xd1d)?, 0x1800)?;
    let palette = binary::range(raw, 0x1c500, 512)?;
    let mut objects = Vec::new();
    for (x, y, width, height, tile, pal) in [
        (87, 52, 16, 16, 0xac, 1),
        (80, 89, 16, 16, 0xd8, 14),
        (112, 4, 32, 16, 0x10d, 12),
        (144, 4, 32, 16, 0x10d, 11),
    ] {
        let tiles = binary::range(&bank.bytes, (tile - 0xac) * 32, width * height / 2)?;
        let mut colors = Vec::with_capacity(width * height);
        for row in 0..height {
            for col in 0..width {
                let at = ((row / 8) * (width / 8) + col / 8) * 32 + (row % 8) * 4 + (col % 8) / 2;
                let index = (tiles[at] >> (4 * (col % 2))) & 15;
                let pos = 32 * pal + 2 * usize::from(index);
                colors.push(if index == 0 {
                    0xffff
                } else {
                    u16::from_le_bytes([palette[pos], palette[pos + 1]])
                });
            }
        }
        objects.push(GardenObject {
            x,
            y,
            width,
            colors,
        });
    }
    Ok(objects)
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
