//! fn_800CEE08 (GSNE8P main.dol): test a moving sphere against one land
//! object's candidate triangles from the last broadphase (fn_800D1020).
//!
//! The query is the same struct the broadphase reads (radius +0, center +4,
//! motion +0x10). For the first active node of the object, each candidate
//! triangle is moved so its first vertex is the origin and rotated so its
//! normal is +Y (table rotations X then Y; the normal is the cross product
//! of the edges, made unit with njUnitVector's bare `frsqrte`). The sphere
//! center then walks along the motion in steps of half the radius, ending
//! exactly at the motion's end. At each point within `radius` of the
//! triangle's plane the routine tests either whether the point lies inside
//! the triangle seen from above (mode 0) or whether it is closer to some
//! vertex than the triangle's own centroid radius (any other mode). A hit
//! records, in the triangle's frame turned back to world space with
//! PSMTXInverse: the foot point below the center (+0x74), the point
//! `radius` above it (+0x80, the radius negated for triangles with
//! attribute 0x00800000 approached from below), the horizontal (+0x8C) and
//! vertical (+0x98) parts of the motion, the normal, its tilt angles
//! (fn_8005071C) and the start and end heights over the plane. At most 0x80
//! hits are kept (count at r13-0x7754). The query then gets the hit count
//! (+0x28), the hit ending lowest (+0x2A) and its angles, normal and motion
//! parts (+0x34.., +0x6C..), and the foot and head points averaged over all
//! hits (+0x54..).
//!
//! The triangle drawing for the debug byte at 0x801CC16B is left out.
use crate::{
    land_nodes::{nj_scalor, rotate_pair, rotate_y, Candidate, LandNodes, Query},
    msl_math::{atan2f, gekko_fres, gekko_frsqrte, normal_angles},
    ninja_matrix::{mult_vec, Mtx, Vec3, IDENTITY},
    ninja_trig::fmadds,
    ninja_vector::acosf,
};

const ZERO: f32 = 0.0; // r2-0x60B0
const ONE: f32 = 1.0; // r2-0x60A8
const STEP: f32 = 0.5; // r2-0x6088
const THIRD: f32 = f32::from_bits(0x3eaa_aa3b); // r2-0x6074
const FLAT: f32 = f32::from_bits(0x3b03_126f); // r2-0x6080, 0.002
const UP: f32 = f32::from_bits(0x3f7f_fd61); // r2-0x6084, 0.99996
const DOWN: f32 = f32::from_bits(0xbf7f_fd61); // r2-0x607C
const NEG_ONE: f32 = -1.0; // r2-0x6078
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb); // r2-0x60A0
/// Triangle attribute whose head point flips when approached from below.
pub const ATTR_TWO_SIDED: u32 = 0x0080_0000;
pub const HIT_LIMIT: usize = 0x80;

/// The counters the routine bumps (r13-0x77A8, -0x77AC, -0x77B0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContactStats {
    pub tested: u32,
    pub swept_to_end: u32,
    pub hits: u32,
}

/// One hit record (0x9C bytes at 0x8028FC98; the triangle copy at +4 and
/// these fields from +0x44).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactHit {
    pub candidate: Candidate,
    /// +0x44/+0x48: fn_8005071C's angles of `facing`.
    pub angles: (i32, i32),
    /// +0x4C: the triangle's unit normal (world space).
    pub normal: Vec3,
    /// +0x58: the normal, negated for a two-sided triangle hit from below.
    pub facing: Vec3,
    /// +0x64: the foot point (world space).
    pub foot: Vec3,
    /// +0x70: the head point (world space).
    pub head: Vec3,
    /// +0x7C: the motion's part along the plane (world space).
    pub along: Vec3,
    /// +0x88: the motion's part along the normal (world space).
    pub across: Vec3,
    /// +0x94: the center's height over the plane.
    pub start_height: f32,
    /// +0x98: the height after the whole motion.
    pub end_height: f32,
}

/// What the query receives.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub hits: Vec<ContactHit>,
    /// +0x2A: the hit with the lowest `end_height` (first on ties).
    pub best: usize,
    /// +0x54: `foot` averaged over the hits.
    pub foot: Vec3,
    /// +0x60: `head` averaged over the hits.
    pub head: Vec3,
}

/// njUnitVector (0x80115D24): scale by Gekko's `frsqrte` estimate alone.
pub(crate) fn unit_vector(v: Vec3) -> Vec3 {
    let d = fmadds(v[2], v[2], fmadds(v[1], v[1], v[0] * v[0]));
    let e = gekko_frsqrte(d as f64);
    v.map(|c| (e * c as f64) as f32)
}

