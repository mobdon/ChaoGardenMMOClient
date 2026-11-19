//! Native zone-one tile composition. Positions are pixels; original 8x8 tiles,
//! palette banks, flips and transparency are retained. The legacy base render
//! is unchanged; render_at adds the source-backed terrain upload schedule and
//! Zone 1 scanline scroll with the camera-origin offset held at its initial zero.
use crate::{binary, graphics, tables, Error, Result};

pub const VIEW_WIDTH: usize = 240;
pub const VIEW_HEIGHT: usize = 160;
pub const STAGE_ZERO_SPAWN: (i32, i32) = (104, 444);
pub const METATILE_TILES: usize = 12;
pub const METATILE_PIXELS: usize = METATILE_TILES * 8;
pub const MAX_FRAME_PIXELS: usize = 1024 * 1024;

/// Decoded prefixes uploaded to the shared foreground character bank. Native
/// phase zero uses the base-bank prefix; subsequent phases use appended blocks.
/// This is a representation of the ordinary single-player upload path, not the
/// separate collect-rings VRAM-source path.
#[derive(Debug, Clone)]
pub struct TileAnimation {
    prefixes: Vec<Vec<u8>>,
    tile_bytes: usize,
    delay: u8,
}

impl TileAnimation {
    pub fn parse(
        base_tiles: &[u8],
        appended: &[u8],
        tile_bytes: usize,
        frame_count: u8,
        delay: u8,
        max_pixels: usize,
    ) -> Result<Self> {
        let count = usize::from(frame_count);
        if count == 0
            || count > 32
            || delay == 0
            || tile_bytes == 0
            || !tile_bytes.is_multiple_of(32)
            || !base_tiles.len().is_multiple_of(32)
            || tile_bytes > base_tiles.len()
        {
            return Err(Error(
                "unsupported terrain animation dimensions or delay".into(),
            ));
        }
        let appended_length = (count - 1)
            .checked_mul(tile_bytes)
            .ok_or_else(|| Error("terrain animation input overflow".into()))?;
        if appended.len() != appended_length {
            return Err(Error(
                "terrain animation appended span has the wrong length".into(),
            ));
        }
        let pixels = count
            .checked_mul(tile_bytes)
            .and_then(|value| value.checked_mul(2))
            .ok_or_else(|| Error("terrain animation pixel overflow".into()))?;
        if pixels > max_pixels.min(MAX_FRAME_PIXELS) {
            return Err(Error("terrain animation pixel budget exceeded".into()));
        }
        let mut prefixes = Vec::new();
        prefixes
            .try_reserve_exact(count)
            .map_err(|_| Error("terrain animation allocation failed".into()))?;
        prefixes.push(graphics::decode_tile_indices(
            &base_tiles[..tile_bytes],
            graphics::BitsPerPixel::Four,
            pixels,
        )?);
        for bytes in appended.chunks_exact(tile_bytes) {
            prefixes.push(graphics::decode_tile_indices(
                bytes,
                graphics::BitsPerPixel::Four,
                pixels,
            )?);
        }
        Ok(Self {
            prefixes,
            tile_bytes,
            delay,
        })
    }

    pub fn tile_bytes(&self) -> usize {
        self.tile_bytes
    }
    pub fn frame_count(&self) -> usize {
        self.prefixes.len()
    }
    pub fn delay(&self) -> u8 {
        self.delay
    }
    pub fn phase_at(&self, completed_updates: u64) -> usize {
        ((completed_updates / u64::from(self.delay)) % self.prefixes.len() as u64) as usize
    }
    pub fn prefix_at(&self, completed_updates: u64) -> &[u8] {
        &self.prefixes[self.phase_at(completed_updates)]
    }
}

/// Native Zone 1 visible-scanline horizontal scroll. The ordinary stage path
/// initializes camera-origin compensation to zero. Positive BG scroll samples
/// pixels to the right; a 256-pixel background wraps the returned low byte.
pub fn zone_one_background_scroll(tick: u64, scanline: usize, camera_x: i32) -> Result<u16> {
    if scanline >= VIEW_HEIGHT || camera_x < 0 {
        return Err(Error(
            "background scanline or camera outside supported bounds".into(),
        ));
    }
    let scroll = match scanline {
        0..=7 => (tick >> 4) & 255,
        8..=23 => (tick >> 5) & 255,
        24..=39 => (tick >> 6) & 255,
        40..=87 => 0,
        _ => (((scanline as u64 - 86) * camera_x as u64) >> 8) & 255,
    };
    Ok(scroll as u16)
}

