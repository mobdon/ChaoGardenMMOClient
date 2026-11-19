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
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactPlayer {
    pub position_bits: [u32; 3],
    pub rotation: [i32; 3],
}

/// Arguments to module zero's 0x8001c9f4 at City Escape text +0x447a4.
/// The implementation of that engine call is not part of the stage REL.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
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

/// City Escape checkpoint placement angles. SET yaw packs a work byte in
/// its low eight bits; those bits never rotate either child.
#[derive(Debug, Clone, Copy)]
pub struct CheckpointPlacement {
    pub rotation: [i32; 3],
}

impl CheckpointPlacement {
    /// In the initialization branch at text +0x43d88, arguments are the
    /// observed engine return and stage byte, whose meanings are unresolved.
    pub fn work_byte(self, engine_result: u32, stage_byte: i8) -> u8 {
        if engine_result <= 1 && stage_byte != 0 {
            self.rotation[1] as u8
        } else {
            0
        }
    }

    /// The two child task angle writes at text +0x44628..0x446d4.
    /// Input is the checkpoint work's three signed 32-bit angle words.
    pub fn child_angles(self, work: [i32; 3]) -> [[i32; 3]; 2] {
        let [x, y, z] = self.rotation;
        let x = work[0].wrapping_add(x);
        let z = work[2].wrapping_add(z);
        let y = y & 0xff00;
        [
            [x, y.wrapping_sub(work[1]), z],
            [x, y.wrapping_add(work[1]), z],
        ]
    }
}

/// Contact and spin portion of an initialized City Escape checkpoint callback.
/// Range handling, collision queries, sounds, rewards and respawn remain with
/// their respective systems. The contact speed is the result of REL 0x44bfc.
pub struct CheckpointTicks {
    pub contact: ContactState,
    pub spin: CheckpointSpin,
}

impl CheckpointTicks {
    pub fn tick(
        &mut self,
        hit: Option<(i32, u32)>,
        players: &[ContactPlayer; 2],
        checkpoint_byte: u8,
        contact_speed: f32,
    ) -> Result<Option<CheckpointContactCall>> {
        self.tick_with_speed(hit, players, checkpoint_byte, |_| contact_speed)
    }

    /// Resolve the stage's external magnitude and angle helpers only after
    /// acceptance. The player work vector lives at +0x64; task yaw is +0xc.
    pub fn tick_with_approach(
        &mut self,
        hit: Option<(i32, u32)>,
        players: &[ContactPlayer; 2],
        placement: CheckpointPlacement,
        checkpoint_byte: u8,
        magnitude: impl FnOnce(usize) -> f32,
        difference: impl FnOnce(i32, i32) -> i32,
    ) -> Result<Option<CheckpointContactCall>> {
        self.tick_with_speed(hit, players, checkpoint_byte, |call| {
            let speed = magnitude(call.slot);
            let angle = difference(
                0x4000_i32.wrapping_sub(call.rotation[1]),
                placement.rotation[1] & 0xff00,
            );
            if ((angle as f32).abs() as i32) > 0x4000 {
                -speed
            } else {
                speed
            }
        })
    }

    fn tick_with_speed(
        &mut self,
        hit: Option<(i32, u32)>,
        players: &[ContactPlayer; 2],
        checkpoint_byte: u8,
        contact_speed: impl FnOnce(&CheckpointContactCall) -> f32,
    ) -> Result<Option<CheckpointContactCall>> {
        match self.contact.mode {
            1 => {
                let old_timers = self.contact.slot_timers;
                let call = self.contact.accept(hit, players, checkpoint_byte)?;
                if let Some(accepted) = &call {
                    let contact_speed = contact_speed(accepted);
                    let scaled = (10.0_f32 * contact_speed) * f32::from_bits(0x43360b61);
                    if !scaled.is_finite() || !(-2147483648.0..2147483648.0).contains(&scaled) {
                        self.contact.mode = 1;
                        self.contact.slot_timers = old_timers;
                        return Err(Error::invalid(
                            "checkpoint speed outside verified integer conversion",
                        ));
                    }
                    self.spin.steps[1] = scaled.trunc() as i32 as i16;
                }
                Ok(call)
            }
            2 | 3 => {
                if self.contact.mode == 2 && self.spin.advance() {
                    self.contact.mode = 3;
                }
                for timer in &mut self.contact.slot_timers {
                    if *timer > 0 {
                        *timer -= 1;
                    }
                }
                Ok(None)
            }
            _ => Err(Error::invalid(
                "expected an initialized active checkpoint mode",
            )),
        }
    }
}

