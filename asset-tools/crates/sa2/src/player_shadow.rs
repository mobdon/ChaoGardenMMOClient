//! The player's floor and ceiling probe (the shadow block at player work
//! +0x1A0), ported from SA2B's GameCube main.dol (GSNE8P) and checked
//! against it by tests/player_shadow.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_nodes::{nj_scalor, Entry, LandNodes, NjModel, NjObject},
    land_probe::vertical_probe,
    ninja_matrix::{Axis, MatrixStack, Vec3},
    ninja_trig::fmadds,
};

/// The block at player work +0x1A0 that fn_8006779C fills.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ShadowBlock {
    /// +0, +4: the floor's angles (x, z).
    pub angle_x: i32,
    pub angle_z: i32,
    /// +8, +0xC: the ceiling's and the floor's selection flags.
    pub ceiling_flags: u32,
    pub floor_flags: u32,
    /// +0x10, +0x14: the ceiling and floor heights.
    pub ceiling_y: f32,
    pub floor_y: f32,
    /// +0x18, +0x1C: the previous frame's +8 and +0xC.
    pub last_ceiling_flags: u32,
    pub last_floor_flags: u32,
}

/// A routine this probe needs that is not ported yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unported(pub u32);

const LIFT: f32 = 1.0; // r2-0x71A0
const NO_CEILING: f32 = 1_000_000.0; // r2-0x717C
const NO_FLOOR: f32 = -1_000_000.0; // r2-0x7180
const REACH_PAD: f32 = 30.0; // r2-0x7170
const WATER_STAGE: i32 = 0x22;

/// fn_8006779C(point, block): probe the land objects selected this frame
/// (r13-0x79F4, `list`) from one above `p` for the lowest ceiling above
/// it and the highest floor at or below it. Dynamic entries are skipped
/// when their task's `twp.flag` lacks 0x100 (`task_flag` gives it, `None`
/// without a task work) or when they are out of reach horizontally (with
/// selection flag 0x10000000 the model's center is turned by the object's
/// angles first); their hits are lifted by `task_offset`. Water entries
/// (flags 0x2002) are first probed by fn_800CE154, which is not ported:
/// meeting one fails closed with `Unported(0x800CE154)`.
#[allow(clippy::too_many_arguments)]
pub fn shadow_probe(
    stack: &mut MatrixStack,
    nodes: &LandNodes,
    objects: &[NjObject],
    models: &[NjModel],
    list: &[Entry],
    task_flag: &dyn Fn(usize) -> Option<i16>,
    task_offset: &dyn Fn(usize) -> Option<f32>,
    stage: i32,
    p: Vec3,
    out: &mut ShadowBlock,
) -> Result<(), Unported> {
    out.last_ceiling_flags = out.ceiling_flags;
    out.last_floor_flags = out.floor_flags;
    out.ceiling_flags = 0;
    out.floor_flags = 0;
    let pt = [p[0], p[1] + LIFT, p[2]];
    out.ceiling_y = NO_CEILING;
    out.floor_y = NO_FLOOR;
    for e in list {
        if e.flags & 0x2002 != 0 {
            return Err(Unported(0x800C_E154));
        }
        if let Some(task) = e.task {
            if let Some(flag) = task_flag(task) {
                if flag & 0x100 == 0 {
                    continue;
                }
            }
            let o = &objects[e.obj];
            let model = &models[o.model];
            let d = if e.flags & 0x1000_0000 != 0 {
                stack.push_unit();
                if o.evalflags & 0x20 != 0 {
                    stack.rotate(Axis::Y, o.ang[1]);
                    stack.rotate(Axis::X, o.ang[0]);
                    stack.rotate(Axis::Z, o.ang[2]);
                } else {
                    stack.rotate(Axis::Z, o.ang[2]);
                    stack.rotate(Axis::Y, o.ang[1]);
                    stack.rotate(Axis::X, o.ang[0]);
                }
                let c = stack.calc_point(model.center);
                stack.pop();
                [(o.pos[0] + c[0]) - p[0], 0.0, (o.pos[2] + c[2]) - p[2]]
            } else {
                [o.pos[0] - p[0], 0.0, o.pos[2] - p[2]]
            };
            let len = nj_scalor(d);
            let mut s = o.scl[0];
            if s < o.scl[1] {
                s = o.scl[1];
            }
            if s < o.scl[2] {
                s = o.scl[2];
            }
            if len > fmadds(s, model.radius, REACH_PAD) {
                continue;
            }
        }
        let Some(mut probe) = vertical_probe(nodes, e.obj, pt) else {
            continue;
        };
        if let Some(lift) = e.task.and_then(task_offset) {
            probe.above_y += lift;
            probe.below_y += lift;
        }
        if probe.above.is_some()
            && (out.ceiling_flags & 0x2002 == 0 || stage == WATER_STAGE)
            && pt[1] < probe.above_y
            && probe.above_y < out.ceiling_y
        {
            out.ceiling_y = probe.above_y;
            out.ceiling_flags = e.flags;
        }
        if let Some(h) = probe.below {
            if pt[1] >= probe.below_y && probe.below_y > out.floor_y {
                out.floor_y = probe.below_y;
                out.floor_flags = e.flags;
                out.angle_x = h.angle_x;
                out.angle_z = h.angle_z;
            }
        }
    }
    Ok(())
}

