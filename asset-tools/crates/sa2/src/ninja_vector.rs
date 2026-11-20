//! Ninja vector products, MSL `acosf` and `VectorAngle`, as SA2B's main.dol
//! (GameCube, GSNE8P) computes them. `tests/ninja_vector_dol.rs` checks
//! them against main.dol run under `tests/dol_ppc.py`.
use crate::{
    msl_math::{atanf, inv_sqrtf},
    ninja_trig::{fmadds, fnmsubs},
};

pub type Vec3 = [f32; 3];

/// `fmsubs d, a, c, b`: a*c - b, fused, rounded to double then to single.
fn fmsubs(a: f32, c: f32, b: f32) -> f32 {
    fmadds(a, c, -b)
}

/// `njInnerProduct` (0x80114FD0).
pub fn inner_product(a: Vec3, b: Vec3) -> f32 {
    let d = b[0] * a[0];
    let d = fmadds(a[1], b[1], d);
    fmadds(a[2], b[2], d)
}

/// `njOuterProduct` (0x8011502C): a x b. It also returns an estimate of the
/// cross product's length (fres of frsqrte of its square), which callers
/// here ignore.
pub fn outer_product(a: Vec3, b: Vec3) -> Vec3 {
    let x = fmsubs(b[2], a[1], b[1] * a[2]);
    let y = fmsubs(b[0], a[2], b[2] * a[0]);
    let z = fmsubs(b[1], a[0], b[0] * a[1]);
    [x, y, z]
}

/// sdata2 constants (r2 = 0x803B6520).
const ONE: f32 = 1.0; // r2-0x585C
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb); // r2-0x5854
const MINUS_ONE: f32 = -1.0; // r2-0x7878
const PLUS_ONE: f32 = 1.0; // r2-0x78DC
const ZERO: f32 = 0.0; // r2-0x7910
/// Radians to BAMS (double at r2-0x7850).
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);

/// MSL `acosf` (0x80111124): pi/2 - atan(x / sqrt(1 - x^2)).
pub fn acosf(x: f32) -> f32 {
    let r = inv_sqrtf(fnmsubs(x, x, ONE));
    HALF_PI - atanf(x * r)
}

/// `VectorAngle` (0x8002B7C4): the BAMS angle between unit vectors `a` and
/// `b`, mirrored to 0x10000 - angle when `side` is given and points against
/// a x b.
pub fn vector_angle(a: Vec3, b: Vec3, side: Option<Vec3>) -> i32 {
    let d = inner_product(a, b);
    if d <= MINUS_ONE {
        return 0x8000;
    }
    if d >= PLUS_ONE {
        return 0;
    }
    let mut angle = fctiwz_f64(BAMS_PER_RAD * acosf(d) as f64);
    let cross = outer_product(a, b);
    if let Some(c) = side {
        let dot = inner_product(c, cross);
        if dot < ZERO {
            angle = 0x10000i32.wrapping_sub(angle);
        }
    }
    angle
}

/// `fctiwz` of a double-precision value (0x80000000 for NaN or overflow).
fn fctiwz_f64(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}