#[cfg(test)]
mod tick_tests {
    use super::*;
    #[test]
    fn contact_spin_and_timer_use_entry_mode() {
        let players = std::array::from_fn(|_| ContactPlayer {
            position_bits: [0; 3],
            rotation: [0; 3],
        });
        let mut state = CheckpointTicks {
            contact: ContactState {
                mode: 1,
                slot_timers: [-1, 0],
            },
            spin: CheckpointSpin {
                angles: [0; 2],
                steps: [0; 2],
                counters: [0; 2],
            },
        };
        assert!(state.tick(Some((1, 0)), &players, 1, f32::NAN).is_err());
        assert_eq!(
            (state.contact.mode, state.contact.slot_timers),
            (1, [-1, 0])
        );
        assert!(state
            .tick(Some((1, 0)), &players, 1, 0.001)
            .unwrap()
            .is_some());
        assert_eq!(
            (state.spin.steps, state.contact.slot_timers),
            ([0, 1], [-1, 300])
        );
        assert!(state
            .tick(Some((0, 0)), &players, 1, f32::NAN)
            .unwrap()
            .is_none());
        assert_eq!(
            (
                state.contact.mode,
                state.spin.angles,
                state.contact.slot_timers
            ),
            (3, [0, 1], [-1, 299])
        );
        for _ in 0..300 {
            state.tick(None, &players, 1, 0.).unwrap();
        }
        assert_eq!(state.contact.slot_timers, [-1, 0]);
    }
}

#[cfg(test)]
mod approach_tests {
    use super::*;
    #[test]
    fn sign_changes_only_beyond_a_quarter_turn() {
        let players = std::array::from_fn(|_| ContactPlayer {
            position_bits: [0; 3],
            rotation: [0, -123, 0],
        });
        for (angle, step) in [
            (16384, 1820),
            (-16384, 1820),
            (16385, -1820),
            (-16385, -1820),
        ] {
            let mut state = CheckpointTicks {
                contact: ContactState {
                    mode: 1,
                    slot_timers: [0; 2],
                },
                spin: CheckpointSpin {
                    angles: [0; 2],
                    steps: [0; 2],
                    counters: [0; 2],
                },
            };
            let placement = CheckpointPlacement {
                rotation: [0, 0x8003, 0],
            };
            state
                .tick_with_approach(None, &players, placement, 3, |_| panic!(), |_, _| panic!())
                .unwrap();
            state
                .tick_with_approach(
                    Some((1, 0)),
                    &players,
                    placement,
                    3,
                    |slot| {
                        assert_eq!(slot, 1);
                        1.0
                    },
                    |a, b| {
                        assert_eq!((a, b), (0x4000 + 123, 0x8000));
                        angle
                    },
                )
                .unwrap();
            assert_eq!(state.spin.steps[1], step);
        }
    }
}

/// Stage-local item delivery table (11 pairs in REL section 5).
/// Offsets refer to section 1 of the identified REL, not PC executable addresses.
#[derive(Debug)]
pub struct StageItemTable {
    dispatcher: u32,
    entries: [(i32, u32); 11],
}

#[derive(Debug, PartialEq, Eq)]
pub struct ItemDispatch {
    /// Arguments for the first direct call at text +0x30724.
    pub notify_args: [i32; 2],
    pub handler_offset: u32,
    /// Arguments for the subsequent indirect item call.
    pub handler_args: [i32; 2],
}

