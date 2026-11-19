//! Original main-ball OBJ pixels. Camera and layer priority are separate ports.
use super::board::{SCREEN_BYTES, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::{
    binary, catalog,
    graphics::{self, ColorDepth},
    sha256,
};

pub const BALL_TILES_OFFSET: usize = 0x4599ac;
pub const BALL_TILES_LENGTH: usize = 512;
pub const BALL_TILES_SHA256: &str =
    "2a0b79175c78349bedc717a18f7c876d12b79f6bc85fbe34c067fed766f270ad";
pub const BALL_PALETTE_OFFSET: usize = 0x459dac;
pub const BALL_PALETTE_LENGTH: usize = 22;
pub const BALL_PALETTE_SHA256: &str =
    "93cfb2110473dc10a293d4e352d77191a846d9f78b06ae332d849223e37f43b6";
pub const BALL_OAM_OFFSET: usize = 0x6ddc70;
pub const BALL_OAM_SHA256: &str =
    "d40e9ce59f3b4f3a78c04c8e42cca0e774a9f84298d394ce0404edc4f077b472";

pub struct NativeBallSprite {
    pub oam: graphics::OamAttributes,
    frames: [Vec<u8>; 4],
}

impl NativeBallSprite {
    pub fn load(rom: &[u8]) -> Result<Self, String> {
        catalog::validate_rom(rom).map_err(|e| e.to_string())?;
        let raw_tiles =
            binary::span(rom, BALL_TILES_OFFSET, BALL_TILES_LENGTH).map_err(|e| e.to_string())?;
        let raw_palette = binary::span(rom, BALL_PALETTE_OFFSET, BALL_PALETTE_LENGTH)
            .map_err(|e| e.to_string())?;
        let raw_oam = binary::span(rom, BALL_OAM_OFFSET, 8).map_err(|e| e.to_string())?;
        if sha256(raw_tiles) != BALL_TILES_SHA256
            || sha256(raw_palette) != BALL_PALETTE_SHA256
            || sha256(raw_oam) != BALL_OAM_SHA256
        {
            return Err("native ball resource fingerprint mismatch".into());
        }
        let oam = graphics::parse_oam(raw_oam).map_err(|e| e.to_string())?;
        if (oam.width, oam.height) != (16, 16) || oam.depth != ColorDepth::Bpp4 || oam.affine {
            return Err("native ball OAM layout mismatch".into());
        }
        let tiles =
            graphics::decode_tiles(raw_tiles, ColorDepth::Bpp4, 16).map_err(|e| e.to_string())?;
        let palette = graphics::decode_palette_le(raw_palette, 11).map_err(|e| e.to_string())?;
        if tiles
            .iter()
            .flatten()
            .any(|&p| usize::from(p) >= palette.len())
        {
            return Err("native ball tile index exceeds its explicit palette".into());
        }
        let frames = std::array::from_fn(|frame| {
            let mut rgba = vec![0; 16 * 16 * 4];
            for y in 0..16 {
                for x in 0..16 {
                    let tile = frame * 4 + (y / 8) * 2 + x / 8;
                    let index = usize::from(tiles[tile][(y % 8) * 8 + x % 8]);
                    if index != 0 {
                        rgba[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4]
                            .copy_from_slice(&palette[index]);
                    }
                }
            }
            rgba
        });
        Ok(Self { oam, frames })
    }

    /// Source ROM29DC6 chooses four128-byte frames from fixedY bits11..12.
    pub fn frame(&self, fixed_y: i32) -> &[u8] {
        &self.frames[((fixed_y as u32 & 0x1800) >> 11) as usize]
    }

    /// Single-object diagnostic compositor. Native center-minus8 is retained;
    /// caller supplies an explicit viewport origin. Other native layers, OAM
    /// wrapping, blend registers and priority resolution are not claimed here.
    pub fn draw_diagnostic(
        &self,
        fixed_y: i32,
        center: (i32, i32),
        out: &mut [u8],
    ) -> Result<(), String> {
        if out.len() != SCREEN_BYTES {
            return Err("ball diagnostic needs exact240x160 RGBA destination".into());
        }
        let left = i64::from(center.0) - 8;
        let top = i64::from(center.1) - 8;
        let rgba = self.frame(fixed_y);
        for y in 0..16 {
            for x in 0..16 {
                let dx = left + x as i64;
                let dy = top + y as i64;
                if dx < 0 || dy < 0 || dx >= SCREEN_WIDTH as i64 || dy >= SCREEN_HEIGHT as i64 {
                    continue;
                }
                let source = (y * 16 + x) * 4;
                if rgba[source + 3] != 0 {
                    let at = (dy as usize * SCREEN_WIDTH + dx as usize) * 4;
                    out[at..at + 4].copy_from_slice(&rgba[source..source + 4]);
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
    #[ignore = "Requires local SPP_ROM; independent original ball OBJ fingerprints"]
    fn original_ball_frames_match_independent_pixel_oracle() {
        use std::io::Read;
        let path = std::env::var_os("SPP_ROM").expect("Set SPP_ROM");
        let mut rom = Vec::new();
        std::fs::File::open(path)
            .unwrap()
            .take((catalog::ROM_LEN + 1) as u64)
            .read_to_end(&mut rom)
            .unwrap();
        let sprite = NativeBallSprite::load(&rom).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/game-graphics-vectors.json")).unwrap();
        for y in [
            0,
            0x7ff,
            0x800,
            0xfff,
            0x1000,
            0x17ff,
            0x1800,
            0x1fff,
            -1,
            i32::MIN,
            i32::MAX,
        ] {
            let index = ((y as u32 & 0x1800) >> 11) as usize;
            assert_eq!(
                sha256(sprite.frame(y)),
                fixture["frameSha256"][index].as_str().unwrap()
            );
        }
    }
    #[test]
    fn diagnostic_compositor_checks_lengths_and_clips_without_wrapping() {
        let sprite = NativeBallSprite {
            oam: graphics::parse_oam(&[0, 0, 0, 0x40, 0, 0, 0, 0]).unwrap(),
            frames: std::array::from_fn(|_| vec![255; 16 * 16 * 4]),
        };
        let mut out = vec![0; SCREEN_BYTES];
        assert!(sprite.draw_diagnostic(0, (0, 0), &mut out[..4]).is_err());
        sprite
            .draw_diagnostic(0, (i32::MIN, i32::MAX), &mut out)
            .unwrap();
        assert!(out.iter().all(|p| *p == 0));
        sprite.draw_diagnostic(0, (0, 0), &mut out).unwrap();
        assert_eq!(
            out.as_chunks::<4>().0.iter().filter(|p| p[3] != 0).count(),
            64
        );
    }
}
