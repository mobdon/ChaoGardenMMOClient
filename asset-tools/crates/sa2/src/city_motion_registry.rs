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

pub struct CityDefaults(Vec<(usize, Slot)>);

impl CityDefaults {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let rel = CityRel::read(bytes)?;
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
                        module: 12,
                        section,
                        offset,
                    }),
                },
            ));
        }
        Err(Error::invalid("unterminated City Escape motion defaults"))
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
        defaults.register(&mut slots);
        assert_eq!(slots.iter().filter(|s| s.address.is_some()).count(), 56);
        assert_eq!(slots[121], foreign);
        assert_eq!(slots[145], slots[147]);
        assert_eq!(slots[145].tag, 2);
        slots[145] = foreign;
        slots[147].tag = 99;
        defaults.register(&mut slots);
        assert_eq!(slots[147].tag, 99);
        defaults.clear_matching(&mut slots);
        assert_eq!(slots[147], Slot::default());
        assert_eq!((slots[121], slots[145]), (foreign, foreign));
        assert_eq!(slots.iter().filter(|s| s.address.is_some()).count(), 2);
    }
}
