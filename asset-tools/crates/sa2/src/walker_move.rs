//! Shared player movement steps the walker exec's mode handlers call
//! (GSNE8P main.dol), ported instruction by instruction and checked against
//! main.dol by tests/walker_move.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    msl_math::atan2f,
    ninja_axis::rotate_axis,
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::{fmadds, fnmsubs, nj_cos, nj_sin},
    ninja_vector::vector_angle,
    walker_collide::{adjust_angle, diff_angle, sub_angle},
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

const TWICE: f32 = 2.0; // r2-0x75FC

/// fn_8004B330: the airborne acceleration. As `accel_run` but with the air
/// resistance at +0x110 for x, no ground braking, and the stick: `analog`
/// is fn_8004E560(task, player, &angle, &magnitude), returning the stick
/// angle and magnitude when it is held. Without it the motion work's aim
/// takes the task's yaw. With it the magnitude is scaled by +0x10C when
/// turning back hard above `run_speed`, dropped when the stick is more than
/// 0x1000 off the yaw (aiming there), else scaled by +0x100 (doubled while
/// falling) with the aim set; then `turn` (fn_80047714) runs. The magnitude
/// adds to x.
#[allow(clippy::too_many_arguments)]
pub fn accel_air(
    stack: &mut MatrixStack,
    angles: [i32; 3],
    w: &AccelWork,
    air_x: f32,
    run_speed: f32,
    f_100: f32,
    f_10c: f32,
    mw_acc: Vec3,
    mw_spd_y: f32,
    aim: &mut i32,
    global: Vec3,
    analog: &mut dyn FnMut() -> Option<(i32, f32)>,
    turn: &mut dyn FnMut(i32),
) -> Vec3 {
    let mut a = mw_acc;
    for j in 0..3 {
        a[j] = fmadds(global[j], w.weight, a[j]);
    }
    a = unrotate_by_angles(stack, angles, a);
    a[0] = fmadds(air_x, w.spd[0], a[0]);
    a[1] = fmadds(w.air_resist[1], w.spd[1], a[1]);
    a[2] = fmadds(w.air_resist[2], w.spd[2], a[2]);
    let mag = match analog() {
        None => {
            *aim = angles[1];
            ZERO
        }
        Some((ang, mag)) => {
            let d = diff_angle(angles[1], ang);
            let mag = if w.spd[0] > run_speed && d > 0x6000 {
                f_10c * mag
            } else if d > 0x1000 {
                *aim = ang;
                ZERO
            } else {
                *aim = ang;
                if mw_spd_y < ZERO {
                    TWICE * (f_100 * mag)
                } else {
                    f_100 * mag
                }
            };
            turn(*aim);
            mag
        }
    };
    a[0] += mag;
    a
}

/// fn_80048724: the motion work's acceleration plus gravity (the global
/// vector times `weight`), taken into the task frame. `None` with task flag
/// 0x4000.
pub fn accel_gravity(
    stack: &mut MatrixStack,
    task_flag: i16,
    angles: [i32; 3],
    weight: f32,
    mw_acc: Vec3,
    global: Vec3,
) -> Option<Vec3> {
    if task_flag & 0x4000 != 0 {
        return None;
    }
    let mut a = mw_acc;
    for j in 0..3 {
        a[j] = fmadds(global[j], weight, a[j]);
    }
    Some(unrotate_by_angles(stack, angles, a))
}

const LIFT_FLIP: f32 = -8.0; // r2-0x7468
const NEG_ZERO: f32 = -0.0; // r2-0x75A4
const SOFT_RESIST: f32 = f32::from_bits(0xb951_b717); // r2-0x7458, -0.0002
const SOFT_LIMIT: f64 = 0.98; // r2-0x7460 (double)

/// fn_80048834, mode 0x43's acceleration: `accel_gravity`, then when faster
/// than `f_e8` (+0xE8) with `dotp` (+0x34) below zero the y term is multiplied by
/// -8; on the ground the across-plane push adds as in `accel` (and is
/// cleared). Air resistance follows, except that with task flag 0x100 x
/// uses -0.0002 unless `dotp` is above 0.98.
#[allow(clippy::too_many_arguments)]
pub fn accel_mode43(
    stack: &mut MatrixStack,
    task_flag: i16,
    angles: [i32; 3],
    w: &AccelWork,
    dotp: f32,
    f_e8: f32,
    mw_acc: Vec3,
    global: Vec3,
    across: &mut Vec3,
) -> Option<Vec3> {
    let mut a = accel_gravity(stack, task_flag, angles, w.weight, mw_acc, global)?;
    if w.spd[0] > f_e8 && dotp < NEG_ZERO {
        a[1] *= LIFT_FLIP;
    }
    if task_flag & 3 != 0 {
        let k = w.f_d0 / (w.weight / WEIGHT_SHARE);
        let c = across.map(|x| x / k);
        *across = [ZERO; 3];
        let c = unrotate_by_angles(stack, angles, c);
        a[1] += c[1];
        a[2] += c[2];
    }
    let rx = if task_flag & 0x100 != 0 && !(f64::from(dotp) > SOFT_LIMIT) {
        SOFT_RESIST
    } else {
        w.air_resist[0]
    };
    a[0] = fmadds(rx, w.spd[0], a[0]);
    a[1] = fmadds(w.air_resist[1], w.spd[1], a[1]);
    a[2] = fmadds(w.air_resist[2], w.spd[2], a[2]);
    Some(a)
}