fn fmsubs(a: f32, c: f32, b: f32) -> f32 {
    fmadds(a, c, -b)
}

fn fnmadds(a: f32, c: f32, b: f32) -> f32 {
    -fmadds(a, c, b)
}

/// PSMTXInverse (0x800ED484) in place, as njInvertMatrix(NULL) calls it.
/// A zero determinant leaves the matrix alone. The reciprocal is Gekko's
/// `fres` estimate and one Newton step.
pub(crate) fn invert(m: &mut Mtx) {
    let [[m00, m01, m02, m03], [m10, m11, m12, m13], [m20, m21, m22, m23]] = *m;
    let a0 = fmsubs(m11, m22, m21 * m12);
    let a1 = fmsubs(m12, m20, m22 * m10);
    let b0 = fmsubs(m21, m02, m01 * m22);
    let b1 = fmsubs(m22, m00, m02 * m20);
    let c0 = fmsubs(m01, m12, m11 * m02);
    let c1 = fmsubs(m02, m10, m12 * m00);
    let d0 = fmsubs(m10, m21, m11 * m20);
    let e0 = fmsubs(m01, m20, m00 * m21);
    let f0 = fmsubs(m00, m11, m01 * m10);
    let det = fmadds(m20, c0, fmadds(m10, b0, m00 * a0));
    if det == 0.0 {
        return;
    }
    let r = gekko_fres(det as f64) as f32;
    let inv = -fmsubs(det, r * r, r + r);
    let row = |a: f32, b: f32, c: f32| {
        let (a, b, c) = (a * inv, b * inv, c * inv);
        [a, b, c, fnmadds(c, m23, fmadds(b, m13, a * m03))]
    };
    *m = [row(a0, b0, c0), row(a1, b1, c1), row(d0, e0, f0)];
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The rotation that turns a triangle normal to +Y: (X angle, Y angle).
fn level_angles(n: Vec3) -> (i32, i32) {
    let small = |v: Vec3| v[2].abs() < FLAT && v[0].abs() < FLAT;
    if n[1] > UP || (small(n) && n[1] > ZERO) {
        return (0, 0);
    }
    if n[1] < DOWN || (small(n) && n[1] < ZERO) {
        return (-0x8000, -0x8000);
    }
    let rx = (BAMS_PER_RAD * acosf(n[1]) as f64) as i32;
    let ry = (BAMS_PER_RAD * atan2f(n[0], n[2]) as f64) as i32;
    (rx.wrapping_neg(), ry.wrapping_neg())
}

/// The two inside tests. `e1`, `e2` are the rotated edges (vertex 0 is the
/// origin), `p` the walking center.
// The negated comparisons keep the branches' NaN behaviour.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn inside(vertex_mode: bool, e1: Vec3, e2: Vec3, p: Vec3) -> bool {
    let o = [ZERO; 3];
    if vertex_mode {
        let c = [0, 1, 2].map(|j| THIRD * (e2[j] + (o[j] + e1[j])));
        let sq = |v: Vec3| {
            let d = sub(v, c);
            fmadds(d[2], d[2], fmadds(d[1], d[1], d[0] * d[0]))
        };
        let mut r2 = sq(o);
        let d1 = sq(e1);
        if d1 > r2 {
            r2 = d1;
        }
        let d2 = sq(e2);
        if d2 > r2 {
            r2 = d2;
        }
        return [o, e1, e2].iter().any(|v| {
            let dx = v[0] - p[0];
            let mut d = dx * dx;
            if d >= r2 {
                return false;
            }
            let dz = v[2] - p[2];
            d = fmadds(dz, dz, d);
            if d >= r2 {
                return false;
            }
            let dy = v[1] - p[1];
            d = fmadds(dy, dy, d);
            d < r2 || d.is_nan() || r2.is_nan()
        });
    }
    let c0 = fmsubs(e1[2] - o[2], p[0] - o[0], (e1[0] - o[0]) * (p[2] - o[2]));
    let c1 = fmsubs(
        e2[2] - e1[2],
        p[0] - e1[0],
        (e2[0] - e1[0]) * (p[2] - e1[2]),
    );
    if !(c0 * c1 >= ZERO) {
        return false;
    }
    let c2 = fmsubs(o[2] - e2[2], p[0] - e2[0], (o[0] - e2[0]) * (p[2] - e2[2]));
    c1 * c2 >= ZERO && c2 * c0 >= ZERO
}