#[derive(Debug, Clone)]
pub struct Layer {
    grid_width: usize,
    grid_height: usize,
    grid: Vec<u8>,
    metatiles: Vec<u16>,
    tiles: Vec<u8>,
    palette: Vec<[u8; 4]>,
    animation: Option<TileAnimation>,
}

fn tile_color(
    tiles: &[u8],
    palette: &[[u8; 4]],
    word: u16,
    x: usize,
    y: usize,
    prefix: Option<&[u8]>,
) -> Result<Option<[u8; 4]>> {
    let entry = graphics::TextEntry::decode(word);
    let tx = if entry.flip_x { 7 - x } else { x };
    let ty = if entry.flip_y { 7 - y } else { y };
    let tiles = prefix
        .filter(|bytes| usize::from(entry.tile) * 64 < bytes.len())
        .unwrap_or(tiles);
    let index = *tiles
        .get(usize::from(entry.tile) * 64 + ty * 8 + tx)
        .ok_or_else(|| Error("stage tile reference outside decoded bank".into()))?;
    if index == 0 {
        return Ok(None);
    }
    palette
        .get(usize::from(entry.palette_bank) * 16 + usize::from(index))
        .copied()
        .map(Some)
        .ok_or_else(|| Error("stage palette reference outside bank".into()))
}

impl Layer {
    pub fn grid_width(&self) -> usize {
        self.grid_width
    }

    pub fn grid_height(&self) -> usize {
        self.grid_height
    }

    pub fn animation(&self) -> Option<&TileAnimation> {
        self.animation.as_ref()
    }

    /// ASOE foreground MapHeader with byte-sized metatile indices. Sizes come
    /// from explicit header fields and the highest reachable metatile index.
    pub fn parse(rom: &[u8], header_offset: usize) -> Result<Self> {
        let map = tables::foreground_map(rom, header_offset, 1024 * 1024)?;
        if map.tileset.width_tiles != 12 || map.tileset.height_tiles != 12 {
            return Err(Error("unsupported metatile dimensions".into()));
        }
        let grid_width = usize::from(map.width_metatiles);
        let grid_height = usize::from(map.height_metatiles);
        let grid = binary::range(rom, map.metatile_map_offset, grid_width * grid_height)?.to_vec();
        let count = usize::from(
            *grid
                .iter()
                .max()
                .ok_or_else(|| Error("empty stage grid".into()))?,
        ) + 1;
        let raw = binary::range(rom, map.tileset.map_offset, count * 12 * 12 * 2)?;
        let metatiles = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect::<Vec<_>>();
        let tiles = graphics::decode_tile_indices(
            map.tileset.tiles,
            graphics::BitsPerPixel::Four,
            1024 * 1024,
        )?;
        let animation = if map.tileset.animation_frames == 0 {
            if map.tileset.animation_tile_bytes != 0 || map.tileset.animation_delay != 0 {
                return Err(Error(
                    "inconsistent inactive terrain animation fields".into(),
                ));
            }
            None
        } else {
            let bytes = usize::from(map.tileset.animation_tile_bytes);
            let appended_length = (usize::from(map.tileset.animation_frames) - 1)
                .checked_mul(bytes)
                .ok_or_else(|| Error("terrain animation extent overflow".into()))?;
            let appended_offset = map
                .tileset
                .tile_offset
                .checked_add(map.tileset.tiles.len())
                .ok_or_else(|| Error("terrain animation source overflow".into()))?;
            Some(TileAnimation::parse(
                map.tileset.tiles,
                binary::range(rom, appended_offset, appended_length)?,
                bytes,
                map.tileset.animation_frames,
                map.tileset.animation_delay,
                MAX_FRAME_PIXELS,
            )?)
        };
        let palette = graphics::decode_palette(
            map.tileset
                .palette
                .ok_or_else(|| Error("foreground has no palette".into()))?,
            256,
        )?;
        for &word in &metatiles {
            if usize::from(word & 1023) >= tiles.len() / 64
                || usize::from(word >> 12) * 16 + 15 >= palette.len()
            {
                return Err(Error("invalid metatile tile/palette reference".into()));
            }
        }
        Ok(Self {
            grid_width,
            grid_height,
            grid,
            metatiles,
            tiles,
            palette,
            animation,
        })
    }

