//! The game's per-Chao body palette: AL_PaletteSetColorRatio (GC ChaoMain.rel .text 0x1F1FC,
//! size 0x814) blends 48 colours from the palette table gPalette by the Chao's type and its
//! growth values, and the three 16-colour banks of the result colour the body's three
//! indexed textures (alpalSetBank 0x1F0D0, TexBankList .data 0x43CA0 = texture ids 0..17 in
//! rows of three). gPalette is filled by alpalLoadTexpalette (0x1FA10) from the 39 files
//! named in pointer table .data 0x43CE8, in the order of [`SLOT_FILES`].

use crate::texture::{read_gvp, rgb565};
use crate::{Error, Result};

/// The palette files in gPalette slot order (ChaoMain.rel .data 0x43CE8, read through its
/// relocations). The files sit loose in the Steam `resource/gd_PC`.
pub const SLOT_FILES: [&str; 39] = [
    "al_nc00", "al_nc01", "al_hcz", "al_hcn", "al_hcs", "al_hcf", "al_hcr", "al_hcp", "al_dc",
    "al_hnz", "al_hnn", "al_hns", "al_hnf", "al_hnr", "al_hnp", "al_hsz", "al_hsn", "al_hss",
    "al_hsf", "al_hsr", "al_hsp", "al_hfz", "al_hfn", "al_hfs", "al_hff", "al_hfr", "al_hfp",
    "al_hrz", "al_hrn", "al_hrs", "al_hrf", "al_hrr", "al_hrp", "al_hpz", "al_hpn", "al_hps",
    "al_hpf", "al_hpr", "al_hpp",
];

/// Colours in one palette: three banks of 16.
pub const COLORS: usize = 48;
/// Bytes in one palette file: 16-byte GVPL header and 48 RGB565 colours.
pub const FILE_BYTES: usize = 112;

pub type Colors = [[u8; 4]; COLORS];

/// The Chao values the blend reads (offsets in the Chao parameter block).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratios {
    /// +0x80, the Chao type byte.
    pub chao_type: u8,
    /// +0xA8: power when positive, run when negative.
    pub h_pos: f32,
    /// +0xAC: fly when positive, swim when negative.
    pub v_pos: f32,
    /// +0xB0: hero when positive, dark when negative. Only the child blend reads it.
    pub a_pos: f32,
    /// +0xC0, used up to 1.
    pub growth: f32,
}

impl Ratios {
    /// A new child: type 2, everything else zero.
    pub const CHILD: Ratios = Ratios {
        chao_type: 2,
        h_pos: 0.,
        v_pos: 0.,
        a_pos: 0.,
        growth: 0.,
    };
}

/// The 39 palette files, concatenated in [`SLOT_FILES`] order, as gPalette.
pub fn read_slots(files: &[u8]) -> Result<Vec<Colors>> {
    if files.len() != SLOT_FILES.len() * FILE_BYTES {
        return Err(Error::invalid("expected the 39 Chao palette files"));
    }
    files
        .chunks(FILE_BYTES)
        .map(|file| {
            let palette = read_gvp(file)?;
            // alpalLoadTexpalette handles formats 0, 1, 2 and 6; the Steam files are all 1.
            if palette.format != 1 {
                return Err(Error::invalid("Chao palette is not RGB565"));
            }
            Colors::try_from(palette.colors.as_slice())
                .map_err(|_| Error::invalid("Chao palette is not 48 colours"))
        })
        .collect()
}

/// First gPalette slot and TexBankList row for a type (jump tables .data 0x43DD0 and
/// 0x43D84). Every other type keeps whatever palette its textures already have.
pub fn rows(chao_type: u8) -> Option<(usize, usize)> {
    Some(match chao_type {
        2 => (0, 0),
        6 => (9, 1),
        9 => (0xF, 2),
        12 => (0x15, 3),
        15 => (0x1B, 4),
        18 => (0x21, 5),
        _ => return None,
    })
}

/// Clamp to -1..1 with zero replaced by 1e-6 (rodata 0x64E4); the sign is returned apart.
fn part(v: f32) -> (f32, bool) {
    let v = if v == 0. {
        f32::from_bits(0x3586_37BD)
    } else {
        v
    };
    let v = v.clamp(-1., 1.);
    (v.abs(), v < 0.)
}

