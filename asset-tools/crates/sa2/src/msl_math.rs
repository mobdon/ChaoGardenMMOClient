//! MSL single-precision arc functions of the GameCube main program
//! (GSNE8P main.dol, SHA-1 e4c50f32242a3cd36cdc8ae776661601a52130a2), and the
//! player routine that turns a floor normal into tilt angles.
//!
//! Ported instruction by instruction:
//!
//! * `atanf` (0x80110E64): range split on the argument's exponent and five
//!   breakpoints, the coefficient and offset tables at 0x801768D0, and a
//!   seven-term odd polynomial.
//! * `atan2f` (0x80111168), `asinf` (0x80111058) and `_inv_sqrtf`
//!   (0x801110B4).
//! * 0x8005071C: tilt angles (BAMS) from a unit floor normal.
//!
//! `fmadds` is evaluated as a fused double-precision multiply-add rounded to
//! single once; `fnmsubs` in double precision, then rounded to single.
//! `frsqrte` returns Gekko's table estimate (`gekko_frsqrte`, the same
//! transcription as tests/gekko_float.py), rounded to single by `frsp`. `frsqrte` is a hardware
//! estimate; it is modelled as the correctly rounded reciprocal square root
//! rounded to single, and three Newton steps follow as in the original.
//! Gekko's estimate table is not reproduced.

/// `.sdata2` constants (r2 = 0x803B6520).
const ATAN_BIG: f32 = f32::from_bits(0x401a_827a); // r2-0x5860, 2.4142136
const ONE: f32 = f32::from_bits(0x3f80_0000); // r2-0x585C
const ATAN_SMALL: f32 = f32::from_bits(0x3ed4_13cd); // r2-0x5858, 0.41421357
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb); // r2-0x5854
const ZERO: f32 = 0.0; // r2-0x5850
const NEWTON_HALF: f32 = 0.5; // r2-0x584C
const NEWTON_THREE: f32 = 3.0; // r2-0x5848
const PI: f32 = f32::from_bits(0x4049_0fdb); // r2-0x5844
/// 0x8005071C thresholds (r2-0x73F0, r2-0x73EC, r2-0x73E8) and the
/// radians-to-BAMS double at r2-0x75E8.
const NORMAL_UP: f32 = f32::from_bits(0x3f7f_fd61);
const NORMAL_FLAT: f32 = f32::from_bits(0x3b03_126f);
const NORMAL_DOWN: f32 = f32::from_bits(0xbf7f_fd61);
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);

/// Table at 0x801768D0, as big-endian single words, offsets 0x00..0xAC.
const ATAN_TABLE: [u32; 44] = [
    0x3f80_0000,
    0xbeaa_aaaa,
    0x3e4c_cc81,
    0xbe12_3e7d,
    0x3de2_1f95,
    0xbdad_417c,
    0x3d41_186d,
    0x40da_826b,
    0x404f_5958,
    0x4000_0000,
    0x3fb9_25ab,
    0x3f95_f61a,
    0x3f85_1081,
    0x36ef_692f,
    0x355c_1df9,
    0x0000_0000,
    0x3529_1d45,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x3ec9_0eaa,
    0x3f16_cbe4,
    0x3f49_0fda,
    0x3f7b_53c5,
    0x3f96_cbe2,
    0x3faf_edd9,
    0x0000_0000,
    0x3718_5d99,
    0x32c5_9189,
    0x3387_4a9e,
    0x353c_fa83,
    0x3486_37bd,
    0x3554_1063,
    0x401a_8277,
    0x3fbf_90c7,
    0x3f80_0000,
    0x3f2b_0dc1,
    0x3ed4_13cd,
    0x3e4b_afaf,
    0x3516_dc59,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
];

/// The word at byte offset `off` of the table (offsets are multiples of 4).
fn t(off: i32) -> f32 {
    f32::from_bits(ATAN_TABLE[(off / 4) as usize])
}

/// `fmadds d, a, c, b` = a * c + b.
fn fmadds(a: f32, c: f32, b: f32) -> f32 {
    (a as f64).mul_add(c as f64, b as f64) as f32
}

/// `fnmsubs d, a, c, b` = -(a * c - b).
fn fnmsubs(a: f32, c: f32, b: f32) -> f32 {
    (-(a as f64 * c as f64 - b as f64)) as f32
}

