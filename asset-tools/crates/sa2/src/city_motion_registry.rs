//! Slot writes performed by the City Escape motion registration and cleanup loops.
//! Player reset callbacks which precede cleanup are the caller's responsibility.
use crate::{city_rel::CityRel, Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionAddress {
    Stage {
        module: u32,
        section: u8,
        offset: u32,
    },
    External(u64),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Slot {
    pub tag: i16,
    pub address: Option<MotionAddress>,
}

/// Stage-local call sites, relative to executable REL section 1.
#[derive(Clone, Copy, Debug)]
pub enum RegistryCall {
    PrologRegister,
    EpilogClearAfterPlayerReset,
}

impl RegistryCall {
    fn instruction(self) -> (usize, usize) {
        match self {
            Self::PrologRegister => (0x11c5c, 0x116c4),
            Self::EpilogClearAfterPlayerReset => (0x11e54, 0x11738),
        }
    }
}

/// Host supplies the two main-executable callbacks and live player work.
pub trait ExitPlayerWork {
    fn present(&self, slot: usize) -> bool;
    fn reset_motion(&mut self, slot: usize) -> Result<()>;
    fn player_number(&self, slot: usize) -> i8;
    fn motion_flags(&mut self, slot: usize) -> &mut u16;
    fn refresh_player(&mut self, slot: usize) -> Result<()>;
}

pub struct CityDefaults(Vec<(usize, Slot)>);

impl CityDefaults {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let rel = CityRel::read(bytes)?;
        let code = rel.data(1)?;
        for call in [
            RegistryCall::PrologRegister,
            RegistryCall::EpilogClearAfterPlayerReset,
        ] {
            let (at, target) = call.instruction();
            let word = u32::from_be_bytes(code[at..at + 4].try_into().unwrap());
            let delta = ((word & 0x03ff_fffc) << 6) as i32 >> 6;
            if word & 0xfc00_0003 != 0x4800_0001 || at as i64 + i64::from(delta) != target as i64 {
                return Err(Error::invalid(
                    "unrecognized City Escape motion lifecycle call",
                ));
            }
        }
        for (at, target) in [
            (0x11774, 0x8003d7d4),
            (0x117b4, 0x8005523c),
            (0x117d0, 0x8003d7d4),
            (0x11810, 0x8005523c),
        ] {
            let r = rel
                .reference(1, at)
                .ok_or_else(|| Error::invalid("missing exit player callback"))?;
            if (r.module, r.kind, r.addend) != (0, 10, target) {
                return Err(Error::invalid("unexpected exit player callback"));
            }
        }
        let mut entries = Vec::new();
        let start = 0x1fd2b0;
        for (i, row) in rel.data(5)?[start..].chunks_exact(8).enumerate() {
            let index = i16::from_be_bytes([row[0], row[1]]);
            if index == -1 {
                return Ok(Self(entries));
            }
            if !(0..300).contains(&index) {
                continue;
            }
            let (section, offset) = rel
                .local_pointer(5, (start + i * 8 + 4) as u32)?
                .ok_or_else(|| Error::invalid("missing City Escape motion default"))?;
            entries.push((
                index as usize,
                Slot {
                    tag: i16::from_be_bytes([row[2], row[3]]),
                    address: Some(MotionAddress::Stage {
                        module: rel.module_id(),
                        section,
                        offset,
                    }),
                },
            ));
        }
        Err(Error::invalid("unterminated City Escape motion defaults"))
    }

    /// Apply the registry portion when the stage reaches the matching call site.
    pub fn apply_call(&self, call: RegistryCall, slots: &mut [Slot; 300]) {
        match call {
            RegistryCall::PrologRegister => self.register(slots),
            RegistryCall::EpilogClearAfterPlayerReset => self.clear_matching(slots),
        }
    }

    /// Reset each present player before removing stage defaults. The target of
    /// the flag write is read after reset; the refresh still uses the outer slot.
    pub fn exit_with_players(
        &self,
        slots: &mut [Slot; 300],
        work: &mut impl ExitPlayerWork,
    ) -> Result<()> {
        for slot in 0..2 {
            if !work.present(slot) {
                continue;
            }
            work.reset_motion(slot)?;
            let target = work.player_number(slot);
            if !(0..2).contains(&target) || !work.present(target as usize) {
                return Err(Error::invalid("unavailable City Escape player flag target"));
            }
            *work.motion_flags(target as usize) |= 4;
            work.refresh_player(slot)?;
        }
        self.clear_matching(slots);
        Ok(())
    }

    pub fn register(&self, slots: &mut [Slot; 300]) {
        for &(index, default) in &self.0 {
            if slots[index].address.is_none() {
                slots[index] = default;
            }
        }
    }

    pub fn clear_matching(&self, slots: &mut [Slot; 300]) {
        for &(index, default) in &self.0 {
            if slots[index].address == default.address {
                slots[index] = Slot::default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires installed SA2_SET_DIR"]
    fn preserves_replacements_and_clears_matching_aliases() {
        let path =
            std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap()).join("stg13D.rel");
        let defaults = CityDefaults::read(&std::fs::read(path).unwrap()).unwrap();
        let mut slots = [Slot::default(); 300];
        let foreign = Slot {
            tag: 99,
            address: Some(MotionAddress::External(7)),
        };
        slots[121] = foreign;
        defaults.apply_call(RegistryCall::PrologRegister, &mut slots);
        assert_eq!(slots.iter().filter(|s| s.address.is_some()).count(), 56);
        assert_eq!(slots[121], foreign);
        assert_eq!(slots[145], slots[147]);
        assert_eq!(slots[145].tag, 2);
        slots[145] = foreign;
        slots[147].tag = 99;
        defaults.apply_call(RegistryCall::PrologRegister, &mut slots);
        assert_eq!(slots[147].tag, 99);
        defaults.apply_call(RegistryCall::EpilogClearAfterPlayerReset, &mut slots);
        assert_eq!(slots[147], Slot::default());
        assert_eq!((slots[121], slots[145]), (foreign, foreign));
        assert_eq!(slots.iter().filter(|s| s.address.is_some()).count(), 2);
    }
}

#[cfg(test)]
mod exit_tests {
    use super::*;
    #[derive(Default)]
    struct Work {
        flags: [u16; 2],
        calls: Vec<(usize, bool)>,
        numbers: [i8; 2],
    }
    impl ExitPlayerWork for Work {
        fn present(&self, _: usize) -> bool {
            true
        }
        fn reset_motion(&mut self, s: usize) -> Result<()> {
            self.calls.push((s, false));
            self.numbers[s] = 1 - s as i8;
            Ok(())
        }
        fn player_number(&self, s: usize) -> i8 {
            self.numbers[s]
        }
        fn motion_flags(&mut self, s: usize) -> &mut u16 {
            &mut self.flags[s]
        }
        fn refresh_player(&mut self, s: usize) -> Result<()> {
            assert_ne!(self.flags[1 - s] & 4, 0);
            self.calls.push((s, true));
            Ok(())
        }
    }
    #[test]
    fn exit_uses_post_reset_player_number_then_refreshes_original_slot() {
        let entry = Slot {
            tag: 2,
            address: Some(MotionAddress::External(11)),
        };
        let defaults = CityDefaults(vec![(145, entry)]);
        let mut slots = [Slot::default(); 300];
        defaults.register(&mut slots);
        let mut work = Work {
            flags: [8, 16],
            ..Default::default()
        };
        defaults.exit_with_players(&mut slots, &mut work).unwrap();
        assert_eq!(work.flags, [12, 20]);
        assert_eq!(work.calls, [(0, false), (0, true), (1, false), (1, true)]);
        assert_eq!(slots[145], Slot::default());
    }
}
