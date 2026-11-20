//! fn_8002334C (GSNE8P main.dol): pick the land-table entries near the
//! players for this frame's collision.
//!
//! The routine first walks the dynamic land objects that movers register
//! (r13-0x79E0, count r13-0x79E4; `select_frame`), then, when
//! r13-0x7F28 == 1, it walks the current LandTable (r13-0x79F0). Each 0x20-byte
//! COL entry has center +0, radius +0xC, object +0x10 and flags +0x1C.
//! Entries with flags & 0x2003 are tested against each player position:
//! horizontal-only when flags & 0x2002, otherwise in 3D. The square root is
//! the routine's inline Gekko `frsqrte` and three double-precision Newton
//! steps. An entry is taken when `distance - range < radius`. `range` is
//! r13-0x7F30 for the first position and 400.0 (r2-0x7AC4) after a miss.
//! Selection stops at 0x80 static entries or 0x400 in total.
//!
//! The picks also go to two subsets, dynamic (r13-0x7A00, count r13-0x7A08)
//! and static (r13-0x79FC, count r13-0x7A04). Every collision query
//! re-filters those against its own sphere (fn_80023808, `reselect`) and
//! rewrites the list with the result.
use crate::{
    land_nodes::{Entry, NjModel, NjObject},
    msl_math::gekko_frsqrte,
    ninja_matrix::{Axis, MatrixStack, Vec3},
    ninja_trig::fmadds,
};

/// r2-0x7AC4: the range used after the first position misses.
pub const RETRY_RANGE: f32 = 400.0;
pub const STATIC_LIMIT: u32 = 0x80;
pub const TOTAL_LIMIT: i16 = 0x400;

/// The fields of a LandTable COL entry the selection reads.
#[derive(Debug, Clone, Copy)]
pub struct LandCol {
    pub center: Vec3,
    pub radius: f32,
    pub flags: u32,
}

/// The LandTable header fields: entry count (+0) and first entry (+2; a
/// negative value means all entries).
#[derive(Debug, Clone, Copy)]
pub struct LandTableHead {
    pub count: i16,
    pub start: i16,
}

fn inline_sqrt(v: f32) -> f32 {
    let v = v as f64;
    let mut e = gekko_frsqrte(v);
    for _ in 0..2 {
        let sq = e * e;
        e *= 0.5;
        e *= (-v).mul_add(sq, 3.0);
    }
    let sq = e * e;
    let r = (0.5 * e) * (-v).mul_add(sq, 3.0);
    (v * r) as f32
}

/// Distance from a COL entry's center, horizontal only when flags & 0x2002.
fn col_distance(e: &LandCol, p: Vec3) -> f32 {
    let dy = e.center[1] - p[1];
    let dz = e.center[2] - p[2];
    let dx = e.center[0] - p[0];
    let mut y2 = dy * dy;
    let z2 = dz * dz;
    let x2 = dx * dx;
    if e.flags & 0x2002 != 0 {
        y2 = 0.0;
    }
    let d = z2 + (x2 + y2);
    if d > 0.0 {
        inline_sqrt(d)
    } else {
        d
    }
}

/// The selection state the routine keeps in r13 globals.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counters {
    /// r13-0x7A08: dynamic entries taken this frame.
    pub dynamics: u32,
    /// r13-0x7A04: static entries taken this frame.
    pub statics: u32,
    /// r13-0x79A4: all entries in the output list.
    pub total: i16,
}

/// Indices (into `cols`) of the static entries taken, in output order.
pub fn select_static(
    head: LandTableHead,
    cols: &[LandCol],
    positions: &[Vec3],
    range: f32,
    counters: &mut Counters,
) -> Vec<usize> {
    let mut out = vec![];
    let (first, mut left) = if head.start >= 0 {
        (head.start as usize, head.count as i32 - head.start as i32)
    } else {
        (0, head.count as i32)
    };
    let mut i = first;
    while left > 0 {
        let e = &cols[i];
        if e.flags & 0x2003 != 0 {
            let mut reach = range;
            for p in positions {
                if col_distance(e, *p) - reach < e.radius {
                    out.push(i);
                    counters.statics += 1;
                    counters.total += 1;
                    if counters.statics >= STATIC_LIMIT || counters.total >= TOTAL_LIMIT {
                        return out;
                    }
                    break;
                }
                reach = RETRY_RANGE;
            }
        }
        i += 1;
        left -= 1;
    }
    out
}

pub const DYNAMIC_LIMIT: u32 = 0x80;
/// r2-0x7AB0: the margin `reselect` adds to a dynamic object's reach.
const DYNAMIC_MARGIN: f32 = 30.0;

/// A registered dynamic land object (12 bytes at r13-0x79E0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynEntry {
    pub flags: u32,
    pub obj: usize,
    /// +8: the registering task.
    pub task: usize,
    /// The task checks fn_8002334C makes: neither the task's nor its
    /// parent's executor is DestroyTask (0x80071AF0), and its `twp.flag` has 0x100.
    pub active: bool,
}

/// What fn_8002334C leaves for the frame.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// r13-0x79F4 (count r13-0x79A4).
    pub list: Vec<Entry>,
    /// r13-0x7A00 (count r13-0x7A08).
    pub dynamic: Vec<DynEntry>,
    /// r13-0x79FC (count r13-0x7A04): indices into the land table.
    pub statics: Vec<usize>,
}

