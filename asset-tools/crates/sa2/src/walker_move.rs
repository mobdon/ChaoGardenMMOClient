//! Shared player movement steps the walker exec's mode handlers call
//! (GSNE8P main.dol), ported instruction by instruction and checked against
//! main.dol by tests/walker_move.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, MatrixStack, Vec3},
    ninja_trig::{fmadds, fnmsubs},
    walker_collide::adjust_angle,
};

const ZERO: f32 = 0.0; // r2-0x75F8
const HALF: f32 = 0.5; // r2-0x75CC
const WEIGHT_SHARE: f32 = f32::from_bits(0x3fb3_3333); // r2-0x74E4, 1.4

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// fn_80047A1C: while airborne and not faster than +0xF4 (or with work flag
/// 0x8000 at +0x24), turn the task's X and Z angles toward the targets at
/// r13-0x79A8 / r13-0x79AC by at most 0x800, keeping the world velocity and
/// the sphere's center where they are.
#[allow(clippy::too_many_arguments)]
pub fn air_level(
    stack: &mut MatrixStack,
    task_flag: i16,
    angles: &mut [i32; 3],
    pos: &mut Vec3,
    spd: &mut Vec3,
    w24: u32,
    f_f4: f32,
    f134: f32,
    target: (i32, i32),
) {
    if task_flag & 3 != 0 {
        return;
    }
    let s = *spd;
    if w24 & 0x8000 == 0 {
        let x2 = s[0] * s[0];
        let y2 = s[1] * s[1];
        let z2 = s[2] * s[2];
        if z2 + (x2 + y2) > f_f4 * f_f4 {
            return;
        }
    }
    let world = rotate_by_angles(stack, *angles, s);
    let half = [ZERO, HALF * f134, ZERO];
    let center = add(rotate_by_angles(stack, *angles, half), *pos);
    angles[0] = adjust_angle(angles[0], target.0, 0x800);
    angles[2] = adjust_angle(angles[2], target.1, 0x800);
    *spd = unrotate_by_angles(stack, *angles, world);
    *pos = center;
    *pos = sub(*pos, rotate_by_angles(stack, *angles, half));
}

/// The player-work fields fn_80048ACC reads.
#[derive(Debug, Clone, Copy)]
pub struct AccelWork {
    /// `spd` (+0x64).
    pub spd: Vec3,
    /// +0x40 (surface drag share).
    pub f40: f32,
    /// PhysicsArray +0xD0, +0x114, +0x118, +0x11C, +0x120, +0x124, +0x128.
    pub f_d0: f32,
    pub air_resist: Vec3,
    pub decel_x: f32,
    pub decel_z: f32,
    pub stop_share: f32,
    /// `weight` (+0x138).
    pub weight: f32,
}

/// fn_80048ACC: the player's acceleration this frame (`acc`, player
/// +0x58). The motion work's acceleration (+0xC) plus gravity (the global
/// vector times `weight`) is taken into the task frame; on the ground the
/// collision query's across-plane motion (+0x78, which this clears) adds to
/// y and z, scaled by +0xD0 over weight/1.4. Air resistance per axis
/// follows. On the ground x and z then brake toward zero by
/// +0x120 / +0x124 times `f40` and the downward pull, never reversing the
/// speed, and stay at zero when the speed is zero and the push is small.
/// Returns `None` (acc unchanged) with task flag 0x4000.
#[allow(clippy::too_many_arguments)]
pub fn accel(
    stack: &mut MatrixStack,
    task_flag: i16,
    angles: [i32; 3],
    w: &AccelWork,
    mw_acc: Vec3,
    global: Vec3,
    across: &mut Vec3,
) -> Option<Vec3> {
    if task_flag & 0x4000 != 0 {
        return None;
    }
    let mut a = mw_acc;
    for j in 0..3 {
        a[j] = fmadds(global[j], w.weight, a[j]);
    }
    a = unrotate_by_angles(stack, angles, a);
    let grounded = task_flag & 3 != 0;
    if grounded {
        let k = w.f_d0 / (w.weight / WEIGHT_SHARE);
        let c = across.map(|x| x / k);
        *across = [ZERO; 3];
        let c = unrotate_by_angles(stack, angles, c);
        a[1] += c[1];
        a[2] += c[2];
    }
    for j in 0..3 {
        a[j] = fmadds(w.air_resist[j], w.spd[j], a[j]);
    }
    if !grounded {
        return Some(a);
    }
    for (j, decel) in [(0usize, w.decel_x), (2, w.decel_z)] {
        let s = w.spd[j];
        if s == ZERO {
            let lim = w.f40 * (a[1] * w.stop_share);
            if a[j] < lim && a[j] > -lim {
                a[j] = ZERO;
            }
            continue;
        }
        let mut brake = ZERO;
        if a[1] < ZERO {
            brake = w.f40 * (a[1] * decel);
        }
        let mut acc = ZERO;
        if a[j] > ZERO {
            acc -= brake;
        } else if a[j] < ZERO {
            acc += brake;
        }
        a[j] += acc;
        if s > ZERO {
            if s < -a[j] {
                a[j] = -s;
            }
        } else if s < ZERO && s > -a[j] {
            a[j] = -s;
        }
    }
    Some(a)
}

