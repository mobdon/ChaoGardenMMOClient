//! Following a stage path (loops, rails): fn_80038F40 and the path helpers
//! under it, as SA2B's main.dol (GameCube, GSNE8P) computes them.
//!
//! A path (the player work's `PathData`, +0xB8) is a `LoopHead`: flags (bit
//! 0x8000 = closed loop, low 15 bits = point type), a point count, a total
//! length and the points. Type 1 points are 0x14 bytes (X and Z angle
//! halfwords, segment length, position). Type 2 points are 0x1C bytes (a
//! vector, segment length, position). Type 0 points are bare positions.
//! `tests/player_path_dol.rs` checks everything here against main.dol.
use crate::{
    msl_math::{asinf, atan2f},
    ninja_matrix::{rotate_by_angles, MatrixStack},
    ninja_trig::{fctiwz, fctiwz_f64, fmadds, fnmsubs, nj_cos, nj_sin},
    ninja_vector::{acosf, inner_product, outer_product, scalor, unit_vector, vector_angle},
    player_range::{distance, engine_sqrtf},
    player_turn::{adjust_angle, diff_angle, sub_angle},
    walker_collide::{plane_distance, Plane},
};

pub type V3 = [f32; 3];

const ZERO: f32 = 0.0; // r2-0x7300
const ONE: f32 = 1.0; // r2-0x72FC
const NEAR_ZERO: f32 = f32::from_bits(0x3ccc_cccd); // r2-0x55A0, 0.025
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb); // r2-0x72F8
const DASH_FLOOR: f32 = f32::from_bits(0x3f99_999a); // r2-0x759C, 1.2
const LOOK_AHEAD: f32 = 9.0; // r2-0x7598
const PARALLEL: f32 = f32::from_bits(0x3f7f_f972); // r2-0x7594, 0.9999
const SMALL_CROSS: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8, 0.1

/// a / b where an invalid quotient (0/0, inf/inf) is PowerPC's positive
/// default NaN rather than x86's negative one.
fn ppc_div(a: f32, b: f32) -> f32 {
    let q = a / b;
    if q.is_nan() && !a.is_nan() && !b.is_nan() {
        f32::from_bits(0x7fc0_0000)
    } else {
        q
    }
}

/// MSL `floorf` (0x80111240).
pub fn floorf(x: f32) -> f32 {
    let i = fctiwz(x);
    let t = (i as f64) as f32;
    if (t - x).to_bits() == 0 {
        return x;
    }
    if x.to_bits() & 0x7f80_0000 >= 0x4b80_0000 {
        return x;
    }
    let i = if x.to_bits() & 0x8000_0000 != 0 {
        i.wrapping_sub(1)
    } else {
        i
    };
    (i as f64) as f32
}

/// fn_8002B770: x - m * floor(x / m), or 0 when m is 0.
pub fn wrap(x: f32, m: f32) -> f32 {
    if m == ZERO {
        return ZERO;
    }
    fnmsubs(m, floorf(x / m), x)
}

/// fn_80027D50: (b - a) . (c - a).
pub fn segment_side(a: V3, b: V3, c: V3) -> f32 {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    inner_product(u, v)
}

/// fn_8011B51C: distance from `p` to the line through `point` along `dir`,
/// and the foot of the perpendicular when asked for.
pub fn line_distance(p: V3, point: V3, dir: V3, want_foot: bool) -> (f32, Option<V3>) {
    let num =
        dir[2] * (p[2] - point[2]) + (dir[0] * (p[0] - point[0]) + dir[1] * (p[1] - point[1]));
    let den = dir[2] * dir[2] + (dir[0] * dir[0] + dir[1] * dir[1]);
    let t = ppc_div(num, den);
    let (d2, foot) = if want_foot {
        let f = [
            point[0] + dir[0] * t,
            point[1] + dir[1] * t,
            point[2] + dir[2] * t,
        ];
        let dz = p[2] - f[2];
        let dx = p[0] - f[0];
        let dy = p[1] - f[1];
        (dz * dz + (dx * dx + dy * dy), Some(f))
    } else {
        let dz = p[2] - (point[2] + dir[2] * t);
        let dx = p[0] - (point[0] + dir[0] * t);
        let dy = p[1] - (point[1] + dir[1] * t);
        (dz * dz + (dx * dx + dy * dy), None)
    };
    // A NaN passes through sqrtf unchanged (PowerPC does not flip its sign
    // in the Newton steps' negated multiply-subtracts).
    let d = if d2 < NEAR_ZERO {
        0.0
    } else if d2.is_nan() {
        d2
    } else {
        engine_sqrtf(d2)
    };
    (d, foot)
}

