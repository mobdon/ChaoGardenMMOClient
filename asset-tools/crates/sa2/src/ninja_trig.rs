//! Native trigonometry of the GameCube main program (GSNE8P main.dol).
//!
//! Two paths, both read from main.dol:
//!
//! * `nj_rotate_sin_cos`: the njRotateX/Y/Z calls the stage modules make
//!   (PC stage-module imports 0x80115918 / 0x801154E0 / 0x80115560, GSNE8P
//!   0x80115C60 / 0x80115828 / 0x801158A8). Each converts the BAMS angle to
//!   radians in double precision (`(double)angle * 2pi/65536`, the constant
//!   at r2-0x5650) and rounds it to single, then the matrix rotate at
//!   0x800ED57C calls MSL `sinf` (0x801115A8) and `cosf` (0x80111414).
//!   The dynamic land-object register (GSNE8P 0x800232A4) uses the same
//!   rotate calls.
//! * `nj_sin` / `nj_cos`: the table lookup at PC import 0x80112050 (GSNE8P
//!   0x80112398 / 0x801123B0): 65536 (sin, cos) pairs built at start-up by
//!   GSNE8P 0x801182CC from `x = (3.1415920 * (float)(2 i)) / 65536`.
//!
//! `sinf` and `cosf` are ported instruction by instruction: a reduction by
//! 2/pi with the four-term 4/pi - 1 correction (copied into place at
//! start-up by 0x8011174C from 0x80176988), a quadrant table (0x80176DA0)
//! and two polynomials (0x80176DC0). `fmadds` and its negated forms are
//! evaluated in double precision and rounded to single once.
use std::sync::OnceLock;

const C0: f32 = f32::from_bits(0x3f22_f983); // r2-0x5838
const HALF: f32 = 0.5; // r2-0x5834
const SMALL: f32 = f32::from_bits(0x39b5_04f3); // r2-0x5830
const FOUR_OVER_PI_M1: [u32; 4] = [0x3e80_0000, 0x3cbe_6080, 0x3437_2200, 0x2da4_4152];
/// (sin, cos) of q * pi/2.
const QUADRANT: [f32; 8] = [0.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0, 0.0];
const POLY: [u32; 10] = [
    0x366c_cfaa,
    0x34a5_e129,
    0xb9aa_e275,
    0xb819_6543,
    0x3c81_e0ed,
    0x3b23_35dd,
    0xbe9d_e9e6,
    0xbda5_5de7,
    0x3f80_0000,
    0x3f49_0fdb,
];
/// njRotate's BAMS-to-radian factor (double at r2-0x5650).
const RAD_PER_BAMS: f64 = f64::from_bits(0x3f19_21fb_0000_0000);
/// The table builder's pi (r2-0x55F4) and divisor (r2-0x55F0).
const TABLE_PI: f32 = f32::from_bits(0x4049_0fd8);
const TABLE_DIV: f32 = 65536.0;

fn p(i: usize) -> f32 {
    f32::from_bits(POLY[i])
}

/// `fmadds d, a, c, b`: a * c + b. The product of two singles is exact in
/// double, so this is the fused result rounded to double, then to single.
pub fn fmadds(a: f32, c: f32, b: f32) -> f32 {
    (a as f64 * c as f64 + b as f64) as f32
}

/// PowerPC negates a negative-multiply-add result unless it is a NaN.
fn negate_unless_nan(v: f32) -> f32 {
    if v.is_nan() {
        v
    } else {
        -v
    }
}

/// `fnmadds`: -(a * c + b).
pub fn fnmadds(a: f32, c: f32, b: f32) -> f32 {
    negate_unless_nan(fmadds(a, c, b))
}

/// `fnmsubs`: -(a * c - b).
pub fn fnmsubs(a: f32, c: f32, b: f32) -> f32 {
    negate_unless_nan(fmadds(a, c, -b))
}

/// `fctiwz`: truncate toward zero. NaN and every out-of-range value give
/// 0x80000000, as QEMU's core (tests/dol_ppc.py) and x86's cvttss2si do.
/// Gekko itself saturates positive overflow to 0x7FFFFFFF.
pub fn fctiwz(x: f32) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

