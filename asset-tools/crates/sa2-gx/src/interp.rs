//! Frames between game frames: the game runs at 60 a second, a monitor may
//! refresh faster. Each draw of the newest frame is matched to one of the
//! frame before (same material, textures and vertex/index counts; among
//! those, the nearest), and its position matrices, which carry both the
//! object's and the camera's movement, are blended between the two. A draw
//! with no match, or one that moved or turned too far in a frame (a camera
//! cut, a teleport), shows as it is.
use crate::api::{FastMap, Frame};
use crate::params::Mtx34;
use std::hash::{Hash, Hasher};

/// A matrix translation that moves more than this in one frame is a jump
/// (camera cut, respawn), not motion to smooth.
const JUMP: f32 = 400.0;
/// A rotation/scale entry that changes more than this in one frame is a cut
/// (the camera looking somewhere else), not a turn to smooth.
const TURN: f32 = 0.5;
/// How many same-looking draws of the frame before are compared to find the
/// nearest (bounds the work for scenes with hundreds of copies).
const CANDIDATES: usize = 48;

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

/// How far the first matrix's translation moved (squared).
fn moved(a: &[Mtx34; 10], b: &[Mtx34; 10]) -> f32 {
    (0..3).map(|r| (a[0][r][3] - b[0][r][3]).powi(2)).sum()
}

fn jumped(a: &[Mtx34; 10], b: &[Mtx34; 10]) -> bool {
    a.iter().zip(b).any(|(a, b)| {
        let d: f32 = (0..3).map(|r| (a[r][3] - b[r][3]).powi(2)).sum();
        let turn = (0..3)
            .flat_map(|r| (0..3).map(move |c| (r, c)))
            .map(|(r, c)| (a[r][c] - b[r][c]).abs())
            .fold(0.0f32, f32::max);
        d.is_nan() || d > JUMP * JUMP || turn.is_nan() || turn > TURN
    })
}

impl Tween {
    /// Match `cur`'s draws to `prev`'s.
    pub fn new(prev: &Frame, cur: &Frame) -> Self {
        let mut by_key: FastMap<u64, Vec<usize>> = FastMap::default();
        for (i, d) in prev.draws.iter().enumerate() {
            by_key.entry(key(d)).or_default().push(i);
        }
        let to: Vec<[Mtx34; 10]> = cur.draws.iter().map(|d| d.draw_params.pos_mtx).collect();
        let from = cur
            .draws
            .iter()
            .zip(&to)
            .map(|(d, to)| {
                let bucket = by_key.get_mut(&key(d))?;
                // The nearest of the first candidates still unmatched, so a
                // copy that vanished doesn't shift every later copy onto its
                // neighbour.
                let (k, _) = bucket
                    .iter()
                    .take(CANDIDATES)
                    .enumerate()
                    .filter_map(|(k, &i)| {
                        Some((k, moved(&prev.draws.get(i)?.draw_params.pos_mtx, to)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))?;
                let i = bucket.remove(k);
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