/// The land a probe sees: the selection fn_80023808 leaves and what the
/// selected entries point at.
pub struct ProbeLand<'a> {
    pub nodes: &'a LandNodes,
    pub objects: &'a [NjObject],
    pub models: &'a [NjModel],
    pub task_flag: &'a dyn Fn(usize) -> Option<i16>,
    pub task_offset: &'a dyn Fn(usize) -> Option<f32>,
    pub stage: i32,
}

const RESELECT_RADIUS: f32 = 200.0; // r2-0x7178
const AHEAD_PAD: f32 = 2.0; // r2-0x7174

/// fn_80067600(task work, player work): the player's two land probes.
/// Under water (ceiling flags 0x2002 at +0x1A8) the splash routines
/// fn_8005237C and fn_8005222C run first; they are not ported and fail
/// closed. `reselect` is fn_80023808 with the task's position and 200;
/// it returns the new selection list. The first probe fills `here`
/// (+0x1A0) at the position, the second fills `ahead` (+0x1C4) at the
/// position plus (2 + +0x130, 2 + +0x134, 0) turned by the task's angles,
/// with the stick's angle (fn_8004E560, `stick`) as the yaw when held.
#[allow(clippy::too_many_arguments)]
pub fn shadow_update(
    stack: &mut MatrixStack,
    reselect: &mut dyn FnMut(Vec3, f32) -> Vec<Entry>,
    land: &ProbeLand,
    angles: [i32; 3],
    pos: Vec3,
    f130: f32,
    f134: f32,
    stick: Option<i32>,
    here: &mut ShadowBlock,
    ahead: &mut ShadowBlock,
) -> Result<(), Unported> {
    if here.ceiling_flags & 0x2002 != 0 {
        return Err(Unported(0x8005_237C));
    }
    let list = reselect(pos, RESELECT_RADIUS);
    let probe = |stack: &mut MatrixStack, p: Vec3, out: &mut ShadowBlock| {
        shadow_probe(
            stack,
            land.nodes,
            land.objects,
            land.models,
            &list,
            land.task_flag,
            land.task_offset,
            land.stage,
            p,
            out,
        )
    };
    probe(stack, pos, here)?;
    let mut a = angles;
    if let Some(yaw) = stick {
        a[1] = yaw;
    }
    let off = [AHEAD_PAD + f130, AHEAD_PAD + f134, 0.0];
    let v = crate::ninja_matrix::rotate_by_angles(stack, a, off);
    let p2 = [pos[0] + v[0], pos[1] + v[1], pos[2] + v[2]];
    probe(stack, p2, ahead)
}
