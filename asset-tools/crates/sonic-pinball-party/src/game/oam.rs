//! A3VE object-submission queue at ROM 0x2614.
//!
//! This is the original 28-byte command queue, not a guessed sprite renderer or
//! completed descriptor-to-hardware-OAM compositor. Link words, inactive slots
//! and the final reserved halfword remain opaque and are preserved.

use serde::Serialize;
use std::fmt;

pub const SUBMIT_OFFSET: usize = 0x2614;
pub const SUBMIT_LENGTH: usize = 0x7c;
pub const SUBMIT_SHA256: &str = "2da13e38a8d7500ff026faa9057c867cfb52da8bf9969f48cfb67f216261fbbc";
pub const CAPACITY: usize = 128;
pub const RECORD_WORDS: usize = 7;
pub const RECORD_BASE: u32 = 0x0300_1250;
pub const ORDER_BASE: u32 = 0x0300_0bf0;
pub const COUNT_ADDRESS: u32 = 0x0300_0018;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OamError {
    RecordCount(usize),
    OrderCount(usize),
}

impl fmt::Display for OamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OamError {}

/// Explicit native arguments. The descriptor's halfword at +4 is supplied by a
/// bounded resource reader; this pure helper never dereferences native pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Submission {
    pub sort_key: u32,
    pub x: u32,
    pub y: u32,
    pub descriptor_address: u32,
    pub descriptor_halfword_at_4: u16,
    pub flags: u32,
    pub auxiliary: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OamSubmissionQueue {
    count: u8,
    records: [[u32; RECORD_WORDS]; CAPACITY],
    order: [u32; CAPACITY + 1],
}

impl Default for OamSubmissionQueue {
    fn default() -> Self {
        Self {
            count: 0,
            records: [[0; RECORD_WORDS]; CAPACITY],
            order: [0; CAPACITY + 1],
        }
    }
}

impl OamSubmissionQueue {
    pub fn from_words(
        count: u8,
        records: &[[u32; RECORD_WORDS]],
        order: &[u32],
    ) -> Result<Self, OamError> {
        if records.len() != CAPACITY {
            return Err(OamError::RecordCount(records.len()));
        }
        if order.len() != CAPACITY + 1 {
            return Err(OamError::OrderCount(order.len()));
        }
        let mut result = Self {
            count,
            ..Self::default()
        };
        result.records.copy_from_slice(records);
        result.order.copy_from_slice(order);
        Ok(result)
    }

    pub fn count(&self) -> u8 {
        self.count
    }

    pub fn records(&self) -> &[[u32; RECORD_WORDS]; CAPACITY] {
        &self.records
    }

    pub fn order(&self) -> &[u32; CAPACITY + 1] {
        &self.order
    }

    /// Submit one command exactly as native 0x2614. The high bit of the count
    /// rejects further commands, including arbitrary 0x80..0xFF count states.
    /// The bool reports whether the native acceptance branch was taken; no
    /// original R0 return-value ABI is inferred from this Rust result.
    pub fn submit(&mut self, command: Submission) -> bool {
        if self.count & 0x80 != 0 {
            return false;
        }
        let index = usize::from(self.count);
        self.count += 1;
        self.order[usize::from(self.count)] = RECORD_BASE + index as u32 * 28;
        let record = &mut self.records[index];
        let flags = command.flags as u16;
        let priority = u32::from((command.descriptor_halfword_at_4 | flags) & 0x0c00);
        record[2] = (command.sort_key & 0x3fff_ffff) | (priority << 20);
        record[3] = command.descriptor_address;
        record[4] = command.auxiliary;
        record[5] = u32::from(command.x as u16) | (u32::from(command.y as u16) << 16);
        record[6] = (record[6] & 0xffff_0000) | u32::from(flags);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn pattern(seed: u32, words: usize) -> Vec<u32> {
        (0..words)
            .map(|index| seed.wrapping_add((index as u32).wrapping_mul(0x0102_0304)))
            .collect()
    }
    fn digest(words: impl IntoIterator<Item = u32>) -> String {
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        crate::sha256(&bytes)
    }
    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }

    #[test]
    fn original_submission_vectors_preserve_complete_queue_and_opaque_words() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/oam-vectors.json")).unwrap();
        for case in data["cases"].as_array().unwrap() {
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
            let a = &case["arguments"];
            let command = Submission {
                sort_key: integer(&a["sortKey"]),
                x: integer(&a["x"]),
                y: integer(&a["y"]),
                descriptor_address: integer(&a["descriptorAddress"]),
                descriptor_halfword_at_4: integer(&a["descriptorHalfwordAt4"]) as u16,
                flags: integer(&a["flags"]),
                auxiliary: integer(&a["auxiliary"]),
            };
            assert_eq!(queue.submit(command), case["accepted"].as_bool().unwrap());
            assert_eq!(u32::from(queue.count()), integer(&case["finalCount"]));
            assert_eq!(
                digest(queue.records().iter().flatten().copied()),
                case["recordsSha256"].as_str().unwrap()
            );
            assert_eq!(
                digest(queue.order().iter().copied()),
                case["orderSha256"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn rejects_incorrect_snapshot_dimensions() {
        assert_eq!(
            OamSubmissionQueue::from_words(0, &[], &[]),
            Err(OamError::RecordCount(0))
        );
        assert_eq!(
            OamSubmissionQueue::from_words(0, &[[0; RECORD_WORDS]; CAPACITY], &[]),
            Err(OamError::OrderCount(0))
        );
    }
}
