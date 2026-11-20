//! fn_80017C00 (GSNE8P main.dol): the enemies' ground and wall collision.
//! It probes six rays from a point above the enemy (fn_800186B0 with
//! GetShadowPosXYZ or fn_80066E1C), pushes the enemy down from a ceiling,
//! settles it on the ground or puts it back where the frame started
//! (0x800184E4), and pushes it off walls, bouncing its speed and turning it
//! (fn_80017D4C). The land probes are `land_probe::shadow_xyz`, which the
//! host runs on its land collision.
use super::{host::EnemyHost, EnemyTask, EnemyWork, V3};
use crate::{
    ninja_matrix::{mult_vec, Axis, MatrixStack, IDENTITY},
    ninja_trig::{fmadds, fnmsubs},
    ninja_vector::{inner_product, outer_product},
};

use std::cmp::Ordering::{Greater, Less};

fn k(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// The rays fn_80017D4C pushes off, with the directions its table at
/// 0x80173F78 gives them: X below, X above, Z below, Z above.
const WALLS: [(usize, V3); 4] = [
    (0, [-1.0, 0.0, 0.0]),
    (1, [1.0, 0.0, 0.0]),
    (4, [0.0, 0.0, -1.0]),
    (5, [0.0, 0.0, 1.0]),
];

/// fn_800186B0: the probe point (the position plus the work's offset
/// +0xBC turned by the task's Z, X and Y angles) and the six-ray probe.
pub fn probe(
    stack: &mut MatrixStack,
    task: &EnemyTask,
    w: &mut EnemyWork,
    host: &mut impl EnemyHost,
) {
    stack.push();
    *stack.current_mut() = IDENTITY;
    stack.rotate(Axis::Z, task.ang[2]);
    stack.rotate(Axis::X, task.ang[0]);
    stack.rotate(Axis::Y, task.ang[1]);
    let v = mult_vec(stack.current(), w.probe_offset);
    stack.pop();
    w.probe = [task.pos[0] + v[0], task.pos[1] + v[1], task.pos[2] + v[2]];
    let water = w.flags & 0x2000 != 0;
    let (shadow, hit) = host.shadow(w.probe, water);
    w.shadow = shadow;
    if water {
        if hit {
            w.flags |= 0x4000;
        } else {
            w.flags &= !0x4000;
        }
    }
}

/// 0x800184E4: the ground under the probe.
pub fn settle(task: &mut EnemyTask, w: &mut EnemyWork) {
    let foot = w.probe[1] + w.hover;
    let ground = w.shadow.coords[2];
    if w.flags & 1 != 0
        && w.flags & 0x200 == 0
        && (foot - w.f_d4 > ground || w.shadow.hits[2].is_none())
    {
        // Back to where the frame started, nudged against the speed.
        task.pos = w.last_pos;
        w.flags |= 0x280;
        let f = k(0x3ecc_cccd);
        task.pos[0] = fnmsubs(f, w.spd[0], task.pos[0]);
        task.pos[2] = fnmsubs(f, w.spd[2], task.pos[2]);
        w.spd = [0.0; 3];
        w.ground_normal = [0.0, 1.0, 0.0];
        return;
    }
    w.flags &= !0x80;
    if foot < ground {
        w.spd[1] *= -w.f_dc;
        w.spd[0] *= w.f_e0;
        w.spd[2] *= w.f_e0;
        task.pos[1] = fmadds(k(0x3e99_999a), ground - foot, task.pos[1]);
        let h = w.shadow.hits[2];
        if w.flags & 2 != 0 {
            let (ax, az) = h.map_or((0, 0), |h| (h.angle_x, h.angle_z));
            task.ang[0] = super::engine::adjust_angle(task.ang[0], ax, 0x80);
            task.ang[2] = super::engine::adjust_angle(task.ang[2], az, 0x80);
        }
        w.ground_normal = h.map_or([0.0; 3], |h| h.normal);
        w.flags |= 0x80;
        w.flags &= !0x200;
    } else {
        w.ground_normal = [0.0, 1.0, 0.0];
    }
}

/// fn_8011B6C8 without its foot point: the distance from `p` to the plane
/// through `q` with normal `n`.
fn plane_distance(p: V3, q: V3, n: V3) -> f32 {
    let c = -(n[2] * q[2] + (n[0] * q[0] + n[1] * q[1]));
    let d = c + (n[2] * p[2] + (n[0] * p[0] + n[1] * p[1]));
    let len_sq = n[2] * n[2] + (n[0] * n[0] + n[1] * n[1]);
    let inv = 1.0 / super::engine::sqrtf_dol(len_sq);
    d.abs() * inv
}

/// fn_80017D4C: walls.
pub fn walls(task: &mut EnemyTask, w: &mut EnemyWork) {
    let radius = w.f_cc;
    let p = w.probe;
    w.flags &= !0x100;
    for (ray, dir) in WALLS {
        let Some(h) = w.shadow.hits[ray] else {
            continue;
        };
        // bge: NaN skips too.
        if inner_product(dir, h.normal).partial_cmp(&-0.5) != Some(Less) {
            continue;
        }
        let coord = w.shadow.coords[ray];
        let q = if ray < 2 {
            [coord, p[1], p[2]]
        } else {
            [p[0], p[1], coord]
        };
        let gap = radius - plane_distance(p, q, h.normal);
        if gap.partial_cmp(&0.0) != Some(Greater) {
            continue;
        }
        let s = k(0x3eb3_3333) * gap;
        let push = [h.normal[0] * s, 0.0 * s, h.normal[2] * s];
        task.pos = [
            task.pos[0] + push[0],
            task.pos[1] + push[1],
            task.pos[2] + push[2],
        ];
        w.flags |= 0x100;
        if inner_product(w.spd, h.normal).partial_cmp(&0.0) != Some(Less) {
            continue;
        }
        let (e, n) = super::engine::unit_vector_len(h.normal);
        let n = if e == 0.0 { [1.0, 0.0, 0.0] } else { n };
        let f = w.f_d8 * (-2.0 * inner_product(w.spd, n));
        let n = [n[0] * f, n[1] * f, n[2] * f];
        w.spd = [w.spd[0] + n[0], w.spd[1] + n[1], w.spd[2] + n[2]];
        if outer_product(n, w.spd)[1] > 0.0 {
            task.ang[1] = task.ang[1].wrapping_sub(w.i_e8);
        } else {
            task.ang[1] = task.ang[1].wrapping_add(w.i_e8);
        }
    }
}

/// fn_80017C00.
pub fn collide(
    stack: &mut MatrixStack,
    task: &mut EnemyTask,
    w: &mut EnemyWork,
    host: &mut impl EnemyHost,
) {
    probe(stack, task, w, host);
    w.flags &= !0x40;
    let top = w.probe[1] + w.f_c8;
    if top > w.shadow.coords[3] {
        w.spd[1] = 0.0;
        task.pos[1] = fnmsubs(k(0x3e99_999a), top - w.shadow.coords[3], task.pos[1]);
        w.flags |= 0x40;
    }
    settle(task, w);
    walls(task, w);
}
