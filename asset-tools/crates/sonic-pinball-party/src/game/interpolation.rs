//! A3VE 32-byte intro interpolation constructor (0x6544) and update (0x663c).
//!
//! Signed halfword comparisons, error wrapping, speed-zero stalls and native
//! endpoint overshoot are retained. The constructor copies an uninitialized
//! first stack word; callers supply that opaque word instead of inventing it.

use super::oam::{OamSubmissionQueue, Submission};
use super::oam_render::{DescriptorSource, RenderError};
use std::fmt;

pub const STATE_BYTES: usize = 32;
pub const CONSTRUCTOR_OFFSET: usize = 0x6544;
pub const CONSTRUCTOR_LENGTH: usize = 0xf8;
pub const CONSTRUCTOR_SHA256: &str =
    "20f0aee31ce1d955ce0f937d5a505594576d7c2a7f77817f0a97241f70566033";
pub const UPDATE_OFFSET: usize = 0x663c;
pub const UPDATE_LENGTH: usize = 0xbc;
pub const UPDATE_SHA256: &str = "5e70c01cb0cf642b1af8f9d88fa2c1d5d5f3c955f9c9dbdc9aea223cae6e7069";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructArgs {
    pub start_x: u32,
    pub start_y: u32,
    pub end_x: u32,
    pub end_y: u32,
    pub descriptor_address: u32,
    pub flags: u32,
    pub speed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpolationError {
    StateLength(usize),
    Descriptor(RenderError),
}
impl fmt::Display for InterpolationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for InterpolationError {}
impl From<RenderError> for InterpolationError {
    fn from(value: RenderError) -> Self {
        Self::Descriptor(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterpolationState {
    bytes: [u8; STATE_BYTES],
}

impl InterpolationState {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, InterpolationError> {
        Ok(Self {
            bytes: bytes
                .try_into()
                .map_err(|_| InterpolationError::StateLength(bytes.len()))?,
        })
    }
    pub fn as_bytes(&self) -> &[u8; STATE_BYTES] {
        &self.bytes
    }
    fn halfword(&self, offset: usize) -> u16 {
        u16::from_le_bytes(self.bytes[offset..offset + 2].try_into().unwrap())
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn put_halfword(&mut self, offset: usize, value: u16) {
        self.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    pub fn x(&self) -> i16 {
        self.halfword(4) as i16
    }
    pub fn y(&self) -> i16 {
        self.halfword(6) as i16
    }
    pub fn progress(&self) -> u16 {
        self.halfword(0x1c)
    }
    pub fn descriptor_address(&self) -> u32 {
        self.word(0xc)
    }

    /// `stack_prefix` is original SP-0x4c's word before native 0x6544 executes.
    /// Every other output byte is assigned by the original constructor.
    pub fn construct(arguments: ConstructArgs, stack_prefix: u32) -> Self {
        let sx = arguments.start_x as u16 as i16;
        let sy = arguments.start_y as u16 as i16;
        let ex = arguments.end_x as u16 as i16;
        let ey = arguments.end_y as u16 as i16;
        let speed = arguments.speed as u16;
        let dx = (i32::from(sx) - i32::from(ex)).unsigned_abs() as u16;
        let dy = (i32::from(sy) - i32::from(ey)).unsigned_abs() as u16;
        // The source compares the truncated absolute deltas as signed i16.
        let error = if dx as i16 >= dy as i16 {
            -(i32::from(dx as i16))
        } else {
            -(i32::from(dy as i16))
        } as u16;
        let mut state = Self::default();
        state.put_word(0, stack_prefix);
        state.put_halfword(4, sx as u16);
        state.put_halfword(6, sy as u16);
        state.put_halfword(8, ex as u16);
        state.put_halfword(0xa, ey as u16);
        state.put_word(0xc, arguments.descriptor_address);
        state.put_halfword(0x10, dx);
        state.put_halfword(0x12, dy);
        state.put_halfword(0x14, error);
        state.put_halfword(0x16, if sx > ex { speed.wrapping_neg() } else { speed });
        state.put_halfword(0x18, if sy > ey { speed.wrapping_neg() } else { speed });
        state.put_halfword(0x1a, speed);
        state.put_halfword(0x1c, 0);
        state.put_halfword(0x1e, arguments.flags as u16);
        state
    }

    /// Original completion bool and actual 0x2614 submission. A full queue
    /// still advances interpolation but avoids descriptor reads. A missing
    /// descriptor rejects before either state/queue is mutated at this bounded
    /// host boundary. No hardware OAM or asset bytes are synthesized.
    pub fn update(
        &mut self,
        queue: &mut OamSubmissionQueue,
        source: &dyn DescriptorSource,
    ) -> Result<bool, InterpolationError> {
        let dx = i32::from(self.halfword(0x10) as i16);
        let dy = i32::from(self.halfword(0x12) as i16);
        let x_major = dx >= dy;
        let major = if x_major { dx } else { dy };
        if i32::from(self.progress()) > major {
            return Ok(true);
        }
        let descriptor = self.descriptor_address();
        let descriptor_halfword_at_4 = if queue.count() & 0x80 == 0 {
            if descriptor & 1 != 0 {
                return Err(RenderError::MisalignedDescriptor(descriptor).into());
            }
            let address = descriptor
                .checked_add(4)
                .ok_or(RenderError::AddressOverflow(descriptor))?;
            source
                .halfword(address)
                .ok_or(RenderError::MissingDescriptorHalfword(address))?
        } else {
            0
        };
        self.put_halfword(0x1c, self.progress().wrapping_add(self.halfword(0x1a)));
        queue.submit(Submission {
            sort_key: 0,
            x: u32::from(self.halfword(4)),
            y: u32::from(self.halfword(6)),
            descriptor_address: descriptor,
            descriptor_halfword_at_4,
            flags: u32::from(self.halfword(0x1e)),
            auxiliary: 0,
        });
        let primary_offset = if x_major { 4 } else { 6 };
        let primary_step = if x_major { 0x16 } else { 0x18 };
        self.put_halfword(
            primary_offset,
            self.halfword(primary_offset)
                .wrapping_add(self.halfword(primary_step)),
        );
        let minor = if x_major { dy } else { dx };
        let next_error = (i32::from(self.halfword(0x14)) + minor * 2) as u16;
        self.put_halfword(0x14, next_error);
        if next_error as i16 >= 0 {
            let secondary_offset = if x_major { 6 } else { 4 };
            let secondary_step = if x_major { 0x18 } else { 0x16 };
            self.put_halfword(
                secondary_offset,
                self.halfword(secondary_offset)
                    .wrapping_add(self.halfword(secondary_step)),
            );
            self.put_halfword(0x14, next_error.wrapping_sub((major * 2) as u16));
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::super::oam::{CAPACITY, RECORD_WORDS};
    use super::super::oam_render::DescriptorBytes;
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn bytes(words: &Value) -> Vec<u8> {
        words
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|v| (integer(v) as u16).to_le_bytes())
            .collect()
    }
    fn arguments(value: &Value) -> ConstructArgs {
        ConstructArgs {
            start_x: integer(&value["startX"]),
            start_y: integer(&value["startY"]),
            end_x: integer(&value["endX"]),
            end_y: integer(&value["endY"]),
            descriptor_address: integer(&value["descriptorAddress"]),
            flags: integer(&value["flags"]),
            speed: integer(&value["speed"]),
        }
    }
    fn pattern(seed: u32, count: usize) -> Vec<u32> {
        (0..count)
            .map(|index| seed.wrapping_add((index as u32).wrapping_mul(0x0102_0304)))
            .collect()
    }

    #[test]
    fn native_constructor_preserves_signed_truncation_and_explicit_stack_residue() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/interpolation-vectors.json")).unwrap();
        assert_eq!(
            data["routines"]["constructor"]["sha256"],
            CONSTRUCTOR_SHA256
        );
        for case in data["constructors"].as_array().unwrap() {
            let state = InterpolationState::construct(
                arguments(&case["arguments"]),
                integer(&case["stackPrefix"]),
            );
            assert_eq!(state.as_bytes().as_slice(), bytes(&case["state"]));
        }
    }

    #[test]
    fn native_update_and_real_submission_match_complete_context_queue_and_order() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/interpolation-vectors.json")).unwrap();
        assert_eq!(data["routines"]["update"]["sha256"], UPDATE_SHA256);
        for case in data["updates"].as_array().unwrap() {
            let mut state = InterpolationState::from_bytes(&bytes(&case["initial"])).unwrap();
            let seed = integer(&case["seed"]);
            let flat = pattern(seed, CAPACITY * RECORD_WORDS);
            let records: Vec<[u32; RECORD_WORDS]> = flat.as_chunks::<RECORD_WORDS>().0.to_vec();
            let order = pattern(seed ^ 0xa55a_1234, CAPACITY + 1);
            let mut queue = OamSubmissionQueue::from_words(
                integer(&case["initialCount"]) as u8,
                &records,
                &order,
            )
            .unwrap();
            let descriptor = [0, 0, integer(&case["descriptorHalfwordAt4"]) as u16];
            let descriptor_bytes: Vec<_> =
                descriptor.into_iter().flat_map(u16::to_le_bytes).collect();
            let source = DescriptorBytes {
                base_address: integer(&data["descriptorAddress"]),
                bytes: &descriptor_bytes,
            };
            for step in case["steps"].as_array().unwrap() {
                assert_eq!(
                    state.update(&mut queue, &source).unwrap(),
                    step["complete"].as_bool().unwrap()
                );
                assert_eq!(state.as_bytes().as_slice(), bytes(&step["state"]));
                assert_eq!(u32::from(queue.count()), integer(&step["queueCount"]));
                let record_bytes: Vec<_> = queue
                    .records()
                    .iter()
                    .flatten()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                let order_bytes: Vec<_> = queue
                    .order()
                    .iter()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                assert_eq!(
                    crate::sha256(&record_bytes),
                    step["recordsSha256"].as_str().unwrap()
                );
                assert_eq!(
                    crate::sha256(&order_bytes),
                    step["orderSha256"].as_str().unwrap()
                );
            }
        }
    }

    #[test]
    fn missing_descriptor_is_bounded_but_full_queue_preserves_native_advance() {
        let args = ConstructArgs {
            start_x: 0,
            start_y: 0,
            end_x: 5,
            end_y: 2,
            descriptor_address: 0x1000,
            flags: 0,
            speed: 2,
        };
        let mut state = InterpolationState::construct(args, 0xa55a_1234);
        let mut queue = OamSubmissionQueue::default();
        let source = DescriptorBytes {
            base_address: 0x1000,
            bytes: &[],
        };
        let before = state.clone();
        let before_queue = queue.clone();
        assert_eq!(
            state.update(&mut queue, &source),
            Err(InterpolationError::Descriptor(
                RenderError::MissingDescriptorHalfword(0x1004)
            ))
        );
        assert_eq!(state, before);
        assert_eq!(queue, before_queue);
        let mut full = OamSubmissionQueue::from_words(255, queue.records(), queue.order()).unwrap();
        assert!(!state.update(&mut full, &source).unwrap());
        assert_eq!(state.progress(), 2);
        assert_eq!(full.count(), 255);
        assert_eq!(
            InterpolationState::from_bytes(&[]),
            Err(InterpolationError::StateLength(0))
        );
    }
}
