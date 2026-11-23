//! The message font's letter spacing for the glyphs actually loaded.
//!
//! The game spaces its text by two tables in main.dol (GSNE8P), one byte a
//! glyph: 0x801760F0 for efmsgfont_ascii24E.bin (96 glyphs) and 0x80176150
//! for efmsgfont_ascii24S.bin (224). fn_800E0F80 returns table[glyph] + 1.
//! Each table byte is the glyph's ink width in the disc's own file: the
//! column after its rightmost set pixel (checked against the disc files:
//! every inked glyph of both files matches its table byte). The Steam release
//! ships a different drawing of the same glyphs (sa2-runtime dvd.rs converts
//! it), so the disc's bytes spaced it wrongly: its thin I took the disc's
//! wide I's room. Here the same rule is applied to the loaded glyphs. With
//! the disc's files nothing changes.

use crate::original;
use sa2_recomp::rt::{Cpu, Mem};

/// fn_800E10DC: loads the three font files (main calls it once at boot).
const FONT_LOAD: u32 = 0x800E_10DC;
/// The loader's files, 8 bytes each: name, loaded data (0 kanji24, 1 ascii24S, 2 ascii24E).
const FONT_FILES: u32 = 0x801C_D8D0;
/// (file, its width table, glyphs).
const TABLES: [(u32, u32, u32); 2] = [(2, 0x8017_60F0, 96), (1, 0x8017_6150, 224)];
/// A glyph: 24 rows of 3 bytes, read little-endian, bit 0 the leftmost pixel.
const GLYPH: u32 = 72;

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(FONT_LOAD, |c, m| {
        original(c, m, FONT_LOAD);
        for (file, table, glyphs) in TABLES {
            let data = m.r32(FONT_FILES + 8 * file + 4);
            if data >> 24 != 0x80 {
                continue;
            }
            for g in 0..glyphs {
                let rows: Vec<u8> = (0..GLYPH).map(|i| m.r8(data + g * GLYPH + i)).collect();
                // A glyph with no ink (the space) keeps the game's own width.
                if let Some(w) = ink_width(&rows) {
                    m.w8(table + g, w);
                }
            }
        }
    });
}

/// The column after the glyph's rightmost set pixel; None when it has no ink.
fn ink_width(glyph: &[u8]) -> Option<u8> {
    let all = glyph
        .chunks(3)
        .fold(0u32, |a, r| a | u32::from_le_bytes([r[0], r[1], r[2], 0]));
    (all != 0).then(|| (32 - all.leading_zeros()) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_the_column_after_the_rightmost_pixel() {
        let mut g = [0u8; 72];
        assert_eq!(ink_width(&g), None, "space");
        g[3] = 0b0000_0110; // row 1: columns 1 and 2, a thin I
        assert_eq!(ink_width(&g), Some(3));
        g[8] = 0x80; // row 2: column 23
        assert_eq!(ink_width(&g), Some(24));
    }
}
