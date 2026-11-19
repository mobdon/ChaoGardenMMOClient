//! ASOE foreground metatiles, native collision profiles and a software viewport.
//!
//! The source graph uses byte-sized metatile indices and 12-by-12 tile chunks.
//! This is separate from the game camera, object scheduler and player resolver.
use crate::{binary, tables, Error, Result};

pub const TILEMAP_TABLE_OFFSET: usize = 0x4a6c40;
pub const COLLISION_TABLE_OFFSET: usize = 0x4a6da4;
pub const STAGE_COUNT: usize = 18;
pub const CHUNK_TILES: usize = 12;
pub const CHUNK_PIXELS: usize = CHUNK_TILES * 8;
pub const MAX_VIEW_PIXELS: usize = 1024 * 1024;
pub const MAX_PROBE_DISTANCE: i32 = 512;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub name: String,
    pub offset: usize,
    pub length: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageEvidence {
    pub stage_index: usize,
    pub source_sha256: String,
    pub width_pixels: usize,
    pub height_pixels: usize,
    pub referenced_metatiles: usize,
    pub referenced_tile_prefix: usize,
    /// Required bounded prefixes, not claims about entire native array extents.
    pub spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionSample {
    pub tile_word: u16,
    pub tile_index: u16,
    /// High signed nibble, sampled at x; -8 is the native full-tile sentinel.
    pub vertical_height: i8,
    /// Low signed nibble, sampled at y; -8 is the native full-tile sentinel.
    pub horizontal_width: i8,
    pub flags: u8,
    pub angle: u8,
    pub horizontal_angle: u8,
}

impl CollisionSample {
    pub fn solid_at_y(self, y_in_tile: u8) -> bool {
        let y = i16::from(y_in_tile & 7);
        let height = i16::from(self.vertical_height);
        if height > 0 {
            y >= 8 - height
        } else if height < 0 {
            y < -height
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundContact {
    pub surface_y: i32,
    pub angle: u8,
}

#[derive(Debug)]
struct Layer<'a> {
    tiles: &'a [u8],
    map: &'a [u8],
    metatiles: &'a [u8],
    width: usize,
    height: usize,
    palette: [u32; 256],
}

#[derive(Debug)]
struct TileAnimation<'a> {
    frames_after_base: &'a [u8],
    frame_bytes: usize,
    count: u8,
    delay: u8,
}

impl TileAnimation<'_> {
    fn frame(&self, tick: u64) -> Option<&[u8]> {
        let phase = (tick / u64::from(self.delay.max(1))) % u64::from(self.count);
        if phase == 0 {
            return None;
        }
        let at = (phase as usize - 1) * self.frame_bytes;
        Some(&self.frames_after_base[at..at + self.frame_bytes])
    }
}

#[derive(Debug)]
struct Background<'a> {
    tiles: &'a [u8],
    map: &'a [u8],
    width_tiles: usize,
    height_tiles: usize,
    palette: [u32; 256],
}

/// A borrowed, immutable stage graph. Native bytes never enter public output.
#[derive(Debug)]
pub struct Stage<'a> {
    stage_index: usize,
    layers: [Layer<'a>; 2],
    tile_animation: Option<TileAnimation<'a>>,
    background: Background<'a>,
    heights: &'a [u8],
    rotations: &'a [u8],
    flags: &'a [u8],
    width_pixels: usize,
    height_pixels: usize,
    evidence: StageEvidence,
}

fn pointer(rom: &[u8], at: usize, alignment: usize) -> Result<usize> {
    binary::rom_offset(binary::u32_at(rom, at)?, rom.len(), alignment)
}

fn span<'a>(
    rom: &'a [u8],
    name: &str,
    offset: usize,
    length: usize,
    evidence: &mut Vec<SourceSpan>,
) -> Result<&'a [u8]> {
    let bytes = binary::range(rom, offset, length)?;
    evidence.push(SourceSpan {
        name: name.into(),
        offset,
        length,
        sha256: binary::sha256(bytes),
    });
    Ok(bytes)
}

fn word(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
}

