//! The hunter controller (Knuckles, Rouge, Tikal, Chaos) of SA2B's main.dol
//! (GameCube, GSNE8P, SHA-1 e4c50f32242a3cd36cdc8ae776661601a52130a2).
//!
//! All four characters run the exec fn_8009F8A0 and its action machine
//! fn_8009B8FC (docs/CHARACTER-PORT-MAP.md). Routines here are ported
//! instruction by instruction and checked bit for bit against main.dol by
//! `tests/hunter_dol.rs`. Shared player code is called, not copied:
//! `ninja_matrix::rotate_by_angles` (fn_8003E9E0) and
//! `enemy_lib::engine::unit_vector` (njUnitVector, 0x80115D24).
//!
//! Field names follow the offsets the code reads: `twp` is the task work
//! (task +0x34), `pwp` the 0x420-byte hunter player work (task +0x40).
use crate::{
    enemy_lib::engine::unit_vector,
    ninja_matrix::{rotate_by_angles, MatrixStack, Vec3},
    ninja_trig::fmadds,
};

const ZERO: f32 = 0.0; // r2-0x6CC4 (0x803AF85C)
const CLEARANCE: f32 = 4.0; // r2-0x6C04 (0x803AF91C)
const MIN_DROP: f32 = 7.0; // r2-0x6C00 (0x803AF920)

/// What fn_80091454 reads.
#[derive(Debug, Clone, Copy)]
pub struct DropCheck {
    /// `twp` +0: the current action.
    pub action: u8,
    /// `twp` +4 (`flag`); bits 0..1 mean grounded.
    pub flag: i16,
    /// `twp->cwp->info` +0xC: the first collision sphere's centre Y.
    pub info_center_y: f32,
    /// `pwp` +0x68: `spd.y`.
    pub spd_y: f32,
    /// `pwp` +0x1A8.
    pub flags_1a8: u32,
    /// `pwp` +0x1B0 and +0x1B4.
    pub height_1b0: f32,
    pub height_1b4: f32,
    /// Bytes [pl_num] of the per-player tables 0x803AD894 and 0x803AD898.
    pub table_894: i8,
    pub table_898: i8,
}

/// fn_80091454: true when bit 1 of `pwp`+0x1A8 is set, +0x1B0 lies above
/// the collision centre by more than 4 (or the player is airborne), and
/// +0x1B0 differs from +0x1B4 by more than 7. Table 0x803AD894 vetoes it;
/// table 0x803AD898 vetoes it in action 6 while `spd.y` is positive.
/// Called nine times from the action machine fn_8009B8FC.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // fcmpo: NaN takes the "not above" branch
pub fn drop_check(c: &DropCheck) -> bool {
    if c.table_894 != 0 {
        return false;
    }
    if c.table_898 != 0 && c.action == 6 && c.spd_y > ZERO {
        return false;
    }
    if c.flags_1a8 & 2 == 0 {
        return false;
    }
    if !(c.height_1b0 > CLEARANCE + c.info_center_y) && c.flag & 3 != 0 {
        return false;
    }
    (c.height_1b0 - c.height_1b4).abs() > MIN_DROP
}

/// The fields fn_80093270 reads and writes in `pwp`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimbRequest {
    /// `pwp` +0x178: the action now in motion.
    pub motion_action: i16,
    /// `pwp` +0x64 and +0x6C: `spd.x`, `spd.z`.
    pub spd_x: f32,
    pub spd_z: f32,
    /// `pwp` +0x176: the requested next action (written).
    pub request: i16,
    /// `pwp` +0x44 (written on the 0x65/0x66 paths).
    pub field_44: f32,
}

/// fn_80093270: unless +0x178 is 0x62, 0x68 or 0x6A, request action 0x67
/// when airborne or not moving in X and Z, 0x64 when |x| > |z|, otherwise
/// 0x66 (z > 0) or 0x65, storing |z| at +0x44 for those two.
pub fn climb_request(flag: i16, w: &mut ClimbRequest) {
    if matches!(w.motion_action, 0x6a | 0x68 | 0x62) {
        return;
    }
    if flag & 1 == 0 || (w.spd_x == ZERO && w.spd_z == ZERO) {
        w.request = 0x67;
        return;
    }
    let z = w.spd_z;
    if w.spd_x.abs() > z.abs() {
        w.request = 0x64;
    } else if z > ZERO {
        w.field_44 = z;
        w.request = 0x66;
    } else {
        w.field_44 = -z;
        w.request = 0x65;
    }
}

/// fn_8009B708 (called once by the exec): the first collision sphere's
/// centre, `twp->cwp->info` +8. It is (0, `center_height` (`pwp`+0x140), 0)
/// rotated into the task's frame (fn_8003E9E0), pushed along that direction
/// by `pwp`+0x3B4 in actions 0x3D, 0x3E, 0x41 and 0x42, plus `twp.pos`.
pub fn collision_center(
    stack: &mut MatrixStack,
    action: u8,
    angles: [i32; 3],
    pos: Vec3,
    center_height: f32,
    push_3b4: f32,
) -> Vec3 {
    let v = rotate_by_angles(stack, angles, [ZERO, center_height, ZERO]);
    let mut c = v;
    if matches!(action, 0x3d | 0x3e | 0x41 | 0x42) {
        // shortcut: enemy_lib's unit_vector keeps x86's negative NaN for
        // inf * 0 (a zero vector); PowerPC gives its positive default NaN.
        // Drop this once the shared njUnitVector does it.
        let u = unit_vector(v).map(|x| {
            if x.is_nan() {
                f32::from_bits(0x7fc0_0000)
            } else {
                x
            }
        });
        for k in 0..3 {
            c[k] = fmadds(push_3b4, u[k], c[k]);
        }
    }
    for k in 0..3 {
        c[k] += pos[k];
    }
    c
}
