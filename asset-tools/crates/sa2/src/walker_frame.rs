//! fn_8003E668 (GSNE8P main.dol): a player's world-space velocity into its
//! own frame, stored as `playerwk.spd`, plus `dotp`.
//!
//! Ported instruction by instruction. The rotations use `ninja_matrix`
//! (njRotateX/Y/Z and njCalcVector on Ninja's matrix stack), the floor
//! tilt uses `msl_math::normal_angles` (fn_8005071C), and `njInnerProduct`
//! (0x80114FD0) is `ninja_vector::inner_product`. `.sdata2` constants are read from r2 =
//! 0x803B6520 at the offsets noted.
use crate::{
    msl_math::{gekko_frsqrte, normal_angles},
    ninja_matrix::{Axis, MatrixStack, Vec3},
    ninja_vector::inner_product,
};

const ZERO: f32 = 0.0; // r2-0x75F8
const ONE: f32 = 1.0; // r2-0x75C8
const SMALL_GROUND: f32 = f32::from_bits(0x3c23_d70a); // r2-0x75B4, 0.01
const ACCEL_SHARE: f32 = f32::from_bits(0x3f66_6666); // r2-0x7548, 0.9
const SMALL_AIR: f32 = f32::from_bits(0x3a83_126f); // r2-0x758C, 0.001
const SLOPE_FLOOR: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8, 0.1
const SLOPE_STEEP: f32 = f32::from_bits(0x3f45_1eb8); // r2-0x7544, 0.77
const SLOPE_SCALE: f32 = f32::from_bits(0x3f99_999a); // r2-0x759C, 1.2
/// Stage whose number triggers the extra slope rule (`GetStageNum() == 0x5A`).
pub const SLOPE_RULE_STAGE: i32 = 0x5a;

/// Inputs the routine reads.
#[derive(Debug, Clone, Copy)]
pub struct FrameInput {
    /// `taskwk.flag` (+4); bits 0..1 mean grounded.
    pub task_flag: i16,
    /// `taskwk` rotation words (+8, +0xC, +0x10).
    pub task_angles: [i32; 3],
    /// The world-space vector (r4 +0).
    pub world: Vec3,
    /// The three angle words after it (r4 +0x18..+0x20).
    pub vector_angles: [i32; 3],
    pub attr: u32,
    pub last_attr: u32,
    pub floor_normal: Vec3,
    pub run_accel: f32,
    pub weight: f32,
    /// The global vector at 0x801E5624 (set at run time).
    pub global: Vec3,
    pub stage: i32,
}

/// What the routine writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameOutput {
    pub spd: Vec3,
    pub dotp: f32,
    pub task_angles: [i32; 3],
    pub vector_angles: [i32; 3],
}

/// The routine's inline square root: Gekko `frsqrte`, three Newton steps in
/// double precision, multiply, `frsp`.
fn inline_sqrt(v: f32) -> f32 {
    let v = v as f64;
    let mut e = gekko_frsqrte(v);
    for _ in 0..2 {
        let sq = e * e;
        e *= 0.5;
        e *= (-v).mul_add(sq, 3.0);
    }
    let sq = e * e;
    let half = 0.5 * e;
    let r = half * (-v).mul_add(sq, 3.0);
    (v * r) as f32
}