fn palette(
    rom: &[u8],
    map: &tables::Tilemap<'_>,
    shared: Option<[u32; 256]>,
) -> Result<[u32; 256]> {
    let Some(offset) = map.palette_offset else {
        return shared.ok_or_else(|| Error("stage has no palette or shared palette".into()));
    };
    let mut colors = shared.unwrap_or([0; 256]);
    let count = usize::from(map.palette_color_count);
    let start = usize::from(map.palette_color_offset);
    if shared.is_none() && (start != 0 || count != 256) {
        return Err(Error(
            "stage renderer needs a complete initial palette or a shared palette".into(),
        ));
    }
    let bytes = binary::range(rom, offset, count * 2)?;
    for i in 0..count {
        let value = word(bytes, i);
        let expand = |v: u16| u32::from((v << 3) | (v >> 2));
        colors[start + i] = (expand(value & 31) << 16)
            | (expand((value >> 5) & 31) << 8)
            | expand((value >> 10) & 31);
    }
    Ok(colors)
}

fn validate_tile_words(words: &[u8], tiles: &[u8]) -> Result<usize> {
    if tiles.is_empty() || !tiles.len().is_multiple_of(32) {
        return Err(Error("stage requires complete native 4bpp tiles".into()));
    }
    let mut maximum = 0;
    for i in 0..words.len() / 2 {
        let index = usize::from(word(words, i) & 1023);
        if index >= tiles.len() / 32 {
            return Err(Error("stage map references a missing tile".into()));
        }
        maximum = maximum.max(index);
    }
    Ok(maximum + 1)
}

impl<'a> Stage<'a> {
    pub fn load(rom: &'a [u8], stage_index: usize) -> Result<Self> {
        binary::verify_asoe(rom)?;
        if stage_index >= STAGE_COUNT {
            return Err(Error(
                "ASOE stage index outside 18 native stage slots".into(),
            ));
        }
        let collision = pointer(rom, COLLISION_TABLE_OFFSET + stage_index * 4, 4)?;
        let maps = [
            pointer(rom, TILEMAP_TABLE_OFFSET + stage_index * 12, 4)?,
            pointer(rom, TILEMAP_TABLE_OFFSET + stage_index * 12 + 4, 4)?,
        ];
        let background = pointer(rom, TILEMAP_TABLE_OFFSET + stage_index * 12 + 8, 4)?;
        Self::from_records(rom, stage_index, collision, maps, background)
    }

