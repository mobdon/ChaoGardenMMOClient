//! `njRotate` (rotation about an arbitrary axis) and the SDK routines under
//! it, `PSMTXRotAxisRad` and `PSVECNormalize`, as SA2B's main.dol (GSNE8P)
//! computes them. Each paired-single lane rounds once to single precision,
//! in the order of the compiled instructions. `tests/walker_move.rs` checks
//! them against main.dol through fn_80048490.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    msl_math::gekko_frsqrte,
    ninja_matrix::{concat, Mtx, Vec3},
    ninja_trig::{fmadds, fnmsubs, msl_cosf, msl_sinf},
};

const RAD_PER_BAMS: f64 = f64::from_bits(0x3f19_21fb_0000_0000); // r2-0x5688 (double)
const HALF: f32 = 0.5; // r2-0x5D30
const THREE: f32 = 3.0; // r2-0x5D2C

/// `PSVECNormalize` (0x800EDDF0): the vector times Gekko's `frsqrte`
/// estimate of its squared length refined by one Newton step.
pub fn ps_vec_normalize(v: Vec3) -> Vec3 {
    let (xx, yy) = (v[0] * v[0], v[1] * v[1]);
    let sum = fmadds(v[2], v[2], xx) + yy;
    let e = gekko_frsqrte(f64::from(sum));
    let e2 = (e * e) as f32;
    let h = (e * f64::from(HALF)) as f32;
    let k = fnmsubs(e2, sum, THREE) * h;
    [v[0] * k, v[1] * k, v[2] * k]
}

/// `PSMTXRotAxisRad` (0x800ED694): the rotation by `rad` about `axis`
/// (normalized first), with MSL sinf/cosf.
pub fn rot_axis_rad(axis: Vec3, rad: f32) -> Mtx {
    let s = msl_sinf(rad);
    let c = msl_cosf(rad);
    let t = 1.0 - c;
    let [x, y, z] = ps_vec_normalize(axis);
    let (xt, yt, zt) = (x * t, y * t, z * t);
    let (xty, yty) = (xt * y, yt * y);
    let xtx = xt * x;
    let (xs, ys) = (x * s, y * s);
    let (xtz, ytz) = (xt * z, yt * z);
    let lo = fnmsubs(z, s, xty); // xty - zs
    let hi = fmadds(z, s, xty); // xty + zs
    let ztz = zt * z;
    [
        [xtx + c, lo, xtz + ys, 0.0],
        [hi, c + yty, -xs + ytz, 0.0],
        [xtz - ys, xs + ytz, ztz + c, 0.0],
    ]
}

/// `njRotate(m, axis, angle)` (0x80115218): m = m * R(axis, angle), the
/// BAMS angle as single-precision radians.
pub fn rotate_axis(m: &mut Mtx, axis: Vec3, angle: i32) {
    let rad = (f64::from(angle) * RAD_PER_BAMS) as f32;
    *m = concat(m, &rot_axis_rad(axis, rad));
}
