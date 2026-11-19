//! Native RLE spatial ring records. Collection belongs only to an ephemeral
//! local test run; this module has no account, profile or MMO reward path.
use crate::{binary, Error, Result};
use std::collections::BTreeSet;

pub const TABLE_OFFSET: usize = 0xb7f4c;
pub const STAGE_COUNT: usize = 18;
pub const MAX_LAYOUT_BYTES: usize = 0x40000;
pub const MAX_RINGS: usize = 65536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ring {
    /// Byte address in the decoded region blob, also the native removal key.
    pub record_offset: usize,
    pub region_x: usize,
    pub region_y: usize,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct RingLayout {
    pub width_regions: usize,
    pub height_regions: usize,
    pub rings: Vec<Ring>,
    pub compressed_offset: Option<usize>,
    pub compressed_length: Option<usize>,
    pub compressed_sha256: Option<String>,
    pub decoded_sha256: String,
}

impl RingLayout {
    pub fn load(rom: &[u8], stage_id: usize) -> Result<Self> {
        binary::verify_asoe(rom)?;
        if stage_id >= STAGE_COUNT {
            return Err(Error("ring stage index exceeds the ASOE table".into()));
        }
        let pointer = binary::u32_at(rom, TABLE_OFFSET + stage_id * 4)?;
        let offset = binary::rom_offset(pointer, rom.len(), 4)?;
        let source = binary::range(rom, offset, rom.len() - offset)?;
        if source.first() != Some(&0x30) {
            return Err(Error("ring layout requires native type-0x30 RLE".into()));
        }
        let decoded = binary::decompress(source, MAX_LAYOUT_BYTES)?;
        let mut layout = Self::parse(&decoded.bytes)?;
        layout.compressed_offset = Some(offset);
        layout.compressed_length = Some(decoded.consumed);
        layout.compressed_sha256 = Some(binary::sha256(&source[..decoded.consumed]));
        Ok(layout)
    }

    /// The region offset base is byte four, not the start of the offset array.
    /// Coordinates are native eight-pixel units and may exceed 31 at a seam.
    pub fn parse(decoded: &[u8]) -> Result<Self> {
        if decoded.len() > MAX_LAYOUT_BYTES || decoded.len() < 12 {
            return Err(Error("ring layout size is outside the read budget".into()));
        }
        if binary::u32_at(decoded, 0)? as usize >> 8 != decoded.len() {
            return Err(Error(
                "ring layout inner length disagrees with RLE output".into(),
            ));
        }
        let width = binary::u32_at(decoded, 4)? as usize;
        let height = binary::u32_at(decoded, 8)? as usize;
        if width == 0 || height == 0 || width > 256 || height > 256 {
            return Err(Error(
                "ring region dimensions exceed the read budget".into(),
            ));
        }
        let regions = width
            .checked_mul(height)
            .ok_or_else(|| Error("ring region overflow".into()))?;
        let table_end = 12 + regions * 4;
        binary::range(decoded, 12, regions * 4)?;
        let mut rings = Vec::new();
        for region in 0..regions {
            let relative = binary::u32_at(decoded, 12 + region * 4)? as usize;
            if relative == 0 {
                continue;
            }
            let mut cursor = relative
                .checked_add(4)
                .ok_or_else(|| Error("ring offset overflow".into()))?;
            if cursor < table_end {
                return Err(Error("ring list overlaps its region table".into()));
            }
            loop {
                let x = binary::range(decoded, cursor, 1)?[0];
                if x == 255 {
                    break;
                }
                if x >= 253 {
                    return Err(Error(
                        "mutated ring state in immutable native layout".into(),
                    ));
                }
                let y = binary::range(decoded, cursor, 2)?[1];
                if rings.len() >= MAX_RINGS {
                    return Err(Error("ring record budget exhausted".into()));
                }
                rings
                    .try_reserve(1)
                    .map_err(|_| Error("ring allocation failed".into()))?;
                rings.push(Ring {
                    record_offset: cursor,
                    region_x: region % width,
                    region_y: region / width,
                    x: ((region % width) * 256 + usize::from(x) * 8) as i32,
                    y: ((region / width) * 256 + usize::from(y) * 8) as i32,
                });
                cursor += 2;
            }
        }
        Ok(Self {
            width_regions: width,
            height_regions: height,
            rings,
            compressed_offset: None,
            compressed_length: None,
            compressed_sha256: None,
            decoded_sha256: binary::sha256(decoded),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct LocalCollection {
    collected: BTreeSet<usize>,
}

impl LocalCollection {
    pub fn contains(&self, ring: &Ring) -> bool {
        self.collected.contains(&ring.record_offset)
    }
    pub fn count(&self) -> usize {
        self.collected.len()
    }

    /// Native upright region scans and inclusive player/ring rectangles.
    /// The caller supplies the live body half height: standing 14 or spin 9.
    /// Magnetism, scattered
    /// rings, extra lives and multiplayer rules are separate unfinished paths.
    pub fn collect(
        &mut self,
        layout: &RingLayout,
        player_x: i32,
        player_y: i32,
        half_height: i32,
    ) -> Result<usize> {
        self.collect_with(layout, player_x, player_y, half_height, |_| {})
    }

    pub(crate) fn collect_with(
        &mut self,
        layout: &RingLayout,
        player_x: i32,
        player_y: i32,
        half_height: i32,
        mut picked: impl FnMut(&Ring),
    ) -> Result<usize> {
        if !matches!(half_height, 9 | 14) {
            return Err(Error("unsupported Sonic ring contact half height".into()));
        }
        let mut added = 0;
        let (px, py) = (i64::from(player_x), i64::from(player_y));
        let half_height = i64::from(half_height);
        let region_left = (px - 6 - 8) >> 8;
        let region_right = (px + 6 - 8) >> 8;
        let region_top = (py - half_height) >> 8;
        let region_bottom = (py + half_height - 16) >> 8;
        // Native loop variables are u16. A negative lower bound wraps beyond
        // the stage's region count and the loop performs no collection.
        if region_left < 0 || region_top < 0 {
            return Ok(0);
        }
        for ring in &layout.rings {
            if !(region_left..=region_right).contains(&(ring.region_x as i64))
                || !(region_top..=region_bottom).contains(&(ring.region_y as i64))
            {
                continue;
            }
            let (rx, ry) = (i64::from(ring.x), i64::from(ring.y));
            if px - 6 <= rx + 8
                && px + 6 >= rx - 8
                && py - half_height <= ry
                && py + half_height >= ry - 16
                && self.collected.insert(ring.record_offset)
            {
                added += 1;
                picked(ring);
            }
        }
        Ok(added)
    }
}
