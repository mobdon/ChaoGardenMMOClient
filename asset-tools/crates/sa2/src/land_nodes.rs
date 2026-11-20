//! fn_800D0904 (GSNE8P main.dol): keep the land collision node list in step
//! with this frame's selection.
//!
//! Each frame fn_8002334C (`land_select`) lists the land objects near the
//! players as {flags, NJS_OBJECT*} records. fn_800D0904 then:
//!
//! 1. Collects the records that need a new node: those with flag
//!    0x08000000, the one equal to the "rebuild" object at r13-0x7750 (which
//!    it clears), and those without an active node. It reads at most 0xC0
//!    records and stops after the 0xC0th new one found by the last rule.
//! 2. Unless 0xC0 were collected, frees each active node whose object is
//!    not in the list, or is listed again with flag 0x08000000 or as the
//!    rebuild object (fn_800D24B4 returns its triangles to the free list).
//! 3. Builds a node for each collected record (fn_800D251C puts it at the
//!    head of the active list). The object's model center, transformed by
//!    the object's own matrix, is the node center (+0x30). Then the object
//!    tree is walked (fn_800D13A0) and every polygon of every basic model
//!    becomes a triangle record (fn_800D14C8, fn_800D17E0): its world
//!    vertices, its center (0.3333 times the vertex sum), the largest
//!    squared vertex distance from that center, and the material's
//!    attribute flags. fn_800D23A8 files each record in one of 128 buckets
//!    by `(center.z - node.z) / 16 + 64`, newest first, and fn_800D2298
//!    joins the buckets in order into the node's list (+0x10/+0x14). The
//!    largest |dz| along any triangle edge is kept at +0x28.
//!
//! The object matrices use Ninja's table rotations (fn_8011BFE0 Z,
//! fn_8011BF64 Y, fn_8011BF0C X, in that order, from njSinCosTbl),
//! njTranslateEx and fn_8011610C (scale), all paired-single code ported
//! here lane by lane. The routine's debug readout (pool use, printed when
//! the byte at 0x801CC16B is 1) is left out.
//!
//! Pools (fn_800D0F3C): 0xC0 nodes and 0x2000 triangle records. Both
//! allocators refuse the last free entry. The game does not check the node
//! allocation; this port stops building when it would fail.
use crate::{
    msl_math::{gekko_fres, gekko_frsqrte},
    ninja_matrix::{mult_vec, MatrixStack, Mtx, Vec3, IDENTITY},
    ninja_trig::{fmadds, nj_cos, nj_sin},
};

/// Flag that forces a fresh node for a listed object.
pub const FLAG_REBUILD: u32 = 0x0800_0000;
pub const NODE_POOL: usize = 0xC0;
pub const TRI_POOL: usize = 0x2000;
/// r2-0x6040: the triangle-center factor (0.3333, not 1/3).
const THIRD: f32 = f32::from_bits(0x3eaa_a64c);
const MAX_LIST: usize = 0xC0;
const STRIP_LIMIT: usize = 0x400;

/// One basic-model mesh set (NJS_MESHSET, 0x18 bytes).
#[derive(Debug, Clone)]
pub struct MeshSet {
    /// +0: polygon kind in bits 14..15, material index in bits 0..13.
    pub type_mat: u16,
    /// +2: polygon or strip count.
    pub count: u16,
    /// +4: the index stream (strips start with a length word, bit 15 flips
    /// the winding).
    pub meshes: Vec<u16>,
}

/// The parts of a basic NJS_MODEL the collision reads.
#[derive(Debug, Clone)]
pub struct NjModel {
    pub points: Vec<Vec3>,
    pub sets: Vec<MeshSet>,
    /// NJS_MATERIAL attribute flags (+0x10 of each 0x14-byte material), or
    /// `None` when the model has no materials.
    pub attrs: Option<Vec<u32>>,
    /// +0x18.
    pub center: Vec3,
    /// +0x24: bounding radius.
    pub radius: f32,
}

/// The NJS_OBJECT fields the collision reads.
#[derive(Debug, Clone)]
pub struct NjObject {
    /// +0: NJD_EVAL flags (0x4 unit scale, 0x20 ZXY rotation order).
    pub evalflags: u32,
    pub model: usize,
    pub pos: Vec3,
    pub ang: [i32; 3],
    pub scl: Vec3,
    pub child: Option<usize>,
    pub sibling: Option<usize>,
}

/// A triangle record (0x40 bytes, +8..+0x3C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tri {
    pub center: Vec3,
    /// Largest squared distance of a vertex from `center`.
    pub radius_sq: f32,
    pub v: [Vec3; 3],
    pub attr: u32,
}