    /// Generic bounded graph reader for explicitly supplied record locations.
    /// Only `load` selects ASOE's version-specific table locations.
    pub fn from_records(
        rom: &'a [u8],
        stage_index: usize,
        collision_offset: usize,
        map_offsets: [usize; 2],
        background_offset: usize,
    ) -> Result<Self> {
        if stage_index >= STAGE_COUNT {
            return Err(Error("stage index exceeds hard limit".into()));
        }
        let mut spans = Vec::new();
        let collision = span(rom, "collision-header", collision_offset, 32, &mut spans)?;
        let width = usize::from(binary::u16_at(collision, 24)?);
        let height = usize::from(binary::u16_at(collision, 26)?);
        let width_pixels = usize::from(binary::u16_at(collision, 28)?);
        let height_pixels = usize::from(binary::u16_at(collision, 30)?);
        if width == 0
            || height == 0
            || width > 1024
            || height > 1024
            || width_pixels != width * CHUNK_PIXELS
            || height_pixels != height * CHUNK_PIXELS
        {
            return Err(Error(
                "stage dimensions disagree with native 96-pixel chunks".into(),
            ));
        }
        let count = width
            .checked_mul(height)
            .ok_or_else(|| Error("stage size overflow".into()))?;
        if count > 65536 {
            return Err(Error("stage metatile map exceeds hard budget".into()));
        }
        // Pointers in a header are relative to the original ROM, not its slice.
        let map_at = [
            binary::rom_offset(binary::u32_at(collision, 12)?, rom.len(), 1)?,
            binary::rom_offset(binary::u32_at(collision, 16)?, rom.len(), 1)?,
        ];
        let maps = [
            span(rom, "foreground-0-byte-map", map_at[0], count, &mut spans)?,
            span(rom, "foreground-1-byte-map", map_at[1], count, &mut spans)?,
        ];
        let metatile_count = usize::from(*maps.iter().flat_map(|m| m.iter()).max().unwrap()) + 1;
        let metatile_offset = binary::rom_offset(binary::u32_at(collision, 8)?, rom.len(), 2)?;
        let metatiles = span(
            rom,
            "referenced-metatile-prefix",
            metatile_offset,
            metatile_count * CHUNK_TILES * CHUNK_TILES * 2,
            &mut spans,
        )?;
        let mut built_layers = Vec::new();
        let mut tile_animation = None;
        let mut shared_palette = None;
        let mut referenced_tiles = 0;
        for i in 0..2 {
            let header = tables::foreground_map(rom, map_offsets[i], 65536)?;
            if i == 0 && header.tileset.animation_frames > 1 {
                let bytes = usize::from(header.tileset.animation_tile_bytes);
                if bytes == 0 || bytes % 32 != 0 || bytes > header.tileset.tiles.len() {
                    return Err(Error("invalid foreground animation frame size".into()));
                }
                let at = header.tileset.tile_offset + header.tileset.tiles.len();
                tile_animation = Some(TileAnimation {
                    frames_after_base: span(
                        rom,
                        "foreground-animation-frames",
                        at,
                        bytes * usize::from(header.tileset.animation_frames - 1),
                        &mut spans,
                    )?,
                    frame_bytes: bytes,
                    count: header.tileset.animation_frames,
                    delay: header.tileset.animation_delay,
                });
            }
            span(
                rom,
                &format!("foreground-{i}-header"),
                map_offsets[i],
                36,
                &mut spans,
            )?;
            if usize::from(header.width_metatiles) != width
                || usize::from(header.height_metatiles) != height
                || header.metatile_map_offset != map_at[i]
                || header.tileset.map_offset != metatile_offset
                || header.tileset.width_tiles != CHUNK_TILES as u16
                || header.tileset.height_tiles != CHUNK_TILES as u16
            {
                return Err(Error("foreground and collision graphs disagree".into()));
            }
            let colors = palette(rom, &header.tileset, shared_palette)?;
            shared_palette = Some(colors);
            span(
                rom,
                &format!("foreground-{i}-tiles"),
                header.tileset.tile_offset,
                header.tileset.tiles.len(),
                &mut spans,
            )?;
            if let Some(at) = header.tileset.palette_offset {
                span(
                    rom,
                    &format!("foreground-{i}-palette"),
                    at,
                    usize::from(header.tileset.palette_color_count) * 2,
                    &mut spans,
                )?;
            }
            referenced_tiles =
                referenced_tiles.max(validate_tile_words(metatiles, header.tileset.tiles)?);
            built_layers.push(Layer {
                tiles: header.tileset.tiles,
                map: maps[i],
                metatiles,
                width,
                height,
                palette: colors,
            });
        }
        let height_at = binary::rom_offset(binary::u32_at(collision, 0)?, rom.len(), 1)?;
        let rotation_at = binary::rom_offset(binary::u32_at(collision, 4)?, rom.len(), 1)?;
        let flags_at = binary::rom_offset(binary::u32_at(collision, 20)?, rom.len(), 2)?;
        let heights = span(
            rom,
            "referenced-height-prefix",
            height_at,
            referenced_tiles * 8,
            &mut spans,
        )?;
        let rotations = span(
            rom,
            "referenced-rotation-prefix",
            rotation_at,
            referenced_tiles,
            &mut spans,
        )?;
        let flags = span(
            rom,
            "referenced-flags-prefix",
            flags_at,
            referenced_tiles.div_ceil(8) * 2,
            &mut spans,
        )?;
        let bg = tables::tilemap(rom, background_offset, 65536)?;
        span(rom, "background-header", background_offset, 28, &mut spans)?;
        let bg_count = usize::from(bg.width_tiles) * usize::from(bg.height_tiles);
        let bg_map = span(
            rom,
            "background-linear-map",
            bg.map_offset,
            bg_count * 2,
            &mut spans,
        )?;
        span(
            rom,
            "background-tiles",
            bg.tile_offset,
            bg.tiles.len(),
            &mut spans,
        )?;
        let bg_palette = palette(rom, &bg, shared_palette)?;
        if let Some(at) = bg.palette_offset {
            span(
                rom,
                "background-palette",
                at,
                usize::from(bg.palette_color_count) * 2,
                &mut spans,
            )?;
        }
        validate_tile_words(bg_map, bg.tiles)?;
        let background = Background {
            tiles: bg.tiles,
            map: bg_map,
            width_tiles: usize::from(bg.width_tiles),
            height_tiles: usize::from(bg.height_tiles),
            palette: bg_palette,
        };
        let layers: [Layer<'a>; 2] = built_layers
            .try_into()
            .map_err(|_| Error("missing stage layer".into()))?;
        Ok(Self {
            stage_index,
            tile_animation,
            layers,
            background,
            heights,
            rotations,
            flags,
            width_pixels,
            height_pixels,
            evidence: StageEvidence {
                stage_index,
                source_sha256: binary::sha256(rom),
                width_pixels,
                height_pixels,
                referenced_metatiles: metatile_count,
                referenced_tile_prefix: referenced_tiles,
                spans,
            },
        })
    }

    pub fn width_px(&self) -> usize {
        self.width_pixels
    }
    pub fn height_px(&self) -> usize {
        self.height_pixels
    }
    pub fn evidence(&self) -> &StageEvidence {
        &self.evidence
    }

    /// Out-of-world coordinates are empty. Unlike the native clamp helper, this
    /// does not turn a probe beyond the world into the last world-edge tile.
    pub fn tile_word(&self, x: i32, y: i32, layer: usize) -> Result<Option<u16>> {
        let layer = self
            .layers
            .get(layer)
            .ok_or_else(|| Error("invalid collision layer".into()))?;
        if x < 0 || y < 0 || x as usize >= self.width_pixels || y as usize >= self.height_pixels {
            return Ok(None);
        }
        Ok(Some(layer.word_at(x as usize, y as usize)))
    }

    pub fn collision_at(&self, x: i32, y: i32, layer: usize) -> Result<Option<CollisionSample>> {
        let Some(tile_word) = self.tile_word(x, y, layer)? else {
            return Ok(None);
        };
        let index = usize::from(tile_word & 1023);
        let flip_x = tile_word & 0x400 != 0;
        let flip_y = tile_word & 0x800 != 0;
        let sample_x = if flip_x {
            7 - (x as usize & 7)
        } else {
            x as usize & 7
        };
        let sample_y = if flip_y {
            7 - (y as usize & 7)
        } else {
            y as usize & 7
        };
        let mut vertical = (self.heights[index * 8 + sample_x] as i8) >> 4;
        let mut horizontal = ((self.heights[index * 8 + sample_y] << 4) as i8) >> 4;
        if vertical == -8 {
            vertical = 8;
        }
        if horizontal == -8 {
            horizontal = 8;
        }
        let mirror_height = |h: i8| {
            if h != 0 && h != 8 {
                if h > 0 {
                    h - 8
                } else {
                    h + 8
                }
            } else {
                h
            }
        };
        if flip_y {
            vertical = mirror_height(vertical);
        }
        if flip_x {
            horizontal = mirror_height(horizontal);
        }
        let mut angle = self.rotations[index];
        if flip_x {
            angle = angle.wrapping_neg();
        }
        if flip_y && vertical != 0 {
            angle = 128_u8.wrapping_sub(angle);
        }
        let mut horizontal_angle = self.rotations[index];
        if flip_y {
            horizontal_angle = 128_u8.wrapping_sub(horizontal_angle);
        }
        if flip_x && horizontal != 0 {
            horizontal_angle = horizontal_angle.wrapping_neg();
        }
        let flags = ((word(self.flags, index / 8) >> ((index % 8) * 2)) & 3) as u8;
        Ok(Some(CollisionSample {
            tile_word,
            tile_index: index as u16,
            vertical_height: vertical,
            horizontal_width: horizontal,
            flags,
            angle,
            horizontal_angle,
        }))
    }

    /// Pixel occupancy from the native signed vertical profile, with world
    /// bounds checked before querying the graph. One-way/path policy is left
    /// to a resolver; the raw native per-tile flags remain available above.
    pub fn solid_at(&self, x: i32, y: i32, layer: usize) -> Result<bool> {
        Ok(self
            .collision_at(x, y, layer)?
            .is_some_and(|sample| sample.solid_at_y((y & 7) as u8)))
    }

    /// Bounded floor-boundary adapter for the standalone testbed. This is not
    /// the original multi-sensor directional/loop/one-way collision resolver.
    pub fn ground_below(
        &self,
        x: i32,
        y: i32,
        max_distance: i32,
        layer: usize,
    ) -> Result<Option<GroundContact>> {
        if !(0..=MAX_PROBE_DISTANCE).contains(&max_distance) {
            return Err(Error("floor probe exceeds 512-pixel budget".into()));
        }
        if layer >= 2 {
            return Err(Error("invalid collision layer".into()));
        }
        let solid = |y: i32| -> Result<Option<CollisionSample>> {
            Ok(self
                .collision_at(x, y, layer)?
                .filter(|s| s.solid_at_y((y & 7) as u8) && (s.angle.wrapping_add(32) & 0xc0) == 0))
        };
        if let Some(sample) = solid(y)? {
            let mut surface = y;
            for _ in 0..max_distance {
                let Some(previous) = surface.checked_sub(1) else {
                    break;
                };
                if solid(previous)?.is_none() {
                    return Ok(Some(GroundContact {
                        surface_y: surface,
                        angle: sample.angle,
                    }));
                }
                surface = previous;
            }
            return Ok(None);
        }
        for distance in 1..=max_distance {
            let Some(probe_y) = y.checked_add(distance) else {
                break;
            };
            if let Some(sample) = solid(probe_y)? {
                return Ok(Some(GroundContact {
                    surface_y: probe_y,
                    angle: sample.angle,
                }));
            }
        }
        Ok(None)
    }

    /// Native front background (priority 1), for composition after priority-2
    /// actors. Index zero preserves the existing actor/background pixel.
    pub fn overlay_front(
        &self,
        camera_x: i32,
        camera_y: i32,
        width: usize,
        height: usize,
        tick: u64,
        output: &mut [u32],
    ) -> Result<()> {
        let count = width
            .checked_mul(height)
            .ok_or_else(|| Error("viewport size overflow".into()))?;
        if width == 0 || height == 0 || count > MAX_VIEW_PIXELS || count != output.len() {
            return Err(Error("viewport dimensions or pixel budget invalid".into()));
        }
        let layer = &self.layers[0];
        let animated = self.tile_animation.as_ref().and_then(|a| a.frame(tick));
        for screen_y in 0..height {
            for screen_x in 0..width {
                let world_x = i64::from(camera_x) + screen_x as i64;
                let world_y = i64::from(camera_y) + screen_y as i64;
                if world_x >= 0
                    && world_y >= 0
                    && world_x < self.width_pixels as i64
                    && world_y < self.height_pixels as i64
                {
                    let word = layer.word_at(world_x as usize, world_y as usize);
                    if let Some(value) = tile_pixel(
                        tile_source(layer.tiles, animated, word),
                        &layer.palette,
                        word,
                        world_x as usize & 7,
                        world_y as usize & 7,
                    ) {
                        output[screen_y * width + screen_x] = value;
                    }
                }
            }
        }
        Ok(())
    }

    /// RGB0 pixels, opaque final viewport. Foreground index zero is transparent.
    /// Zone 1's native HBlank cloud/water offsets are reproduced at 240x160;
    /// other stages use their actual static background with no guessed effects.
    /// Tick counts foreground animation updates from the initial character data.
    pub fn render_view(
        &self,
        camera_x: i32,
        camera_y: i32,
        width: usize,
        height: usize,
        tick: u64,
        output: &mut [u32],
    ) -> Result<()> {
        let count = width
            .checked_mul(height)
            .ok_or_else(|| Error("viewport size overflow".into()))?;
        if width == 0 || height == 0 || count > MAX_VIEW_PIXELS || count != output.len() {
            return Err(Error("viewport dimensions or pixel budget invalid".into()));
        }
        let animated = self.tile_animation.as_ref().and_then(|a| a.frame(tick));
        for screen_y in 0..height {
            let bg_shift = if self.stage_index < 2 && width == 240 && height == 160 {
                match screen_y {
                    0..8 => ((tick >> 4) & 255) as i64,
                    8..24 => ((tick >> 5) & 255) as i64,
                    24..40 => ((tick >> 6) & 255) as i64,
                    40..88 => 0,
                    _ => ((screen_y as i64 - 86) * i64::from(camera_x)) >> 8,
                }
            } else {
                0
            };
            for screen_x in 0..width {
                let mut color = self
                    .background
                    .pixel(screen_x as i64 + bg_shift, screen_y as i64);
                let world_x = i64::from(camera_x) + screen_x as i64;
                let world_y = i64::from(camera_y) + screen_y as i64;
                if world_x >= 0
                    && world_y >= 0
                    && world_x < self.width_pixels as i64
                    && world_y < self.height_pixels as i64
                {
                    for layer in self.layers.iter().rev() {
                        let w = layer.word_at(world_x as usize, world_y as usize);
                        let uploaded = if layer.tiles.as_ptr() == self.layers[0].tiles.as_ptr() {
                            animated
                        } else {
                            None
                        };
                        if let Some(value) = tile_pixel(
                            tile_source(layer.tiles, uploaded, w),
                            &layer.palette,
                            w,
                            world_x as usize & 7,
                            world_y as usize & 7,
                        ) {
                            color = value;
                        }
                    }
                }
                output[screen_y * width + screen_x] = color;
            }
        }
        Ok(())
    }
}

impl Layer<'_> {
    fn word_at(&self, x: usize, y: usize) -> u16 {
        debug_assert!(x < self.width * CHUNK_PIXELS && y < self.height * CHUNK_PIXELS);
        let chunk = usize::from(self.map[(y / CHUNK_PIXELS) * self.width + x / CHUNK_PIXELS]);
        let local = ((y / 8) % CHUNK_TILES) * CHUNK_TILES + (x / 8) % CHUNK_TILES;
        word(self.metatiles, chunk * CHUNK_TILES * CHUNK_TILES + local)
    }
}