/// fn_800CE5E8: distance from `p` to the segment a-b, and the nearest point.
pub fn segment_distance(p: V3, a: V3, b: V3) -> (f32, V3) {
    if segment_side(a, b, p) < 0.0 {
        return (distance(p, a), a);
    }
    if segment_side(b, a, p) < 0.0 {
        return (distance(p, b), b);
    }
    let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let (d, foot) = line_distance(p, a, dir, true);
    (d, foot.unwrap())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Points {
    /// Type 0: positions.
    Plain(Vec<V3>),
    /// Type 1: (X angle, Z angle, segment length, position).
    Angled(Vec<(i16, i16, f32, V3)>),
    /// Type 2: (vector, segment length, position).
    Vectored(Vec<(V3, f32, V3)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Raw flags halfword (+0): 0x8000 closed loop, low 15 bits the type.
    pub flags: i16,
    /// Point count (+2) and total length (+4).
    pub count: i16,
    pub total: f32,
    pub points: Points,
}

impl Path {
    fn looped(&self) -> bool {
        self.flags as u16 & 0x8000 != 0
    }

    fn kind(&self) -> i32 {
        (self.flags as u16 & 0x7fff) as i32
    }

    /// The four floats at byte offset `off` of point `i`'s record and the
    /// next ones (the type-2 wrap reads the first record at +8).
    fn floats_at(&self, i: usize, off: usize) -> V3 {
        let words: Vec<f32> = match &self.points {
            Points::Plain(p) => p[i].to_vec(),
            Points::Angled(p) => {
                let (_, _, l, pos) = p[i];
                vec![f32::NAN, l, pos[0], pos[1], pos[2]]
            }
            Points::Vectored(p) => {
                let (a, l, b) = p[i];
                vec![a[0], a[1], a[2], l, b[0], b[1], b[2]]
            }
        };
        let k = off / 4;
        [words[k], words[k + 1], words[k + 2]]
    }
}

/// fn_8005C460's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nearest {
    /// Path types other than 1 and 2: 0.0, outputs untouched.
    Unsupported,
    /// The search loop did not run (an open path with fewer than two
    /// points): the original returns its caller's f28 and writes nothing.
    Empty,
    /// The distance, the nearest point and the path distance to it (the
    /// segment's start plus the distance along it).
    Found { dist: f32, point: V3, along: f32 },
}

/// fn_8005C460: the nearest point on the path to `pos`.
pub fn nearest(path: &Path, pos: V3) -> Nearest {
    let stop: i32 = if path.looped() { 0 } else { 1 };
    let pos_off = match path.kind() {
        1 => 8usize,
        2 => 16,
        _ => return Nearest::Unsupported,
    };
    let mut best: Option<(f32, V3, f32)> = None;
    let mut acc = ZERO;
    let mut n = path.count as i32;
    let mut i = 0usize;
    while n > stop {
        let seg_len = match &path.points {
            Points::Angled(p) => p[i].2,
            Points::Vectored(p) => p[i].1,
            Points::Plain(_) => unreachable!(),
        };
        let a = path.floats_at(i, pos_off);
        // The last point closes onto the first, read at +8 for both types.
        let b = if n == 1 {
            path.floats_at(0, 8)
        } else {
            path.floats_at(i + 1, pos_off)
        };
        let (d, q) = segment_distance(pos, a, b);
        // While the running length is still 0 every segment replaces the
        // best; afterwards only a strictly nearer one does.
        if ZERO == acc || best.is_some_and(|(bd, _, _)| d < bd) {
            best = Some((d, q, acc + distance(a, q)));
        }
        acc += seg_len;
        n -= 1;
        i += 1;
    }
    match best {
        Some((dist, point, along)) => Nearest::Found { dist, point, along },
        None => Nearest::Empty,
    }
}

/// fn_8005CDCC's record for type 1 and 2 segments.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Segment {
    pub a0: [i16; 2],
    pub a1: [i16; 2],
    pub v0: V3,
    pub v1: V3,
    pub len0: f32,
    pub len1: f32,
    pub p0: V3,
    pub p1: V3,
}

