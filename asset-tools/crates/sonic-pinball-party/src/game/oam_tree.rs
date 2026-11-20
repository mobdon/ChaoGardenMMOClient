//! Bounded A3VE reverse-in-order descriptor compositor at ROM 0x2a74.
//!
//! Record words 0/1 are left/right child links. The routine visits right, node,
//! left, using the order buffer as a zero-based stack; popped slots remain
//! untouched. The submission count and node sort keys are not consulted.

use super::oam::{OamSubmissionQueue, CAPACITY, RECORD_BASE, RECORD_WORDS};
use super::oam_render::{DescriptorSource, RenderError, StagedOam};
use std::fmt;

pub const COMPOSE_OFFSET: usize = 0x2a74;
pub const COMPOSE_LENGTH: usize = 0x104;
pub const COMPOSE_SHA256: &str = "0d8d621c178d94551ead556cee5ee010eee8da35dfe2f2f29c4a079e59ffaa7b";
pub const ROOT_ADDRESS: u32 = 0x0300_23b0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    Memory(RenderError),
    /// Native submitters create a tree. Reject cycles and shared subtrees at
    /// the Rust boundary instead of allowing unbounded native traversal.
    RepeatedRecord(u32),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TreeError {}
impl From<RenderError> for TreeError {
    fn from(value: RenderError) -> Self {
        Self::Memory(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeComposition {
    /// Native ORDER_BASE after its use as a zero-based traversal stack. This
    /// is retained scratch RAM, not a list of the final traversal order.
    pub scratch_order: [u32; CAPACITY + 1],
    pub staged: StagedOam,
}

fn record_index(address: u32) -> Result<usize, RenderError> {
    let offset = address
        .checked_sub(RECORD_BASE)
        .ok_or(RenderError::InvalidRecordPointer(address))?;
    let stride = (RECORD_WORDS * 4) as u32;
    if offset % stride != 0 || offset / stride >= CAPACITY as u32 {
        return Err(RenderError::InvalidRecordPointer(address));
    }
    Ok((offset / stride) as usize)
}

fn read(
    source: &dyn DescriptorSource,
    address: u32,
    displacement: u32,
) -> Result<u16, RenderError> {
    let at = address
        .checked_add(displacement)
        .ok_or(RenderError::AddressOverflow(address))?;
    source
        .halfword(at)
        .ok_or(RenderError::MissingDescriptorHalfword(at))
}

fn emit(
    record: &[u32; RECORD_WORDS],
    source: &dyn DescriptorSource,
    staged: &mut StagedOam,
) -> Result<(), RenderError> {
    if staged.count & 0x80 != 0 {
        return Ok(());
    }
    let mut address = record[3];
    if address & 1 != 0 {
        return Err(RenderError::MisalignedDescriptor(address));
    }
    let x = record[5] as u16;
    let y = (record[5] >> 16) as u16;
    let flags = record[6] as u16;
    loop {
        let a0 = read(source, address, 0)?;
        if a0 == 0xffff {
            return Ok(());
        }
        let a1 = read(source, address, 2)?;
        let a2 = read(source, address, 4)?;
        let output = &mut staged.entries[usize::from(staged.count)];
        output[0] = (a0 & 0xff00) | (a0.wrapping_add(y) & 0x00ff) | ((flags & 0x001f) << 8);
        output[1] = (a1 & 0xfe00) | (a1.wrapping_add(x) & 0x01ff) | ((flags & 0x03e0) << 4);
        output[2] = (a2 & 0xfc00)
            | (u32::from(a2).wrapping_add(record[4]) as u16 & 0x03ff)
            | (flags & 0xfc00);
        staged.count += 1;
        if staged.count & 0x80 != 0 {
            return Ok(());
        }
        address = address
            .checked_add(6)
            .ok_or(RenderError::AddressOverflow(address))?;
    }
}

/// Port the native traversal and descriptor stores, preserving all unmodified
/// scratch/output RAM. At most 128 distinct bounded record pointers are visited;
/// malformed links/resources return an error without changing caller state.
pub fn compose_tree(
    queue: &OamSubmissionQueue,
    root: u32,
    source: &dyn DescriptorSource,
    initial: &StagedOam,
) -> Result<TreeComposition, TreeError> {
    let mut scratch_order = *queue.order();
    let mut staged = initial.clone();
    let mut seen = [false; CAPACITY];
    let mut depth = 0;
    let mut pending = root;
    loop {
        while pending != 0 {
            let index = record_index(pending)?;
            if seen[index] {
                return Err(TreeError::RepeatedRecord(pending));
            }
            seen[index] = true;
            scratch_order[depth] = pending;
            depth += 1;
            pending = queue.records()[index][1];
        }
        if depth == 0 {
            return Ok(TreeComposition {
                scratch_order,
                staged,
            });
        }
        depth -= 1;
        let index = record_index(scratch_order[depth])?;
        let record = &queue.records()[index];
        emit(record, source, &mut staged)?;
        pending = record[0];
    }
}

#[cfg(test)]
mod tests {
    use super::super::oam_render::DescriptorBytes;
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn pattern(seed: u32, count: usize) -> Vec<u32> {
        (0..count)
            .map(|index| seed.wrapping_add((index as u32).wrapping_mul(0x0102_0304)))
            .collect()
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_tree_compositor_matches_synthetic_scratch_and_hardware_ram() {
        let data: Value = serde_json::from_str(vectors!("oam_tree-vectors.json")).unwrap();
        assert_eq!(data["routine"]["sha256"], COMPOSE_SHA256);
        for case in data["cases"].as_array().unwrap() {
            let seed = integer(&case["seed"]);
            let flat = pattern(seed, CAPACITY * RECORD_WORDS);
            let mut records: Vec<[u32; RECORD_WORDS]> = flat.as_chunks::<RECORD_WORDS>().0.to_vec();
            for (index, active) in case["records"].as_array().unwrap().iter().enumerate() {
                records[index] = std::array::from_fn(|word| integer(&active[word]));
            }
            let order = pattern(seed ^ 0xa55a_1234, CAPACITY + 1);
            let queue = OamSubmissionQueue::from_words(
                integer(&case["queueCount"]) as u8,
                &records,
                &order,
            )
            .unwrap();
            let hardware = pattern(seed ^ 0x7ace_1200, CAPACITY * 2);
            let halfwords: Vec<_> = hardware
                .iter()
                .flat_map(|word| [*word as u16, (*word >> 16) as u16])
                .collect();
            let initial = StagedOam {
                count: integer(&case["initialOutputCount"]) as u8,
                entries: std::array::from_fn(|index| {
                    std::array::from_fn(|field| halfwords[index * 4 + field])
                }),
            };
            let descriptor_bytes: Vec<_> = case["descriptorHalfwords"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|word| (integer(word) as u16).to_le_bytes())
                .collect();
            let source = DescriptorBytes {
                base_address: integer(&data["descriptorBase"]),
                bytes: &descriptor_bytes,
            };
            let output = compose_tree(&queue, integer(&case["root"]), &source, &initial).unwrap();
            assert_eq!(
                u32::from(output.staged.count),
                integer(&case["finalOutputCount"]),
                "{}",
                case["name"]
            );
            let order_bytes: Vec<_> = output
                .scratch_order
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            assert_eq!(
                crate::sha256(&order_bytes),
                case["orderSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            let hardware_bytes: Vec<_> = output
                .staged
                .entries
                .into_iter()
                .flatten()
                .flat_map(u16::to_le_bytes)
                .collect();
            assert_eq!(
                crate::sha256(&hardware_bytes),
                case["hardwareSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn malformed_tree_links_and_descriptors_return_bounded_errors() {
        let mut records = [[0; RECORD_WORDS]; CAPACITY];
        records[0][1] = RECORD_BASE;
        let order = [0; CAPACITY + 1];
        let queue = OamSubmissionQueue::from_words(0, &records, &order).unwrap();
        let source = DescriptorBytes {
            base_address: 0,
            bytes: &[],
        };
        let input = StagedOam::default();
        assert_eq!(
            compose_tree(&queue, RECORD_BASE, &source, &input),
            Err(TreeError::RepeatedRecord(RECORD_BASE))
        );
        assert_eq!(
            compose_tree(&queue, RECORD_BASE + 1, &source, &input),
            Err(TreeError::Memory(RenderError::InvalidRecordPointer(
                RECORD_BASE + 1
            )))
        );
        records[0][1] = 0;
        records[0][3] = 0x1000;
        let queue = OamSubmissionQueue::from_words(255, &records, &order).unwrap();
        assert_eq!(
            compose_tree(&queue, RECORD_BASE, &source, &input),
            Err(TreeError::Memory(RenderError::MissingDescriptorHalfword(
                0x1000
            )))
        );
        assert_eq!(
            compose_tree(&queue, 0, &source, &input).unwrap().staged,
            input
        );
        let full = StagedOam {
            count: 255,
            ..StagedOam::default()
        };
        assert_eq!(
            compose_tree(&queue, RECORD_BASE, &source, &full)
                .unwrap()
                .staged,
            full
        );
        assert_eq!(input, StagedOam::default());
    }
}