/// The routines fn_80048490 hands over to (the Sonic thread's
/// `player_turn` / `player_slope`, and `player_analog`).
pub trait SteerCalls {
    /// fn_80038F40(task, motion, player), with task flag 0x2000.
    fn flagged_turn(&mut self);
    /// fn_8004E560(task, player, &angle, NULL): the stick angle when held.
    fn analog(&mut self) -> Option<i32>;
    /// fn_80048150(task, motion, player).
    fn slope_turn(&mut self);
    /// fn_800473B4(task, motion, player, aim).
    fn turn_toward(&mut self, aim: &mut i32);
    /// fn_80047714(task, motion, player, aim).
    fn turn_smoothly(&mut self, aim: &mut i32);
}

/// What fn_80048490 reads besides its callees.
#[derive(Debug, Clone, Copy)]
pub struct SteerInput {
    pub task_flag: i16,
    /// Task angles (+8, +0xC, +0x10).
    pub angles: [i32; 3],
    /// `attr` (+0x88).
    pub attr: u32,
    /// `floor_normal` (+0x7C).
    pub normal: Vec3,
    /// The player's stick halfwords at 0x801E5438 + 0x34 * pnum, +0x1C and
    /// +0x1E.
    pub stick: (i16, i16),
    /// The X (+0xC) and Y (+0x10) angles of the camera the player's entry
    /// in 0x80175378 points at.
    pub camera: (i32, i32),
}

const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb); // r2-0x75E8
const ONE: f32 = 1.0; // r2-0x75C8
const MINUS_ONE: f32 = -1.0; // r2-0x75B0

fn fctiwz_f64(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// fn_80048490: the walker's steering. With task flag 0x4000 nothing
/// happens; with 0x2000 `flagged_turn` runs. Without the stick a grounded
/// player turns down slopes (`slope_turn`) and an airborne one aims at its
/// yaw. With the stick on a surface with `attr` bit 0x100000 the stick's
/// direction is taken from the camera onto the floor plane, and the aim is
/// the yaw plus the angle from the task's forward to it (`turn_toward`
/// first); otherwise the aim is the stick angle. Both then end in
/// `turn_smoothly`. `aim` is the motion work's +0x1C.
pub fn steer(stack: &mut MatrixStack, inp: &SteerInput, aim: &mut i32, calls: &mut dyn SteerCalls) {
    if inp.task_flag & 0x4000 != 0 {
        return;
    }
    if inp.task_flag & 0x2000 != 0 {
        calls.flagged_turn();
        return;
    }
    match calls.analog() {
        None => {
            if inp.task_flag & 3 != 0 {
                calls.slope_turn();
                return;
            }
            *aim = inp.angles[1];
        }
        Some(_) if inp.attr & 0x10_0000 != 0 => {
            let a = atan2f(f32::from(inp.stick.0), f32::from(inp.stick.1));
            let turn = fctiwz_f64(BAMS_PER_RAD * f64::from(a));
            stack.push_unit();
            stack.rotate(Axis::Y, inp.camera.1);
            stack.rotate(Axis::X, inp.camera.0);
            let view = stack.calc_vector([ZERO, ZERO, MINUS_ONE]);
            stack.pop();
            stack.push_unit();
            rotate_axis(stack.current_mut(), inp.normal, turn);
            let dir = stack.calc_vector(view);
            stack.pop();
            let fwd = rotate_by_angles(stack, inp.angles, [ONE, ZERO, ZERO]);
            let d = vector_angle(fwd, dir, Some(inp.normal));
            *aim = (inp.angles[1].wrapping_add(d)) & 0xFFFF;
            calls.turn_toward(aim);
        }
        Some(ang) => *aim = ang,
    }
    calls.turn_smoothly(aim);
}

/// One 0x20-byte entry of the player's jump-target list (player +0xB4,
/// count at +0x1E).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JumpTarget {
    pub pos: Vec3,
    /// +0xC: the reach (horizontal distance limit).
    pub reach: f32,
    /// +0x10: the rank; the lowest wins.
    pub rank: f32,
    /// +0x14, +0x18, +0x1C: written by `jump_assist` (horizontal distance,
    /// yaw difference, direction).
    pub dist: f32,
    pub turn: i32,
    pub dir: i32,
}

/// What fn_80036CE4 reads from the task, motion and player work besides
/// what `steer` and `accel_mode43` take.
#[derive(Debug, Clone, Copy)]
pub struct JumpWork {
    pub task_pos: Vec3,
    /// Motion work `spd` (+0) and `acc` (+0xC).
    pub mw_spd: Vec3,
    /// Player +0x110, +0x118 and +0x11C.
    pub air_resist: Vec3,
    /// Player +0xCC (fall speed limit), +0xE8, +0xFC (`run_accel`).
    pub f_cc: f32,
    pub f_e8: f32,
    pub run_accel: f32,
    /// `weight` (+0x138).
    pub weight: f32,
}

