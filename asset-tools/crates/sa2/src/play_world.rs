//! Live collision queries over a mounted stage's baked Basic collision.
//!
//! The retail collision engine (CCL, broadphase, surface responses) is not
//! ported. These queries are an explicit playability adaptation: vertical
//! ground probes and sphere push-out against the original solid triangles,
//! accelerated by an XZ grid. Triangles come from `CollisionWorld`, which keeps
//! the native positions, winding and surface flags.
use crate::collision::{CollisionWorld, SURFACE_DYNAMIC_COLLISION, SURFACE_SOLID};
use std::collections::HashMap;

/// Minimum |normal.y| for a surface to count as ground (about 53 degrees).
pub const WALKABLE_NORMAL_Y: f32 = 0.6;
const CELL: f32 = 64.0;

pub type V3 = [f32; 3];

#[derive(Debug, Clone)]
pub struct Tri {
    pub v: [V3; 3],
    /// Unit normal following source winding.
    pub n: V3,
    pub flags: u32,
    /// Index of the owning collision mesh, or `u32::MAX` for an object solid.
    pub mesh: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ground {
    pub y: f32,
    /// Normal oriented upward.
    pub normal: V3,
    pub tri: u32,
}

#[derive(Debug, Clone)]
pub struct PlayWorld {
    pub tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    pub min: V3,
    pub max: V3,
    stamp: std::cell::RefCell<(u32, Vec<u32>)>,
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

impl PlayWorld {
    /// Solid, non-dynamic triangles of the mounted landtable.
    pub fn from_collision(world: &CollisionWorld) -> Self {
        let mut tris = vec![];
        for (mi, mesh) in world.meshes.iter().enumerate() {
            if mesh.surface_flags & SURFACE_SOLID == 0
                || mesh.surface_flags & SURFACE_DYNAMIC_COLLISION != 0
            {
                continue;
            }
            for t in &mesh.triangles {
                let v = t.map(|i| mesh.vertices[i as usize].map(|c| c as f32));
                if let Some(tri) = make_tri(v, mesh.surface_flags, mi as u32) {
                    tris.push(tri);
                }
            }
        }
        Self::from_tris(tris)
    }

    pub fn from_tris(tris: Vec<Tri>) -> Self {
        let mut w = Self {
            tris: vec![],
            grid: HashMap::new(),
            min: [f32::MAX; 3],
            max: [f32::MIN; 3],
            stamp: Default::default(),
        };
        for t in tris {
            w.push(t);
        }
        w
    }

    /// Add one triangle (used for object solids such as bridges).
    pub fn push(&mut self, t: Tri) {
        let id = self.tris.len() as u32;
        let (lo, hi) = tri_bounds(&t);
        for i in 0..3 {
            self.min[i] = self.min[i].min(lo[i]);
            self.max[i] = self.max[i].max(hi[i]);
        }
        for cx in cell(lo[0])..=cell(hi[0]) {
            for cz in cell(lo[2])..=cell(hi[2]) {
                self.grid.entry((cx, cz)).or_default().push(id);
            }
        }
        self.tris.push(t);
        self.stamp.borrow_mut().1.push(0);
    }

    /// Visit each triangle overlapping the XZ rectangle once.
    fn each_in(&self, lo: [f32; 2], hi: [f32; 2], mut f: impl FnMut(u32, &Tri)) {
        let mut st = self.stamp.borrow_mut();
        st.0 = st.0.wrapping_add(1);
        if st.0 == 0 {
            st.1.iter_mut().for_each(|s| *s = 0);
            st.0 = 1;
        }
        let mark = st.0;
        for cx in cell(lo[0])..=cell(hi[0]) {
            for cz in cell(lo[1])..=cell(hi[1]) {
                let Some(ids) = self.grid.get(&(cx, cz)) else {
                    continue;
                };
                for &id in ids {
                    if st.1[id as usize] == mark {
                        continue;
                    }
                    st.1[id as usize] = mark;
                    f(id, &self.tris[id as usize]);
                }
            }
        }
    }

    /// Every surface crossed by the vertical line through (x, z), as (y, upward normal, tri).
    pub fn column(&self, x: f32, z: f32, mut f: impl FnMut(Ground)) {
        self.each_in([x, z], [x, z], |id, t| {
            if let Some(y) = vertical_hit(t, x, z) {
                let n = if t.n[1] < 0.0 { scale(t.n, -1.0) } else { t.n };
                f(Ground {
                    y,
                    normal: n,
                    tri: id,
                });
            }
        });
    }

    /// Highest walkable surface at or below `top` and above `top - depth`.
    pub fn ground_below(&self, p: V3, depth: f32) -> Option<Ground> {
        let mut best: Option<Ground> = None;
        self.column(p[0], p[2], |g| {
            if g.normal[1] >= WALKABLE_NORMAL_Y
                && g.y <= p[1]
                && g.y >= p[1] - depth
                && best.is_none_or(|b| g.y > b.y)
            {
                best = Some(g);
            }
        });
        best
    }

