//! Native A3VE board raster for port diagnostics. No collision inference.
use crate::{
    binary, catalog,
    graphics::{self, ColorDepth},
    sha256, tables,
};
use std::fmt;

pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 160;
pub const SCREEN_BYTES: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
pub const BOARD_RGBA_SHA256: &str =
    "947f0e412a386426ab6e363c3c3c225e9e18534c201bcd1a62d60bbd1515c09c";

#[derive(Debug)]
pub struct BoardError(pub String);
impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for BoardError {}

/// This is one verified resource pairing, not a port of the native board loader
/// or camera. Other descriptor rows are deliberately not assigned game names.
pub struct NativeBoard {
    pub image: graphics::RgbaImage,
    backdrop: [u8; 4],
}

impl NativeBoard {
    pub fn load(rom: &[u8]) -> Result<Self, BoardError> {
        catalog::validate_rom(rom).map_err(|e| BoardError(e.to_string()))?;
        let table = tables::parse_tables(rom).map_err(|e| BoardError(e.to_string()))?;
        let d = &table.descriptors[0];
        let allocation = &d.inferred_tile_allocations[0];
        let raw_tiles = binary::span(rom, allocation.rom_offset, allocation.bytes)
            .map_err(|e| BoardError(e.to_string()))?;
        let tiles = graphics::decode_tiles(raw_tiles, ColorDepth::Bpp4, 4096)
            .map_err(|e| BoardError(e.to_string()))?;
        let raw_palette = binary::span(rom, d.primary_palettes[0].rom_offset, 512)
            .map_err(|e| BoardError(e.to_string()))?;
        let palette =
            graphics::decode_palette_le(raw_palette, 256).map_err(|e| BoardError(e.to_string()))?;
        let map = binary::decode(&rom[d.columns[1].rom_offset..], 4096)
            .map_err(|e| BoardError(e.to_string()))?;
        if map.bytes.len() != 4096 || map.consumed != 3549 {
            return Err(BoardError("verified board map length mismatch".into()));
        }
        let image = graphics::render_text_bg(
            &tiles,
            &map.bytes,
            &palette,
            ColorDepth::Bpp4,
            (32, 64),
            256 * 512,
        )
        .map_err(|e| BoardError(e.to_string()))?;
        if sha256(&image.pixels) != BOARD_RGBA_SHA256 {
            return Err(BoardError("verified board raster mismatch".into()));
        }
        Ok(Self {
            image,
            backdrop: palette[0],
        })
    }

    /// Diagnostic viewport, with explicit user-selected origin. This does not
    /// claim the original runtime's camera or layer-compositing behavior.
    pub fn viewport(&self, x: usize, y: usize, out: &mut [u8]) -> Result<(), BoardError> {
        if out.len() != SCREEN_BYTES {
            return Err(BoardError("viewport must be exactly240x160 RGBA".into()));
        }
        if self.image.width < SCREEN_WIDTH || self.image.height < SCREEN_HEIGHT {
            return Err(BoardError("board smaller than viewport".into()));
        }
        if x > self.image.width - SCREEN_WIDTH || y > self.image.height - SCREEN_HEIGHT {
            return Err(BoardError("viewport origin outside board".into()));
        }
        for row in 0..SCREEN_HEIGHT {
            let source = ((y + row) * self.image.width + x) * 4;
            let destination = row * SCREEN_WIDTH * 4;
            out[destination..destination + SCREEN_WIDTH * 4]
                .copy_from_slice(&self.image.pixels[source..source + SCREEN_WIDTH * 4]);
        }
        // A native GBA backdrop color makes this single transparent BG layer
        // visible in the host; other original layers remain unported.
        for pixel in out.as_chunks_mut::<4>().0 {
            if pixel[3] == 0 {
                pixel.copy_from_slice(&self.backdrop);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_rejects_invalid_bounds_before_reading() {
        let board = NativeBoard {
            image: graphics::RgbaImage {
                width: 256,
                height: 512,
                pixels: vec![0; 256 * 512 * 4],
            },
            backdrop: [1, 2, 3, 255],
        };
        let mut frame = vec![0; SCREEN_BYTES];
        assert!(board.viewport(17, 0, &mut frame).is_err());
        assert!(board.viewport(0, 353, &mut frame).is_err());
        assert!(board.viewport(0, 0, &mut frame[..4]).is_err());
        board.viewport(16, 352, &mut frame).unwrap();
        assert_eq!(&frame[..4], &[1, 2, 3, 255]);
    }
}