/// The positions fn_8002334C tests: player 1 and player 2 (`playertwp`),
/// or the default position at *0x80175378 when neither exists.
pub fn player_positions(p1: Option<Vec3>, p2: Option<Vec3>, default: Vec3) -> Vec<Vec3> {
    match (p1, p2) {
        (Some(a), Some(b)) => vec![a, b],
        (Some(a), None) | (None, Some(a)) => vec![a],
        (None, None) => vec![default],
    }
}

/// A dynamic object's world center: position plus model center, the model
/// center rotated first (njRotate, Z-Y-X, or Y-X-Z with NJD_EVAL_ZXY_ANG)
/// when the entry has flag 0x10000000.
fn dynamic_center(stack: &mut MatrixStack, flags: u32, o: &NjObject, m: &NjModel) -> Vec3 {
    let c = if flags & 0x1000_0000 == 0 {
        m.center
    } else {
        *stack.current_mut() = crate::ninja_matrix::IDENTITY;
        if o.evalflags & 0x20 != 0 {
            stack.rotate(Axis::Y, o.ang[1]);
            stack.rotate(Axis::X, o.ang[0]);
            stack.rotate(Axis::Z, o.ang[2]);
        } else {
            stack.rotate(Axis::Z, o.ang[2]);
            stack.rotate(Axis::Y, o.ang[1]);
            stack.rotate(Axis::X, o.ang[0]);
        }
        stack.calc_point(m.center)
    };
    [o.pos[0] + c[0], o.pos[1] + c[1], o.pos[2] + c[2]]
}

/// fn_8002334C in full: dynamic objects from the end of the register
/// backwards, then the land table (`select_static`) when `head` is given
/// (r13-0x7F28 == 1 and a LandTable is set).
#[allow(clippy::too_many_arguments)]
pub fn select_frame(
    register: &[DynEntry],
    objects: &[NjObject],
    models: &[NjModel],
    head: Option<LandTableHead>,
    cols: &[LandCol],
    col_objs: &[usize],
    positions: &[Vec3],
    range: f32,
) -> Selection {
    let mut sel = Selection::default();
    let mut counters = Counters::default();
    let mut stack = MatrixStack::new();
    stack.push();
    for e in register.iter().rev() {
        if !e.active {
            continue;
        }
        let o = &objects[e.obj];
        let m = &models[o.model];
        let c = dynamic_center(&mut stack, e.flags, o, m);
        let mut reach = range;
        let mut taken = false;
        for p in positions {
            let dy = p[1] - c[1];
            let dx = p[0] - c[0];
            let dz = p[2] - c[2];
            let lim = m.radius + reach;
            let d = fmadds(dz, dz, fmadds(dx, dx, dy * dy));
            if d < lim * lim {
                taken = true;
                break;
            }
            reach = RETRY_RANGE;
        }
        if taken {
            sel.dynamic.push(*e);
            sel.list.push(Entry {
                flags: e.flags,
                obj: e.obj,
                task: Some(e.task),
            });
            counters.dynamics += 1;
            counters.total += 1;
            if counters.dynamics >= DYNAMIC_LIMIT {
                break;
            }
            if counters.total >= TOTAL_LIMIT {
                return sel;
            }
        }
    }
    if let Some(head) = head {
        for i in select_static(head, cols, positions, range, &mut counters) {
            sel.statics.push(i);
            sel.list.push(Entry {
                flags: cols[i].flags,
                obj: col_objs[i],
                task: None,
            });
        }
    }
    sel
}

impl Selection {
    /// fn_80023808: rewrite `list` with the dynamic and static picks whose
    /// bounds reach a sphere at `center` with `radius` (each collision
    /// query does this before searching the nodes).
    pub fn reselect(
        &mut self,
        objects: &[NjObject],
        models: &[NjModel],
        cols: &[LandCol],
        col_objs: &[usize],
        center: Vec3,
        radius: f32,
    ) {
        self.list.clear();
        let mut stack = MatrixStack::new();
        stack.push();
        for e in self.dynamic.iter().rev() {
            let o = &objects[e.obj];
            let m = &models[o.model];
            let (dx, dy, dz) = if e.flags & 0x2002 != 0 {
                let x = center[0] - (o.pos[0] + m.center[0]);
                let z = center[2] - (o.pos[2] + m.center[2]);
                (x, 0.0, z)
            } else {
                let c = dynamic_center(&mut stack, e.flags, o, m);
                (center[0] - c[0], center[1] - c[1], center[2] - c[2])
            };
            let d = fmadds(dz, dz, fmadds(dx, dx, dy * dy));
            let lim = if o.evalflags & 4 != 0 {
                DYNAMIC_MARGIN + (m.radius + radius)
            } else {
                let mut s = o.scl[0];
                if s > o.scl[1] {
                    s = o.scl[1];
                }
                if s > o.scl[2] {
                    s = o.scl[2];
                }
                DYNAMIC_MARGIN + fmadds(m.radius, s, radius)
            };
            if d < lim * lim {
                self.list.push(Entry {
                    flags: e.flags,
                    obj: e.obj,
                    task: Some(e.task),
                });
                if self.list.len() >= TOTAL_LIMIT as usize {
                    return;
                }
            }
        }
        for &i in &self.statics {
            let e = &cols[i];
            if e.flags & 0x2003 == 0 {
                continue;
            }
            if col_distance(e, center) - radius < e.radius {
                self.list.push(Entry {
                    flags: e.flags,
                    obj: col_objs[i],
                    task: None,
                });
                if self.list.len() >= TOTAL_LIMIT as usize {
                    return;
                }
            }
        }
    }
}
