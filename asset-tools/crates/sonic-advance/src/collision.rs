//! Borrowed ASOE collision geometry and bounded pixel probes.
//!
//! Format facts: 12x12 little-endian tile words per metatile, byte-valued
//! metatile grids, two signed boundary nibbles per height-map byte, and packed
//! two-bit tile flags. The direction-specific native boundary/angle transforms
//! are represented without executing ROM code. The ray probes below are a
//! deliberately small runtime adapter; they do not reproduce native player
//! penetration recovery, sensor placement, wall running or loop physics.
use crate::{binary, Error, Result};

pub const COLLISION_TABLE_OFFSET: usize = 0x4a6da4;
pub const COLLISION_STAGE_COUNT: usize = 18;
pub const MAX_GRID_ENTRIES: usize = 1024 * 1024;
pub const MAX_PROBE_DISTANCE: u16 = 4096;
const METATILE_TILES: usize = 12;
const METATILE_PIXELS: usize = METATILE_TILES * 8;
const TILE_WORDS_PER_METATILE: usize = METATILE_TILES * METATILE_TILES;

#[derive(Debug)]
pub struct CollisionStage<'a> {
    header_offset: usize,
    raw_header: &'a [u8],
    offsets: [usize; 6],
    width_metatiles: u16,
    height_metatiles: u16,
    pixel_width: u16,
    pixel_height: u16,
    height_map: &'a [u8],
    angles: &'a [u8],
    metatiles: &'a [u8],
    maps: [&'a [u8]; 2],
    flags: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionSample {
    pub metatile_index: u8,
    pub tile_word: u16,
    pub tile_index: u16,
    /// Preserved two-bit value. Bit zero is suppressed by layer option 0x80;
    /// this reader does not invent gameplay names for either flag.
    pub flags: u8,
    /// Zero is empty; eight is full. Other positive values are the first
    /// occupied pixel coordinate; a negative value is the excluded upper bound.
    pub vertical_boundary: i8,
    pub horizontal_boundary: i8,
    pub vertical_angle: u8,
    pub horizontal_angle: u8,
    pub solid_vertical: bool,
    pub solid_horizontal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeHit {
    pub x: i32,
    pub y: i32,
    pub distance: u16,
    pub angle: u8,
    pub flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeDirection {
    Floor,
    Ceiling,
    Left,
    Right,
}

fn nibble(value: u8) -> i8 {
    match value & 15 {
        8 => 8,
        n @ 9..=15 => n as i8 - 16,
        n => n as i8,
    }
}

fn mirror_boundary(boundary: i8) -> i8 {
    match boundary {
        1..=7 => boundary - 8,
        -7..=-1 => boundary + 8,
        _ => boundary,
    }
}

fn occupied(boundary: i8, coordinate: u8) -> bool {
    match boundary {
        8 => true,
        1..=7 => coordinate >= boundary as u8,
        -7..=-1 => coordinate < (-boundary) as u8,
        _ => false,
    }
}

impl<'a> CollisionStage<'a> {
    /// Use only the complete, fingerprinted USA revision-zero cartridge.
    pub fn load_asoe(rom: &'a [u8], stage_index: usize) -> Result<Self> {
        binary::verify_asoe(rom)?;
        if stage_index >= COLLISION_STAGE_COUNT {
            return Err(Error("ASOE collision stage index out of range".into()));
        }
        let pointer = binary::u32_at(rom, COLLISION_TABLE_OFFSET + stage_index * 4)?;
        let at = binary::rom_offset(pointer, rom.len(), 4)?;
        Self::parse(rom, at)
    }

    /// Parse a generic 32-byte SA1 collision header for synthetic fixtures or a
    /// previously fingerprinted source. Borrow only the graph's needed prefixes;
    /// these lengths do not assert complete original array extents.
    pub fn parse(rom: &'a [u8], header_offset: usize) -> Result<Self> {
        let raw_header = binary::range(rom, header_offset, 32)?;
        let mut offsets = [0; 6];
        for (index, offset) in offsets.iter_mut().enumerate() {
            let alignment = if index == 2 || index == 5 { 2 } else { 1 };
            *offset =
                binary::rom_offset(binary::u32_at(raw_header, index * 4)?, rom.len(), alignment)?;
        }
        let width_metatiles = binary::u16_at(raw_header, 24)?;
        let height_metatiles = binary::u16_at(raw_header, 26)?;
        let pixel_width = binary::u16_at(raw_header, 28)?;
        let pixel_height = binary::u16_at(raw_header, 30)?;
        let grid_entries = usize::from(width_metatiles)
            .checked_mul(usize::from(height_metatiles))
            .ok_or_else(|| Error("collision grid overflow".into()))?;
        if grid_entries == 0
            || grid_entries > MAX_GRID_ENTRIES
            || pixel_width == 0
            || pixel_height == 0
            || usize::from(pixel_width) > usize::from(width_metatiles) * METATILE_PIXELS
            || usize::from(pixel_height) > usize::from(height_metatiles) * METATILE_PIXELS
        {
            return Err(Error("invalid collision dimensions or grid budget".into()));
        }
        let maps = [
            binary::range(rom, offsets[3], grid_entries)?,
            binary::range(rom, offsets[4], grid_entries)?,
        ];
        let metatile_count = usize::from(
            *maps
                .iter()
                .flat_map(|map| map.iter())
                .max()
                .ok_or_else(|| Error("empty collision grid".into()))?,
        ) + 1;
        let metatiles = binary::range(
            rom,
            offsets[2],
            metatile_count * TILE_WORDS_PER_METATILE * 2,
        )?;
        let tile_count = metatiles
            .as_chunks::<2>()
            .0
            .iter()
            .map(|word| usize::from(u16::from_le_bytes(*word) & 1023))
            .max()
            .ok_or_else(|| Error("empty metatile prefix".into()))?
            + 1;
        Ok(Self {
            header_offset,
            raw_header,
            offsets,
            width_metatiles,
            height_metatiles,
            pixel_width,
            pixel_height,
            height_map: binary::range(rom, offsets[0], tile_count * 8)?,
            angles: binary::range(rom, offsets[1], tile_count)?,
            metatiles,
            maps,
            flags: binary::range(rom, offsets[5], tile_count.div_ceil(8) * 2)?,
        })
    }

    pub fn header_offset(&self) -> usize {
        self.header_offset
    }

    pub fn raw_header(&self) -> &'a [u8] {
        self.raw_header
    }

    /// Height map, angles, metatile words, front grid, back grid, flags.
    pub fn offsets(&self) -> [usize; 6] {
        self.offsets
    }

    pub fn width_metatiles(&self) -> u16 {
        self.width_metatiles
    }

    pub fn height_metatiles(&self) -> u16 {
        self.height_metatiles
    }

    pub fn pixel_width(&self) -> u16 {
        self.pixel_width
    }

    pub fn pixel_height(&self) -> u16 {
        self.pixel_height
    }

    pub fn metatile_prefix_count(&self) -> usize {
        self.metatiles.len() / (TILE_WORDS_PER_METATILE * 2)
    }

    pub fn tile_prefix_count(&self) -> usize {
        self.angles.len()
    }

    /// Outside coordinates produce None rather than native edge clamping.
    /// Layer 0/1 selects the map; option 0x80 suppresses flag-bit-zero tiles.
    /// Other layer bits are rejected, not silently masked into another map.
    pub fn sample(&self, x: i32, y: i32, layer: u8) -> Result<Option<CollisionSample>> {
        if layer & !0x81 != 0 {
            return Err(Error("unsupported collision layer bits".into()));
        }
        if x < 0 || y < 0 || x >= i32::from(self.pixel_width) || y >= i32::from(self.pixel_height) {
            return Ok(None);
        }
        let x = x as usize;
        let y = y as usize;
        let grid_position =
            (y / METATILE_PIXELS) * usize::from(self.width_metatiles) + x / METATILE_PIXELS;
        let metatile_index = self.maps[usize::from(layer & 1)][grid_position];
        let word_position = usize::from(metatile_index) * TILE_WORDS_PER_METATILE
            + (y / 8 % METATILE_TILES) * METATILE_TILES
            + x / 8 % METATILE_TILES;
        let tile_word = binary::u16_at(self.metatiles, word_position * 2)?;
        let tile_index = tile_word & 1023;
        let index = usize::from(tile_index);
        let flip_x = tile_word & 0x400 != 0;
        let flip_y = tile_word & 0x800 != 0;
        let column = if flip_x { 7 - x % 8 } else { x % 8 };
        let row = if flip_y { 7 - y % 8 } else { y % 8 };
        let mut vertical_boundary = nibble(self.height_map[index * 8 + column] >> 4);
        let mut horizontal_boundary = nibble(self.height_map[index * 8 + row]);
        let flag_word = binary::u16_at(self.flags, index / 8 * 2)?;
        let flags = ((flag_word >> ((index % 8) * 2)) & 3) as u8;
        if layer & 0x80 != 0 && flags & 1 != 0 {
            vertical_boundary = 0;
            horizontal_boundary = 0;
        }
        if flip_y {
            vertical_boundary = mirror_boundary(vertical_boundary);
        }
        if flip_x {
            horizontal_boundary = mirror_boundary(horizontal_boundary);
        }
        // Empty directional profiles have deliberately different angle rules.
        let mut vertical_angle = self.angles[index];
        if flip_x {
            vertical_angle = vertical_angle.wrapping_neg();
        }
        if flip_y && vertical_boundary != 0 {
            vertical_angle = 128_u8.wrapping_sub(vertical_angle);
        }
        let mut horizontal_angle = self.angles[index];
        if flip_y {
            horizontal_angle = 128_u8.wrapping_sub(horizontal_angle);
        }
        if flip_x && horizontal_boundary != 0 {
            horizontal_angle = horizontal_angle.wrapping_neg();
        }
        Ok(Some(CollisionSample {
            metatile_index,
            tile_word,
            tile_index,
            flags,
            vertical_boundary,
            horizontal_boundary,
            vertical_angle,
            horizontal_angle,
            solid_vertical: occupied(vertical_boundary, (y % 8) as u8),
            solid_horizontal: occupied(horizontal_boundary, (x % 8) as u8),
        }))
    }

    /// Find the first occupied pixel, including the starting pixel. This is a
    /// bounded ray query over the decoded geometry, not native player physics.
    pub fn probe(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        direction: ProbeDirection,
        max_distance: u16,
    ) -> Result<Option<ProbeHit>> {
        if max_distance > MAX_PROBE_DISTANCE {
            return Err(Error("collision probe distance exceeds budget".into()));
        }
        let (dx, dy, vertical) = match direction {
            ProbeDirection::Floor => (0_i32, 1_i32, true),
            ProbeDirection::Ceiling => (0, -1, true),
            ProbeDirection::Left => (-1, 0, false),
            ProbeDirection::Right => (1, 0, false),
        };
        for distance in 0..=max_distance {
            let Some(px) = x.checked_add(dx * i32::from(distance)) else {
                continue;
            };
            let Some(py) = y.checked_add(dy * i32::from(distance)) else {
                continue;
            };
            let Some(sample) = self.sample(px, py, layer)? else {
                continue;
            };
            if if vertical {
                sample.solid_vertical
            } else {
                sample.solid_horizontal
            } {
                return Ok(Some(ProbeHit {
                    x: px,
                    y: py,
                    distance,
                    angle: if vertical {
                        sample.vertical_angle
                    } else {
                        sample.horizontal_angle
                    },
                    flags: sample.flags,
                }));
            }
        }
        Ok(None)
    }

    pub fn floor(&self, x: i32, y: i32, layer: u8, max_distance: u16) -> Result<Option<ProbeHit>> {
        self.probe(x, y, layer, ProbeDirection::Floor, max_distance)
    }

    pub fn ceiling(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        max_distance: u16,
    ) -> Result<Option<ProbeHit>> {
        self.probe(x, y, layer, ProbeDirection::Ceiling, max_distance)
    }

    pub fn wall_left(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        max_distance: u16,
    ) -> Result<Option<ProbeHit>> {
        self.probe(x, y, layer, ProbeDirection::Left, max_distance)
    }

    pub fn wall_right(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        max_distance: u16,
    ) -> Result<Option<ProbeHit>> {
        self.probe(x, y, layer, ProbeDirection::Right, max_distance)
    }
}
