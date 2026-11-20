//! The main program's vertical land probes (GSNE8P main.dol), built on
//! `land_nodes` and `land_select` (#427).
//!
//! * `vertical_probe` is fn_800D05D0: for one land object's node, find the
//!   triangles whose XZ footprint holds the point (three edge cross
//!   products of matching sign), within 220 (squared 48400, r2-0x6068) of
//!   their first vertex, and not near-vertical (normal y squared at least
//!   1e-4, r2-0x60AC). The plane height at the point is
//!   `-((nx * x + nz * z) - (nx * v0x + ny * v0y + nz * v0z)) / ny`. The
//!   highest hit below the point and the lowest at or above it are kept,
//!   each with the node's flags, the unit normal (njUnitVector: Gekko's
//!   `frsqrte` alone; a zero normal becomes (0, 1, 0)) and two angles,
//!   `trunc(10430.38 * asinf(nz))` and `-trunc(10430.38 * atan2f(nx, ny))`.
//! * `ground_height` is fn_80067AF0, the probe stage objects such as
//!   SOLIDBOX use: reselect the land objects within 10 of the point
//!   (fn_80023808), skip dynamic objects whose model is out of reach
//!   horizontally and entries with flags 0x2002, probe each with
//!   fn_800D05D0, lift dynamic hits by their task's offset, and keep the
//!   highest hit below the point. It returns -1000000 when there is none.
use crate::{
    land_nodes::{nj_scalor, LandNodes, NjModel, NjObject},
    land_select::{LandCol, Selection},
    msl_math::{asinf, atan2f, gekko_frsqrte},
    ninja_matrix::Vec3,
    ninja_trig::fmadds,
};

const REACH_SQ: f32 = 48400.0; // r2-0x6068
const MIN_NY_SQ: f32 = f32::from_bits(0x38d1_b717); // r2-0x60AC
const LIMIT: f32 = 220.0; // r2-0x6090
const BAMS_PER_RADIAN: f64 = 10430.38043493439; // r2-0x60A0
const NO_GROUND: f32 = -1_000_000.0; // r2-0x7180
const PROBE_RADIUS: f32 = 10.0; // r2-0x716C

/// One kept triangle (the 0x20-byte records at +0xC and +0x2C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// +4: the node's selection flags.
    pub flags: u32,
    /// +8: `trunc(10430.38 * asinf(nz))`.
    pub angle_x: i32,
    /// +0xC: `-trunc(10430.38 * atan2f(nx, ny))`.
    pub angle_z: i32,
    /// +0x10: the plane height at the point.
    pub y: f32,
    /// +0x14: the unit normal.
    pub normal: Vec3,
}

/// The probe's output block: the highest hit below the point and the lowest
/// at or above it. With no hit, `y` keeps its start value (point y + 220
/// below, - 220 above).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    pub below: Option<Hit>,
    pub below_y: f32,
    pub above: Option<Hit>,
    pub above_y: f32,
}

fn fmsubs(a: f32, c: f32, b: f32) -> f32 {
    (a as f64 * c as f64 - b as f64) as f32
}

fn fnmadds(a: f32, c: f32, b: f32) -> f32 {
    let v = (a as f64 * c as f64 + b as f64) as f32;
    if v.is_nan() {
        v
    } else {
        -v
    }
}

/// njUnitVector (0x80115D24).
fn unit_vector(v: Vec3) -> Vec3 {
    let d = fmadds(v[2], v[2], fmadds(v[1], v[1], v[0] * v[0]));
    let e = gekko_frsqrte(d as f64);
    v.map(|c| (e * c as f64) as f32)
}

fn fctiwz(x: f64) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

