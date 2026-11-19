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

/// Player-task work used at the accepted-contact call (taskwk +0x14 and +8).
pub struct ContactPlayer {
    pub position_bits: [u32; 3],
    pub rotation: [i32; 3],
}

/// Arguments to module zero's 0x8001c9f4 at City Escape text +0x447a4.
/// The implementation of that engine call is not part of the stage REL.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckpointContactCall {
    pub slot: usize,
    pub position_bits: [u32; 3],
    pub rotation: [i32; 3],
    pub checkpoint_byte: u8,
}

pub struct ContactState {
    pub mode: i8,
    pub slot_timers: [i32; 2],
}

/// Consume a collision-engine observation. None represents no hit; the tuple
/// holds the resolved player slot and the contact object's word at offset 8.
impl ContactState {
    pub fn accept(
        &mut self,
        hit: Option<(i32, u32)>,
        players: &[ContactPlayer; 2],
        checkpoint_byte: u8,
    ) -> Result<Option<CheckpointContactCall>> {
        if self.mode != 1 {
            return Ok(None);
        }
        let Some((slot, word)) = hit else {
            return Ok(None);
        };
        if slot == -1 || word != 0 {
            return Ok(None);
        }
        let p = usize::try_from(slot)
            .ok()
            .and_then(|i| players.get(i))
            .ok_or_else(|| Error::invalid("checkpoint player resolver returned invalid slot"))?;
        let slot = slot as usize;
        self.mode = 2;
        self.slot_timers[slot] = 300;
        Ok(Some(CheckpointContactCall {
            slot,
            position_bits: p.position_bits,
            rotation: p.rotation,
            checkpoint_byte,
        }))
    }
}

#[cfg(test)]
mod contact_tests {
    use super::*;
    #[test]
    fn accepts_only_once_and_selects_the_contacting_player() {
        let players = [
            ContactPlayer {
                position_bits: [0, 1, 2],
                rotation: [3, 4, 5],
            },
            ContactPlayer {
                position_bits: [6, 7, 8],
                rotation: [-9, 10, 11],
            },
        ];
        let mut state = ContactState {
            mode: 1,
            slot_timers: [12, 13],
        };
        for hit in [None, Some((-1, 0)), Some((1, 1))] {
            assert_eq!(state.accept(hit, &players, 255).unwrap(), None);
        }
        assert!(state.accept(Some((2, 0)), &players, 255).is_err());
        assert_eq!(state.slot_timers, [12, 13]);
        assert_eq!(
            state.accept(Some((1, 0)), &players, 255).unwrap(),
            Some(CheckpointContactCall {
                slot: 1,
                position_bits: [6, 7, 8],
                rotation: [-9, 10, 11],
                checkpoint_byte: 255
            })
        );
        assert_eq!((state.mode, state.slot_timers), (2, [12, 300]));
        assert_eq!(state.accept(Some((0, 0)), &players, 89).unwrap(), None);
    }
}

pub use crate::collision_record::CollisionRecord;
/// Stage-authored parent collision template and the two child task offsets.
/// Offsets are model-local; the native initialization transforms them before use.
pub struct CheckpointGeometry {
    pub parent: CollisionRecord,
    pub child_offsets: [[f32; 3]; 2],
    pub parent_sha256: String,
    pub offsets_sha256: String,
}

pub fn city_geometry(module: &[u8]) -> Result<CheckpointGeometry> {
    let rel = crate::city_rel::CityRel::read(module)?;
    let data = rel.data(5)?;
    let shape = data
        .get(0x24d1d4..0x24d204)
        .ok_or_else(|| Error::invalid("missing checkpoint collision"))?;
    let offsets = data
        .get(0x24d244..0x24d25c)
        .ok_or_else(|| Error::invalid("missing checkpoint child offsets"))?;
    Ok(CheckpointGeometry {
        parent: crate::collision_record::collision_record(shape, 0)?,
        child_offsets: std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let at = 12 * i + 4 * j;
                f32::from_be_bytes(offsets[at..at + 4].try_into().unwrap())
            })
        }),
        parent_sha256: crate::manifest::sha256(shape),
        offsets_sha256: crate::manifest::sha256(offsets),
    })
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    #[test]
    #[ignore = "requires installed Steam stage and executable"]
    fn compares_to_pc_checkpoint_geometry() {
        let g = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let exe = std::fs::read(g.join("sonic2app.exe")).unwrap();
        crate::object_table::verify_image(&exe).unwrap();
        let s =
            city_geometry(&std::fs::read(g.join("resource/gd_PC/stg13D.rel")).unwrap()).unwrap();
        let mut bytes = vec![
            s.parent.kind as u8,
            s.parent.form,
            s.parent.push as u8,
            s.parent.damage as u8,
        ];
        bytes.extend(s.parent.attr.to_le_bytes());
        for f in s.parent.center.into_iter().chain(s.parent.parameters) {
            bytes.extend(f.to_le_bytes())
        }
        for a in s.parent.angles {
            bytes.extend(a.to_le_bytes())
        }
        assert_eq!(bytes, &exe[0x71ad80..0x71adb0]);
        let offsets: Vec<_> = s
            .child_offsets
            .into_iter()
            .flatten()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(offsets, &exe[0x71adb8..0x71add0]);
    }
}
