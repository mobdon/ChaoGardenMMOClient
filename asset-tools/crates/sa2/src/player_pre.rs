//! fn_80056450 (GSNE8P main.dol), the step five character execs (the
//! walkers' 0x800B3A8C among them) run before their mode switch: the
//! gravity-direction attach to objects and walls, snapping onto the floor
//! the probe found, and the reset to standing. Checked against main.dol by
//! tests/player_pre.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_contact::unit_vector,
    ninja_matrix::{rotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::nj_cos,
    ninja_vector::inner_product,
    player_shadow::ShadowBlock,
    walker_collide::{diff_angle, line_distance, Line},
};

/// Another task a collision query found: its id, its task work's position
/// and Y and Z angles, and the attribute word (+4) of its first collision
/// info entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OtherTask {
    pub id: usize,
    pub pos: Vec3,
    pub yaw: i32,
    pub angle_z: i32,
    pub info_attr: u32,
}

/// The engine's collision-list queries on this task (the CCL module).
pub trait PreCcl {
    /// fn_800060EC(task, kind): the task met through an info entry of
    /// `kind`.
    fn kind_hit(&mut self, kind: u8) -> Option<OtherTask>;
    /// fn_800063F4(task) and fn_80006270(task): whether they find a hit.
    fn hit_63f4(&mut self) -> bool;
    fn hit_6270(&mut self) -> bool;
    /// fn_80005C74(task, 0): the task of the first hit.
    fn first_hit(&mut self) -> Option<OtherTask>;
}

/// The game state fields read (0x801CC168 +0x1E, +0x39, +0x42, +0x49 and
/// the per-player byte at +0x3F).
#[derive(Debug, Clone, Copy, Default)]
pub struct PreGame {
    pub b1e: i8,
    pub b39: i8,
    pub b42: u8,
    pub b49: i8,
    pub b3f: i8,
    /// r13-0x7B10 (fn_80005500): the gravity direction 0..5.
    pub gravity_mode: i32,
    /// r13-0x79A8 / r13-0x79AC: the X and Z angles to level to.
    pub level: (i32, i32),
    pub gravity: Vec3,
}

/// What fn_80056450 reads and writes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PreWork {
    /// Task work mode (+0), byte +1, flag (+4), halfword +6, angles,
    /// position, and its first collision info entry's attribute word.
    pub mode: i8,
    pub task_b1: u8,
    pub flag: i16,
    pub task_h6: i16,
    pub angles: [i32; 3],
    pub pos: Vec3,
    pub info_attr: u32,
    /// Motion work speed (+0) and angles +0x18, +0x1C (aim), +0x20.
    pub mw_spd: Vec3,
    pub mw_ang: [i32; 3],
    /// Player bytes +0 (player number), +1 (character), +0xC; words +0x28,
    /// +0x88 (`attr`), +0x9C, +0xA0, the task at +0xA4; halfwords +0x10,
    /// +0x18, +0x176; floats +0x64.. (`spd`) and +0x138.
    pub pnum: i8,
    pub character: i8,
    pub p_bc: u8,
    pub p_w28: i32,
    pub attr: u32,
    pub p_w9c: u32,
    pub p_wa0: u32,
    pub ride: Option<usize>,
    pub p_h10: i16,
    pub p_h18: i16,
    pub action: i16,
    pub spd: Vec3,
    pub f138: f32,
    /// The floor block at +0x1A0.
    pub here: ShadowBlock,
    /// Set when `playerpwp[pnum]` +0x180 gets bit 4.
    pub pwp_h180_4: bool,
}

/// The steps fn_80056450 hands over to.
pub trait PreSteps {
    /// fn_8003D7D4(motion work).
    fn settle(&mut self, w: &mut PreWork);
    /// fn_80054D04(task, motion, player).
    fn stand(&mut self, w: &mut PreWork);
    /// fn_8003D8D0(motion work).
    fn motion(&mut self, w: &mut PreWork);
}

const MOVE_EPS: f32 = f32::from_bits(0x3727_c5ac); // r2-0x7320
const MOVE_EPS_NEG: f32 = f32::from_bits(0xb727_c5ac); // r2-0x7324
const POLE_NEAR: f32 = 5.0; // r2-0x73DC
const POLE_PULL: f32 = 0.5; // r2-0x7398
const DOWN: f32 = f32::from_bits(0xbf7f_ff58); // r2-0x7354

fn mode_angles(mode: i32) -> Option<(i32, i32)> {
    Some(match mode {
        0 => (0, 0x4000),
        1 => (0, 0xC000),
        2 => (0, 0x8000),
        3 => (0, 0),
        4 => (0xC000, 0),
        5 => (0x4000, 0),
        _ => return None,
    })
}