    pub fn pixel(&self, x: i32, y: i32) -> Result<Option<[u8; 4]>> {
        self.pixel_with_prefix(x, y, None)
    }

    fn pixel_with_prefix(&self, x: i32, y: i32, prefix: Option<&[u8]>) -> Result<Option<[u8; 4]>> {
        if x < 0 || y < 0 {
            return Ok(None);
        }
        let x = x as usize;
        let y = y as usize;
        let mx = x / 96;
        let my = y / 96;
        if mx >= self.grid_width || my >= self.grid_height {
            return Ok(None);
        }
        let metatile = usize::from(self.grid[my * self.grid_width + mx]);
        let word = self.metatiles[metatile * 144 + (y % 96) / 8 * 12 + (x % 96) / 8];
        tile_color(&self.tiles, &self.palette, word, x % 8, y % 8, prefix)
    }
}

#[derive(Debug, Clone)]
struct Background {
    map: Vec<u16>,
    width: usize,
    height: usize,
    tiles: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Stage {
    index: usize,
    width: i32,
    height: i32,
    spawn: (i32, i32),
    front: Layer,
    back: Layer,
    background: Background,
}

impl Stage {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn spawn(&self) -> (i32, i32) {
        self.spawn
    }

    pub fn load_asoe(rom: &[u8], stage_index: usize) -> Result<Self> {
        binary::verify_asoe(rom)?;
        if stage_index > 1 {
            return Err(Error(
                "native zone-one rendering supports Neo Green Hill Acts 1 and 2 only".into(),
            ));
        }
        let headers = binary::pointer_table(rom, 0x4a6c40 + stage_index * 12, 3, 3)?;
        let front_map = tables::foreground_map(rom, headers[0], MAX_FRAME_PIXELS)?;
        let back_map = tables::foreground_map(rom, headers[1], MAX_FRAME_PIXELS)?;
        if front_map.tileset.tile_offset != back_map.tileset.tile_offset
            || front_map.tileset.tiles.len() != back_map.tileset.tiles.len()
            || front_map.tileset.palette_offset != back_map.tileset.palette_offset
            || front_map.width_metatiles != back_map.width_metatiles
            || front_map.height_metatiles != back_map.height_metatiles
        {
            return Err(Error(
                "unsupported shared foreground character bank or dimensions".into(),
            ));
        }
        let front = Layer::parse(rom, headers[0])?;
        let back = Layer::parse(rom, headers[1])?;
        if front.tiles != back.tiles || back.animation.is_some() {
            return Err(Error("unsupported shared foreground animation bank".into()));
        }
        let bg = tables::tilemap(rom, headers[2], 65536)?;
        if bg.width_tiles != 32 || bg.height_tiles != 32 || bg.palette.is_some() {
            return Err(Error("unsupported zone-one background layout".into()));
        }
        let map_raw = binary::range(rom, bg.map_offset, 32 * 32 * 2)?;
        let map = map_raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect::<Vec<_>>();
        let tiles =
            graphics::decode_tile_indices(bg.tiles, graphics::BitsPerPixel::Four, 1024 * 1024)?;
        for &word in &map {
            if usize::from(word & 1023) >= tiles.len() / 64 {
                return Err(Error("background tile out of range".into()));
            }
        }
        let spawn = (
            i32::from(binary::u16_at(rom, 0x4a6dec + stage_index * 4)? as i16),
            i32::from(binary::u16_at(rom, 0x4a6dee + stage_index * 4)? as i16),
        );
        let width = i32::try_from(front.grid_width * METATILE_PIXELS)
            .map_err(|_| Error("stage width overflow".into()))?;
        let height = i32::try_from(front.grid_height * METATILE_PIXELS)
            .map_err(|_| Error("stage height overflow".into()))?;
        if spawn.0 < 0 || spawn.1 < 0 || spawn.0 >= width || spawn.1 >= height {
            return Err(Error("stage spawn outside foreground dimensions".into()));
        }
        Ok(Self {
            index: stage_index,
            width,
            height,
            spawn,
            front,
            back,
            background: Background {
                map,
                width: 32,
                height: 32,
                tiles,
            },
        })
    }