/// Gekko `frsqrte` estimate table (base, slope) per 2048-entry segment.
const FRSQRTE_EXPECTED: [(i64, i64); 32] = [
    (0x1A7E800, -0x568),
    (0x17CB800, -0x4F3),
    (0x1552800, -0x48D),
    (0x130C000, -0x435),
    (0x10F2000, -0x3E7),
    (0x0EFF000, -0x3A2),
    (0x0D2E000, -0x365),
    (0x0B7C000, -0x32E),
    (0x09E5000, -0x2FC),
    (0x0867000, -0x2D0),
    (0x06FF000, -0x2A8),
    (0x05AB800, -0x283),
    (0x046A000, -0x261),
    (0x0339800, -0x243),
    (0x0218800, -0x226),
    (0x0105800, -0x20B),
    (0x3FFA000, -0x7A4),
    (0x3C29000, -0x700),
    (0x38AA000, -0x670),
    (0x3572000, -0x5F2),
    (0x3279000, -0x584),
    (0x2FB7000, -0x524),
    (0x2D26000, -0x4CC),
    (0x2AC0000, -0x47E),
    (0x2881000, -0x43A),
    (0x2665000, -0x3FA),
    (0x2468000, -0x3C2),
    (0x2287000, -0x38E),
    (0x20C1000, -0x35E),
    (0x1F12000, -0x332),
    (0x1D79000, -0x30A),
    (0x1BF4000, -0x2E6),
];

/// Gekko's `frsqrte` on a double.
pub fn gekko_frsqrte(x: f64) -> f64 {
    let bits = x.to_bits();
    let mut mantissa = bits & ((1u64 << 52) - 1);
    let sign = bits & (1u64 << 63);
    let mut exponent = (bits & (0x7ffu64 << 52)) as i64;
    if mantissa == 0 && exponent == 0 {
        return f64::from_bits(if sign != 0 {
            0xfff0_0000_0000_0000
        } else {
            0x7ff0_0000_0000_0000
        });
    }
    if exponent == 0x7ff << 52 {
        if mantissa == 0 {
            return f64::from_bits(if sign != 0 { 0x7ff8_0000_0000_0000 } else { 0 });
        }
        return f64::from_bits(bits | 0x0008_0000_0000_0000);
    }
    if sign != 0 {
        return f64::from_bits(0x7ff8_0000_0000_0000);
    }
    if exponent == 0 {
        loop {
            exponent -= 1 << 52;
            mantissa <<= 1;
            if mantissa & (1u64 << 52) != 0 {
                break;
            }
        }
        mantissa &= (1u64 << 52) - 1;
        exponent += 1 << 52;
    }
    let exponent_lsb = (exponent & (1 << 52)) as u64;
    // C++ signed division truncates toward zero, as Rust's does.
    let half = (exponent - (0x3fe << 52)) / 2;
    let exponent = (((0x3ff_i64 << 52) - half) & (0x7ff << 52)) as u64;
    let i = ((exponent_lsb | mantissa) >> 37) as i64;
    let (base, dec) = FRSQRTE_EXPECTED[(i / 2048) as usize];
    let m = ((base + dec * (i % 2048)) as u64) << 26;
    f64::from_bits(sign | exponent | m)
}

fn frsqrte(x: f32) -> f32 {
    gekko_frsqrte(x as f64) as f32
}

/// MSL `_inv_sqrtf` (0x801110B4).
pub fn inv_sqrtf(x: f32) -> f32 {
    if x > ZERO {
        let mut e = frsqrte(x);
        for _ in 0..2 {
            let sq = e * e;
            let half = NEWTON_HALF * e;
            e = half * fnmsubs(x, sq, NEWTON_THREE);
        }
        let sq = e * e;
        let half = NEWTON_HALF * e;
        return half * fnmsubs(x, sq, NEWTON_THREE);
    }
    if x == ZERO {
        f32::from_bits(0x7f80_0000)
    } else {
        f32::from_bits(0x7fff_ffff)
    }
}

/// MSL `atanf` (0x80110E64).
pub fn atanf(x: f32) -> f32 {
    let bits = x.to_bits();
    let sign = bits & 0x8000_0000;
    let ax = f32::from_bits(bits & 0x7fff_ffff);
    let mut idx: i32 = -1;
    let mut inverted = false;
    let u;
    if ax >= ATAN_BIG {
        inverted = true;
        u = ONE / ax;
    } else if ATAN_SMALL >= ax {
        u = ax;
    } else {
        let w = ax.to_bits() as i32;
        let exp = w & 0x7f80_0000;
        idx = 0;
        if exp == 0x3f80_0000 {
            idx = if w >= 0x3f9b_f7ec { 3 } else { 2 };
            if w >= 0x3fef_789e {
                idx += 1;
            }
        } else if exp == 0x3f00_0000 {
            if w >= 0x3f08_d5b9 {
                idx = 1;
            }
            if w >= 0x3f52_1801 {
                idx += 1;
            }
        } else if exp == 0x4000_0000 {
            idx = 4;
        }
        let o = idx * 4;
        let (b, a) = (t(0x9c + o), t(0x84 + o));
        let r = ONE / (a + (ax + b));
        u = fnmsubs(r, t(0x1c + o), a) + fnmsubs(r, t(0x34 + o), b);
    }
    let u2 = u * u;
    let u3 = u * u2;
    let mut p = fmadds(u2, t(0x18), t(0x14));
    p = fmadds(u2, p, t(0x10));
    p = fmadds(u2, p, t(0x0c));
    p = fmadds(u2, p, t(0x08));
    p = fmadds(u2, p, t(0x04));
    let mut r = fmadds(u3, p, u);
    let o = idx * 4;
    r += t(0x6c + o);
    r += t(0x50 + o);
    if inverted {
        r -= HALF_PI;
        if sign == 0 {
            -r
        } else {
            r
        }
    } else {
        f32::from_bits(r.to_bits() | sign)
    }
}