/// fn_8005CDCC: segment `i` with its neighbours (angles halfway to them for
/// type 1).
pub fn segment(path: &Path, i: i32) -> Segment {
    let count = path.count as i32;
    let mut prev = i - 1;
    let mut next = i + 1;
    if path.looped() {
        if prev < 0 {
            prev = count - 1;
        } else if next >= count {
            next = 0;
        }
    } else if prev < 0 {
        prev = 0;
    } else if next >= count {
        next = count - 1;
    }
    let mut s = Segment::default();
    match &path.points {
        Points::Plain(p) => {
            s.p0 = p[i as usize];
            s.p1 = p[next as usize];
        }
        Points::Angled(p) => {
            let (cx, cz, cl, cp) = p[i as usize];
            let (px, pz, _, _) = p[prev as usize];
            let (nx, nz, nl, np) = p[next as usize];
            let half = |from: i16, to: i16| {
                (from as i32).wrapping_add(sub_angle(from as i32, to as i32) >> 1) as i16
            };
            s.a0 = [half(px, cx), half(pz, cz)];
            s.a1 = [half(cx, nx), half(cz, nz)];
            s.p0 = cp;
            s.p1 = np;
            s.len0 = cl;
            s.len1 = nl;
        }
        Points::Vectored(p) => {
            let (ca, cl, cb) = p[i as usize];
            let (na, nl, nb) = p[next as usize];
            s.v0 = ca;
            s.v1 = na;
            s.len0 = cl;
            s.len1 = nl;
            s.p0 = cb;
            s.p1 = nb;
        }
    }
    s
}

/// fn_8005C910's in/out record (+0x10 is the path distance in).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    pub angles: [i32; 4],
    pub dist: f32,
    pub pos: V3,
    pub up0: V3,
    pub up1: V3,
    pub dir: V3,
}

/// fn_8005C910: the path at `s.dist` (wrapped onto a loop). False when the
/// distance is off an open path or the type has no samples. Also false
/// (where the original reads past its point array) when the stored segment
/// lengths add up to less than the distance.
pub fn sample(path: &Path, s: &mut Sample) -> bool {
    if path.looped() {
        s.dist = wrap(s.dist, path.total);
    } else if s.dist < ZERO || s.dist > path.total || s.dist.is_nan() || path.total.is_nan() {
        return false;
    }
    let kind = path.kind();
    if kind != 1 && kind != 2 {
        return false;
    }
    let d = s.dist;
    let mut acc = ZERO;
    let mut seg_len = ZERO;
    let mut i = 0i32;
    for k in 0..path.count.max(0) as usize {
        seg_len = match &path.points {
            Points::Angled(p) => p[k].2,
            Points::Vectored(p) => p[k].1,
            Points::Plain(_) => unreachable!(),
        };
        if seg_len == ZERO {
            seg_len = ONE;
        }
        if d <= acc + seg_len {
            break;
        }
        acc += seg_len;
        i += 1;
    }
    if i >= path.count as i32 {
        // Past the last record: the original reads beyond the point array.
        return false;
    }
    let seg = segment(path, i);
    if kind == 1 {
        let t = (d - acc) / seg_len;
        s.angles[0] = seg.a0[0] as i32;
        s.angles[1] = seg.a0[1] as i32;
        let lerp = |a: i16, b: i16| {
            let delta = (sub_angle(a as i32, b as i32) as f64) as f32;
            (a as i32).wrapping_add(fctiwz(t * delta))
        };
        s.angles[2] = lerp(seg.a0[0], seg.a1[0]);
        s.angles[3] = lerp(seg.a0[1], seg.a1[1]);
        let v = [
            seg.p1[0] - seg.p0[0],
            seg.p1[1] - seg.p0[1],
            seg.p1[2] - seg.p0[2],
        ];
        s.pos = [0, 1, 2].map(|k| fmadds(v[k], t, seg.p0[k]));
        let up = |x: i32, z: i32| {
            let cx = nj_cos(x);
            [-nj_sin(z) * cx, nj_cos(z) * cx, nj_sin(x)]
        };
        s.up0 = up(s.angles[0], s.angles[1]);
        s.up1 = up(s.angles[2], s.angles[3]);
        s.dir = unit_vector(v).0;
    } else {
        let t = (d - acc) / seg_len;
        let rest = ONE - t;
        let v = [
            seg.p1[0] - seg.p0[0],
            seg.p1[1] - seg.p0[1],
            seg.p1[2] - seg.p0[2],
        ];
        s.pos = [0, 1, 2].map(|k| fmadds(v[k], t, seg.p0[k]));
        s.up0 = seg.v0;
        s.up1 = [0, 1, 2].map(|k| fmadds(seg.v0[k], t, seg.v1[k] * rest));
        s.up1 = unit_vector(s.up1).0;
        s.dir = unit_vector(v).0;
        let bams = |r: f32| fctiwz_f64(BAMS_PER_RAD * r as f64);
        s.angles[0] = bams(asinf(s.up0[2]));
        s.angles[1] = bams(atan2f(s.up0[0], s.up0[1])).wrapping_neg();
        s.angles[2] = bams(asinf(s.up1[2]));
        s.angles[3] = bams(atan2f(s.up1[0], s.up1[1])).wrapping_neg();
    }
    true
}

