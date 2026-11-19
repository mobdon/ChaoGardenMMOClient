//! Source translations of A3VE's main-ball reset and fixed-point integrator.
//! Collision, flippers, launch-state dispatch and drawing are separate native
//! routines. Calling this integrator alone is not a playable pinball engine.

pub const STATE_BYTES: usize = 0xc4;
pub const INITIALIZE_OFFSET: usize = 0x29a40;
pub const INITIALIZE_LENGTH: usize = 0x168;
pub const INITIALIZE_SHA256: &str =
    "209276c2813b578ac79743dbb4ce212b769b846c532d9ddf9349c8cc20099ef6";
pub const INTEGRATE_OFFSET: usize = 0x2a04c;
pub const INTEGRATE_LENGTH: usize = 0x138;
pub const INTEGRATE_SHA256: &str =
    "fcbf0d803980db7bdde77b122d12d0b321095e17b05addf69e68233d8ffda34e";
pub const TRAIL_RESET_OFFSET: usize = 0x2a4d0;
pub const TRAIL_RESET_LENGTH: usize = 0x8c;
pub const TRAIL_RESET_SHA256: &str =
    "d6b43ba7a8e7790794aaf23b2c8baa3dd11b5fd95b0a266f9d3ab0bbffbd3542";
pub const PROJECT_OFFSET: usize = 0x29c90;
pub const PROJECT_LENGTH: usize = 0xf8;
pub const PROJECT_SHA256: &str = "254e95a868038aef1f099e44de329cc7c34fd2ead9cc04497d7b6aafbd8abd39";
pub const VELOCITY_LIMIT_OFFSET: usize = 0x2d698;
pub const VELOCITY_LIMIT_LENGTH: usize = 0x74;
pub const VELOCITY_LIMIT_SHA256: &str =
    "fada3d2e88e4369914d4ef42710822543d80cd1bda6f5ba2be9273dff77af366";

/// The bounded prefix of the native state referenced by IWRAM 0x030040b0.
/// Uninterpreted bytes are retained, including fields not written by a routine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainBall {
    bytes: [u8; STATE_BYTES],
}

impl Default for MainBall {
    fn default() -> Self {
        Self {
            bytes: [0; STATE_BYTES],
        }
    }
}

