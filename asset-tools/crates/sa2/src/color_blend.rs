//! The main program's ARGB colour blend (GSNE8P main.dol 0x800334B0; PC
//! stage-module import 0x80033248), which stage objects use to fade a tint
//! (in stg09D.rel: TANK, EFFOBJ0, KNUDAI, ROCKETMISSSILE, CHAOPIPE and
//! others).
//!
//! `blend_argb` is copied unchanged from `kart_course::blend_argb` on #423
//! (the kart thread's port, used for the CartD sea), so the two can become one
//! when both land. examples/color_blend_check.rs compares it with a Unicorn
//! run of 0x800334B0.

/// main.dol colour blend (PC import 0x80033248, GSNE8P 0x800334B0): each
/// ARGB byte becomes trunc(t * (b - a) + a) in single precision with one
/// fused add; a result outside 0..=255 becomes 255.
pub fn blend_argb(a: u32, b: u32, t: f32) -> u32 {
    (0..4).fold(0, |out, k| {
        let shift = 24 - 8 * k;
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        let v = (t as f64 * (y - x) as f64 + x as f64) as f32 as i32;
        out | (if v as u32 & 0xffff_ff00 != 0 {
            0xff
        } else {
            v as u32
        }) << shift
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_and_saturates() {
        assert_eq!(blend_argb(0xff20_2020, 0xffff_ffff, 0.0), 0xff20_2020);
        assert_eq!(blend_argb(0xff20_2020, 0xffff_ffff, 1.0), 0xffff_ffff);
        assert_eq!(blend_argb(0x0000_0000, 0x6400_c8ff, 0.5), 0x3200_647f);
        // Out of range on either side saturates to 0xFF.
        assert_eq!(blend_argb(0x0000_0000, 0xff00_0010, 2.0), 0xff00_0020);
        assert_eq!(blend_argb(0x0000_0010, 0x0000_0000, 2.0), 0x0000_00ff);
    }
}