fn tile_source<'a>(base: &'a [u8], prefix: Option<&'a [u8]>, entry: u16) -> &'a [u8] {
    prefix
        .filter(|p| usize::from(entry & 1023) < p.len() / 32)
        .unwrap_or(base)
}

fn tile_pixel(
    tiles: &[u8],
    palette: &[u32; 256],
    entry: u16,
    mut x: usize,
    mut y: usize,
) -> Option<u32> {
    if entry & 0x400 != 0 {
        x = 7 - x;
    }
    if entry & 0x800 != 0 {
        y = 7 - y;
    }
    let nibble = y * 8 + x;
    let byte = tiles[usize::from(entry & 1023) * 32 + nibble / 2];
    let index = if nibble & 1 == 0 {
        byte & 15
    } else {
        byte >> 4
    };
    if index == 0 {
        None
    } else {
        Some(palette[usize::from(entry >> 12) * 16 + usize::from(index)])
    }
}

impl Background<'_> {
    fn pixel(&self, x: i64, y: i64) -> u32 {
        let x = x.rem_euclid((self.width_tiles * 8) as i64) as usize;
        let y = y.rem_euclid((self.height_tiles * 8) as i64) as usize;
        let entry = word(self.map, (y / 8) * self.width_tiles + x / 8);
        tile_pixel(self.tiles, &self.palette, entry, x & 7, y & 7).unwrap_or(self.palette[0])
    }
}