pub type CityItemTable = StageItemTable;

impl StageItemTable {
    pub fn read(module: &[u8]) -> Result<Self> {
        Self::read_stage(13, module)
    }

    pub fn read_stage(level: u16, module: &[u8]) -> Result<Self> {
        let layouts: &[(u16, usize, u32)] = include!("checkpoint_item_layouts.rs");
        let &(_, table, dispatcher) = layouts
            .iter()
            .find(|r| r.0 == level)
            .ok_or_else(|| Error::invalid("unsupported checkpoint stage"))?;
        let filename = format!("stg{level:02}D.rel");
        let info = crate::stage_land::catalog()
            .iter()
            .find(|r| r.filename == filename)
            .ok_or_else(|| Error::invalid("missing stage identity"))?;
        if info.size != module.len() || info.sha256 != crate::manifest::sha256(module) {
            return Err(Error::invalid("unsupported checkpoint REL revision"));
        }
        let rel = crate::city_rel::CityRel::read_identified(module, info.module)?;
        let data = rel.data(5)?;
        let code = rel.data(1)?;
        let mut entries = [(0, 0); 11];
        for (i, row) in entries.iter_mut().enumerate() {
            let at = table + i * 8;
            let id = i32::from_be_bytes(
                data.get(at..at + 4)
                    .ok_or_else(|| Error::invalid("truncated item ID"))?
                    .try_into()
                    .unwrap(),
            );
            let link = rel
                .reference(5, (at + 4) as u32)
                .ok_or_else(|| Error::invalid("missing item handler relocation"))?;
            if (link.module, link.section, link.kind) != (info.module, 1, 1)
                || link.addend % 4 != 0
                || code.get(link.addend as usize..).is_none_or(|s| s.len() < 4)
            {
                return Err(Error::invalid("invalid item handler target"));
            }
            *row = (id, link.addend);
        }
        Ok(Self {
            entries,
            dispatcher,
        })
    }

    /// Mirrors the item helper's early exits and first matching ID. Returns
    /// the two calls in order; does not apply the handler's gameplay effects.
    pub fn dispatch(&self, slot: usize, equipment: u16, item: i32) -> Result<Option<ItemDispatch>> {
        if slot >= 2 {
            return Err(Error::invalid("invalid item player slot"));
        }
        if equipment & 0x4000 != 0 || item == 7 {
            return Ok(None);
        }
        Ok(self
            .entries
            .iter()
            .find(|(id, _)| *id == item)
            .map(|&(id, handler_offset)| ItemDispatch {
                notify_args: [slot as i32, id],
                handler_offset,
                handler_args: [0, slot as i32],
            }))
    }
}