fn reduce(x: f32) -> (f32, usize) {
    let scaled = C0 * x;
    // fctiwz truncates toward zero.
    let n = if x.is_sign_negative() {
        fctiwz(scaled - HALF)
    } else {
        fctiwz(HALF + scaled)
    };
    // slwi wraps; the exact integer is rounded once to single.
    let twice = (n as u32).wrapping_shl(1) as i32;
    let mut y = x - twice as f32;
    for w in FOUR_OVER_PI_M1 {
        y = fmadds(f32::from_bits(w), x, y);
    }
    (y, (n & 3) as usize)
}

/// MSL `cosf` (GSNE8P 0x80111414).
pub fn msl_cosf(x: f32) -> f32 {
    let (y, q) = reduce(x);
    let (s, c) = (QUADRANT[2 * q], QUADRANT[2 * q + 1]);
    if y.abs() < SMALL || y.is_nan() {
        return fnmsubs(y, s, c);
    }
    let y2 = y * y;
    if q & 1 != 0 {
        let mut t = fmadds(p(1), y2, p(3));
        t = fmadds(y2, t, p(5));
        t = fmadds(y2, t, p(7));
        let t = fnmadds(y2, t, p(9));
        return (y * t) * s;
    }
    let mut t = fmadds(p(0), y2, p(2));
    t = fmadds(y2, t, p(4));
    t = fmadds(y2, t, p(6));
    t = fmadds(y2, t, p(8));
    t * c
}

/// MSL `sinf` (GSNE8P 0x801115A8).
pub fn msl_sinf(x: f32) -> f32 {
    let (y, q) = reduce(x);
    let (s, c) = (QUADRANT[2 * q], QUADRANT[2 * q + 1]);
    if y.abs() < SMALL || y.is_nan() {
        return fmadds(p(9), y * c, s);
    }
    let y2 = y * y;
    if q & 1 != 0 {
        let mut t = fmadds(p(0), y2, p(2));
        t = fmadds(y2, t, p(4));
        t = fmadds(y2, t, p(6));
        t = fmadds(y2, t, p(8));
        return t * s;
    }
    let mut t = fmadds(p(1), y2, p(3));
    t = fmadds(y2, t, p(5));
    t = fmadds(y2, t, p(7));
    t = fmadds(y2, t, p(9));
    (y * t) * c
}

/// Sine and cosine njRotateX/Y/Z use for a BAMS angle (task rotation word).
pub fn nj_rotate_sin_cos(angle: i32) -> (f32, f32) {
    let rad = (angle as f64 * RAD_PER_BAMS) as f32;
    (msl_sinf(rad), msl_cosf(rad))
}

fn table() -> &'static [(f32, f32)] {
    static T: OnceLock<Vec<(f32, f32)>> = OnceLock::new();
    T.get_or_init(|| {
        (0..65536u32)
            .map(|i| {
                let x = (TABLE_PI * (2 * i) as f32) / TABLE_DIV;
                (msl_sinf(x), msl_cosf(x))
            })
            .collect()
    })
}

/// njSin: the start-up table entry for `angle & 0xFFFF`.
pub fn nj_sin(angle: i32) -> f32 {
    table()[(angle & 0xffff) as usize].0
}

/// njCos: the start-up table entry for `angle & 0xFFFF`.
pub fn nj_cos(angle: i32) -> f32 {
    table()[(angle & 0xffff) as usize].1
}

/// njSinCosTbl (GSNE8P 0x802D7758) as main.dol stores it: big-endian
/// (sin, cos) single pairs.
pub fn table_be_bytes() -> Vec<u8> {
    table()
        .iter()
        .flat_map(|&(s, c)| s.to_be_bytes().into_iter().chain(c.to_be_bytes()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msl_trig_tracks_libm_closely() {
        for i in -400..400 {
            let x = i as f32 * 0.037;
            assert!((msl_sinf(x) - x.sin()).abs() < 6e-5, "{x}");
            assert!((msl_cosf(x) - x.cos()).abs() < 6e-5, "{x}");
        }
    }

    #[test]
    fn quarter_turns_are_not_exact_zeros() {
        // njRotate's radians for 0x4000 are 1.5707964 in single precision.
        let (s, c) = nj_rotate_sin_cos(0x4000);
        assert_eq!(s, 1.0);
        assert!(c != 0.0 && c.abs() < 1e-6, "{c}");
        assert_eq!(nj_rotate_sin_cos(0), (0.0, 1.0));
        // The table's pi is 3.1415920, so its quarter-turn cosine is positive.
        assert!(nj_cos(0x4000) > 0.0 && nj_cos(0x4000) < 1e-6);
        assert_eq!(nj_sin(0x4000), 1.0);
    }
}