impl MainBall {
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
    fn put_u16(&mut self, at: usize, value: u16) {
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn i32(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.bytes[at..at + 4].try_into().expect("fixed field"))
    }
    fn put_i32(&mut self, at: usize, value: i32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    /// Native coordinate units; initial coordinates are converted to screen
    /// coordinates by a distinct native rendering path, not by this method.
    pub fn native_position(&self) -> (i16, i16) {
        (self.i16(0xa0), self.i16(0xa2))
    }
    pub fn screen_position(&self) -> (i16, i16) {
        (self.i16(0x98), self.i16(0x9a))
    }
    pub fn fixed_position(&self) -> (i32, i32) {
        (self.i32(0x90), self.i32(0x94))
    }
    pub fn velocity(&self) -> (i16, i16) {
        (self.i16(0xa8), self.i16(0xaa))
    }

    /// Native velocity-vector limit, ROM 0x2d698..0x2d70c. BIOS Sqrt/ArcTan2
    /// remain explicit dependencies. A missing BIOS implementation returns an
    /// error before any velocity mutation; no host floating-point substitute
    /// or guessed direction is used.
    pub fn limit_velocity(
        &mut self,
        table: &super::math::TrigTable,
        bios: &mut impl super::math::BiosMath,
    ) -> Result<bool, super::math::MathError> {
        let (vx, vy) = self.velocity();
        let x = i32::from(vx);
        let y = i32::from(vy);
        let squared = x.wrapping_mul(x).wrapping_add(y.wrapping_mul(y)) as u32;
        if bios.sqrt(squared)? <= 512 {
            return Ok(false);
        }
        let angle = bios.atan2(vx, vy.wrapping_neg())?;
        let x = super::math::signed_divide(i32::from(table.cosine(u32::from(angle))) * 512, 20000);
        let y = super::math::signed_divide(i32::from(table.sine(u32::from(angle))) * -512, 20000);
        self.put_u16(0xa8, x as u16);
        self.put_u16(0xaa, y as u16);
        Ok(true)
    }

    /// Coordinate prefix of native drawing, ROM 0x29c90..0x29d88. The caller
    /// must separately handle the original early gate at 0x29ff0 and auxiliary
    /// path at 0x29cfe. `bottom_extent` is IWRAM halfword 0x030040e0.
    /// Returned coordinates are the 16x16 object's top-left in the 240x160
    /// viewport, and may be outside that viewport. This method does no clipping.
    pub fn project_draw_coordinates(
        &mut self,
        table_code: u16,
        bottom_extent: u16,
    ) -> DrawCoordinates {
        self.put_u16(0x9c, self.u16(0x98));
        self.put_u16(0x9e, self.u16(0x9a));
        let x = i32::from(self.i16(0xa0));
        let x_offset = match table_code {
            0 | 4 => 16,
            2 => 8,
            _ => 0,
        };
        self.put_u16(0x98, ((x - x_offset) >> 1) as u16);
        self.put_u16(0x9a, (i32::from(self.i16(0xa2)) >> 1) as u16);
        let y = i32::from(self.u16(0x9a));
        let top = if y <= 0x57 {
            (y - 8) as u16
        } else if y > i32::from(bottom_extent) - 0x58 {
            (y + 0xa8 - i32::from(bottom_extent)) as u16
        } else {
            0x50
        };
        let left = if table_code == 4 {
            self.u16(0x98).wrapping_sub(8)
        } else {
            (i32::from(self.i16(0x98)) - i32::from(self.u16(0x10)) - 8) as u16
        };
        DrawCoordinates {
            left: left as i16,
            top: top.wrapping_sub(8) as i16,
            frame: (((self.i32(0x94) as u32) & 0x1800) >> 11) as u8,
        }
    }

    /// Port of ROM 0x2a04c. `freeze` is the word at EWRAM 0x020328bc;
    /// `table_code` is the halfword at IWRAM 0x03003068. Integer casts and
    /// wrapping stores match ARM/Thumb, including arbitrary synthetic states.
    pub fn integrate(&mut self, table_code: u16, freeze: u32) {
        self.put_u16(0xa4, self.u16(0xa0));
        self.put_u16(0xa6, self.u16(0xa2));
        if freeze != 0 {
            self.project_fixed();
            return;
        }
        if self.u16(0xb8) == 0 {
            let phase = self.u16(0xb6) ^ 1;
            self.put_u16(0xb6, phase);
            let increment = if table_code <= 3 && self.i16(0xaa) <= 0 {
                phase
            } else {
                1
            };
            self.put_u16(0xaa, self.u16(0xaa).wrapping_add(increment));
            self.put_i32(0x90, self.i32(0x90).wrapping_add(i32::from(self.i16(0xa8))));
            self.put_i32(0x94, self.i32(0x94).wrapping_add(i32::from(self.i16(0xaa))));
        } else if self.u16(0x6e) == 8 {
            self.project_fixed();
            return;
        }
        self.project_fixed();
        self.put_u16(0xae, self.u16(0xac));
        self.put_i32(0xb0, self.i32(0xb0).wrapping_add(i32::from(self.i16(0xac))));
    }

    fn project_fixed(&mut self) {
        // Native adds 127 before arithmetic shift for negative values, giving
        // signed division by 128 rounded toward zero rather than floor.
        self.put_u16(0xa0, (self.i32(0x90) / 128) as u16);
        self.put_u16(0xa2, (self.i32(0x94) / 128) as u16);
    }

    /// Port of ROM 0x29a40 plus its trail reset at 0x2a4d0. An optional native
    /// palette call is returned explicitly for the presentation adapter; this
    /// method does not pretend to execute an unported graphics routine.
    pub fn initialize(
        &mut self,
        table_code: u16,
        auxiliary: &mut ResetAuxiliary,
    ) -> Option<PaletteCall> {
        let palette = if auxiliary.fields_ac[0] != 0 {
            match table_code {
                1 => Some(PaletteCall {
                    source_address: 0x08459dac,
                    argument1: 0x100,
                    argument2: 11,
                }),
                2 => Some(PaletteCall {
                    source_address: 0x0853351c,
                    argument1: 0x100,
                    argument2: 11,
                }),
                _ => None,
            }
        } else {
            None
        };
        auxiliary.fields_ac = [0; 4];
        self.put_u16(0x8c, 1);
        self.put_i32(0x90, 0);
        self.put_i32(0x94, 0);
        for at in (0x98..=0xac).step_by(2) {
            self.put_u16(at, 0);
        }
        self.put_i32(0xb0, 0);
        self.put_u16(0xb4, 0);
        self.put_u16(0xb6, 1);
        self.put_u16(0xb8, 0);
        self.bytes[3] = 0;
        self.put_u16(0xc0, 0);
        self.put_u16(0xc2, 0x100);
        match table_code {
            0 | 1 => {
                self.bytes[6] = 1;
                self.put_u16(0xa0, 478);
                self.put_u16(0xa2, 773);
            }
            2 => {
                self.bytes[6] = 1;
                self.put_u16(0xa0, 494);
                self.put_u16(0xa2, 808);
            }
            3 => {
                self.bytes[6] = 0;
                self.put_u16(0xa0, 478);
                self.put_u16(0xa2, 648);
            }
            _ => {}
        }
        self.put_u16(0x6c, (self.bytes[6] as i8) as u16);
        let (x, y) = self.native_position();
        self.put_u16(0xa4, x as u16);
        self.put_u16(0xa6, y as u16);
        self.put_i32(0x90, i32::from(x) << 7);
        self.put_i32(0x94, i32::from(y) << 7);
        self.put_u16(0x98, (x / 2) as u16);
        self.put_u16(0x9a, (y / 2) as u16);
        self.put_u16(0x9c, self.u16(0x98));
        self.put_u16(0x9e, self.u16(0x9a));
        auxiliary.trail.count = 0;
        auxiliary.trail.x = [self.u16(0x98); 20];
        auxiliary.trail.y = [self.u16(0x9a); 20];
        auxiliary.trail.zero = [0; 20];
        auxiliary.trail.phase = std::array::from_fn(|i| (i % 3) as u16);
        auxiliary.trail.camera_y = [self.u16(0x12); 20];
        palette
    }
}

/// Exact arguments of the original call to ROM 0x5724. The native renderer
/// adapter must handle it before claiming presentation parity for reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCall {
    pub source_address: u32,
    pub argument1: u32,
    pub argument2: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawCoordinates {
    pub left: i16,
    pub top: i16,
    pub frame: u8,
}
impl PaletteCall {
    pub const ROUTINE_ADDRESS: u32 = 0x08005724;
}

/// Four halfwords at auxiliary address 0x020328c0 + 0xac, and the native trail
/// arrays at 0x020327d0/2800/2830/2860/2890, count at 0x020328b8.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResetAuxiliary {
    pub fields_ac: [u16; 4],
    pub trail: Trail,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trail {
    pub count: u16,
    pub phase: [u16; 20],
    pub x: [u16; 20],
    pub y: [u16; 20],
    pub zero: [u16; 20],
    pub camera_y: [u16; 20],
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn state(hex: &str) -> MainBall {
        assert_eq!(hex.len(), STATE_BYTES * 2);
        let mut bytes = [0; STATE_BYTES];
        for (index, value) in bytes.iter_mut().enumerate() {
            *value = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        MainBall::from_bytes(bytes)
    }

    fn halfwords<const N: usize>(value: &Value) -> [u16; N] {
        let values = value.as_array().unwrap();
        assert_eq!(values.len(), N);
        std::array::from_fn(|index| values[index].as_u64().unwrap().try_into().unwrap())
    }

    #[test]
    fn integrator_matches_192_original_rom_instruction_cases() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/ball-vectors.json")).unwrap();
        let cases = vectors["integrator"].as_array().unwrap();
        assert_eq!(cases.len(), 192);
        for (index, case) in cases.iter().enumerate() {
            let mut actual = state(case["input"].as_str().unwrap());
            actual.integrate(
                case["table"].as_u64().unwrap() as u16,
                case["freeze"].as_u64().unwrap() as u32,
            );
            assert_eq!(
                actual,
                state(case["state"].as_str().unwrap()),
                "case {index}"
            );
        }
    }

    #[test]
    fn initialize_and_trail_match_24_original_rom_instruction_cases() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/ball-vectors.json")).unwrap();
        let cases = vectors["initialize"].as_array().unwrap();
        assert_eq!(cases.len(), 24);
        for (index, case) in cases.iter().enumerate() {
            let mut actual = state(case["input"].as_str().unwrap());
            let mut auxiliary = ResetAuxiliary {
                fields_ac: halfwords(&case["input_auxiliary"]),
                ..Default::default()
            };
            let call = actual.initialize(case["table"].as_u64().unwrap() as u16, &mut auxiliary);
            assert_eq!(
                actual,
                state(case["state"].as_str().unwrap()),
                "state case {index}"
            );
            assert_eq!(
                auxiliary.fields_ac,
                halfwords::<4>(&case["auxiliary"]),
                "auxiliary case {index}"
            );
            let expected = &case["trail"];
            assert_eq!(
                auxiliary.trail,
                Trail {
                    count: expected["count"].as_u64().unwrap() as u16,
                    phase: halfwords(&expected["phase"]),
                    x: halfwords(&expected["x"]),
                    y: halfwords(&expected["y"]),
                    zero: halfwords(&expected["zero"]),
                    camera_y: halfwords(&expected["camera_y"]),
                },
                "trail case {index}"
            );
            let expected_calls = case["palette_calls"].as_array().unwrap();
            match call {
                Some(call) => {
                    assert_eq!(expected_calls.len(), 1, "call count case {index}");
                    assert_eq!(
                        [call.source_address, call.argument1, call.argument2],
                        std::array::from_fn(|i| expected_calls[0][i].as_u64().unwrap() as u32),
                        "call arguments case {index}"
                    );
                }
                None => assert!(expected_calls.is_empty(), "unexpected call case {index}"),
            }
        }
    }

