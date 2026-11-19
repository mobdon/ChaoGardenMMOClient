//! Bounded ASOE ring layout reader and an ephemeral solo-practice ledger.
//!
//! Layout coordinates come from the exact ROM. Pickup rectangles follow the
//! pinned SA1 reference; camera-region scheduling, invulnerability, lives,
//! magnetic shields and native execution parity remain separate work. Nothing
//! here writes saves, grants MMO rewards or communicates with a service.

use crate::{binary, Error, Result};
use std::collections::{BTreeMap, BTreeSet};

pub const RING_TABLE_OFFSET: usize = 0xb7f4c;
pub const STAGE_ZERO_RINGS_OFFSET: usize = 0x6c8d8c;
pub const STAGE_ZERO_COMPRESSED_LEN: usize = 969;
pub const STAGE_ZERO_DECODED_LEN: usize = 1886;
pub const STAGE_ZERO_RING_COUNT: usize = 291;
pub const STAGE_ZERO_COMPRESSED_SHA256: &str =
    "50a06a7609e24ca43a1b8ccc2cdc2e1d330c074572717e8494eaa89cc51a3987";
pub const STAGE_ZERO_DECODED_SHA256: &str =
    "fdf280edc42746ec4e505ce5618828f5a6ff3fbb82d6734eca19e9e60445132f";
pub const MAX_RING_MAP_BYTES: usize = 1024 * 1024;
pub const MAX_REGIONS: usize = 4096;
pub const MAX_RINGS: usize = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    pub x: i32,
    pub y: i32,
    /// Dense identity assigned by first row-major encounter of a native record.
    /// Aliased region pointers retain one identity, matching shared native data.
    pub id: usize,
}

pub struct RingMap;

impl RingMap {
    /// Only zone 1 act 1 is currently verified for the executable practice slice.
    pub fn load_asoe(rom: &[u8], stage: usize) -> Result<Vec<Ring>> {
        if stage != 0 {
            return Err(Error("only ASOE stage zero ring layout is verified".into()));
        }
        binary::verify_asoe(rom)?;
        let pointer = binary::u32_at(rom, RING_TABLE_OFFSET)?;
        if binary::rom_offset(pointer, rom.len(), 4)? != STAGE_ZERO_RINGS_OFFSET {
            return Err(Error("ASOE ring table root mismatch".into()));
        }
        let compressed = binary::range(rom, STAGE_ZERO_RINGS_OFFSET, STAGE_ZERO_COMPRESSED_LEN)?;
        if compressed.first() != Some(&0x30)
            || binary::sha256(compressed) != STAGE_ZERO_COMPRESSED_SHA256
        {
            return Err(Error("ASOE ring RLE stream fingerprint mismatch".into()));
        }
        let decoded = binary::decompress(compressed, STAGE_ZERO_DECODED_LEN)?;
        if decoded.consumed != STAGE_ZERO_COMPRESSED_LEN
            || decoded.bytes.len() != STAGE_ZERO_DECODED_LEN
            || binary::sha256(&decoded.bytes) != STAGE_ZERO_DECODED_SHA256
        {
            return Err(Error("ASOE ring decoded fingerprint mismatch".into()));
        }
        let rings = Self::decode(&decoded.bytes, MAX_RINGS)?;
        if rings.len() != STAGE_ZERO_RING_COUNT {
            return Err(Error("ASOE ring count differs from source evidence".into()));
        }
        Ok(rings)
    }