pub fn player_speed_from_world(stack: &mut MatrixStack, i: &FrameInput) -> FrameOutput {
    let grounded = i.task_flag & 3 != 0;
    let (ry, rx, rz) = if grounded && i.attr & 0x100 != 0 {
        let (ax, az) = normal_angles(i.floor_normal[0], i.floor_normal[1], i.floor_normal[2]);
        (i.task_angles[1], ax, az)
    } else {
        (i.task_angles[1], i.task_angles[0], i.task_angles[2])
    };
    stack.push_unit();
    stack.rotate(Axis::Y, ry);
    stack.rotate(Axis::X, rx.wrapping_neg());
    stack.rotate(Axis::Z, rz.wrapping_neg());
    let mut v = stack.calc_vector(i.world);
    stack.pop();

    if grounded {
        v[1] = ZERO;
        if i.last_attr & 0xc == 0 {
            if v[0].abs() < SMALL_GROUND {
                v[0] = ZERO;
            }
            if v[2].abs() <= ACCEL_SHARE * i.run_accel {
                v[2] = ZERO;
            }
        }
    } else {
        if v[0].abs() <= SMALL_AIR {
            v[0] = ZERO;
        }
        if v[2].abs() <= SMALL_AIR {
            v[2] = ZERO;
        }
    }
    let mask = |a: [i32; 3]| a.map(|x| x & 0xffff);
    let mut dotp = if grounded {
        -inner_product(i.global, i.floor_normal)
    } else {
        ONE
    };
    if i.stage == SLOPE_RULE_STAGE {
        let mut threshold = SLOPE_FLOOR * i.weight;
        let mut flat = ONE - dotp * dotp;
        if flat > ZERO {
            flat = inline_sqrt(flat);
        }
        if v[0].abs() <= i.weight * flat && dotp > SLOPE_STEEP {
            threshold = SLOPE_SCALE * (i.weight * dotp);
        }
        if v[0].abs() < threshold {
            v[0] = ZERO;
        }
    }
    if i.attr & 0x100 != 0 {
        dotp = ONE;
    }
    FrameOutput {
        spd: v,
        dotp,
        task_angles: mask(i.task_angles),
        vector_angles: mask(i.vector_angles),
    }
}

/// fn_80046950: clamp a player-frame velocity to the PhysicsArray limits,
/// x and z to ±`lim_h_spd` (+0xC8) and y to ±`lim_v_spd` (+0xCC). Both arms
/// of its `taskwk.flag & 0x800` branch are identical. Comparisons are
/// ordered, so a NaN component is left as it is.
pub fn clamp_speed(v: Vec3, lim_h_spd: f32, lim_v_spd: f32) -> Vec3 {
    let clamp = |x: f32, lim: f32| {
        let x = if lim < x { lim } else { x };
        if -lim > x {
            -lim
        } else {
            x
        }
    };
    [
        clamp(v[0], lim_h_spd),
        clamp(v[1], lim_v_spd),
        clamp(v[2], lim_h_spd),
    ]
}

const JOG_SHARE: f32 = f32::from_bits(0x3f5e_b852); // r2-0x749C, 0.87
const SLOPE_STOP: f32 = f32::from_bits(0x3f33_3333); // r2-0x7534, 0.7
const NEG_ZERO: f32 = f32::from_bits(0x8000_0000); // r2-0x75A4
const LEDGE_HEIGHT: f32 = 20.0; // r2-0x74A0
const BRAKE_JOG: f32 = f32::from_bits(0x3f4c_cccd); // r2-0x7560, 0.8
const BRAKE_MID: f32 = 0.5; // r2-0x75CC
const REST_POS: f32 = f32::from_bits(0x3ca3_d70a); // r2-0x74A8, 0.02
const REST_NEG: f32 = f32::from_bits(0xbca3_d70a); // r2-0x74A4, -0.02

/// The player-work fields 0x80046DE4 reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct SpeedWork {
    /// `spd` (+0x64), updated in place.
    pub spd: Vec3,
    /// `acc` (+0x58).
    pub acc: Vec3,
    pub dotp: f32,
    pub attr: u32,
    pub floor_normal: Vec3,
    /// PhysicsArray `jog_speed`, `run_speed`, `run_break` (+0xE4, +0xE8, +0x108).
    pub jog_speed: f32,
    pub run_speed: f32,
    pub run_break: f32,
    pub lim_h_spd: f32,
    pub lim_v_spd: f32,
    /// Word +0x1A8 (bits 0x2000 and 0x0002 gate the brake) and float +0x1B0.
    pub flags_1a8: u32,
    pub height_1b0: f32,
}