    #[test]
    fn projection_matches_96_original_rom_instruction_cases() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/ball-vectors.json")).unwrap();
        let cases = vectors["projection"].as_array().unwrap();
        assert_eq!(cases.len(), 96);
        for (index, case) in cases.iter().enumerate() {
            let mut actual = state(case["input"].as_str().unwrap());
            let coordinates = actual.project_draw_coordinates(
                case["table"].as_u64().unwrap() as u16,
                case["bottom_extent"].as_u64().unwrap() as u16,
            );
            assert_eq!(
                actual,
                state(case["state"].as_str().unwrap()),
                "state case {index}"
            );
            assert_eq!(
                coordinates,
                DrawCoordinates {
                    left: case["left"].as_i64().unwrap() as i16,
                    top: case["top"].as_i64().unwrap() as i16,
                    frame: case["frame"].as_u64().unwrap() as u8
                },
                "coordinates case {index}"
            );
        }
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; native trigonometry data, controlled BIOS outputs"]
    fn velocity_cap_matches_32_original_instruction_cases_with_controlled_bios() {
        use crate::game::math::{BiosMath, MathError, RejectBiosMath, TrigTable};
        struct ControlledBios {
            sqrt: u16,
            angle: u16,
            calls: Vec<Value>,
        }
        impl BiosMath for ControlledBios {
            fn sqrt(&mut self, x: u32) -> Result<u16, MathError> {
                self.calls
                    .push(serde_json::json!({"service": 8, "x": x, "result": self.sqrt}));
                Ok(self.sqrt)
            }
            fn atan2(&mut self, x: i16, y: i16) -> Result<u16, MathError> {
                self.calls
                    .push(serde_json::json!({"service": 10, "x": x, "y": y, "result": self.angle}));
                Ok(self.angle)
            }
        }
        let rom = std::fs::read(
            std::env::var_os("SPP_ROM").expect("Set SPP_ROM to the supplied local A3VE ROM"),
        )
        .unwrap();
        let table = TrigTable::from_rom(&rom).unwrap();
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/ball-vectors.json")).unwrap();
        let cases = vectors["velocity_cap"].as_array().unwrap();
        assert_eq!(cases.len(), 32);
        for (index, case) in cases.iter().enumerate() {
            let mut actual = state(case["input"].as_str().unwrap());
            let before = actual.clone();
            assert_eq!(
                actual.limit_velocity(&table, &mut RejectBiosMath),
                Err(MathError::BiosUnavailable { service: 8 })
            );
            assert_eq!(actual, before, "BIOS rejection must be atomic");
            let mut bios = ControlledBios {
                sqrt: case["controlled_bios_outputs"]["sqrt"].as_u64().unwrap() as u16,
                angle: case["controlled_bios_outputs"]["atan2"].as_u64().unwrap() as u16,
                calls: Vec::new(),
            };
            assert_eq!(
                actual.limit_velocity(&table, &mut bios).unwrap(),
                case["clamped"].as_bool().unwrap(),
                "branch case {index}"
            );
            assert_eq!(
                actual,
                state(case["state"].as_str().unwrap()),
                "state case {index}"
            );
            assert_eq!(
                bios.calls,
                *case["bios_calls"].as_array().unwrap(),
                "BIOS inputs case {index}"
            );
        }
    }
}
