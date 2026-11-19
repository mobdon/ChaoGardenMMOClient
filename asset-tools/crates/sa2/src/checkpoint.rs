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

/// City Escape savepoint work fields advanced by module 12, text 0x4496c.
/// Angles and steps are sign-extended from their low 16 bits each tick.
#[derive(Debug, PartialEq)]
pub struct CheckpointSpin {
    pub angles: [i16; 2],
    pub steps: [i16; 2],
    pub counters: [i16; 2],
}

impl CheckpointSpin {
    /// Mode 2's spin update; true requests native mode 3. Does not activate a checkpoint.
    pub fn advance(&mut self) -> bool {
        for (angle, step) in self.angles.iter_mut().zip(self.steps) {
            *angle = angle.wrapping_add(step);
        }
        self.steps[1] = (f32::from(self.steps[1]) * 0.995) as i16;
        let speed = i32::from(self.steps[1]).abs();
        if speed < 0xe38 {
            let angle = i32::from(self.angles[0]);
            let bend = if angle > 0x2000 {
                0x47d2 - angle
            } else {
                angle
            };
            self.steps[0] = -((0.1 * bend as f32 * (0xe38 - speed) as f32 / 8192.0) as i16);
        }
        if self.steps == [0, 0] {
            return true;
        }
        for n in &mut self.counters {
            if *n < 3 {
                *n += 1;
            }
        }
        false
    }
}

#[cfg(test)]
mod spin_reference {
    use super::*;
    #[test]
    fn installed_ppc_vectors() {
        for (input, expected, settled) in [
            ([0, 0, 0, 0], [0, 0, 0, 0], true),
            (
                [32760, -32760, 120, -180],
                [-32656, 32596, 1379, -179],
                false,
            ),
            (
                [-16000, 18000, -3000, 8000],
                [-19000, 26000, -3000, 7960],
                false,
            ),
            ([8200, 600, -1, -1], [8199, 599, -452, 0], false),
        ] {
            let [a, b, c, d] = input;
            let mut s = CheckpointSpin {
                angles: [a, b],
                steps: [c, d],
                counters: [-1, 3],
            };
            assert_eq!(s.advance(), settled);
            assert_eq!([s.angles[0], s.angles[1], s.steps[0], s.steps[1]], expected);
            assert_eq!(s.counters, [if settled { -1 } else { 0 }, 3]);
        }
    }
}

/// Result of the bonus-selection branch after an accepted City Escape
/// checkpoint contact. IDs are the original arguments to helper 0x3069c.
/// The increment is applied to two native signed halfword counters.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckpointBonus {
    pub item_id: u8,
    pub counter_increment: i16,
}

/// The original helper's return is compared as a signed halfword. The two low
/// equipment bits are consulted only for the highest tier.
pub fn contact_bonus(value: i16, equipment: u16) -> Option<CheckpointBonus> {
    let (item_id, counter_increment) = match value {
        ..=19 => return None,
        20..=39 => (2, 5),
        40..=59 => (0, 10),
        60..=79 => (1, 20),
        80..=89 => (8, 0),
        _ if equipment & 3 != 0 => (6, 0),
        _ => (3, 0),
    };
    Some(CheckpointBonus {
        item_id,
        counter_increment,
    })
}

#[cfg(test)]
mod bonus_reference {
    use super::*;
    #[test]
    fn contact_tier_boundaries() {
        for (v, flags, expected) in [
            (i16::MIN, 0, None),
            (19, 0, None),
            (20, 0, Some((2, 5))),
            (39, 3, Some((2, 5))),
            (40, 0, Some((0, 10))),
            (59, 0, Some((0, 10))),
            (60, 0, Some((1, 20))),
            (79, 0, Some((1, 20))),
            (80, 0, Some((8, 0))),
            (89, 3, Some((8, 0))),
            (90, 0, Some((3, 0))),
            (90, 1, Some((6, 0))),
            (90, 2, Some((6, 0))),
            (i16::MAX, 4, Some((3, 0))),
        ] {
            assert_eq!(
                contact_bonus(v, flags).map(|b| (b.item_id, b.counter_increment)),
                expected
            );
        }
    }
}