/// An active collision node (0x40 bytes).
#[derive(Debug, Clone)]
pub struct Node {
    /// +8: the object (index into the caller's object arena).
    pub obj: usize,
    /// +0xC: the selection record's flags.
    pub flags: u32,
    /// +0x28: largest |dz| along any triangle edge.
    pub max_dz: f32,
    /// +0x30: the model center in world space.
    pub center: Vec3,
    /// +0x10 onwards: the triangle list in bucket order.
    pub tris: Vec<Tri>,
    /// +0x1C: where this node's candidates start in the last query's list
    /// (fn_800D1020), or `None`.
    pub cand_first: Option<usize>,
    /// +0x24: how many candidates it added.
    pub cand_count: u32,
}

/// A selection record (12 bytes at r13-0x79F4: flags, object, and the
/// registering task for dynamic land objects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub flags: u32,
    pub obj: usize,
    pub task: Option<usize>,
}

/// A candidate triangle from fn_800D1020: the node's position in the active
/// list at query time and the triangle's position in the node's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub node: usize,
    pub tri: usize,
}

/// The query fn_800D1020 reads: radius +0, center +4, motion +0x10.
#[derive(Debug, Clone, Copy)]
pub struct Query {
    pub radius: f32,
    pub center: Vec3,
    pub motion: Vec3,
}

/// The candidate list's size (0x8028F998, count at r13-0x7758).
pub const CANDIDATE_LIMIT: usize = 0xC0;

/// The land collision state fn_800D0904 keeps.
#[derive(Debug, Clone)]
pub struct LandNodes {
    /// The active list, head first (r13-0x7738).
    pub nodes: Vec<Node>,
    /// Free triangle records (the r13-0x7740 list).
    pub free_tris: usize,
    /// r13-0x7750: an object whose node is rebuilt on its next listing.
    pub rebuild: Option<usize>,
    /// The candidate list. Entries past the last query's count keep older
    /// queries' contents, as the game's array does. (A node freed since then
    /// is not reproduced: the game would read whatever reused its memory.)
    pub candidates: Vec<Candidate>,
}

impl Default for LandNodes {
    fn default() -> Self {
        Self::new()
    }
}

// --- Ninja's paired-single matrix routines -------------------------------

/// njTranslateEx (0x8011616C): m = m * T(v).
fn translate(m: &mut Mtx, v: Vec3) {
    for row in m.iter_mut() {
        let a = fmadds(row[2], v[2], row[0] * v[0]);
        let b = fmadds(row[3], 1.0, row[1] * v[1]);
        row[3] = a + b;
    }
}

/// fn_8011610C: m = m * S(v).
fn scale(m: &mut Mtx, v: Vec3) {
    for row in m.iter_mut() {
        row[0] *= v[0];
        row[1] *= v[1];
        row[2] *= v[2];
    }
}

/// fn_8011BF0C / fn_8011BFE0: rotate about X (columns 1, 2) or Z (columns
/// 0, 1) with the table's sine and cosine.
pub(crate) fn rotate_pair(m: &mut Mtx, angle: i32, a: usize, b: usize) {
    let (s, c) = (nj_sin(angle), nj_cos(angle));
    for row in m.iter_mut() {
        let (ma, mb) = (row[a], row[b]);
        row[a] = fmadds(s, mb, c * ma);
        row[b] = fmadds(c, mb, -s * ma);
    }
}

/// fn_8011BF64: rotate about Y (columns 0, 2).
pub(crate) fn rotate_y(m: &mut Mtx, angle: i32) {
    let (s, c) = (nj_sin(angle), nj_cos(angle));
    for row in m.iter_mut() {
        let (m0, m2) = (row[0], row[2]);
        row[0] = fmadds(-s, m2, c * m0);
        row[2] = fmadds(c, m2, s * m0);
    }
}

/// The object transform fn_800D0904 and fn_800D13A0 apply.
fn apply_object(m: &mut Mtx, o: &NjObject) {
    translate(m, o.pos);
    rotate_pair(m, o.ang[2], 0, 1);
    rotate_y(m, o.ang[1]);
    rotate_pair(m, o.ang[0], 1, 2);
    scale(m, o.scl);
}

/// `fctiwz` as Gekko does it: saturating, NaN to 0x80000000.
fn fctiwz(x: f32) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

/// SqMag (0x800D24A4).
fn sq_mag(x: f32, y: f32, z: f32) -> f32 {
    fmadds(z, z, fmadds(x, x, y * y))
}

/// Per-node build state (the r13-0x7744/-0x7748 globals and the bucket
/// table at 0x80298910).
struct Build<'a> {
    objects: &'a [NjObject],
    models: &'a [NjModel],
    stack: MatrixStack,
    center_z: f32,
    max_dz: f32,
    buckets: Vec<Vec<Tri>>,
    free_tris: &'a mut usize,
}

