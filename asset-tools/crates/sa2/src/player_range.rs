//! The main program's player range test (GSNE8P main.dol 0x800398D8; PC
//! stage-module import 0x80039670) and the vector length it uses
//! (0x801151F4).
//!
//! The range test walks the two player slots (the table at 0x801E7768:
//! slot task, its +0x2C, that block's +0xC, position at +8), forms
//! `player - query` with `fsubs`, takes its length, and returns `slot + 1`
//! for the first player whose `length - radius` (`fsubs`) is below the
//! `.sdata2` zero at r2-0x75F8 (`fcmpo`, so a NaN never counts); otherwise
//! 0. An empty slot is skipped.
//!
//! The length is not a square root: `fmuls` x*x, two `fmadds`, then
//! `frsqrte` and `fres`, both Gekko hardware estimates. `frsqrte` is
//! `msl_math::gekko_frsqrte`; `fres` is `gekko_fres`, Dolphin's transcription
//! of the hardware table (Source/Core/Common/FloatUtils.cpp,
//! `ApproximateReciprocal`).
use crate::msl_math::gekko_frsqrte;
use crate::play_world::V3;

/// Gekko `fres` estimate table (base, slope) per 1024-entry segment.
const FRES_EXPECTED: [(i64, i64); 32] = [
    (0x7ff800, 0x3e1),
    (0x783800, 0x3a7),
    (0x70ea00, 0x371),
    (0x6a0800, 0x340),
    (0x638800, 0x313),
    (0x5d6200, 0x2ea),
    (0x579000, 0x2c4),
    (0x520800, 0x2a0),
    (0x4cc800, 0x27f),
    (0x47ca00, 0x261),
    (0x430800, 0x245),
    (0x3e8000, 0x22a),
    (0x3a2c00, 0x212),
    (0x360800, 0x1fb),
    (0x321400, 0x1e5),
    (0x2e4a00, 0x1d1),
    (0x2aa800, 0x1be),
    (0x272c00, 0x1ac),
    (0x23d600, 0x19b),
    (0x209e00, 0x18b),
    (0x1d8800, 0x17c),
    (0x1a9000, 0x16e),
    (0x17ae00, 0x15b),
    (0x14f800, 0x15b),
    (0x124400, 0x143),
    (0x0fbe00, 0x143),
    (0x0d3800, 0x12d),
    (0x0ade00, 0x12d),
    (0x088400, 0x11a),
    (0x065000, 0x11a),
    (0x041c00, 0x108),
    (0x020c00, 0x106),
];

/// Gekko's `fres` on a double.
pub fn gekko_fres(x: f64) -> f64 {
    let bits = x.to_bits() as i64;
    let mantissa = bits & ((1i64 << 52) - 1);
    let sign = bits & (1u64 << 63) as i64;
    let exponent = bits & (0x7ffi64 << 52);
    if mantissa == 0 && exponent == 0 {
        return f64::INFINITY.copysign(x);
    }
    if exponent == 0x7ff << 52 {
        if mantissa == 0 {
            return 0.0f64.copysign(x);
        }
        return f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000);
    }
    if exponent < 895 << 52 {
        return (f32::MAX as f64).copysign(x);
    }
    if exponent >= 1149 << 52 {
        return 0.0f64.copysign(x);
    }
    let exponent = (0x7fdi64 << 52) - exponent;
    let i = mantissa >> 37;
    let (base, dec) = FRES_EXPECTED[(i / 1024) as usize];
    let m = (base - (dec * (i % 1024) + 1) / 2) << 29;
    f64::from_bits((sign | exponent | m) as u64)
}

/// 0x801151F4: `fres(frsqrte(x*x + y*y + z*z))`.
pub fn vector_length(v: V3) -> f32 {
    let xx = v[0] * v[0];
    let s = f64::from(v[1]).mul_add(f64::from(v[1]), f64::from(xx)) as f32;
    let s = f64::from(v[2]).mul_add(f64::from(v[2]), f64::from(s)) as f32;
    gekko_fres(gekko_frsqrte(f64::from(s))) as f32
}

/// 0x800398D8: the first player slot (1 or 2) whose position lies within
/// `radius` of `query`, else 0. `players` holds each slot's position, or
/// `None` for an empty slot.
pub fn player_in_range(players: &[Option<V3>; 2], query: V3, radius: f32) -> u32 {
    for (slot, p) in players.iter().enumerate() {
        let Some(p) = p else { continue };
        let d = [p[0] - query[0], p[1] - query[1], p[2] - query[2]];
        if vector_length(d) - radius < 0.0 {
            return slot as u32 + 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_are_close_to_exact() {
        for v in [[3.0f32, 4.0, 0.0], [100.0, -20.0, 7.5], [0.01, 0.0, 0.0]] {
            let exact = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let est = vector_length(v);
            assert!(
                (est - exact).abs() / exact < 1.0 / 512.0,
                "{v:?}: {est} vs {exact}"
            );
        }
        assert_eq!(vector_length([0.0; 3]), 0.0);
    }

    #[test]
    fn first_slot_in_range_wins() {
        let players = [Some([10.0, 0.0, 0.0]), Some([1.0, 0.0, 0.0])];
        assert_eq!(player_in_range(&players, [0.0; 3], 5.0), 2);
        assert_eq!(player_in_range(&players, [0.0; 3], 50.0), 1);
        assert_eq!(player_in_range(&[None, None], [0.0; 3], 50.0), 0);
    }
}
