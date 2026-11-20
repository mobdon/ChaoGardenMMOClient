//! Angle helpers and fn_800473B4 (turn toward a target yaw), as SA2B's
//! main.dol (GameCube, GSNE8P) computes them. `tests/player_turn_dol.rs`
//! checks them against main.dol run under `tests/dol_ppc.py`.
use crate::{
    msl_math::gekko_frsqrte,
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, MatrixStack},
    ninja_trig::{fctiwz, fmadds, fnmsubs},
};

/// `SubAngle` (0x8002B904): b - a as a signed 16-bit angle.
pub fn sub_angle(a: i32, b: i32) -> i32 {
    ((b & 0xffff) - (a & 0xffff)) as i16 as i32
}

/// `DiffAngle` (0x8002B8E0): |b - a| as a 16-bit angle (0x8000 for half a turn).
pub fn diff_angle(a: i32, b: i32) -> i32 {
    let d = sub_angle(a, b);
    (if d < 0 { -d } else { d }) & 0xffff
}

/// `AdjustAngle` (0x8002B918): turn `a` toward `b` by at most `step`. Both
/// are taken as 16-bit angles and the result is 16-bit.
pub fn adjust_angle(a: i32, b: i32, step: i32) -> i32 {
    let a16 = a & 0xffff;
    let b16 = b & 0xffff;
    let d = (b16 - a16) as i16 as i32;
    if d <= step && d >= step.wrapping_neg() {
        return b16;
    }
    let r = if d & 0x8000 != 0 {
        a16.wrapping_sub(step)
    } else {
        a16.wrapping_add(step)
    };
    r & 0xffff
}

/// sdata2 constants (r2 = 0x803B6520).
const OVERSPEED_SCALE: f32 = 0.0625; // r2-0x7490
const TURN_RATE: f32 = 256.0; // r2-0x748C
const BASE_STEP: i32 = 0x100;
const UP_THRESHOLD: f32 = f32::from_bits(0x3ecc_cccd); // r2-0x7518, 0.4
const KEEP_UPRIGHT: f32 = f32::from_bits(0x3f7d_70a4); // r2-0x7498, 0.99
const BLEND_UPRIGHT: f32 = f32::from_bits(0x3c23_d700); // r2-0x7494
const HALF: f32 = 0.5; // r2-0x75CC
const NEWTON_HALF: f64 = 0.5; // r2-0x75E0
const NEWTON_THREE: f64 = 3.0; // r2-0x75D8

/// The inline square root the compiler emits for `sqrtf`: Gekko's frsqrte
/// estimate, three Newton steps in double precision, times x, rounded once.
pub fn inline_sqrtf(x: f32) -> f32 {
    let x = x as f64;
    let mut e = gekko_frsqrte(x);
    for _ in 0..2 {
        let t = e * e;
        let h = NEWTON_HALF * e;
        e = h * -x.mul_add(t, -NEWTON_THREE);
    }
    let t = e * e;
    let h = NEWTON_HALF * e;
    let r = h * -x.mul_add(t, -NEWTON_THREE);
    (x * r) as f32
}

/// fn_800473B4(task work, -, player work, target yaw): turn the task's yaw
/// toward `target` and carry the local velocity round with it.
///
/// `angles` are the task work's (X, Y, Z) words, `speed` the player work's
/// local velocity (+0x64), `dash_speed` its physics `dash_speed` (+0xF4) and
/// `up` the player work's float at +0x34.
pub fn turn_toward(
    stack: &mut MatrixStack,
    angles: &mut [i32; 3],
    speed: &mut [f32; 3],
    dash_speed: f32,
    up: f32,
    target: i32,
) {
    let world = rotate_by_angles(stack, *angles, *speed);
    let mut step = BASE_STEP;
    if speed[0] > dash_speed {
        let mut over = (speed[0] - dash_speed) * OVERSPEED_SCALE;
        if over > 0.0 {
            over = inline_sqrtf(over);
        }
        step = fctiwz(fnmsubs(TURN_RATE, over, (BASE_STEP as f64) as f32));
    }
    angles[1] = adjust_angle(angles[1], target, step);
    let local = unrotate_by_angles(stack, *angles, world);
    let (keep, take) = if up > UP_THRESHOLD {
        (KEEP_UPRIGHT, BLEND_UPRIGHT)
    } else {
        (HALF, HALF)
    };
    for k in 0..3 {
        speed[k] = fmadds(speed[k], keep, local[k] * take);
    }
}