/// fn_80048EC4: the running acceleration, like `accel` without the
/// across-plane term but with the run brake (+0x108, scaled by `f40`)
/// pushing x and z against their speed. On the ground x is braked toward
/// zero without crossing it, then kept from reversing the speed; z brakes
/// as in `accel`. `turn` is fn_80047714(task, motion, player, aim), which
/// the routine calls before storing the result (also when airborne).
#[allow(clippy::too_many_arguments)]
pub fn accel_run(
    stack: &mut MatrixStack,
    task_flag: i16,
    angles: [i32; 3],
    w: &AccelWork,
    run_break: f32,
    mw_acc: Vec3,
    global: Vec3,
    turn: &mut dyn FnMut(),
) -> Option<Vec3> {
    if task_flag & 0x4000 != 0 {
        return None;
    }
    let mut a = mw_acc;
    for j in 0..3 {
        a[j] = fmadds(global[j], w.weight, a[j]);
    }
    a = unrotate_by_angles(stack, angles, a);
    for j in 0..3 {
        a[j] = fmadds(w.air_resist[j], w.spd[j], a[j]);
    }
    let push = |s: f32| {
        if s > ZERO {
            fmadds(run_break, w.f40, ZERO)
        } else if s < ZERO {
            fnmsubs(run_break, w.f40, ZERO)
        } else {
            ZERO
        }
    };
    let px = push(w.spd[0]);
    let mut pz = push(w.spd[2]);
    if task_flag & 3 != 0 {
        let s = w.spd[0];
        if s == ZERO {
            let lim = w.f40 * (a[1] * w.stop_share);
            if a[0] < lim && a[0] > -lim {
                a[0] = ZERO;
            }
        } else {
            let mut brake = ZERO;
            if a[1] < ZERO {
                brake = w.f40 * (a[1] * w.decel_x);
            }
            let mut x = a[0] + px;
            if x > ZERO {
                x -= brake;
                if x < ZERO {
                    x = ZERO;
                }
            } else if x < ZERO {
                x += brake;
                if x > ZERO {
                    x = ZERO;
                }
            }
            a[0] = x;
            if s > ZERO {
                if s < -a[0] {
                    a[0] = -s;
                }
            } else if s < ZERO && s > -a[0] {
                a[0] = -s;
            }
        }
        let s = w.spd[2];
        if s == ZERO {
            let lim = w.f40 * (a[1] * w.stop_share);
            if a[2] < lim && a[2] > -lim {
                a[2] = ZERO;
            }
        } else {
            let mut brake = ZERO;
            if a[1] < ZERO {
                brake = w.f40 * (a[1] * w.decel_z);
            }
            if a[2] > ZERO {
                pz -= brake;
            } else if a[2] < ZERO {
                pz += brake;
            }
            a[2] += pz;
            if s > ZERO {
                if s < -a[2] {
                    a[2] = -s;
                }
            } else if s < ZERO && s > -a[2] {
                a[2] = -s;
            }
        }
    }
    turn();
    Some(a)
}