/// fn_80056450(task, task work, motion work, player work).
pub fn pre_switch(
    stack: &mut MatrixStack,
    w: &mut PreWork,
    g: &PreGame,
    ccl: &mut dyn PreCcl,
    steps: &mut dyn PreSteps,
) {
    w.p_bc = 0;
    w.p_w9c = 0;
    let s = w.mw_spd;
    let moving_on = match g.gravity_mode {
        0 => s[0] >= MOVE_EPS_NEG,
        1 => s[0] <= MOVE_EPS,
        2 => s[1] >= MOVE_EPS_NEG,
        3 => s[1] <= MOVE_EPS,
        4 => s[2] >= MOVE_EPS_NEG,
        5 => s[2] <= MOVE_EPS,
        _ => false,
    };
    let pole = if g.b42 == 1 || g.b39 == 1 || w.mode == 0x36 {
        None
    } else {
        ccl.kind_hit(0x11)
    };
    let attach = |w: &mut PreWork| {
        if let Some((x, z)) = mode_angles(g.gravity_mode) {
            w.angles[0] = x;
            w.mw_ang[0] = x;
            w.angles[2] = z;
            w.mw_ang[2] = z;
        }
        w.flag |= 2;
    };
    if let Some(o) = pole {
        stack.push_unit();
        stack.rotate(Axis::Y, o.yaw);
        stack.rotate(Axis::Z, o.angle_z);
        let up = stack.calc_vector([0.0, 1.0, 0.0]);
        stack.pop();
        let line = Line {
            point: o.pos,
            dir: up,
        };
        let mut foot = [0.0; 3];
        line_distance(w.pos, &line, Some(&mut foot));
        let off = [foot[0] - w.pos[0], foot[1] - w.pos[1], foot[2] - w.pos[2]];
        if crate::land_nodes::nj_scalor(off) < POLE_NEAR {
            if g.b42 != 1 && w.flag & 0x20 != 0 && moving_on {
                attach(w);
            }
        } else if inner_product(unit_vector(off), g.gravity) > POLE_PULL {
            w.flag |= 2;
            let fwd = rotate_by_angles(stack, w.angles, [1.0, 0.0, 0.0]);
            w.angles[0] = g.level.0;
            w.mw_ang[0] = g.level.0;
            w.angles[2] = g.level.1;
            w.mw_ang[2] = g.level.1;
            let fwd2 = rotate_by_angles(stack, w.angles, [1.0, 0.0, 0.0]);
            if inner_product(fwd, fwd2) < 0.0 {
                w.mw_ang[1] = w.mw_ang[1].wrapping_add(0x8000);
                w.angles[1] = w.angles[1].wrapping_add(0x8000);
            }
        }
    } else {
        let mut keep = g.b42 != 1 && w.flag & 0x20 != 0 && moving_on;
        if keep && ccl.hit_63f4() && !ccl.hit_6270() {
            keep = false;
        }
        if keep && ccl.kind_hit(0xB).is_some() {
            keep = false;
        }
        if keep {
            let other = ccl.first_hit();
            w.ride = other.map(|o| o.id);
            let theirs = other.map_or(0, |o| o.info_attr);
            if w.info_attr & 0x4000 != 0 && theirs & 0x4000 != 0 {
                w.flag &= !2;
                w.ride = None;
            } else {
                attach(w);
            }
        } else {
            w.flag &= !2;
            w.ride = None;
        }
    }
    let c = w.character;
    if w.flag & 3 == 0
        && w.mode != 0x35
        && w.mode != 0x12
        && !((c == 4 || c == 5) && w.mode == 0x45)
        && g.b3f != 0
        && g.gravity[1] < DOWN
        && w.here.floor_y >= w.pos[1] - w.f138
        && w.here.floor_flags & 0x10_2102 == 0
    {
        let gneg = g.gravity.map(|v| -v);
        let up = rotate_by_angles(stack, w.angles, [0.0, 1.0, 0.0]);
        stack.push_unit();
        stack.rotate(Axis::Z, w.here.angle_z);
        stack.rotate(Axis::X, w.here.angle_x);
        let floor_up = stack.calc_vector([0.0, 1.0, 0.0]);
        stack.pop();
        if inner_product(up, gneg) >= nj_cos(0x2000)
            && inner_product(up, floor_up) >= nj_cos(0x2000)
        {
            if g.b39 == 0 {
                w.pos[1] = w.here.floor_y;
            }
            w.attr = w.here.floor_flags;
            let h = &mut w.here;
            if diff_angle(w.angles[0], h.angle_x) > 0x4000
                && diff_angle(w.angles[2], h.angle_z) < 0x4000
            {
                h.angle_x = h.angle_x.wrapping_add(0x8000);
            } else if diff_angle(w.angles[2], h.angle_z) > 0x4000
                && diff_angle(w.angles[0], h.angle_x) < 0x4000
            {
                h.angle_z = h.angle_z.wrapping_add(0x8000);
            }
            w.angles[0] = h.angle_x;
            w.mw_ang[0] = h.angle_x;
            w.angles[2] = h.angle_z;
            w.mw_ang[2] = h.angle_z;
            w.flag |= 1;
        }
    }
    if w.task_b1 == 0x24 {
        w.flag &= !0x3C;
        w.flag &= !0xF00;
        w.flag &= !0xE000u16 as i16;
        w.p_h10 &= 0x3FFC;
        w.p_wa0 = 0;
        steps.settle(w);
        w.pwp_h180_4 = true;
        if !(w.mode == 0x36 && g.b49 & (1 << w.pnum) != 0) {
            w.mode = 0;
            w.p_h18 = 0;
            steps.stand(w);
        }
        steps.motion(w);
    }
    if (w.mode == 0x39 || w.mode == 0x37)
        && w.task_b1 == 9
        && (w.p_w28 == 0x36 || (w.p_w28 == 0x55 && g.b1e != 0))
    {
        w.mode = 0x12;
        w.action = w.p_w28 as i16;
        w.spd = [0.0; 3];
        w.mw_spd = [0.0; 3];
        w.flag &= !0x2500;
        w.task_h6 = 0;
    }
}
