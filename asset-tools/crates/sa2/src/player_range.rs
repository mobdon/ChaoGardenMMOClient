//! The main program's player range test (GSNE8P main.dol 0x800398D8; PC
//! stage-module import 0x80039670) and the vector length it uses
//! (0x801151F4).
//!
//! The range test walks the two player slots (the table at 0x801E7768:
//! slot task, its +0x2C, that block's +0xC, position at +8), forms
//! `player - query` with `fsubs`, takes its length, and returns `slot + 1`
//! for the first player whose `length - radius` (`fsubs`) is below the
//! `.sdata2` zero at r2-0x75F8 (`fcmpo`, so a NaN never counts); otherwise
//! 0. An empty slot is skipped.
//!
//! The length is not a square root: `fmuls` x*x, two `fmadds`, then
//! `frsqrte` and `fres`, both Gekko hardware estimates. It is the shared
//! `land_nodes::nj_scalor` (owned by the Iron Gate thread).
use crate::land_nodes::nj_scalor;
use crate::play_world::V3;

/// 0x801151F4 (njScalor): the shared `land_nodes::nj_scalor`.
pub fn vector_length(v: V3) -> f32 {
    nj_scalor(v)
}

/// 0x800398D8: the first player slot (1 or 2) whose position lies within
/// `radius` of `query`, else 0. `players` holds each slot's position, or
/// `None` for an empty slot.
pub fn player_in_range(players: &[Option<V3>; 2], query: V3, radius: f32) -> u32 {
    for (slot, p) in players.iter().enumerate() {
        let Some(p) = p else { continue };
        let d = [p[0] - query[0], p[1] - query[1], p[2] - query[2]];
        if vector_length(d) - radius < 0.0 {
            return slot as u32 + 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_are_close_to_exact() {
        for v in [[3.0f32, 4.0, 0.0], [100.0, -20.0, 7.5], [0.01, 0.0, 0.0]] {
            let exact = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let est = vector_length(v);
            assert!(
                (est - exact).abs() / exact < 1.0 / 512.0,
                "{v:?}: {est} vs {exact}"
            );
        }
        assert_eq!(vector_length([0.0; 3]), 0.0);
    }

    #[test]
    fn first_slot_in_range_wins() {
        let players = [Some([10.0, 0.0, 0.0]), Some([1.0, 0.0, 0.0])];
        assert_eq!(player_in_range(&players, [0.0; 3], 5.0), 2);
        assert_eq!(player_in_range(&players, [0.0; 3], 50.0), 1);
        assert_eq!(player_in_range(&[None, None], [0.0; 3], 50.0), 0);
    }
}
