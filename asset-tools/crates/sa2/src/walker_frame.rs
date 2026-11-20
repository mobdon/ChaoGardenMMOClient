//! fn_8003E668 (GSNE8P main.dol): a player's world-space velocity into its
//! own frame, stored as `playerwk.spd`, plus `dotp`.
//!
//! Ported instruction by instruction. The rotations use `ninja_matrix`
//! (njRotateX/Y/Z and njCalcVector on Ninja's matrix stack), the floor
//! tilt uses `msl_math::normal_angles` (fn_8005071C), and `njInnerProduct`
//! (0x80114FD0) is inlined below. `.sdata2` constants are read from r2 =
//! 0x803B6520 at the offsets noted.
use crate::{
    msl_math::{gekko_frsqrte, normal_angles},
    ninja_matrix::{Axis, MatrixStack, Vec3},
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

fn fmadds(a: f32, c: f32, b: f32) -> f32 {
    (a as f64).mul_add(c as f64, b as f64) as f32
}

/// `njInnerProduct` (0x80114FD0).
pub fn inner_product(a: Vec3, b: Vec3) -> f32 {
    let x = b[0] * a[0];
    let xy = fmadds(a[1], b[1], x);
    fmadds(a[2], b[2], xy)
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
