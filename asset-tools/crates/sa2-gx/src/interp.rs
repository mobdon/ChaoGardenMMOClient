//! Frames between game frames: the game runs at 60 a second, a monitor may
//! refresh faster. Each draw of the newest frame is matched to one of the
//! frame before (same material, textures and vertex/index counts, taken in
//! order), and its position matrices, which carry both the object's and the
//! camera's movement, are blended between the two. A draw with no match, or
//! one that moved too far in a frame (a cut, a teleport), shows as it is.
use crate::api::{FastMap, Frame};
use crate::params::Mtx34;
use std::hash::{Hash, Hasher};

/// A matrix translation that moves more than this in one frame is a jump
/// (camera cut, respawn), not motion to smooth.
const JUMP: f32 = 400.0;

/// The matrices to blend from, for each draw of the newest frame.
pub struct Tween {
    from: Vec<Option<[Mtx34; 10]>>,
    to: Vec<[Mtx34; 10]>,
}

fn key(d: &crate::api::DrawCmd) -> u64 {
    let mut h = crate::api::FastHasher::default();
    d.material.hash(&mut h);
    d.vertices.len().hash(&mut h);
    d.indices.len().hash(&mut h);
    for m in &d.material_params.texture_mappings {
        m.texture.hash(&mut h);
    }
    h.finish()
}

fn jumped(a: &[Mtx34; 10], b: &[Mtx34; 10]) -> bool {
    a.iter().zip(b).any(|(a, b)| {
        let d: f32 = (0..3).map(|r| (a[r][3] - b[r][3]).powi(2)).sum();
        d.is_nan() || d > JUMP * JUMP
    })
}

impl Tween {
    /// Match `cur`'s draws to `prev`'s.
    pub fn new(prev: &Frame, cur: &Frame) -> Self {
        let mut by_key: FastMap<u64, std::collections::VecDeque<usize>> = FastMap::default();
        for (i, d) in prev.draws.iter().enumerate() {
            by_key.entry(key(d)).or_default().push_back(i);
        }
        let to: Vec<[Mtx34; 10]> = cur.draws.iter().map(|d| d.draw_params.pos_mtx).collect();
        let from = cur
            .draws
            .iter()
            .zip(&to)
            .map(|(d, to)| {
                let i = by_key.get_mut(&key(d))?.pop_front()?;
                let from = prev.draws.get(i)?.draw_params.pos_mtx;
                (!jumped(&from, to)).then_some(from)
            })
            .collect();
        Self { from, to }
    }

    /// Draws that will be blended (for tests and the profiler).
    pub fn matched(&self) -> usize {
        self.from.iter().flatten().count()
    }

    /// Set `cur`'s position matrices to `t` of the way from the previous
    /// frame's (0) to its own (1). `cur` must be the frame given to `new`.
    pub fn apply(&self, cur: &mut Frame, t: f32) {
        let t = t.clamp(0.0, 1.0);
        for ((d, from), to) in cur.draws.iter_mut().zip(&self.from).zip(&self.to) {
            d.draw_params.pos_mtx = match from {
                Some(from) if t < 1.0 => std::array::from_fn(|k| {
                    std::array::from_fn(|r| {
                        std::array::from_fn(|c| from[k][r][c] + (to[k][r][c] - from[k][r][c]) * t)
                    })
                }),
                _ => *to,
            };
        }
    }
}