impl Build<'_> {
    /// fn_800D13A0: transform `obj` under the current matrix, build its
    /// children and then itself. Returns its sibling, or `None` after a
    /// failed allocation.
    fn subtree(&mut self, obj: usize) -> Option<usize> {
        let objects = self.objects;
        self.stack.push();
        apply_object(self.stack.current_mut(), &objects[obj]);
        let mut child = objects[obj].child;
        while let Some(c) = child {
            self.stack.push();
            apply_object(self.stack.current_mut(), &objects[c]);
            let mut grand = objects[c].child;
            while let Some(g) = grand {
                grand = self.subtree(g);
            }
            let ok = self.polygons(c);
            self.stack.pop();
            child = if ok { objects[c].sibling } else { None };
        }
        let ok = self.polygons(obj);
        self.stack.pop();
        if ok {
            objects[obj].sibling
        } else {
            None
        }
    }

    /// fn_800D14C8: every polygon of the object's model, under the current
    /// matrix. False when the triangle pool ran out.
    fn polygons(&mut self, obj: usize) -> bool {
        let model = &self.models[self.objects[obj].model];
        let mut count = 0usize;
        for set in &model.sets {
            let kind = set.type_mat & 0xC000;
            match kind {
                0 => count = 3,
                0x4000 => count = 4,
                _ => {}
            }
            let mut at = 0usize;
            for _ in 0..set.count {
                // fn_800D17E0: transform this polygon's or strip's points.
                let mut flip = 0usize;
                if kind & 0x8000 != 0 {
                    let head = set.meshes[at];
                    count = (head & 0x7FFF) as usize;
                    flip = (head >> 15) as usize;
                    count = count.min(STRIP_LIMIT);
                    at += 1;
                }
                let verts: Vec<Vec3> = (0..count)
                    .map(|k| {
                        let index = set.meshes[at + k] as i16 as usize;
                        mult_vec(self.stack.current(), model.points[index])
                    })
                    .collect();
                at += count;
                for i in 0..count.saturating_sub(2) {
                    if !self.triangle(model, set, &verts[i..i + 3], (i + flip) & 1 == 1) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn triangle(&mut self, model: &NjModel, set: &MeshSet, p: &[Vec3], swap: bool) -> bool {
        let [x0, y0, z0] = p[0];
        let [x1, y1, z1] = p[1];
        let [x2, y2, z2] = p[2];
        let cz = THIRD * (z2 + (z0 + z1));
        let cx = THIRD * (x2 + (x0 + x1));
        let cy = THIRD * (y2 + (y0 + y1));
        // fn_800D23A8: the bucket, and the pool's last record is never used.
        let bucket = ((fctiwz(cz - self.center_z) >> 4) + 0x40).clamp(0, 0x7F) as usize;
        if *self.free_tris <= 1 {
            return false;
        }
        *self.free_tris -= 1;
        let v = if swap {
            [p[0], p[2], p[1]]
        } else {
            [p[0], p[1], p[2]]
        };
        let mut dz = (z1 - z0).abs();
        let d1 = (z2 - z1).abs();
        let d2 = (z0 - z2).abs();
        if d1 > dz {
            dz = d1;
        }
        if d2 > dz {
            dz = d2;
        }
        if dz > self.max_dz {
            self.max_dz = dz;
        }
        let mut r = sq_mag(x0 - cx, y0 - cy, z0 - cz);
        let r1 = sq_mag(x1 - cx, y1 - cy, z1 - cz);
        if r < r1 {
            r = r1;
        }
        let r2 = sq_mag(x2 - cx, y2 - cy, z2 - cz);
        if r < r2 {
            r = r2;
        }
        let attr = model
            .attrs
            .as_ref()
            .map_or(0, |a| a[(set.type_mat & 0x3FFF) as usize]);
        self.buckets[bucket].push(Tri {
            center: [cx, cy, cz],
            radius_sq: r,
            v,
            attr,
        });
        true
    }
}

impl LandNodes {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            free_tris: TRI_POOL,
            rebuild: None,
            candidates: vec![],
        }
    }

    /// fn_800D0904 after its fn_8002334C call: `entries` is the selection
    /// list (count at r13-0x79A4).
    pub fn update(&mut self, entries: &[Entry], objects: &[NjObject], models: &[NjModel]) {
        // 1. Records that need a node.
        let mut fresh: Vec<Entry> = vec![];
        for e in entries.iter().take(MAX_LIST) {
            if e.flags & FLAG_REBUILD != 0 {
                fresh.push(*e);
            } else if self.rebuild == Some(e.obj) {
                self.rebuild = None;
                fresh.push(*e);
            } else if !self.nodes.iter().any(|n| n.obj == e.obj) {
                fresh.push(*e);
                if fresh.len() == MAX_LIST {
                    break;
                }
            }
        }
        // 2. Drop nodes whose object left the list or is rebuilt.
        if fresh.len() != MAX_LIST {
            let rebuild = self.rebuild;
            let mut freed = 0;
            self.nodes.retain(|n| {
                let keep = entries
                    .iter()
                    .find(|e| e.obj == n.obj)
                    .is_some_and(|e| e.flags & FLAG_REBUILD == 0 && rebuild != Some(e.obj));
                if !keep {
                    freed += n.tris.len();
                }
                keep
            });
            self.free_tris += freed;
        }
        // 3. Build the new nodes.
        for e in fresh {
            // fn_800D251C refuses the pool's last node.
            if NODE_POOL - self.nodes.len() <= 1 {
                break;
            }
            let o = &objects[e.obj];
            let mut m = IDENTITY;
            apply_object(&mut m, o);
            let center = mult_vec(&m, models[o.model].center);
            let mut b = Build {
                objects,
                models,
                stack: MatrixStack::new(),
                center_z: center[2],
                max_dz: 0.0,
                buckets: vec![vec![]; 0x80],
                free_tris: &mut self.free_tris,
            };
            b.stack.push();
            apply_object(b.stack.current_mut(), o);
            let mut child = o.child;
            while let Some(c) = child {
                child = b.subtree(c);
            }
            b.polygons(e.obj);
            let max_dz = b.max_dz;
            let tris = b
                .buckets
                .into_iter()
                .flat_map(|t| t.into_iter().rev())
                .collect();
            self.nodes.insert(
                0,
                Node {
                    obj: e.obj,
                    flags: e.flags,
                    max_dz,
                    center,
                    tris,
                    cand_first: None,
                    cand_count: 0,
                },
            );
        }
    }
}

/// njScalor (0x801151F4): `fres(frsqrte(x*x + y*y + z*z))`, both Gekko
/// table estimates.
pub fn nj_scalor(v: Vec3) -> f32 {
    let d = fmadds(v[2], v[2], fmadds(v[1], v[1], v[0] * v[0]));
    gekko_fres(gekko_frsqrte(d as f64)) as f32
}

impl LandNodes {
    /// fn_800D1020 after its fn_80023808 call (`land_select::reselect`,
    /// which rewrote `list`): collect the triangles whose bounding spheres
    /// come within `radius + |motion|` of the query center. Each listed
    /// node's triangle list is searched from the end nearer the query's
    /// depth band, `z +/- (max_dz + reach)`. Returns the count (r13-0x7758).
    /// At 0xC0 candidates it stops at once and later nodes keep their
    /// previous `cand_first`/`cand_count`.
    // The negated comparisons keep the branches' NaN behaviour.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn query(&mut self, q: &Query, list: &[Entry]) -> usize {
        let reach = q.radius + nj_scalor(q.motion);
        let reach_sq = reach * reach;
        let mut count = 0usize;
        for ni in 0..self.nodes.len() {
            let node = &mut self.nodes[ni];
            node.cand_first = None;
            node.cand_count = 0;
            if !list.iter().any(|e| e.obj == node.obj) {
                continue;
            }
            let tris = &node.tris;
            let Some(last) = tris.len().checked_sub(1) else {
                // The game would read the empty list's head through NULL.
                continue;
            };
            let z = q.center[2];
            let lo = (z - node.max_dz) - reach;
            let hi = reach + (z + node.max_dz);
            let mut at = Some(0);
            if tris[0].center[2] < lo && tris[last].center[2] > hi {
                at = (0..tris.len()).find(|&i| !(tris[i].center[2] <= lo));
            }
            let hit = |t: &Tri| {
                let lim = reach_sq + t.radius_sq;
                let dx = t.center[0] - q.center[0];
                let mut d = dx * dx;
                if d > lim {
                    return false;
                }
                let dy = t.center[1] - q.center[1];
                d = fmadds(dy, dy, d);
                if d > lim {
                    return false;
                }
                let dz = t.center[2] - q.center[2];
                d = fmadds(dz, dz, d);
                !(d > lim)
            };
            let order: Box<dyn Iterator<Item = usize>> = match at {
                Some(i) if tris[i].center[2] > lo => {
                    Box::new((i..tris.len()).take_while(|&k| tris[k].center[2] < hi))
                }
                _ => Box::new((0..=last).rev().take_while(|&k| tris[k].center[2] > lo)),
            };
            let picks: Vec<usize> = order.filter(|&k| hit(&tris[k])).collect();
            for tri in picks {
                let node = &mut self.nodes[ni];
                if node.cand_first.is_none() {
                    node.cand_first = Some(count);
                }
                node.cand_count += 1;
                let c = Candidate { node: ni, tri };
                if count < self.candidates.len() {
                    self.candidates[count] = c;
                } else {
                    self.candidates.push(c);
                }
                count += 1;
                if count == CANDIDATE_LIMIT {
                    return count;
                }
            }
        }
        count
    }
}
