//! fn_80047CBC and fn_80048150: turn the player to face down a slope, as
//! SA2B's main.dol (GameCube, GSNE8P) computes them.
//!
//! Both rotate the gravity vector (0x801E5624) into the player's frame. On a
//! slope they aim the motion work's yaw (+0x1C) down it, faster when the
//! status has bit 0x100. fn_80047CBC then turns the task yaw by up to
//! 0x2000 toward that aim and carries the velocity round. fn_80048150 hands
//! over to fn_80047714. `tests/player_slope_dol.rs` checks both against
//! main.dol.
use crate::{
    msl_math::atan2f,
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, MatrixStack},
    ninja_trig::fctiwz,
    ninja_vector::{inner_product, scalor, unit_vector},
    player_analog::AnalogContext,
    player_turn::{adjust_angle, turn_smoothly, TurnPlayer},
};

/// sdata2 constants (r2 = 0x803B6520).
const STEEP_DOT: f64 = -0.86; // r2-0x7488 (double)
const FLAT_UP: f32 = f32::from_bits(0xbf5e_b852); // r2-0x7480, -0.87
const NEAR_FLAT_UP: f32 = f32::from_bits(0xbf66_6666); // r2-0x746C, -0.9
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb); // r2-0x75E8
const STRONG_RATE: f32 = 4096.0; // r2-0x747C
const STRONG_IDLE: f32 = 2048.0; // r2-0x7478
const SOFT_RATE: f32 = 3072.0; // r2-0x7474
const SOFT_IDLE: f32 = 1536.0; // r2-0x7470
/// fn_80048150 also aims down slopes up to -0.9 on this stage.
const SLOPE_STAGE: i16 = 0x22;

/// The player and stage state both routines read.
#[derive(Debug, Clone, Copy, Default)]
pub struct SlopePlayer {
    /// Task work status (+4), angles (+8..+0x10).
    pub status: i16,
    pub angles: [i32; 3],
    /// Player work no-control timer (+0x14), surface flags (+0x88),
    /// local velocity (+0x64), physics jog_speed (+0xE4).
    pub nocon: i16,
    pub surface: u32,
    pub speed: [f32; 3],
    pub jog_speed: f32,
    /// Motion work velocity (+0) and aim yaw (+0x1C).
    pub motion: [f32; 3],
    pub aim: i32,
}

/// The aim toward the downhill direction in the player's frame `g`.
fn downhill_aim(p: &SlopePlayer, mut g: [f32; 3], rates: (f32, f32), scale_by_speed: bool) -> i32 {
    // fcmpo ...; bge: a NaN is negated too.
    if g[0] < 0.0 || g[0].is_nan() {
        g[0] = -g[0];
    }
    let a = atan2f(g[2], g[0]);
    let target = p.angles[1].wrapping_add(fctiwz_f64(BAMS_PER_RAD * a as f64));
    if g[2] < 0.0 || g[2].is_nan() {
        g[2] = -g[2];
    }
    let step = if p.status & 0x100 != 0 {
        if scale_by_speed {
            fctiwz((rates.0 * g[2] * p.speed[0]) / p.jog_speed)
        } else {
            fctiwz(rates.0 * g[2])
        }
    } else {
        fctiwz(rates.1 * g[2])
    };
    adjust_angle(p.aim, target, step)
}

fn fctiwz_f64(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// gravity . unit(motion velocity), when that velocity is faster than
/// jog_speed (estimated by njScalor); None otherwise.
fn motion_dot(p: &SlopePlayer, gravity: [f32; 3]) -> Option<f64> {
    let length = scalor(p.motion);
    if length <= p.jog_speed || length.is_nan() || p.jog_speed.is_nan() {
        return None;
    }
    let (v, _) = unit_vector(p.motion);
    Some(inner_product(gravity, v) as f64)
}

/// fn_80047CBC(task work, motion work, player work).
pub fn slope_turn_strong(stack: &mut MatrixStack, p: &mut SlopePlayer, gravity: [f32; 3]) {
    if p.nocon != 0 {
        return;
    }
    // Continue only when moving fast and not steeply along gravity.
    match motion_dot(p, gravity) {
        Some(d) if d >= STEEP_DOT => {}
        _ => return,
    }
    let g = unrotate_by_angles(stack, p.angles, gravity);
    p.aim = if g[1] > FLAT_UP {
        downhill_aim(p, g, (STRONG_RATE, STRONG_IDLE), true)
    } else {
        p.angles[1]
    };
    let world = rotate_by_angles(stack, p.angles, p.speed);
    p.angles[1] = adjust_angle(p.angles[1], p.aim, 0x2000);
    p.speed = unrotate_by_angles(stack, p.angles, world);
}

/// fn_80048150(task work, motion work, player work); `turn` carries the
/// fields fn_80047714 also reads.
pub fn slope_turn(
    stack: &mut MatrixStack,
    p: &mut SlopePlayer,
    gravity: [f32; 3],
    stage: i16,
    turn: TurnPlayer,
    analog: AnalogContext,
) {
    if p.nocon != 0 || p.surface & 0x100 != 0 {
        return;
    }
    if motion_dot(p, gravity).is_some_and(|d| d < STEEP_DOT) {
        return;
    }
    let g = unrotate_by_angles(stack, p.angles, gravity);
    let downhill = g[1] < 0.0 && (g[1] > FLAT_UP || (g[1] > NEAR_FLAT_UP && stage == SLOPE_STAGE));
    p.aim = if downhill {
        downhill_aim(p, g, (SOFT_RATE, SOFT_IDLE), false)
    } else {
        p.angles[1]
    };
    turn_smoothly(stack, &mut p.angles, &mut p.speed, turn, analog, p.aim);
}