/// AL_PaletteSetColorRatio. `None` for a type the routine leaves alone. The result is the
/// RGB565 the game stores, widened the way the palette files are read.
pub fn blend(slots: &[Colors], r: &Ratios) -> Result<Option<Colors>> {
    let Some((base, _)) = rows(r.chao_type) else {
        return Ok(None);
    };
    if slots.len() != SLOT_FILES.len() {
        return Err(Error::invalid("expected 39 Chao palettes"));
    }
    let t = if r.growth > 1. { 1. } else { r.growth };
    let (a, run) = part(r.h_pos);
    let (b, swim) = part(r.v_pos);
    let inv = 1. / (a + b);
    let (wa, wb) = (inv * a, inv * b);
    // Slots after the base: zero, normal, swim, fly, run, power.
    let child = r.chao_type == 2;
    let first = if child { 2 } else { base };
    let zero = &slots[first];
    let normal = &slots[first + 1];
    let side_a = &slots[first + if run { 4 } else { 5 }];
    let side_b = &slots[first + if swim { 2 } else { 3 }];
    let mut out = [[0u8; 4]; COLORS];
    for (i, color) in out.iter_mut().enumerate() {
        let mut c = [0u8; 3];
        for (k, c) in c.iter_mut().enumerate() {
            let v = |p: &Colors| f32::from(p[i][k]);
            let mix_b = v(normal).mul_add(1. - b, v(side_b) * b);
            let mix_a = v(normal).mul_add(1. - a, v(side_a) * a);
            let grown = v(zero).mul_add(1. - t, wa.mul_add(mix_a, wb * mix_b) * t);
            let value = if !child {
                grown
            } else {
                // Child: neutral base al_nc00 -> al_nc01 by growth, then toward the
                // hero blend (alignment >= 0) or al_dc (alignment < 0).
                let plain = v(&slots[0]).mul_add(1. - t, v(&slots[1]) * t);
                if r.a_pos >= 0. {
                    let k = if r.a_pos > 1. { 1. } else { r.a_pos };
                    plain.mul_add(1. - k, grown * k)
                } else {
                    let k = -(if r.a_pos < -1. { -1. } else { r.a_pos });
                    plain.mul_add(1. - k, v(&slots[8]) * k)
                }
            };
            *c = value as i32 as u8;
        }
        let packed = u16::from(c[0] >> 3) << 11 | u16::from(c[1] >> 2) << 5 | u16::from(c[2] >> 3);
        *color = rgb565(packed);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Vec<Colors> {
        // Slot n is one flat colour whose green is n * 4 + 2: its six RGB565 bits are n,
        // with room for the blend's truncation.
        (0..39u8)
            .map(|n| [[0, n * 4 + 2, 0, 255]; COLORS])
            .collect()
    }
    fn slot(r: Ratios) -> u8 {
        blend(&slots(), &r).unwrap().unwrap()[0][1] >> 2
    }

    #[test]
    fn child_blend_picks_the_game_slots() {
        let c = Ratios::CHILD;
        assert_eq!(slot(c), 0, "new child is al_nc00");
        assert_eq!(
            slot(Ratios { growth: 2., ..c }),
            1,
            "grown child is al_nc01"
        );
        assert_eq!(slot(Ratios { a_pos: -3., ..c }), 8, "dark child is al_dc");
        assert_eq!(
            slot(Ratios { a_pos: 1., ..c }),
            2,
            "hero child starts at al_hcz"
        );
        let hero = Ratios {
            a_pos: 1.,
            growth: 1.,
            ..c
        };
        assert_eq!(slot(Ratios { h_pos: 1., ..hero }), 7, "power");
        assert_eq!(slot(Ratios { h_pos: -1., ..hero }), 6, "run");
        assert_eq!(slot(Ratios { v_pos: 1., ..hero }), 5, "fly");
        assert_eq!(slot(Ratios { v_pos: -1., ..hero }), 4, "swim");
    }

    #[test]
    fn adult_blend_reads_its_own_six_slots() {
        let a = Ratios {
            chao_type: 12,
            ..Ratios::CHILD
        };
        assert_eq!(slot(a), 0x15);
        let grown = Ratios { growth: 1., ..a };
        assert_eq!(slot(Ratios { h_pos: 1., ..grown }), 0x15 + 5);
        assert_eq!(
            slot(Ratios {
                v_pos: -1.,
                ..grown
            }),
            0x15 + 2
        );
        for other in [0, 1, 3, 5, 7, 19, 255] {
            let r = Ratios {
                chao_type: other,
                ..a
            };
            assert!(blend(&slots(), &r).unwrap().is_none());
        }
        assert!(read_slots(&[0; 100]).is_err());
    }
}