/// The task-work fields it reads.
#[derive(Debug, Clone, Copy)]
pub struct SpeedTask {
    /// Byte +0 (mode 6 is special-cased).
    pub mode: u8,
    /// +8, +0xC, +0x10.
    pub angles: [i32; 3],
    /// Position Y (+0x18).
    pub pos_y: f32,
}

/// 0x80046DE4: add `acc` to `spd` with the native stop rules, rotate `spd`
/// back to world space into `world` (the motion work's vector), clamp it with
/// fn_80046950 and drop tiny components when there is no stick input.
/// `analog_active` is fn_8004E560(task, player, NULL, NULL) != 0.
pub fn player_world_speed(
    stack: &mut MatrixStack,
    w: &mut SpeedWork,
    task: &SpeedTask,
    world: &mut Vec3,
    mut analog_active: impl FnMut(&mut MatrixStack) -> bool,
) {
    let jog = JOG_SHARE * w.jog_speed;
    // X: stop at zero crossings and, without input on a shallow floor, below
    // 0.87 * jog speed.
    let old_x = w.spd[0];
    w.spd[0] = old_x + w.acc[0];
    if old_x > ZERO {
        if w.spd[0] < ZERO {
            w.spd[0] = ZERO;
        } else if !analog_active(stack) && w.spd[0] < jog && w.dotp > SLOPE_STOP {
            w.spd[0] = ZERO;
        }
    } else if old_x < ZERO {
        if w.spd[0] > NEG_ZERO {
            w.spd[0] = ZERO;
        } else if !analog_active(stack) && w.spd[0] > -jog && w.dotp > SLOPE_STOP {
            w.spd[0] = ZERO;
        }
    }
    w.spd[1] += w.acc[1];
    // Z: stop when it was already below 0.87 * jog and did not change, or
    // when it falls through that speed.
    let old_z = w.spd[2];
    w.spd[2] = old_z + w.acc[2];
    if old_z > ZERO {
        if old_z < jog && old_z == w.spd[2] {
            w.spd[2] = ZERO;
        } else if old_z >= jog && w.spd[2] < jog {
            w.spd[2] = ZERO;
        }
    } else if old_z < ZERO {
        if old_z > -jog && old_z == w.spd[2] {
            w.spd[2] = ZERO;
        } else if old_z <= -jog && w.spd[2] > -jog {
            w.spd[2] = ZERO;
        }
    }
    // Brake on a ledge above the player.
    let high = w.height_1b0 > LEDGE_HEIGHT + task.pos_y;
    if w.flags_1a8 & 0x2000 != 0 {
        if high && w.spd[0] >= BRAKE_JOG * w.jog_speed {
            w.spd[0] += w.run_break;
        }
    } else if w.flags_1a8 & 0x2002 != 0
        && high
        && w.spd[0] >= BRAKE_MID * (w.run_speed + w.jog_speed)
    {
        w.spd[0] += w.run_break;
    }
    // Back to world space.
    stack.push_unit();
    if w.attr & 0x100 != 0 && (task.mode != 6 || world[1] > ZERO) {
        let (ax, az) = normal_angles(w.floor_normal[0], w.floor_normal[1], w.floor_normal[2]);
        stack.rotate(Axis::Z, az);
        stack.rotate(Axis::X, ax);
    } else {
        stack.rotate(Axis::Z, task.angles[2]);
        stack.rotate(Axis::X, task.angles[0]);
    }
    stack.rotate(Axis::Y, task.angles[1].wrapping_neg());
    *world = stack.calc_vector(w.spd);
    stack.pop();
    *world = clamp_speed(*world, w.lim_h_spd, w.lim_v_spd);
    if !analog_active(stack) {
        for c in world.iter_mut() {
            if REST_POS > *c && REST_NEG < *c {
                *c = ZERO;
            }
        }
    }
}