/// fn_800D05D0. `None` when no node belongs to `obj` (the game returns 0
/// with the output block cleared).
pub fn vertical_probe(nodes: &LandNodes, obj: usize, p: Vec3) -> Option<Probe> {
    let [x, y, z] = p;
    let mut out = Probe {
        below: None,
        below_y: LIMIT + y,
        above: None,
        above_y: y - LIMIT,
    };
    let node = nodes.nodes.iter().find(|n| n.obj == obj)?;
    for t in &node.tris {
        let [v0, v1, v2] = t.v;
        let d = (x - v0[0]) * (x - v0[0]);
        if d > REACH_SQ {
            continue;
        }
        let d = fmadds(z - v0[2], z - v0[2], d);
        if d > REACH_SQ {
            continue;
        }
        let d = fmadds(y - v0[1], y - v0[1], d);
        if d > REACH_SQ {
            continue;
        }
        let e01x = v1[0] - v0[0];
        let e01z = v1[2] - v0[2];
        let c0 = fmsubs(e01z, x - v0[0], e01x * (z - v0[2]));
        let e12x = v2[0] - v1[0];
        let e12z = v2[2] - v1[2];
        let c1 = fmsubs(e12z, x - v1[0], e12x * (z - v1[2]));
        if c0 * c1 < 0.0 {
            continue;
        }
        let e20x = v0[0] - v2[0];
        let e20z = v0[2] - v2[2];
        let c2 = fmsubs(e20z, x - v2[0], e20x * (z - v2[2]));
        if c1 * c2 < 0.0 || c2 * c0 < 0.0 {
            continue;
        }
        let e01y = v1[1] - v0[1];
        let e20y = v0[1] - v2[1];
        let ny = fmsubs(e20z, e01x, e20x * e01z);
        let nx = fmsubs(e20y, e01z, e20z * e01y);
        let nz = fmsubs(e20x, e01y, e20y * e01x);
        if ny * ny < MIN_NY_SQ {
            continue;
        }
        let plane = fnmadds(nz, v0[2], fmadds(nx, v0[0], ny * v0[1]));
        let at = fmadds(nz, z, nx * x);
        let hy = -(plane + at) / ny;
        let slot = if hy < y {
            match out.below {
                Some(_) if hy <= out.below_y => None,
                _ => Some(true),
            }
        } else {
            match out.above {
                // fcmpo/bge: a NaN height is not taken.
                Some(_) if hy.partial_cmp(&out.above_y) != Some(std::cmp::Ordering::Less) => None,
                _ => Some(false),
            }
        };
        let Some(below) = slot else { continue };
        let n = [nx, ny, nz];
        let normal = if nj_scalor(n) == 0.0 {
            [0.0, 1.0, 0.0]
        } else {
            unit_vector(n)
        };
        let hit = Hit {
            flags: node.flags,
            angle_x: fctiwz(BAMS_PER_RADIAN * f64::from(asinf(normal[2]))),
            angle_z: fctiwz(BAMS_PER_RADIAN * f64::from(atan2f(normal[0], normal[1])))
                .wrapping_neg(),
            y: hy,
            normal,
        };
        if below {
            out.below = Some(hit);
            out.below_y = hy;
        } else {
            out.above = Some(hit);
            out.above_y = hy;
        }
    }
    Some(out)
}

/// What fn_80067AF0 returns (the height) and writes into its rotation
/// argument: the hit's `angle_x` at +0 and `angle_z` at +8 (+4 is left
/// alone).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ground {
    pub y: f32,
    pub angle_x: i32,
    pub angle_z: i32,
}

/// fn_80067AF0. `task_offset` gives a dynamic entry's lift (the float at
/// +0x14 of the block its task holds at +0x3C), or `None`.
#[allow(clippy::too_many_arguments)]
pub fn ground_height(
    sel: &mut Selection,
    nodes: &LandNodes,
    objects: &[NjObject],
    models: &[NjModel],
    cols: &[LandCol],
    col_objs: &[usize],
    task_offset: impl Fn(usize) -> Option<f32>,
    p: Vec3,
) -> Option<Ground> {
    sel.reselect(objects, models, cols, col_objs, p, PROBE_RADIUS);
    let mut best = NO_GROUND;
    let mut found = None;
    for e in &sel.list {
        if e.task.is_some() {
            let o = &objects[e.obj];
            let len = nj_scalor([o.pos[0] - p[0], 0.0, o.pos[2] - p[2]]);
            let mut s = o.scl[0];
            if s < o.scl[1] {
                s = o.scl[1];
            }
            if s < o.scl[2] {
                s = o.scl[2];
            }
            if len > s * models[o.model].radius {
                continue;
            }
        }
        if e.flags & 0x2002 != 0 {
            continue;
        }
        let Some(mut probe) = vertical_probe(nodes, e.obj, p) else {
            continue;
        };
        if probe.below.is_none() && probe.above.is_none() {
            continue;
        }
        if let Some(lift) = e.task.and_then(&task_offset) {
            probe.below_y += lift;
            probe.above_y += lift;
        }
        if let Some(h) = probe.below {
            if p[1] >= probe.below_y && probe.below_y > best {
                best = probe.below_y;
                found = Some(Ground {
                    y: best,
                    angle_x: h.angle_x,
                    angle_z: h.angle_z,
                });
            }
        }
    }
    found
}