/// MSL `atan2f` (0x80111168): atan of y / x with quadrant from both signs.
pub fn atan2f(y: f32, x: f32) -> f32 {
    let ys = y.to_bits() & 0x8000_0000;
    let xs = x.to_bits() & 0x8000_0000;
    if xs == ys {
        if xs != 0 {
            return atanf(y / x) - PI;
        }
        if x != ZERO {
            return atanf(y / x);
        }
        return HALF_PI;
    }
    if x < ZERO {
        return PI + atanf(y / x);
    }
    if x != ZERO {
        return atanf(y / x);
    }
    f32::from_bits(ys.wrapping_add(0x3fc9_0fdb))
}

/// MSL `asinf` (0x80111058): atanf(x / sqrt(1 - x * x)).
pub fn asinf(x: f32) -> f32 {
    atanf(x * inv_sqrtf(fnmsubs(x, x, ONE)))
}

/// MSL `acosf` (0x80111124): pi/2 - atanf(x / sqrt(1 - x * x)), the same
/// arctangent as `asinf`.
pub fn acosf(x: f32) -> f32 {
    HALF_PI - asinf(x)
}

/// 0x8005071C: tilt angles (x, z) in BAMS from a unit floor normal.
pub fn normal_angles(nx: f32, ny: f32, nz: f32) -> (i32, i32) {
    let flat = ny.abs() < NORMAL_FLAT && nx.abs() < NORMAL_FLAT;
    if nz > NORMAL_UP || (flat && nz > ZERO) {
        return (0x4000, 0);
    }
    if nz < NORMAL_DOWN || (flat && nz < ZERO) {
        return (0xc000, 0);
    }
    // fctiwz of the double product truncates toward zero.
    let ax = (BAMS_PER_RAD * asinf(nz) as f64) as i32;
    let az = (BAMS_PER_RAD * atan2f(nx, ny) as f64) as i32;
    (ax, -az)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f64, ulps: f64) -> bool {
        (a as f64 - b).abs() <= ulps * (b.abs().max(1e-30) * f32::EPSILON as f64)
    }

    /// Sanity only: MSL's single-precision routines are a few ulps from the
    /// true values. Bit equality with main.dol is tests/msl_math.rs.
    #[test]
    fn arc_functions_track_the_true_values() {
        for i in -2000..=2000 {
            let x = i as f32 * 0.005;
            assert!(
                close(atanf(x), (x as f64).atan(), 8.0),
                "atanf({x}) = {} vs {}",
                atanf(x),
                (x as f64).atan()
            );
            if x.abs() < 1.0 {
                assert!(close(asinf(x), (x as f64).asin(), 8.0), "asinf({x})");
            }
            for &x2 in &[-3.0f32, -0.5, 0.25, 2.0] {
                let got = atan2f(x, x2);
                assert!(
                    close(got, (x as f64).atan2(x2 as f64), 8.0),
                    "atan2f({x},{x2})"
                );
            }
        }
        assert_eq!(atan2f(1.0, 0.0), HALF_PI);
        assert_eq!(atan2f(-1.0, 0.0), -HALF_PI);
        assert!(inv_sqrtf(0.0).is_infinite());
        assert!(inv_sqrtf(-1.0).is_nan());
    }

    #[test]
    fn normal_angle_shortcuts() {
        assert_eq!(normal_angles(0.0, 0.0, 1.0), (0x4000, 0));
        assert_eq!(normal_angles(0.0, 0.0, -1.0), (0xc000, 0));
        let (ax, az) = normal_angles(0.0, 1.0, 0.0);
        assert_eq!((ax, az), (0, 0));
        let (_, az) = normal_angles(1.0, 0.0, 0.0);
        assert_eq!(az, -16384);
    }
}