/// The player state fn_80038F40 reads and writes.
#[derive(Debug, Clone, Default)]
pub struct PathPlayer {
    /// Task work status (+4), angles (+8..+0x10), position (+0x14).
    pub status: i16,
    pub angles: [i32; 3],
    pub pos: V3,
    /// Player work: forward speed (+0x64), dash_speed (+0xF4), path
    /// distance (+0x30), floor normal (dynamic-collision info +0x1C).
    pub speed_x: f32,
    pub dash_speed: f32,
    pub path_dist: f32,
    pub normal: V3,
    /// Motion work aim yaw (+0x1C).
    pub aim: i32,
}

/// fn_80038F40(task work, motion work, player work): run along the path.
/// `stick` is the player's stick magnitude. Returns false where the
/// original clears status bit 0x2000 and stops.
pub fn follow(stack: &mut MatrixStack, path: &Path, p: &mut PathPlayer, stick: f32) -> bool {
    let floor = DASH_FLOOR * p.dash_speed;
    let held = ZERO != stick;
    if p.speed_x < floor && held {
        p.speed_x = floor;
    } else if !held {
        p.status &= !0x2000;
        return false;
    }
    let mut near_point = [0.0; 3];
    if let Nearest::Found { point, along, .. } = nearest(path, p.pos) {
        near_point = point;
        p.path_dist = along;
    }
    let fwd = rotate_by_angles(stack, p.angles, [1.0, 0.0, 0.0]);
    let mut s = Sample {
        dist: p.path_dist,
        pos: near_point,
        ..Default::default()
    };
    if sample(path, &mut s) {
        if inner_product(fwd, s.dir) >= 0.0 {
            p.path_dist += p.speed_x;
        } else {
            p.path_dist -= p.speed_x;
        }
    }
    let prev = s.pos;
    s.dist = p.path_dist;
    if !sample(path, &mut s) {
        p.status &= !0x2000;
        return false;
    }
    // The second sample's direction (the first one's flip is overwritten).
    let dir = s.dir;
    let plane = Plane {
        point: [0.0; 3],
        normal: p.normal,
    };
    let mut rel = [
        s.pos[0] - p.pos[0],
        s.pos[1] - p.pos[1],
        s.pos[2] - p.pos[2],
    ];
    let side = rotate_by_angles(stack, p.angles, [1.0, 0.0, 0.0]);
    let mut seg = [s.pos[0] - prev[0], s.pos[1] - prev[1], s.pos[2] - prev[2]];
    rel = unit_vector(rel).0;
    seg = unit_vector(seg).0;
    if inner_product(side, seg) < 0.0 {
        seg = seg.map(|v| -v);
    }
    rel = [0, 1, 2].map(|k| fmadds(LOOK_AHEAD, seg[k], rel[k]));
    let mut ahead = [0.0; 3];
    plane_distance(rel, &plane, Some(&mut ahead));
    let mut facing = [0.0; 3];
    plane_distance(fwd, &plane, Some(&mut facing));
    if scalor(facing) == ZERO || scalor(ahead) == ZERO {
        p.status &= !0x2000;
        return false;
    }
    let ahead = unit_vector(ahead).0;
    let facing = unit_vector(facing).0;
    let turn = vector_angle(ahead, facing, Some(p.normal));
    let mut target = p.angles[1].wrapping_add(turn) & 0xffff;
    if diff_angle(target, p.angles[1]) > 0x4000 {
        target += 0x8000;
    }
    p.aim = target;
    let gap = diff_angle(target, p.angles[1]);
    let flat = |c: V3| {
        if scalor(c) <= SMALL_CROSS {
            p.normal
        } else {
            unit_vector(c).0
        }
    };
    let c1 = flat(outer_product(fwd, dir));
    let c2 = flat(outer_product(ahead, dir));
    let step = if inner_product(c1, c2) < 0.0 {
        gap >> 1
    } else if inner_product(c1, c2) <= PARALLEL {
        gap >> 4
    } else if gap > 0x400 {
        fctiwz_f64(BAMS_PER_RAD * acosf(inner_product(dir, fwd)) as f64) >> 4
    } else {
        gap >> 7
    };
    p.angles[1] = adjust_angle(target, p.angles[1], step);
    true
}