/// fn_800CFE6C's six rays, in its block order: X below / above the point,
/// Y below / above, Z below / above.
pub const RAYS: usize = 6;

/// The probe reach along each ray (r2-0x6090).
pub const PROBE_REACH: f32 = LIMIT;

/// fn_800CFE6C's output block: per ray, the kept hit and its coordinate
/// along the ray's axis. With no hit the coordinate keeps its start value
/// (the point's coordinate + 220 for "below", - 220 for "above").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe6 {
    pub hits: [Option<Hit>; RAYS],
    pub coords: [f32; RAYS],
}

/// One fn_800CFE6C section: the triangles whose footprint on axes `a` and
/// `b` holds the point, cast along the third axis `w` into rays `ray` (the
/// hit below the point) and `ray + 1` (at or above). The three sections are
/// the same code with the axes renamed: A casts along Z (a = X, b = Y), B
/// along X (a = Y, b = Z), C along Y (a = Z, b = X).
#[allow(clippy::too_many_arguments)]
fn cast(
    out: &mut Probe6,
    flags: u32,
    tv: &[Vec3; 3],
    p: Vec3,
    a: usize,
    b: usize,
    w: usize,
    ray: usize,
) {
    let [v0, v1, v2] = *tv;
    let edge = |s: Vec3, e: Vec3| fmsubs(e[a] - s[a], p[b] - s[b], (e[b] - s[b]) * (p[a] - s[a]));
    let c0 = edge(v0, v1);
    let c1 = edge(v1, v2);
    if c0 * c1 < 0.0 {
        return;
    }
    let c2 = edge(v2, v0);
    if c1 * c2 < 0.0 || c2 * c0 < 0.0 {
        return;
    }
    // The normal is the same expression in every section.
    let nx = fmsubs(
        v0[1] - v2[1],
        v1[2] - v0[2],
        (v0[2] - v2[2]) * (v1[1] - v0[1]),
    );
    let ny = fmsubs(
        v0[2] - v2[2],
        v1[0] - v0[0],
        (v0[0] - v2[0]) * (v1[2] - v0[2]),
    );
    let nz = fmsubs(
        v0[0] - v2[0],
        v1[1] - v0[1],
        (v0[1] - v2[1]) * (v1[0] - v0[0]),
    );
    let n = [nx, ny, nz];
    if n[w] * n[w] < MIN_NY_SQ {
        return;
    }
    let plane = fnmadds(nz, v0[2], fmadds(nx, v0[0], ny * v0[1]));
    let at = fmadds(n[a], p[a], n[b] * p[b]);
    let h = -(plane + at) / n[w];
    use std::cmp::Ordering::{Greater, Less};
    let slot = if h < p[w] {
        // ble: an existing hit is replaced only by a higher one.
        match out.hits[ray] {
            Some(_) if h.partial_cmp(&out.coords[ray]) != Some(Greater) => return,
            _ => ray,
        }
    } else {
        // bge: an existing hit is replaced only by a lower one.
        match out.hits[ray + 1] {
            Some(_) if h.partial_cmp(&out.coords[ray + 1]) != Some(Less) => return,
            _ => ray + 1,
        }
    };
    let normal = if nj_scalor(n) == 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        unit_vector(n)
    };
    out.hits[slot] = Some(Hit {
        flags,
        angle_x: fctiwz(BAMS_PER_RADIAN * f64::from(asinf(normal[2]))),
        angle_z: fctiwz(BAMS_PER_RADIAN * f64::from(atan2f(normal[0], normal[1]))).wrapping_neg(),
        y: h,
        normal,
    });
    out.coords[slot] = h;
}

