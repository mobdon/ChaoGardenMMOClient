//! Bounded source translation of A3VE's sorted descriptor compositor, 0x2700.
//!
//! Descriptors are native six-byte attribute triplets terminated by a first
//! halfword of 0xffff. The native 128-entry output limit bounds decoding even
//! when the terminator lies beyond the part actually consumed. This helper
//! stages OAM only; DMA, affine setup and the alternate linked-tree compositor
//! at 0x2a74 remain separate native routines.

use super::oam::{OamSubmissionQueue, CAPACITY, RECORD_BASE, RECORD_WORDS};
use std::fmt;

pub const COMPOSE_OFFSET: usize = 0x2700;
pub const COMPOSE_LENGTH: usize = 0x1ec;
pub const COMPOSE_SHA256: &str = "7e5c1f49a3aec57c60cbedbb7dc51effb0bc99accd8c4202848a49518704b7e0";
pub const MASK_OFFSET: usize = 0x66c28;
pub const MASK_LENGTH: usize = 20;
pub const MASK_SHA256: &str = "3077a920b59da1a92cd4bd3d00b57ce6a3dd22e0a3dbde98cc4ef4bdfd946059";
pub const OUTPUT_COUNT_ADDRESS: u32 = 0x0300_04f4;
pub const OUTPUT_BASE: u32 = 0x0300_0e50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    QueueCount(u8),
    InvalidRecordPointer(u32),
    MisalignedDescriptor(u32),
    MissingDescriptorHalfword(u32),
    AddressOverflow(u32),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RenderError {}

/// Resolve individual little-endian native halfwords from a bounded local
/// resource. Implementations must return None outside the actual resource.
pub trait DescriptorSource {
    fn halfword(&self, address: u32) -> Option<u16>;
}

/// Synthetic or user-local resource view; no ROM or graphics are embedded.
pub struct DescriptorBytes<'a> {
    pub base_address: u32,
    pub bytes: &'a [u8],
}

