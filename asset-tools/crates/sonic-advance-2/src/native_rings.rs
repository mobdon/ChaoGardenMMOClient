//! Retail A2NE stationary ring placements and normal single-player collection.
use crate::{Error, VerifiedRom, native_player::PlayerState};
use serde::Serialize;
use std::ops::Range;

const TABLE: usize = 0x0d4bfc;
use crate::native_placements::{RegionLayout, stage_stream};

/// Post-intro HUD animation 1127 writes this palette to shared OBJ bank 6
/// after both ordinary-ring and collection-effect tasks.
pub fn stage_ring_palette(rom: &VerifiedRom<'_>) -> Result<[u16; 16], Error> {
    let raw = rom.range("rom", 0x16c228, 32)?;
    Ok(std::array::from_fn(|i| {
        u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]])
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RingPosition {
    pub x: i32,
    pub y: i32,
}

pub struct StageRings {
    columns: usize,
    rows: usize,
    regions: Vec<Range<usize>>,
    positions: Vec<RingPosition>,
    taken: Vec<bool>,
}

impl StageRings {
    /// Retail table slots 0..=30 have placements. The final three slots are null.
    pub fn load(rom: &VerifiedRom<'_>, level_id: u8) -> Result<Self, Error> {
        Self::from_layout(&stage_stream(rom, TABLE, level_id)?)
    }
    fn from_layout(raw: &[u8]) -> Result<Self, Error> {
        let RegionLayout {
            columns,
            rows,
            regions,
            records: positions,
        } = RegionLayout::parse::<2>(raw, |x, y, _| RingPosition { x, y })?;
        let taken = vec![false; positions.len()];
        Ok(Self {
            columns,
            rows,
            regions,
            positions,
            taken,
        })
    }
    pub fn placements(&self) -> &[RingPosition] {
        &self.positions
    }

    /// Ordinary alive player, with no magnetic shield or boss-ring respawn.
    pub fn collect(&mut self, player: &PlayerState) -> Vec<RingPosition> {
        let mut hits = Vec::new();
        if player.move_state & 0x80 != 0 {
            return hits;
        }
        let x = player.q_world_x >> 8;
        let y = player.q_world_y >> 8;
        let w = i32::from(player.sprite_offset_x);
        let h = i32::from(player.sprite_offset_y);
        // Lower region indices are assigned to u16 by Task_RingsMgrMain.
        // At the top/left world edge a negative start skips the loop.
        let col_start = ((x - w - 8) >> 8) as u16 as usize;
        let row_start = ((y - h) >> 8) as u16 as usize;
        let col_end = ((x + w + 16) >> 8).min(self.columns as i32 - 1);
        let row_end = ((y + h + 8) >> 8).min(self.rows as i32 - 1);
        for row in row_start as i32..=row_end {
            for col in col_start as i32..=col_end {
                for index in self.regions[row as usize * self.columns + col as usize].clone() {
                    let ring = self.positions[index];
                    if !self.taken[index]
                        && x - w <= ring.x + 8
                        && x + w >= ring.x - 8
                        && y - h <= ring.y
                        && y + h >= ring.y - 16
                    {
                        self.taken[index] = true;
                        hits.push(ring);
                    }
                }
            }
        }
        hits
    }

    /// Native ordinary drawing branch: first restrict the visited regions,
    /// then apply the original inclusive per-ring screen limits.
    pub fn visible(&self, camera_x: i32, camera_y: i32) -> impl Iterator<Item = RingPosition> + '_ {
        let cx = i64::from(camera_x);
        let cy = i64::from(camera_y);
        let first_x = ((camera_x >> 8) as u16) as i64;
        let first_y = ((camera_y >> 8) as u16) as i64;
        self.positions
            .iter()
            .zip(&self.taken)
            .filter_map(move |(&ring, &taken)| {
                let x = i64::from(ring.x);
                let y = i64::from(ring.y);
                let rx = x >> 8;
                let ry = y >> 8;
                (!taken
                    && rx >= first_x
                    && ry >= first_y
                    && rx * 256 < cx + 240
                    && ry * 256 < cy + 160
                    && (-8..=248).contains(&(x - cx))
                    && (0..=176).contains(&(y - cy)))
                .then_some(ring)
            })
    }
}

/// Retail single-player non-extra stage INCREMENT_RINGS(1); true requests
/// 1UP music even if the life total has already saturated.
pub fn increment_single_player(rings: &mut u16, lives: &mut u8) -> bool {
    let previous = *rings;
    *rings = rings.wrapping_add(1);
    if previous / 100 != *rings / 100 {
        *lives = lives.saturating_add(1);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_player::Character;

    fn fixture() -> StageRings {
        let mut data = Vec::new();
        for n in [19u32 << 8, 1, 1, 12] {
            data.extend_from_slice(&n.to_le_bytes());
        }
        data.extend_from_slice(&[12, 12, 255]);
        StageRings::from_layout(&data).unwrap()
    }

    #[test]
    fn normal_pickups_match_original_compiled_c_overlap() {
        let mut stage = fixture();
        let mut player = PlayerState::new_at(Character::Sonic, 0, 0);
        let mut results = Vec::new();
        for height in [14, 9] {
            player.sprite_offset_x = 6;
            player.sprite_offset_y = height;
            for dy in -40..=40 {
                for dx in -24..=24 {
                    player.q_world_x = (96 + dx) * 256;
                    player.q_world_y = (96 + dy) * 256;
                    stage.taken[0] = false;
                    let taken = !stage.collect(&player).is_empty();
                    results.push(if taken { b'1' } else { b'0' });
                    assert!(stage.collect(&player).is_empty());
                }
            }
        }
        assert_eq!(results.len(), 7938);
        assert_eq!(
            crate::gba::sha256(&results),
            "1a4749a737045b8cd7cd8e15cf577d5410b6a275ad5d648a2ae9632206ce3dd2"
        );
    }

    #[test]
    fn death_prevents_collection_and_crossing_one_hundred_requests_music() {
        let mut stage = fixture();
        let mut player = PlayerState::new_at(Character::Sonic, 96 * 256, 96 * 256);
        player.move_state |= 0x80;
        assert!(stage.collect(&player).is_empty());
        assert_eq!(stage.visible(0, 0).count(), 1);
        player.move_state &= !0x80;
        assert_eq!(stage.collect(&player).len(), 1);
        assert_eq!(stage.visible(0, 0).count(), 0);
        let (mut rings, mut lives) = (99, 3);
        assert!(increment_single_player(&mut rings, &mut lives));
        assert_eq!((rings, lives), (100, 4));
        assert!(!increment_single_player(&mut rings, &mut lives));
        (rings, lives) = (u16::MAX, u8::MAX);
        assert!(increment_single_player(&mut rings, &mut lives));
        assert_eq!((rings, lives), (0, 255));
    }
}