impl LandNodes {
    /// fn_800CEE08(query, object, mode). `None` when the radius is not
    /// positive, the object has no node, or nothing was hit (the routine
    /// returns 0 and leaves the query's outputs alone).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn collide(
        &self,
        q: &Query,
        obj: usize,
        vertex_mode: bool,
        stats: &mut ContactStats,
    ) -> Option<Contact> {
        if q.radius <= ZERO {
            return None;
        }
        let motion_len = nj_scalor(q.motion);
        let step = STEP * q.radius;
        let node = self.nodes.iter().find(|n| n.obj == obj)?;
        let first = node.cand_first.unwrap_or(0);
        let mut hits: Vec<ContactHit> = vec![];
        for &cand in self
            .candidates
            .iter()
            .skip(first)
            .take(node.cand_count as usize)
        {
            stats.tested += 1;
            let tri = &self.nodes[cand.node].tris[cand.tri];
            let v0 = tri.v[0];
            let e1 = sub(tri.v[1], v0);
            let e2 = sub(tri.v[2], v0);
            let rel = sub(q.center, v0);
            // The normal: (origin - e2) x (e1 - origin).
            let a = sub(e1, [ZERO; 3]);
            let b = sub([ZERO; 3], e2);
            let mut n = [
                fmsubs(b[1], a[2], b[2] * a[1]),
                fmsubs(b[2], a[0], b[0] * a[2]),
                fmsubs(b[0], a[1], b[1] * a[0]),
            ];
            n = if nj_scalor(n) == ZERO {
                [ZERO, ONE, ZERO]
            } else {
                unit_vector(n)
            };
            let (rx, ry) = level_angles(n);
            let mut m = IDENTITY;
            rotate_pair(&mut m, rx, 1, 2);
            rotate_y(&mut m, ry);
            let e1 = mult_vec(&m, e1);
            let e2 = mult_vec(&m, e2);
            let rel = mult_vec(&m, rel);
            let mot = mult_vec(&m, q.motion);
            let two_sided = tri.attr & ATTR_TWO_SIDED != 0;
            let mut height = q.radius;
            if two_sided && rel[1] < ZERO {
                height *= NEG_ONE;
            }
            let dir = if nj_scalor(mot) == ZERO {
                [ZERO; 3]
            } else {
                unit_vector(mot)
            };
            let dir = dir.map(|c| c * step);
            let mut p = rel;
            let mut travelled = ZERO;
            let mut hit = false;
            loop {
                let mut more = true;
                if travelled >= motion_len {
                    p = add(rel, mot);
                    stats.swept_to_end += 1;
                    more = false;
                }
                if p[1].abs() <= q.radius && inside(vertex_mode, e1, e2, p) {
                    hit = true;
                    stats.hits += 1;
                    break;
                }
                if !more {
                    break;
                }
                travelled += step;
                p = add(p, dir);
            }
            if !hit {
                continue;
            }
            let facing = if two_sided && !(rel[1] >= ZERO) {
                n.map(|c| -c)
            } else {
                n
            };
            let angles = normal_angles(facing[0], facing[1], facing[2]);
            invert(&mut m);
            let back = |v: Vec3| mult_vec(&m, v);
            hits.push(ContactHit {
                candidate: cand,
                angles,
                normal: n,
                facing,
                foot: add(back([rel[0], ZERO, rel[2]]), v0),
                head: add(back([rel[0], height, rel[2]]), v0),
                along: back([mot[0], ZERO, mot[2]]),
                across: back([ZERO, mot[1], ZERO]),
                start_height: rel[1],
                end_height: rel[1] + mot[1],
            });
            if hits.len() == HIT_LIMIT {
                break;
            }
        }
        if hits.is_empty() {
            return None;
        }
        let mut best = 0;
        for (i, h) in hits.iter().enumerate().skip(1) {
            if h.end_height < hits[best].end_height {
                best = i;
            }
        }
        let (mut foot, mut head) = (hits[0].foot, hits[0].head);
        if hits.len() > 1 {
            for h in &hits[1..] {
                foot = add(foot, h.foot);
                head = add(head, h.head);
            }
            let k = ONE / hits.len() as f32;
            foot = foot.map(|c| c * k);
            head = head.map(|c| c * k);
        }
        Some(Contact {
            hits,
            best,
            foot,
            head,
        })
    }
}
