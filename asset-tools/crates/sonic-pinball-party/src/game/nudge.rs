//! Native directional nudge input and decay, ROM 0x39ee4..0x3a098.
//! This writes collision offsets; it does not invent a launch impulse or tilt.
use super::ball::MainBall;
use crate::{binary, catalog, sha256};

pub const OFFSET: usize = 0x39ee4;
pub const LENGTH: usize = 0x1b4;
pub const SHA256: &str = "c0ec8ad410575cd2bf3034a983dee8e237bf4e63a047ab7b47c5722d639667ed";
pub const GLOBAL_ADDRESS: u32 = 0x02035240;
pub const GLOBAL_BYTES: usize = 16;
/// The routine uses pointer 0x086e9ed0 and index counter-1. Its final active
/// counter is zero, so the preceding halfword is a required source operand.
pub const CURVE_OFFSET: usize = 0x6e9ece;
pub const CURVE_LENGTH: usize = 20;
pub const CURVE_SHA256: &str = "1c92b60ec86694dfdcaf503b699753480e1aedfde4dedd375e07bbb39832286a";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Source,
    Curve,
    Counter(i16),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub struct Curve {
    samples: [u16; 10],
}
impl Curve {
    pub fn load(rom: &[u8]) -> Result<Self, Error> {
        catalog::validate_rom(rom).map_err(|_| Error::Source)?;
        let bytes = binary::span(rom, CURVE_OFFSET, CURVE_LENGTH).map_err(|_| Error::Curve)?;
        if sha256(bytes) != CURVE_SHA256 {
            return Err(Error::Curve);
        }
        Self::decode(bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != CURVE_LENGTH {
            return Err(Error::Curve);
        }
        let mut samples = [0; 10];
        for (index, value) in samples.iter_mut().enumerate() {
            *value = binary::u16_le(bytes, index * 2).map_err(|_| Error::Curve)?;
        }
        Ok(Self { samples })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    pub pressed: u16,    // 0x03000510 + 6
    pub left_mask: u16,  // 0x030040c0 + 8
    pub right_mask: u16, // 0x030040c0 + 0xa
    pub up_mask: u16,    // 0x030040c0 + 0xc
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nudge {
    bytes: [u8; GLOBAL_BYTES],
}
impl Nudge {
    pub fn from_bytes(bytes: [u8; GLOBAL_BYTES]) -> Self {
        Self { bytes }
    }
    pub fn as_bytes(&self) -> &[u8; GLOBAL_BYTES] {
        &self.bytes
    }
    fn u16(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]])
    }
    fn put16(&mut self, offset: usize, value: u16) {
        self.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// A source-level port of the complete routine. Only the selected active
    /// direction's timer indexes the resource; dormant timers are preserved.
    /// Valid native timer states 9..0 include the final preceding-halfword read.
    /// An invalid selected timer fails before changing either state snapshot.
    pub fn update(
        &mut self,
        ball: &mut MainBall,
        input: Inputs,
        curve: &Curve,
    ) -> Result<(), Error> {
        let mut next = self.clone();
        let mut next_ball = ball.clone();
        if next_ball.as_bytes()[0x14] == 0 {
            next.put16(0xc, 0);
            next.put16(0xe, 0);
            next_ball.as_bytes_mut()[0x21..0x23].fill(0);
            for offset in [0, 2, 4] {
                next.put16(offset, 0);
                // Native repeats this same +6 store three times. It leaves
                // the right/up timers at +8/+a untouched.
                next.put16(6, 0);
            }
        } else {
            let free = next.u16(0) == 0 && next.u16(2) == 0 && next.u16(4) == 0;
            let direction = if input.pressed & input.left_mask != 0 {
                Some(0)
            } else if input.pressed & input.right_mask != 0 {
                Some(1)
            } else if input.pressed & input.up_mask != 0 {
                Some(2)
            } else {
                None
            };
            if free {
                if let Some(direction) = direction {
                    next.put16(direction * 2, 1);
                    next.put16(6 + direction * 2, 9);
                }
            }
            let active = (0..3).find(|&direction| next.u16(direction * 2) != 0);
            if let Some(direction) = active {
                let counter_offset = 6 + direction * 2;
                let counter = next.u16(counter_offset) as i16;
                if !(0..=9).contains(&counter) {
                    return Err(Error::Counter(counter));
                }
                let value = curve.samples[counter as usize];
                let component = if direction == 2 { 0xe } else { 0xc };
                next.put16(
                    component,
                    if direction == 0 {
                        value.wrapping_neg()
                    } else {
                        value
                    },
                );
                let counter = next.u16(counter_offset).wrapping_sub(1);
                next.put16(counter_offset, counter);
                if (counter as i16) < 0 {
                    next.put16(direction * 2, 0);
                    next.put16(0xc, 0);
                    next.put16(0xe, 0);
                }
                next_ball.as_bytes_mut()[0x21] = next.u16(0xc) as u8;
                next_ball.as_bytes_mut()[0x22] = next.u16(0xe) as u8;
            }
        }
        *self = next;
        *ball = next_ball;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
            .collect()
    }
    fn check(vectors: &Value, curve: &Curve) {
        let input = |value: &Value| Inputs {
            pressed: value["pressed"].as_u64().unwrap() as u16,
            left_mask: value["left_mask"].as_u64().unwrap() as u16,
            right_mask: value["right_mask"].as_u64().unwrap() as u16,
            up_mask: value["up_mask"].as_u64().unwrap() as u16,
        };
        for (index, case) in vectors["cases"].as_array().unwrap().iter().enumerate() {
            let mut ball = MainBall::from_bytes(
                bytes(case["input_state"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
            );
            let mut nudge = Nudge::from_bytes(
                bytes(case["input_globals"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
            );
            nudge
                .update(&mut ball, input(&case["inputs"]), curve)
                .unwrap();
            assert_eq!(
                ball.as_bytes().as_slice(),
                bytes(case["state"].as_str().unwrap()),
                "state case {index}"
            );
            assert_eq!(
                nudge.as_bytes().as_slice(),
                bytes(case["globals"].as_str().unwrap()),
                "globals case {index}"
            );
        }
        for (index, sequence) in vectors["sequences"].as_array().unwrap().iter().enumerate() {
            let mut ball = MainBall::from_bytes(
                bytes(sequence["input_state"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
            );
            let mut nudge = Nudge::from_bytes(
                bytes(sequence["input_globals"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
            );
            for (frame, case) in sequence["frames"].as_array().unwrap().iter().enumerate() {
                nudge
                    .update(&mut ball, input(&case["inputs"]), curve)
                    .unwrap();
                assert_eq!(
                    ball.as_bytes().as_slice(),
                    bytes(case["state"].as_str().unwrap()),
                    "sequence {index} frame {frame}"
                );
                assert_eq!(
                    nudge.as_bytes().as_slice(),
                    bytes(case["globals"].as_str().unwrap()),
                    "globals sequence {index} frame {frame}"
                );
            }
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn matches_original_instructions_with_synthetic_curve() {
        let vectors: Value = serde_json::from_str(vectors!("nudge-vectors.json")).unwrap();
        assert_eq!(vectors["cases"].as_array().unwrap().len(), 320);
        assert_eq!(vectors["sequences"].as_array().unwrap().len(), 16);
        check(
            &vectors,
            &Curve::decode(&bytes(vectors["synthetic_curve"].as_str().unwrap())).unwrap(),
        );
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; original native nudge curve"]
    fn matches_original_instructions_with_native_curve() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let curve = Curve::load(&rom).unwrap();
        assert_eq!(crate::sha256(&rom[OFFSET..OFFSET + LENGTH]), SHA256);
        let vectors: Value = serde_json::from_str(vectors!("nudge-native-vectors.json")).unwrap();
        check(&vectors, &curve);
    }
    #[test]
    fn rejects_invalid_selected_curve_index_without_mutation() {
        let curve = Curve::decode(&[0; CURVE_LENGTH]).unwrap();
        let mut ball = MainBall::default();
        ball.as_bytes_mut()[0x14] = 1;
        let mut nudge = Nudge::from_bytes([0; GLOBAL_BYTES]);
        nudge.put16(0, 1);
        nudge.put16(6, 10);
        let before = (nudge.clone(), ball.clone());
        let input = Inputs {
            pressed: 0,
            left_mask: 1,
            right_mask: 2,
            up_mask: 4,
        };
        assert_eq!(
            nudge.update(&mut ball, input, &curve),
            Err(Error::Counter(10))
        );
        assert_eq!((nudge, ball), before);
        assert!(matches!(Curve::load(&[0; 4]), Err(Error::Source)));
        assert!(matches!(
            Curve::decode(&[0; CURVE_LENGTH - 1]),
            Err(Error::Curve)
        ));
    }
}