    pub fn camera(&self, player_x: i32, player_y: i32, width: usize, height: usize) -> (i32, i32) {
        // A camera query allocates nothing. Oversized viewports anchor at zero;
        // render_into separately rejects invalid dimensions/output budgets.
        let width = i32::try_from(width).unwrap_or(i32::MAX);
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        (
            player_x
                .saturating_sub(width / 2)
                .clamp(0, self.width.saturating_sub(width).max(0)),
            player_y
                .saturating_sub(height / 2)
                .clamp(0, self.height.saturating_sub(height).max(0)),
        )
    }

    /// Preserve the original base-phase API: screen-anchored background and
    /// unchanged foreground bank. Use render_at to advance native presentation.
    pub fn render_into(
        &self,
        output: &mut [u8],
        width: usize,
        height: usize,
        camera_x: i32,
        camera_y: i32,
    ) -> Result<()> {
        self.render_internal(output, width, height, camera_x, camera_y, None)
    }

    /// Paint the native 240x160 viewport after completed_updates background
    /// updates since stage initialization. Both foreground layers observe the
    /// same tile-prefix upload; background scroll follows visible scanlines.
    /// Runtime camera movement and stage-clock ownership remain with the caller.
    pub fn render_at(
        &self,
        output: &mut [u8],
        width: usize,
        height: usize,
        camera_x: i32,
        camera_y: i32,
        completed_updates: u64,
    ) -> Result<()> {
        if width != VIEW_WIDTH || height != VIEW_HEIGHT {
            return Err(Error(
                "native scanline presentation requires a 240x160 viewport".into(),
            ));
        }
        self.render_internal(
            output,
            width,
            height,
            camera_x,
            camera_y,
            Some(completed_updates),
        )
    }

    fn render_internal(
        &self,
        output: &mut [u8],
        width: usize,
        height: usize,
        camera_x: i32,
        camera_y: i32,
        tick: Option<u64>,
    ) -> Result<()> {
        let pixels = width
            .checked_mul(height)
            .ok_or_else(|| Error("viewport overflow".into()))?;
        if pixels == 0 || pixels > MAX_FRAME_PIXELS || pixels.checked_mul(4) != Some(output.len()) {
            return Err(Error("invalid frame buffer or viewport budget".into()));
        }
        if camera_x < 0 || camera_y < 0 || camera_x > self.width || camera_y > self.height {
            return Err(Error("camera outside stage".into()));
        }
        let bg = &self.background;
        let prefix = tick.and_then(|tick| {
            self.front
                .animation
                .as_ref()
                .map(|animation| animation.prefix_at(tick))
        });
        for y in 0..height {
            let scroll = tick
                .map(|tick| zone_one_background_scroll(tick, y, camera_x))
                .transpose()?
                .unwrap_or(0);
            for x in 0..width {
                let bx = (x + usize::from(scroll)) % (bg.width * 8);
                let by = y % (bg.height * 8);
                let bg_word = bg.map[by / 8 * bg.width + bx / 8];
                let mut color = tile_color(
                    &bg.tiles,
                    &self.front.palette,
                    bg_word,
                    bx % 8,
                    by % 8,
                    None,
                )?
                .unwrap_or(self.front.palette[0]);
                let wx = camera_x + x as i32;
                let wy = camera_y + y as i32;
                if let Some(back) = self.back.pixel_with_prefix(wx, wy, prefix)? {
                    color = back;
                }
                if let Some(front) = self.front.pixel_with_prefix(wx, wy, prefix)? {
                    color = front;
                }
                let start = (y * width + x) * 4;
                output[start..start + 4].copy_from_slice(&color);
            }
        }
        Ok(())
    }
}