    /// Lowest surface strictly above `p.y` within `reach` (ceilings and overhangs).
    pub fn ceiling_above(&self, p: V3, reach: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        self.column(p[0], p[2], |g| {
            if g.y > p[1] && g.y <= p[1] + reach && best.is_none_or(|b| g.y < b) {
                best = Some(g.y);
            }
        });
        best
    }

    /// Push a sphere out of non-walkable triangles (walls and ceilings).
    /// Returns the total correction and the last contact normal.
    pub fn push_out(&self, mut c: V3, r: f32, iterations: usize) -> (V3, Option<V3>) {
        let start = c;
        let mut normal = None;
        for _ in 0..iterations {
            let mut moved = false;
            let mut deepest: Option<(f32, V3)> = None;
            self.each_in([c[0] - r, c[2] - r], [c[0] + r, c[2] + r], |_, t| {
                if t.n[1].abs() >= WALKABLE_NORMAL_Y {
                    return;
                }
                let q = closest_point(c, t);
                let d = sub(c, q);
                let dist = len(d);
                if dist < r {
                    let pen = r - dist;
                    let dir = if dist > 1e-4 {
                        scale(d, 1.0 / dist)
                    } else {
                        let s = if dot(sub(c, t.v[0]), t.n) >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        };
                        scale(t.n, s)
                    };
                    if deepest.is_none_or(|(p, _)| pen > p) {
                        deepest = Some((pen, dir));
                    }
                }
            });
            if let Some((pen, dir)) = deepest {
                // Walls push horizontally so the body neither climbs nor sinks.
                let mut h = [dir[0], 0.0, dir[2]];
                let hl = len(h);
                if hl > 0.2 {
                    h = scale(h, 1.0 / hl);
                    c = add(c, scale(h, pen / hl.max(0.2) + 0.01));
                    normal = Some(h);
                } else {
                    c = add(c, scale(dir, pen + 0.01));
                    normal = Some(dir);
                }
                moved = true;
            }
            if !moved {
                break;
            }
        }
        (sub(c, start), normal)
    }
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

fn tri_bounds(t: &Tri) -> (V3, V3) {
    let mut lo = t.v[0];
    let mut hi = t.v[0];
    for p in &t.v[1..] {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

pub fn make_tri(v: [V3; 3], flags: u32, mesh: u32) -> Option<Tri> {
    let n = cross(sub(v[1], v[0]), sub(v[2], v[0]));
    let l = len(n);
    (l > 1e-6 && l.is_finite()).then(|| Tri {
        v,
        n: scale(n, 1.0 / l),
        flags,
        mesh,
    })
}

fn vertical_hit(t: &Tri, x: f32, z: f32) -> Option<f32> {
    if t.n[1].abs() < 1e-4 {
        return None;
    }
    let [a, b, c] = t.v;
    let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
    let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
    let l3 = 1.0 - l1 - l2;
    const E: f32 = -1e-4;
    (l1 >= E && l2 >= E && l3 >= E).then(|| l1 * a[1] + l2 * b[1] + l3 * c[1])
}

/// Closest point on a triangle (Ericson, Real-Time Collision Detection 5.1.5).
pub fn closest_point(p: V3, t: &Tri) -> V3 {
    let [a, b, c] = t.v;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(y: f32, x0: f32, x1: f32, z0: f32, z1: f32) -> Vec<Tri> {
        let a = [x0, y, z0];
        let b = [x1, y, z0];
        let c = [x1, y, z1];
        let d = [x0, y, z1];
        vec![
            make_tri([a, c, b], 1, 0).unwrap(),
            make_tri([a, d, c], 1, 0).unwrap(),
        ]
    }

    #[test]
    fn ground_and_walls() {
        let mut tris = quad(0.0, -100.0, 100.0, -100.0, 100.0);
        tris.extend(quad(50.0, -100.0, 100.0, 200.0, 300.0));
        // A wall in the plane x = 20.
        tris.push(make_tri([[20., 0., -50.], [20., 40., -50.], [20., 0., 50.]], 1, 0).unwrap());
        tris.push(make_tri([[20., 40., -50.], [20., 40., 50.], [20., 0., 50.]], 1, 0).unwrap());
        let w = PlayWorld::from_tris(tris);
        let g = w.ground_below([0.0, 10.0, 0.0], 100.0).unwrap();
        assert_eq!(g.y, 0.0);
        assert!(g.normal[1] > 0.99);
        assert!(w.ground_below([0.0, -1.0, 0.0], 100.0).is_none());
        assert_eq!(w.ground_below([0.0, 60.0, 250.0], 100.0).unwrap().y, 50.0);
        let (corr, n) = w.push_out([15.0, 10.0, 0.0], 8.0, 4);
        assert!((corr[0] + 3.0).abs() < 0.05, "{corr:?}");
        assert!(n.unwrap()[0] < -0.9);
        let (corr, _) = w.push_out([0.0, 10.0, 0.0], 8.0, 4);
        assert_eq!(corr, [0.0; 3]);
    }
}