const SQRT_TINY: f32 = 0.0;
const REACH_FAR: f32 = 0.75; // r2-0x75D0
const REACH_NEAR: f32 = 0.5; // r2-0x75CC

fn hdist(dx: f32, dz: f32) -> f32 {
    let d = dz * dz + dx * dx;
    if d > SQRT_TINY {
        crate::land_select::inline_sqrt(d)
    } else {
        d
    }
}

/// fn_80036CE4: the walker's jump assist. Of the targets at or below the
/// task, ahead within 0x1800 of its yaw or within their reach, the one with
/// the lowest rank is taken if it is ahead and the stick (through `steer`,
/// whose yaw and aim changes are undone) points within 0x1800 of it. The
/// fall is then simulated for up to 0x78 frames with the motion work's
/// speed, air resistance and gravity; on the frame it drops to the target's
/// height the speed toward it is set from what is left to cover (scaled by
/// the frame count, clamped to `run_accel`) and added to the motion work's
/// acceleration x and z, the yaw turns toward it by 0x100 when within
/// 0x1000, and `accel_mode43` runs. Returns whether it did.
#[allow(clippy::too_many_arguments)]
pub fn jump_assist(
    stack: &mut MatrixStack,
    steer_in: &SteerInput,
    task_yaw: &mut i32,
    aim: &mut i32,
    mw_acc: &mut Vec3,
    w: &JumpWork,
    targets: &mut [JumpTarget],
    calls: &mut dyn SteerCalls,
    accel43: &mut dyn FnMut(&mut MatrixStack, i32, Vec3),
) -> bool {
    let p = w.task_pos;
    let mut best: Option<usize> = None;
    for k in 0..targets.len() {
        let e = targets[k];
        if p[1] < e.pos[1] {
            continue;
        }
        let a = atan2f(e.pos[2] - p[2], e.pos[0] - p[0]);
        let dir = fctiwz_f64(BAMS_PER_RAD * f64::from(a));
        let d = sub_angle(*task_yaw, dir);
        if d > 0x1800 || d < -0x1800 {
            let h = hdist(e.pos[0] - p[0], e.pos[2] - p[2]);
            if h > e.reach {
                continue;
            }
        }
        if let Some(b) = best {
            if targets[b].rank < e.rank {
                continue;
            }
        }
        targets[k].turn = d;
        targets[k].dir = dir;
        best = Some(k);
    }
    let Some(b) = best else { return false };
    let t = targets[b];
    if t.turn > 0x1800 || t.turn < -0x1800 || !(t.rank >= ZERO) {
        return false;
    }
    let mut new_aim = *aim;
    let mut inp = *steer_in;
    inp.angles[1] = *task_yaw;
    steer(stack, &inp, &mut new_aim, calls);
    if diff_angle(new_aim, t.dir) > 0x1800 {
        return false;
    }
    let mut rel = sub(p, t.pos);
    let dist = hdist(rel[0], rel[2]);
    targets[b].dist = dist;
    let mut v = w.mw_spd;
    let fall = -w.weight;
    for k in 0..0x78 {
        v[0] += fmadds(w.air_resist[0], v[0], ZERO);
        v[1] += fmadds(w.air_resist[1], v[1], fall);
        v[2] += fmadds(w.air_resist[2], v[2], ZERO);
        if -w.f_cc > v[1] {
            v[1] = -w.f_cc;
        }
        rel = add(rel, v);
        if !(rel[1] <= t.pos[1]) {
            continue;
        }
        rel[0] += t.pos[0] - p[0];
        rel[2] += t.pos[2] - p[2];
        let h = hdist(rel[0], rel[2]);
        let left = if h > dist {
            Some(fnmsubs(REACH_FAR, t.reach, dist) - h)
        } else if h > fnmsubs(REACH_FAR, t.reach, dist) {
            if fmadds(v[2], v[2], v[0] * v[0]) > w.f_e8 {
                Some((dist - t.reach) - h)
            } else {
                None
            }
        } else {
            Some(fnmsubs(REACH_NEAR, t.reach, dist) - h)
        };
        let mut s = match left {
            Some(l) => [l * nj_cos(t.dir), ZERO, l * nj_sin(t.dir)],
            None => [ZERO; 3],
        };
        let frames = ONE + k as f32;
        s[0] /= frames;
        s[2] /= frames;
        for j in [0, 2] {
            if s[j] > w.run_accel {
                s[j] = w.run_accel;
            }
            if s[j] < -w.run_accel {
                s[j] = -w.run_accel;
            }
        }
        mw_acc[0] += s[0];
        mw_acc[2] += s[2];
        if t.turn < 0x1000 && t.turn > -0x1000 {
            *task_yaw = adjust_angle(*task_yaw, t.dir, 0x100);
        }
        accel43(stack, *task_yaw, *mw_acc);
        return true;
    }
    false
}
