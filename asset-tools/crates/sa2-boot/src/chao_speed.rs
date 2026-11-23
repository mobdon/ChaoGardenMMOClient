//! Chao MMO glue: the player's Chao's speed in Chao World (lobby, gardens).
//! Levels keep the full row chao_player sets. In Chao World:
//! - SA2_CHAO_SPEED=metal (default, the owner's Chao): Metal Sonic's row as is.
//! - SA2_CHAO_SPEED=run: Sonic's own row with its speeds and acceleration
//!   scaled by the Chao's Run points, full at FULL_RUN_POINTS.
//!
//! Save record (ChaoMain, research STATS.md): skill points u16 at +0x38 + 2*i,
//! Run = i 2 (AL_ParameterGetSkill 0x20990); type byte +0x80, 2 = child
//! (CreateChaoExtra 0x314E0), 5..=0x16 adults.
//! Physics row field offsets: sa2 player_physics.rs PlayerParameters order.
use sa2_recomp::rt::Mem;

const RUN_POINTS: u32 = 0x38 + 2 * 2;
/// Run points of a perfect-S level-99 Chao (server speed table).
const FULL_RUN_POINTS: f32 = 2940.0;
/// shortcut: slowest share at 0 Run is an MMO choice, not from the game; tune with mob.
const MIN_SHARE: f32 = 0.5;
/// max_x_spd, max_psh_spd, run_speed, rush_speed, run_accel.
const SCALED: [u32; 5] = [0x10, 0x14, 0x28, 0x2C, 0x3C];

/// The row to use: `metal` for the owner profile, else `own` scaled by Run.
/// `record` is the Chao's 0x800-byte save record.
pub fn row(m: &Mem, record: u32, own: &[u32], metal: &[u32]) -> Vec<u32> {
    if std::env::var("SA2_CHAO_SPEED").map_or(true, |v| v.trim() != "run") {
        return metal.to_vec();
    }
    scaled(own, share(m.r16(record + RUN_POINTS)))
}

fn share(points: u16) -> f32 {
    MIN_SHARE + (1.0 - MIN_SHARE) * (f32::from(points) / FULL_RUN_POINTS).min(1.0)
}

fn scaled(own: &[u32], k: f32) -> Vec<u32> {
    let mut r = own.to_vec();
    for o in SCALED {
        let i = (o / 4) as usize;
        r[i] = (f32::from_bits(r[i]) * k).to_bits();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_run_keeps_the_row_and_zero_run_halves_the_speeds() {
        let own: Vec<u32> = (0..33).map(|i| (i as f32).to_bits()).collect();
        assert_eq!(scaled(&own, share(2940)), own);
        let half = scaled(&own, share(0));
        assert_eq!(f32::from_bits(half[0x28 / 4]), 5.0);
        assert_eq!(half[0x18 / 4], own[0x18 / 4]);
    }
}