    /// Decode the uncompressed layout, with a hard allocation/work budget.
    /// Header: raw size word, width, height, then row-major region index words.
    /// Index offsets are relative to byte 4, not the start of the whole blob.
    pub fn decode(bytes: &[u8], max_rings: usize) -> Result<Vec<Ring>> {
        if bytes.len() > MAX_RING_MAP_BYTES || max_rings > MAX_RINGS {
            return Err(Error("ring map budget exceeded".into()));
        }
        let raw_header = binary::u32_at(bytes, 0)?;
        if raw_header & 0xff != 0 || (raw_header >> 8) as usize != bytes.len() {
            return Err(Error("invalid uncompressed ring size header".into()));
        }
        let width = binary::u32_at(bytes, 4)? as usize;
        let height = binary::u32_at(bytes, 8)? as usize;
        let regions = width
            .checked_mul(height)
            .filter(|n| *n <= MAX_REGIONS)
            .ok_or_else(|| Error("ring region budget exceeded".into()))?;
        if width == 0 || height == 0 || width > 256 || height > 256 {
            return Err(Error("invalid ring region dimensions".into()));
        }
        let table_length = regions * 4;
        binary::range(bytes, 12, table_length)?;
        let records_start = 12 + table_length;
        let mut rings = Vec::new();
        let mut identities = BTreeMap::new();
        let mut coordinate_tails = BTreeSet::new();
        let mut terminators = BTreeSet::new();
        for region in 0..regions {
            let offset = binary::u32_at(bytes, 12 + region * 4)? as usize;
            if offset == 0 {
                continue;
            }
            let mut cursor = offset
                .checked_add(4)
                .filter(|start| *start >= records_start && *start < bytes.len())
                .ok_or_else(|| Error("ring index points outside record data".into()))?;
            loop {
                if coordinate_tails.contains(&cursor) {
                    return Err(Error("ring index points into a coordinate pair".into()));
                }
                let x = binary::range(bytes, cursor, 1)?[0];
                if x == 0xff {
                    terminators.insert(cursor);
                    break;
                }
                if x == 0xfe {
                    return Err(Error(
                        "source layout contains runtime ring tombstone".into(),
                    ));
                }
                let pair = binary::range(bytes, cursor, 2)?;
                if identities.contains_key(&(cursor + 1)) || terminators.contains(&(cursor + 1)) {
                    return Err(Error("overlapping ring coordinate boundaries".into()));
                }
                if rings.len() == max_rings {
                    return Err(Error("ring record budget exceeded".into()));
                }
                coordinate_tails.insert(cursor + 1);
                let next_id = identities.len();
                let id = *identities.entry(cursor).or_insert(next_id);
                rings.push(Ring {
                    x: ((region % width) * 256 + usize::from(pair[0]) * 8) as i32,
                    y: ((region / width) * 256 + usize::from(pair[1]) * 8) as i32,
                    id,
                });
                cursor += 2;
            }
        }
        Ok(rings)
    }
}

/// Session-local collected flags; resetting the practice session creates a new
/// ledger. These counts never enter the canonical Chao wallet or native saves.
#[derive(Debug, Clone)]
pub struct PracticeRings {
    rings: Vec<Ring>,
    collected: Vec<bool>,
    count: usize,
}

impl PracticeRings {
    pub fn new(rings: Vec<Ring>) -> Result<Self> {
        if rings.len() > MAX_RINGS {
            return Err(Error("practice ring budget exceeded".into()));
        }
        let mut seen = vec![false; rings.len()];
        let mut identities = 0;
        for ring in &rings {
            if ring.id >= rings.len()
                || ring.x < 0
                || ring.y < 0
                || ring.x > 131072
                || ring.y > 131072
            {
                return Err(Error(
                    "invalid practice ring identity or coordinates".into(),
                ));
            }
            seen[ring.id] = true;
            identities = identities.max(ring.id + 1);
        }
        if seen[..identities].iter().any(|present| !present) {
            return Err(Error("practice ring identities are not dense".into()));
        }
        Ok(Self {
            rings,
            collected: vec![false; identities],
            count: 0,
        })
    }

    pub fn rings(&self) -> &[Ring] {
        &self.rings
    }

    /// Invalid IDs are treated as absent, and therefore already unavailable.
    pub fn is_collected(&self, id: usize) -> bool {
        self.collected.get(id).copied().unwrap_or(true)
    }

    pub fn collected_count(&self) -> usize {
        self.count
    }

    /// Pixel-coordinate, inclusive rectangle contact from RECT_TOUCHING_RING:
    /// ring bounds [x-8,x+8] by [y-16,y], against caller-supplied body offsets.
    /// Normal standing offsets are (6,14); jump/roll offsets require state work.
    pub fn collect_at(
        &mut self,
        center_x: i32,
        center_y: i32,
        half_width: i32,
        half_height: i32,
    ) -> Result<usize> {
        if !(0..=64).contains(&half_width) || !(0..=64).contains(&half_height) {
            return Err(Error("practice pickup body bounds exceeded".into()));
        }
        let left = i64::from(center_x) - i64::from(half_width);
        let right = i64::from(center_x) + i64::from(half_width);
        let top = i64::from(center_y) - i64::from(half_height);
        let bottom = i64::from(center_y) + i64::from(half_height);
        let mut gained = 0;
        for ring in &self.rings {
            let x = i64::from(ring.x);
            let y = i64::from(ring.y);
            if !self.collected[ring.id]
                && left <= x + 8
                && right >= x - 8
                && top <= y
                && bottom >= y - 16
            {
                self.collected[ring.id] = true;
                gained += 1;
            }
        }
        self.count += gained;
        Ok(gained)
    }
}
