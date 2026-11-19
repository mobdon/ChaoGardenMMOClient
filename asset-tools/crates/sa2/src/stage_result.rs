//! Result rows within a little-endian Steam SAVE_DATA memory image, not a
//! disk-container decoder. Stage slots are physical indices, not LevelIDs.
use crate::{Error, Result};
use serde::Serialize;

const ARRAY: usize = 0x38;
const STRIDE: usize = 196;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Best {
    /// SAMT names this a 32-bit ring field; SA2Structs describes two shorts.
    /// Preserve all four bytes without assuming the high half is a ring count.
    ring_word_bits: u32,
    score: i32,
    time_bytes: [u8; 3],
    padding: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionResult {
    slot: usize,
    array: &'static str,
    array_index: usize,
    mission: u8,
    row_offset: usize,
    rank_byte: u8,
    rank: Option<&'static str>,
    rank_area_byte5: u8,
    play_count: i16,
    best: [Best; 3],
}

/// Read one mission from the 60 stage rows followed by two large kart rows.
/// Only the result prefix is required. No checksum, unlock, or owner decision.
pub fn read(snapshot: &[u8], slot: usize, mission: u8) -> Result<MissionResult> {
    if slot >= 62 || !(1..=5).contains(&mission) {
        return Err(Error::invalid(
            "expected result slot 0..61 and mission 1..5",
        ));
    }
    let rows = snapshot
        .get(ARRAY..ARRAY + 62 * STRIDE)
        .ok_or_else(|| Error::invalid("truncated Steam SAVE_DATA result array"))?;
    let r = &rows[slot * STRIDE..(slot + 1) * STRIDE];
    let m = usize::from(mission - 1);
    let rank_byte = r[m];
    let best = std::array::from_fn(|i| {
        let at = 16 + (3 * m + i) * 12;
        Best {
            ring_word_bits: u32::from_le_bytes(r[at..at + 4].try_into().unwrap()),
            score: i32::from_le_bytes(r[at + 4..at + 8].try_into().unwrap()),
            time_bytes: r[at + 8..at + 11].try_into().unwrap(),
            padding: r[at + 11],
        }
    });
    Ok(MissionResult {
        slot,
        array: if slot < 60 { "stages" } else { "bigCart" },
        array_index: if slot < 60 { slot } else { slot - 60 },
        mission,
        row_offset: ARRAY + slot * STRIDE,
        rank_byte,
        rank: ["none", "E", "D", "C", "B", "A"]
            .get(usize::from(rank_byte))
            .copied(),
        rank_area_byte5: r[5],
        play_count: i16::from_le_bytes(r[6 + 2 * m..8 + 2 * m].try_into().unwrap()),
        best,
    })
}
