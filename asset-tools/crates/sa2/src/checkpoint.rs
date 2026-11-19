//! Offline snapshots of the Steam CheckpointRecords array, not SET placements.
//! SA2Variables.h declares two CheckpointRespawnData at VA 0x1934b88.
use crate::{Error, Result};

#[derive(Debug, PartialEq)]
pub struct CheckpointRecord {
    pub level: i16,
    pub checkpoint_index: i16,
    /// Original minute, second and frame bytes; a snapshot may be inactive.
    pub timer: [u8; 3],
    pub padding: u8,
    /// Preserve original binary32 coordinates including signed zero and NaNs.
    pub position_bits: [u32; 3],
    pub rotation: [i32; 3],
    pub gravity_angle_z: i32,
    pub gravity_angle_x: i32,
    pub stage_restart_mode: i32,
}

/// Read both player slots without assigning any validity or activation semantics.
pub fn read_records(raw: &[u8]) -> Result<[CheckpointRecord; 2]> {
    let raw: &[u8; 88] = raw
        .try_into()
        .map_err(|_| Error::invalid("expected two 44-byte checkpoint records"))?;
    Ok(std::array::from_fn(|slot| {
        let r = &raw[slot * 44..][..44];
        let word = |i: usize| i16::from_le_bytes(r[i..i + 2].try_into().unwrap());
        let long = |i: usize| i32::from_le_bytes(r[i..i + 4].try_into().unwrap());
        CheckpointRecord {
            level: word(0),
            checkpoint_index: word(2),
            timer: r[4..7].try_into().unwrap(),
            padding: r[7],
            position_bits: std::array::from_fn(|i| long(8 + 4 * i) as u32),
            rotation: std::array::from_fn(|i| long(20 + 4 * i)),
            gravity_angle_z: long(32),
            gravity_angle_x: long(36),
            stage_restart_mode: long(40),
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_player_slots_and_native_fields_distinct() {
        let bytes: [u8; 88] = std::array::from_fn(|i| i as u8);
        let [a, b] = read_records(&bytes).unwrap();
        assert_eq!(a.gravity_angle_x, 0x27262524);
        assert_eq!(a.stage_restart_mode, 0x2b2a2928);
        assert_eq!(b.level, 0x2d2c);
        assert_eq!(b.checkpoint_index, 0x2f2e);
        assert_eq!(b.timer, [48, 49, 50]);
        assert_eq!(b.padding, 51);
        assert_eq!(b.position_bits, [0x37363534, 0x3b3a3938, 0x3f3e3d3c]);
        assert_eq!(b.rotation, [0x43424140, 0x47464544, 0x4b4a4948]);
        assert_eq!(b.gravity_angle_z, 0x4f4e4d4c);
        assert!(read_records(&bytes[..87]).is_err());
    }
}
