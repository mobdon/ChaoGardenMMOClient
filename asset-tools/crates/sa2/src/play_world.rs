//! Native collision geometry of a mounted stage and its placed object solids.
//!
//! Triangles come from `CollisionWorld`, which keeps the native positions,
//! winding and surface flags, plus object collision models placed by
//! `play_session`. No collision queries run here: the retail collision engine
//! (CCL, broadphase, surface responses) is main-program code and is not
//! ported.
use crate::collision::{CollisionWorld, SURFACE_DYNAMIC_COLLISION, SURFACE_SOLID};

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

#[derive(Debug, Clone)]
pub struct PlayWorld {
    pub tris: Vec<Tri>,
    pub min: V3,
    pub max: V3,
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
            min: [f32::MAX; 3],
            max: [f32::MIN; 3],
        };
        for t in tris {
            w.push(t);
        }
        w
    }

    /// Add one triangle (used for object solids).
    pub fn push(&mut self, t: Tri) {
        for p in &t.v {
            for i in 0..3 {
                self.min[i] = self.min[i].min(p[i]);
                self.max[i] = self.max[i].max(p[i]);
            }
        }
        self.tris.push(t);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_winding_and_bounds() {
        let t = make_tri([[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]], 1, 0).unwrap();
        assert_eq!(t.n, [0.0, 1.0, 0.0]);
        assert!(make_tri([[0.0; 3]; 3], 1, 0).is_none());
        let w = PlayWorld::from_tris(vec![t]);
        assert_eq!((w.min, w.max), ([0.0; 3], [1.0, 0.0, 1.0]));
    }
}