impl DescriptorSource for DescriptorBytes<'_> {
    fn halfword(&self, address: u32) -> Option<u16> {
        let offset = usize::try_from(address.checked_sub(self.base_address)?).ok()?;
        let pair = self.bytes.get(offset..offset.checked_add(2)?)?;
        Some(u16::from_le_bytes([pair[0], pair[1]]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedOam {
    pub count: u8,
    /// Native attr0/attr1/attr2 plus the preserved affine/reserved halfword.
    pub entries: [[u16; 4]; CAPACITY],
}

impl Default for StagedOam {
    fn default() -> Self {
        Self {
            count: 0,
            entries: [[0; 4]; CAPACITY],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedOam {
    /// The native routine sorts the one-based pointer array in place. Slot 0
    /// and every inactive slot retain their original values.
    pub sorted_order: [u32; CAPACITY + 1],
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

fn sift(order: &mut [u32; CAPACITY + 1], keys: &[u32; CAPACITY], mut root: usize, end: usize) {
    while root <= end / 2 {
        let mut child = root * 2;
        // All active pointers were validated before sorting.
        let key = |slot: usize| keys[((order[slot] - RECORD_BASE) / 28) as usize];
        if child < end && key(child) < key(child + 1) {
            child += 1;
        }
        if key(root) >= key(child) {
            break;
        }
        order.swap(root, child);
        root = child;
    }
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

/// Execute native heap ordering and descriptor expansion on bounded snapshots.
/// Invalid memory returns an error without mutating the caller's queue/output.
/// Native queue and output counts are not cleared by this routine.
pub fn compose(
    queue: &OamSubmissionQueue,
    source: &dyn DescriptorSource,
    initial: &StagedOam,
) -> Result<ComposedOam, RenderError> {
    let count = usize::from(queue.count());
    if count > CAPACITY {
        return Err(RenderError::QueueCount(queue.count()));
    }
    let mut order = *queue.order();
    for pointer in &order[1..=count] {
        record_index(*pointer)?;
    }
    let keys = std::array::from_fn(|index| queue.records()[index][2]);
    for root in (1..=count / 2).rev() {
        sift(&mut order, &keys, root, count);
    }
    for end in (2..=count).rev() {
        order.swap(1, end);
        sift(&mut order, &keys, 1, end - 1);
    }
    let mut staged = initial.clone();
    for pointer in &order[1..=count] {
        let record = &queue.records()[record_index(*pointer)?];
        // The native high-bit guard avoids every descriptor read when full,
        // including the otherwise malformed 0x81..0xff output-count states.
        if staged.count & 0x80 != 0 {
            continue;
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
                break;
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
                break;
            }
            address = address
                .checked_add(6)
                .ok_or(RenderError::AddressOverflow(address))?;
        }
    }
    Ok(ComposedOam {
        sorted_order: order,
        staged,
    })
}

#[cfg(test)]
mod tests {
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
    fn digest(values: impl IntoIterator<Item = u16>) -> String {
        crate::sha256(
            &values
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }
    fn queue(count: u8, pointer: u32, descriptor: u32) -> OamSubmissionQueue {
        let mut records = [[0; RECORD_WORDS]; CAPACITY];
        records[0][3] = descriptor;
        let mut order = [0; CAPACITY + 1];
        order[1] = pointer;
        OamSubmissionQueue::from_words(count, &records, &order).unwrap()
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn original_native_compositor_vectors_match_complete_order_and_staged_ram() {
        let data: Value = serde_json::from_str(vectors!("oam_render-vectors.json")).unwrap();
        assert_eq!(data["routine"]["sha256"], COMPOSE_SHA256);
        assert_eq!(data["masks"]["sha256"], MASK_SHA256);
        for case in data["cases"].as_array().unwrap() {
            let seed = integer(&case["seed"]);
            let flat = pattern(seed, CAPACITY * RECORD_WORDS);
            let mut records: Vec<[u32; RECORD_WORDS]> = flat.as_chunks::<RECORD_WORDS>().0.to_vec();
            for (index, active) in case["records"].as_array().unwrap().iter().enumerate() {
                records[index] = std::array::from_fn(|word| integer(&active[word]));
            }
            let mut order = pattern(seed ^ 0xa55a_1234, CAPACITY + 1);
            for (index, value) in case["initialOrder"].as_array().unwrap().iter().enumerate() {
                order[index + 1] = integer(value);
            }
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
            let output = compose(&queue, &source, &initial).unwrap();
            assert_eq!(
                u32::from(output.staged.count),
                integer(&case["finalOutputCount"]),
                "{}",
                case["name"]
            );
            let order_bytes: Vec<_> = output
                .sorted_order
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            assert_eq!(
                crate::sha256(&order_bytes),
                case["orderSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                digest(output.staged.entries.into_iter().flatten()),
                case["hardwareSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn malformed_resources_are_bounded_and_do_not_mutate_inputs() {
        let source = DescriptorBytes {
            base_address: 0x1000,
            bytes: &[1, 0, 2, 0],
        };
        let input = StagedOam::default();
        let valid = queue(1, RECORD_BASE, 0x1000);
        assert_eq!(
            compose(&valid, &source, &input),
            Err(RenderError::MissingDescriptorHalfword(0x1004))
        );
        assert_eq!(input, StagedOam::default());
        assert_eq!(
            compose(&queue(129, RECORD_BASE, 0x1000), &source, &input),
            Err(RenderError::QueueCount(129))
        );
        assert_eq!(
            compose(&queue(1, RECORD_BASE + 1, 0x1000), &source, &input),
            Err(RenderError::InvalidRecordPointer(RECORD_BASE + 1))
        );
        assert_eq!(
            compose(&queue(1, RECORD_BASE, 0x1001), &source, &input),
            Err(RenderError::MisalignedDescriptor(0x1001))
        );
        let full = StagedOam {
            count: 255,
            ..StagedOam::default()
        };
        assert_eq!(
            compose(&queue(1, RECORD_BASE, u32::MAX), &source, &full)
                .unwrap()
                .staged,
            full
        );
        let terminator = DescriptorBytes {
            base_address: 0x1000,
            bytes: &[255, 255],
        };
        assert_eq!(compose(&valid, &terminator, &input).unwrap().staged, input);
    }
}
