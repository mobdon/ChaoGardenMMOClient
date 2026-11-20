//! A3VE flipper input/angle update and integer side-of-flipper test.
//! The main ball collision response is a separate, unported native routine.
use crate::{binary, catalog, sha256};
use std::fmt;

pub const STATE_BYTES: usize = 0x108;
pub const INITIALIZE_OFFSET: usize = 0x2a778;
pub const INITIALIZE_LENGTH: usize = 0x21c;
pub const INITIALIZE_SHA256: &str =
    "8f4b03a036c173bd866cb6b42db80931e08f38500352761ecbee0b4556bc693a";
pub const UPDATE_OFFSET: usize = 0x2ac18;
pub const UPDATE_LENGTH: usize = 0x258;
pub const UPDATE_SHA256: &str = "b87031892b924596255532029cea638f6edb72671aca3647cf216af2da83c576";
pub const SIDE_OFFSET: usize = 0x2ae70;
pub const SIDE_LENGTH: usize = 0x310;
pub const SIDE_SHA256: &str = "dd0a4cfde79c661192950741317d2c7e934fbb6d255770bc411faff9986353a9";
pub const GEOMETRY_OFFSET: usize = 0x6e91c0;
pub const GEOMETRY_LENGTH: usize = 0x168;
pub const GEOMETRY_SHA256: &str =
    "5da92d0db33d0213ce881598ca9127a70c25fd16eac938c588de21068793038e";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Source,
    Geometry,
    Table(u16),
    FlipperCount(u16),
    Angle { index: usize, value: i16 },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub struct Geometry {
    banks: [[[u16; 4]; 13]; 3],
}
impl Geometry {
    pub fn load(rom: &[u8]) -> Result<Self, Error> {
        catalog::validate_rom(rom).map_err(|_| Error::Source)?;
        let bytes =
            binary::span(rom, GEOMETRY_OFFSET, GEOMETRY_LENGTH).map_err(|_| Error::Geometry)?;
        if sha256(bytes) != GEOMETRY_SHA256 {
            return Err(Error::Geometry);
        }
        Self::decode(bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != GEOMETRY_LENGTH {
            return Err(Error::Geometry);
        }
        let mut banks = [[[0; 4]; 13]; 3];
        for (bank_index, bank) in banks.iter_mut().enumerate() {
            for (angle, row) in bank.iter_mut().enumerate() {
                for (column, value) in row.iter_mut().enumerate() {
                    *value = binary::u16_le(bytes, bank_index * 0x80 + angle * 8 + column * 2)
                        .map_err(|_| Error::Geometry)?;
                }
            }
        }
        Ok(Self { banks })
    }
}

/// Native registers/configuration read by ROM 0x2ac18. Names describe their
/// input role; exact IWRAM offsets are retained to prevent adapter assumptions.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    pub table_code: u16,    // 0x03003068
    pub left_mask: u16,     // 0x030040c0 + 4
    pub right_mask: u16,    // 0x030040c0 + 6
    pub held: u16,          // 0x03000510 + 4
    pub pressed: u16,       // 0x03000510 + 6
    pub audio_enabled: u16, // native state + 0x6e2
}