/// fn_800CFE6C: fn_800D05D0 along all three axes. `None` when no node
/// belongs to `obj` (the game returns 0 with the block reset).
pub fn six_way_probe(nodes: &LandNodes, obj: usize, p: Vec3) -> Option<Probe6> {
    let mut out = Probe6 {
        hits: [None; RAYS],
        coords: [
            LIMIT + p[0],
            p[0] - LIMIT,
            LIMIT + p[1],
            p[1] - LIMIT,
            LIMIT + p[2],
            p[2] - LIMIT,
        ],
    };
    let node = nodes.nodes.iter().find(|n| n.obj == obj)?;
    for t in &node.tris {
        let v0 = t.v[0];
        let d = (p[0] - v0[0]) * (p[0] - v0[0]);
        if d > REACH_SQ {
            continue;
        }
        let d = fmadds(p[2] - v0[2], p[2] - v0[2], d);
        if d > REACH_SQ {
            continue;
        }
        let d = fmadds(p[1] - v0[1], p[1] - v0[1], d);
        if d > REACH_SQ {
            continue;
        }
        cast(&mut out, node.flags, &t.v, p, 0, 1, 2, 4);
        cast(&mut out, node.flags, &t.v, p, 1, 2, 0, 0);
        cast(&mut out, node.flags, &t.v, p, 2, 0, 1, 2);
    }
    Some(out)
}

/// GetShadowPosXYZ's block (0xCC bytes at the point): per ray the kept hit
/// and its coordinate, starting at -1e6 for the "below" rays and +1e6 for
/// the "above" ones.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shadow {
    pub hits: [Option<Hit>; RAYS],
    pub coords: [f32; RAYS],
}

/// GetShadowPosXYZ (0x80067210, `water` false) and fn_80066E1C (`water`
/// true): reselect the land objects within 1 of the point (fn_80023808),
/// skip dynamic objects whose model (radius times its largest scale) is
/// out of reach in 3D, probe each with fn_800CFE6C and keep, per ray, the
/// hit nearest the point. GetShadowPosXYZ skips entries with flags 0x2002
/// and returns whether any probe hit; fn_80066E1C probes them too and
/// returns whether one of them supplied the ground (ray 2) at the time.
#[allow(clippy::too_many_arguments)]
pub fn shadow_xyz(
    sel: &mut Selection,
    nodes: &LandNodes,
    objects: &[NjObject],
    models: &[NjModel],
    cols: &[LandCol],
    col_objs: &[usize],
    p: Vec3,
    water: bool,
) -> (Shadow, bool) {
    let mut out = Shadow {
        hits: [None; RAYS],
        coords: [
            NO_GROUND, -NO_GROUND, NO_GROUND, -NO_GROUND, NO_GROUND, -NO_GROUND,
        ],
    };
    let mut ret = false;
    sel.reselect(objects, models, cols, col_objs, p, 1.0);
    for e in &sel.list {
        if e.task.is_some() {
            let o = &objects[e.obj];
            let len = nj_scalor([o.pos[0] - p[0], o.pos[1] - p[1], o.pos[2] - p[2]]);
            let mut s = o.scl[0];
            if s < o.scl[1] {
                s = o.scl[1];
            }
            if s < o.scl[2] {
                s = o.scl[2];
            }
            if len > s * models[o.model].radius {
                continue;
            }
        }
        let special = e.flags & 0x2002 != 0;
        if special && !water {
            continue;
        }
        let Some(probe) = six_way_probe(nodes, e.obj, p) else {
            continue;
        };
        if probe.hits.iter().all(Option::is_none) {
            continue;
        }
        if !water {
            ret = true;
        }
        for r in 0..RAYS {
            let Some(h) = probe.hits[r] else { continue };
            let axis = r / 2;
            let cur = (p[axis] - out.coords[r]).abs();
            let new = (p[axis] - probe.coords[r]).abs();
            // ble: taken only when the old coordinate is farther.
            if cur > new {
                out.hits[r] = Some(h);
                out.coords[r] = probe.coords[r];
                if water && r == 2 && special {
                    ret = true;
                }
            }
        }
    }
    (out, ret)
}