#[cfg(test)]
mod item_tests {
    use super::*;
    #[test]
    #[ignore = "requires installed stage modules"]
    fn installed_stage_item_tables() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap())
            .join("resource/gd_PC");
        let layouts: &[(u16, usize, u32)] = include!("checkpoint_item_layouts.rs");
        for &(level, _, _) in layouts {
            let bytes = std::fs::read(root.join(format!("stg{level:02}D.rel"))).unwrap();
            let table = StageItemTable::read_stage(level, &bytes)
                .unwrap_or_else(|e| panic!("stage {level}: {e}"));
            for item in [0, 1, 2, 3, 6, 8] {
                assert!(table.visit_checkpoint_item(0, 0, item, |_| {}).unwrap());
            }
        }
    }
    #[test]
    #[ignore = "requires installed City Escape module"]
    fn installed_item_dispatch_targets() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let t =
            CityItemTable::read(&std::fs::read(root.join("resource/gd_PC/stg13D.rel")).unwrap())
                .unwrap();
        for (id, offset) in [
            (0, 0x303c0),
            (1, 0x30520),
            (2, 0x30498),
            (3, 0x30570),
            (4, 0x305a8),
            (5, 0x305e4),
            (6, 0x30628),
            (8, 0x304e8),
            (9, 0x30664),
            (10, 0x30410),
        ] {
            assert_eq!(
                t.dispatch(1, 0, id).unwrap(),
                Some(ItemDispatch {
                    notify_args: [1, id],
                    handler_offset: offset,
                    handler_args: [0, 1]
                })
            );
            assert_eq!(t.dispatch(1, 0x4000, id).unwrap(), None);
        }
        for (id, points, amount, scalar, last) in [
            (0, 100, Some(10), Some(0.5f32), 0x80035a88),
            (1, 200, Some(20), Some(1.0), 0x80035a88),
            (2, 50, Some(5), Some(0.25), 0x80035a88),
            (3, 200, None, None, 0x80037278),
            (6, 200, None, None, 0x80037394),
            (8, 200, None, None, 0x800374b0),
        ] {
            let mut calls = Vec::new();
            assert!(t
                .visit_checkpoint_item(1, 0, id, |c| calls.push(c))
                .unwrap());
            assert_eq!(
                calls[0],
                ItemEngineCall {
                    target: 0x80063524,
                    r3: points,
                    r4: None,
                    f1_bits: None
                }
            );
            assert_eq!(calls.len(), if amount.is_some() { 3 } else { 2 });
            if amount.is_some() {
                assert_eq!(
                    calls[1],
                    ItemEngineCall {
                        target: 0x80063584,
                        r3: 1,
                        r4: amount,
                        f1_bits: None
                    }
                );
            }
            assert_eq!(
                calls.last(),
                Some(&ItemEngineCall {
                    target: last,
                    r3: 1,
                    r4: None,
                    f1_bits: scalar.map(f32::to_bits)
                })
            );
            assert!(!t
                .visit_checkpoint_item(1, 0x4000, id, |_| panic!())
                .unwrap());
        }
        for id in [-1, 7, 11] {
            assert_eq!(t.dispatch(0, 0, id).unwrap(), None);
        }
        assert!(t.dispatch(2, 0, 1).is_err());
    }
}

/// Arguments explicitly set by a stage item handler before it calls the
/// external engine. Float arguments retain their original binary32 bits.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct ItemEngineCall {
    pub target: u32,
    pub r3: i32,
    pub r4: Option<i32>,
    pub f1_bits: Option<u32>,
}

impl StageItemTable {
    /// Execute the stage-local part of the six handlers reachable from
    /// checkpoint bonus selection. A host supplies the external engine calls.
    pub fn visit_checkpoint_item(
        &self,
        slot: usize,
        equipment: u16,
        item: i32,
        mut visit: impl FnMut(ItemEngineCall),
    ) -> Result<bool> {
        let Some(dispatch) = self.dispatch(slot, equipment, item)? else {
            return Ok(false);
        };
        let player = dispatch.handler_args[1];
        // Verified distance back from the dispatcher to each handler entry.
        let (points, amount, scalar, target) =
            match self.dispatcher.checked_sub(dispatch.handler_offset) {
                Some(0x2dc) => (100, Some(10), Some(0x3f000000), 0x80035a88),
                Some(0x17c) => (200, Some(20), Some(0x3f800000), 0x80035a88),
                Some(0x204) => (50, Some(5), Some(0x3e800000), 0x80035a88),
                Some(0x12c) => (200, None, None, 0x80037278),
                Some(0x74) => (200, None, None, 0x80037394),
                Some(0x1b4) => (200, None, None, 0x800374b0),
                _ => return Err(Error::invalid("unsupported checkpoint item handler")),
            };
        visit(ItemEngineCall {
            target: 0x80063524,
            r3: points,
            r4: None,
            f1_bits: None,
        });
        if let Some(n) = amount {
            visit(ItemEngineCall {
                target: 0x80063584,
                r3: player,
                r4: Some(n),
                f1_bits: None,
            });
        }
        visit(ItemEngineCall {
            target,
            r3: player,
            r4: None,
            f1_bits: scalar,
        });
        Ok(true)
    }
}