/// Exact writes made by flipper initialization outside its bounded state prefix.
/// A complete host must apply these to the same canonical native state before
/// using the original flipper drawing/attachment routines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeStateWrite {
    pub offset: usize,
    pub width: u8,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flippers {
    bytes: [u8; STATE_BYTES],
}
impl Flippers {
    pub fn from_bytes(bytes: [u8; STATE_BYTES]) -> Self {
        Self { bytes }
    }
    pub fn as_bytes(&self) -> &[u8; STATE_BYTES] {
        &self.bytes
    }
    pub fn as_bytes_mut(&mut self) -> &mut [u8; STATE_BYTES] {
        &mut self.bytes
    }
    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }
    fn i16(&self, at: usize) -> i16 {
        self.u16(at) as i16
    }
    fn put16(&mut self, at: usize, value: u16) {
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// Port of ROM 0x2a778. Coordinates and record resets are native constants;
    /// unspecified fields remain unchanged, including table0/1 pivots FC/104.
    /// Unhandled table codes follow the native default record-reset path.
    pub fn initialize(&mut self, table_code: u16) -> Vec<NativeStateWrite> {
        let mut writes = Vec::new();
        let mut half = |offset: usize, value: u16| {
            writes.push(NativeStateWrite {
                offset,
                width: 2,
                value: u32::from(value),
            })
        };
        match table_code {
            0 | 1 => {
                for (offset, value) in [
                    (0xfa, 355),
                    (0x100, 79),
                    (0x102, 121),
                    (0xfe, 176),
                    (0x106, 190),
                ] {
                    self.put16(offset, value);
                }
                for (offset, value) in [
                    (0x10a, 349),
                    (0x110, 71),
                    (0x112, 113),
                    (0x10e, 166),
                    (0x116, 182),
                ] {
                    half(offset, value);
                }
                self.put16(0xf8, 4);
                writes.push(NativeStateWrite {
                    offset: 0x1d8,
                    width: 4,
                    value: 0x0866a0b8,
                });
                writes.push(NativeStateWrite {
                    offset: 0x1dc,
                    width: 4,
                    value: 0x0866a0b8,
                });
            }
            2 => {
                for (offset, value) in [
                    (0xfa, 370),
                    (0x100, 82),
                    (0x102, 126),
                    (0xfc, 180),
                    (0x104, 28),
                    (0xfe, 252),
                    (0x106, 197),
                ] {
                    self.put16(offset, value);
                }
                for (offset, value) in [
                    (0x10a, 363),
                    (0x110, 78),
                    (0x112, 122),
                    (0x10c, 164),
                    (0x114, 24),
                    (0x10e, 236),
                    (0x116, 193),
                ] {
                    half(offset, value);
                }
                self.put16(0xf8, 4);
                writes.push(NativeStateWrite {
                    offset: 0x1d8,
                    width: 4,
                    value: 0x0866a0b8,
                });
                writes.push(NativeStateWrite {
                    offset: 0x1dc,
                    width: 4,
                    value: 0x0866a0b8,
                });
            }
            3 => {
                for (offset, value) in [(0xfa, 284), (0x100, 78), (0x102, 122)] {
                    self.put16(offset, value);
                }
                for (offset, value) in [(0x10a, 276), (0x110, 70), (0x112, 114)] {
                    half(offset, value);
                }
                self.put16(0xf8, 2);
                writes.push(NativeStateWrite {
                    offset: 0x1d8,
                    width: 4,
                    value: 0x0866a0b8,
                });
            }
            _ => {}
        }
        for index in 0..4 {
            let record = 0xc4 + index * 12;
            self.put16(record, 0);
            self.put16(record + 2, 0);
            self.bytes[record + 8] = 0;
            self.bytes[record + 4] = 0;
            self.bytes[record + 9] = 0;
        }
        writes
    }

    /// Port of 0x2ac18 and 0x2ae70. All geometry indices and the four native
    /// flipper records are checked before mutation. Counts with a signed
    /// negative native representation skip the loop, matching the original.
    /// Returns each native sound ID passed to routine 0x080038d4; the caller
    /// must handle those calls before claiming presentation/audio parity.
    pub fn update(&mut self, input: Inputs, geometry: &Geometry) -> Result<Vec<u16>, Error> {
        if input.table_code > 3 {
            return Err(Error::Table(input.table_code));
        }
        let count = self.u16(0xf8);
        let count = if (count as i16) < 0 {
            0
        } else {
            usize::from(count)
        };
        if count > 4 {
            return Err(Error::FlipperCount(count as u16));
        }
        for index in 0..count {
            let angle = self.i16(0xc4 + index * 12);
            if !(0..=12).contains(&angle) {
                return Err(Error::Angle {
                    index,
                    value: angle,
                });
            }
        }
        let mut sounds = Vec::new();
        for index in 0..count {
            let record = 0xc4 + index * 12;
            self.side(index, input.table_code, geometry);
            self.put16(record + 2, self.u16(record));
            self.bytes[record + 4] = 0;
            let left = if input.table_code <= 1 {
                index == 0
            } else {
                index.is_multiple_of(2)
            };
            let mask = if left {
                input.left_mask
            } else {
                input.right_mask
            };
            if input.pressed & mask != 0 && input.audio_enabled != 0 {
                sounds.push(0x39);
            }
            if input.held & mask != 0 {
                self.bytes[record + 8] = 1;
                self.bytes[record + 9] = 0;
            } else if self.i16(record) == 12 {
                if (self.bytes[record + 9] as i8) > 2 {
                    self.bytes[record + 8] = 0;
                }
                self.bytes[record + 9] = self.bytes[record + 9].wrapping_add(1);
            }
            let moving: i16;
            if self.bytes[record + 8] != 0 {
                moving = i16::from(self.i16(record) != 12);
                self.bytes[record + 4] = (self.i16(record) / 2 + 1) as u8;
                self.put16(record, self.u16(record).wrapping_add(4));
            } else {
                moving = if self.i16(record) != 0 {
                    self.bytes[record + 4] = (self.i16(record) / 2 + 7) as u8;
                    -1
                } else {
                    0
                };
                self.put16(record, self.u16(record).wrapping_sub(2));
            }
            let product = (i16::from(self.bytes[record + 5] as i8) * moving) as u8;
            self.bytes[record + 5] = product;
            if (product as i8) <= 0 {
                self.bytes[record + 6] = 0;
            }
            self.bytes[record + 5] = moving as u8;
            let angle = self.i16(record).clamp(0, 12);
            self.put16(record, angle as u16);
        }
        Ok(sounds)
    }

    fn side(&mut self, index: usize, table_code: u16, geometry: &Geometry) {
        let angle = self.u16(0xc4 + index * 12) as usize;
        let bank = match index {
            0 | 1 => 0,
            2 => 2,
            3 if table_code == 1 => 1,
            _ => 2,
        };
        let row = geometry.banks[bank][angle];
        let pivot_x = self.u16(0x100 + index * 2);
        let pivot_y = self.u16(0xfa + (if index < 2 { 0 } else { index - 1 }) * 2);
        let (x0, x1) = if index.is_multiple_of(2) {
            (pivot_x.wrapping_add(row[0]), pivot_x.wrapping_add(row[2]))
        } else {
            (
                pivot_x.wrapping_add(31).wrapping_sub(row[0]),
                pivot_x.wrapping_add(31).wrapping_sub(row[2]),
            )
        };
        let y_offset = if index == 2 { 8 } else { 0 };
        let y0 = pivot_y.wrapping_add(row[1]).wrapping_sub(y_offset);
        let y1 = pivot_y.wrapping_add(row[3]).wrapping_sub(y_offset);
        let x0 = i32::from(x0 as i16);
        let x1 = i32::from(x1 as i16);
        let y0 = i32::from(y0 as i16);
        let y1 = i32::from(y1 as i16);
        let ball_x = i32::from(self.i16(0x98));
        let ball_y = i32::from(self.i16(0x9a));
        let numerator = (y1 - y0).wrapping_mul(ball_x - x0).wrapping_mul(30);
        let value = super::math::signed_divide(numerator, x1 - x0)
            .wrapping_add((y0 - ball_y).wrapping_mul(30));
        let side = if (value as i16) < 0 { 0xff } else { 1 };
        self.bytes[0xce + index * 12] = if self.u16(0x6e) == 10 { 1 } else { side };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn bytes(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0);
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    fn check_cases(vectors: &Value, geometry: &Geometry) {
        let initialize_cases = vectors["initialize_cases"].as_array().unwrap();
        assert_eq!(initialize_cases.len(), 24);
        for (index, case) in initialize_cases.iter().enumerate() {
            let mut actual =
                Flippers::from_bytes(bytes(case["input"].as_str().unwrap()).try_into().unwrap());
            let writes = actual.initialize(case["inputs"]["table_code"].as_u64().unwrap() as u16);
            assert_eq!(
                actual.as_bytes().as_slice(),
                bytes(case["state"].as_str().unwrap()),
                "initialize state case {index}"
            );
            assert_eq!(
                writes
                    .into_iter()
                    .map(|write| [
                        write.offset as u64,
                        u64::from(write.width),
                        u64::from(write.value)
                    ])
                    .collect::<Vec<_>>(),
                case["outer_writes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|write| {
                        let values = write.as_array().unwrap();
                        [
                            values[0].as_u64().unwrap(),
                            values[1].as_u64().unwrap(),
                            values[2].as_u64().unwrap(),
                        ]
                    })
                    .collect::<Vec<_>>(),
                "initialize external write case {index}"
            );
            assert!(
                case["sound_calls"].as_array().unwrap().is_empty(),
                "initialize sound case {index}"
            );
        }
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (index, case) in cases.iter().enumerate() {
            let mut actual =
                Flippers::from_bytes(bytes(case["input"].as_str().unwrap()).try_into().unwrap());
            let input = &case["inputs"];
            let value = |key: &str| input[key].as_u64().unwrap() as u16;
            let sounds = actual
                .update(
                    Inputs {
                        table_code: value("table_code"),
                        left_mask: value("left_mask"),
                        right_mask: value("right_mask"),
                        held: value("held"),
                        pressed: value("pressed"),
                        audio_enabled: value("audio_enabled"),
                    },
                    geometry,
                )
                .unwrap();
            assert_eq!(
                actual.as_bytes().as_slice(),
                bytes(case["state"].as_str().unwrap()),
                "state case {index}"
            );
            assert_eq!(
                sounds,
                case["sound_calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>(),
                "sound case {index}"
            );
        }
    }

    #[test]
    fn matches_256_original_instruction_cases_with_synthetic_geometry() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/flipper-vectors.json")).unwrap();
        let geometry =
            Geometry::decode(&bytes(vectors["synthetic_geometry"].as_str().unwrap())).unwrap();
        check_cases(&vectors, &geometry);
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; exact native flipper geometry"]
    fn matches_256_original_instruction_cases_with_native_geometry() {
        let rom = std::fs::read(
            std::env::var_os("SPP_ROM").expect("Set SPP_ROM to the supplied local A3VE ROM"),
        )
        .unwrap();
        let geometry = Geometry::load(&rom).unwrap();
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/flipper-native-vectors.json")).unwrap();
        check_cases(&vectors, &geometry);
    }

    #[test]
    fn rejects_unproven_indices_before_mutation() {
        let geometry = Geometry::decode(&[0; GEOMETRY_LENGTH]).unwrap();
        let input = Inputs {
            table_code: 0,
            left_mask: 1,
            right_mask: 2,
            held: 0,
            pressed: 0,
            audio_enabled: 0,
        };
        let mut state = Flippers::from_bytes([0; STATE_BYTES]);
        state.put16(0xf8, 5);
        let before = state.clone();
        assert_eq!(state.update(input, &geometry), Err(Error::FlipperCount(5)));
        assert_eq!(state, before);
        state.put16(0xf8, 1);
        state.put16(0xc4, 13);
        let before = state.clone();
        assert_eq!(
            state.update(input, &geometry),
            Err(Error::Angle {
                index: 0,
                value: 13
            })
        );
        assert_eq!(state, before);
        assert_eq!(
            state.update(
                Inputs {
                    table_code: 4,
                    ..input
                },
                &geometry
            ),
            Err(Error::Table(4))
        );
        assert_eq!(state, before);
        assert!(matches!(Geometry::load(&[0; 4]), Err(Error::Source)));
        assert!(matches!(
            Geometry::decode(&[0; GEOMETRY_LENGTH - 1]),
            Err(Error::Geometry)
        ));
    }
}
