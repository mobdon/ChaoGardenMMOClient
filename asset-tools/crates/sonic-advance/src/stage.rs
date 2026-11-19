//! Native zone-one tile composition. Positions are pixels; original 8x8 tiles,
//! palette banks, flips and transparency are retained. Camera/parallax and the
//! current static tile-animation phase are presentation policies, not engine parity.
use crate::{binary, graphics, tables, Error, Result};

pub const VIEW_WIDTH: usize = 240;
pub const VIEW_HEIGHT: usize = 160;
pub const STAGE_ZERO_SPAWN: (i32, i32) = (104, 444);
pub const METATILE_TILES: usize = 12;
pub const METATILE_PIXELS: usize = METATILE_TILES * 8;
pub const MAX_FRAME_PIXELS: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Layer {
    grid_width: usize,
    grid_height: usize,
    grid: Vec<u8>,
    metatiles: Vec<u16>,
    tiles: Vec<u8>,
    palette: Vec<[u8; 4]>,
}

fn tile_color(
    tiles: &[u8],
    palette: &[[u8; 4]],
    word: u16,
    x: usize,
    y: usize,
) -> Result<Option<[u8; 4]>> {
    let entry = graphics::TextEntry::decode(word);
    let tx = if entry.flip_x { 7 - x } else { x };
    let ty = if entry.flip_y { 7 - y } else { y };
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
        })
    }

    pub fn pixel(&self, x: i32, y: i32) -> Result<Option<[u8; 4]>> {
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
        tile_color(&self.tiles, &self.palette, word, x % 8, y % 8)
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
    width: i32,
    height: i32,
    spawn: (i32, i32),
    front: Layer,
    back: Layer,
    background: Background,
}

impl Stage {
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
        if stage_index != 0 {
            return Err(Error(
                "this playable slice supports Neo Green Hill Act 1 only".into(),
            ));
        }
        let front = Layer::parse(rom, 0x4bf1b0)?;
        let back = Layer::parse(rom, 0x4bf1d4)?;
        let bg = tables::tilemap(rom, 0x62f474, 65536)?;
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
            i32::from(binary::u16_at(rom, 0x4a6dec)? as i16),
            i32::from(binary::u16_at(rom, 0x4a6dee)? as i16),
        );
        Ok(Self {
            width: 13056,
            height: 1344,
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

    /// Paint original background/back/front pixels. The background is currently
    /// screen-anchored; original parallax and foreground animation remain open.
    pub fn render_into(
        &self,
        output: &mut [u8],
        width: usize,
        height: usize,
        camera_x: i32,
        camera_y: i32,
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
        for y in 0..height {
            for x in 0..width {
                let bx = x % (bg.width * 8);
                let by = y % (bg.height * 8);
                let bg_word = bg.map[by / 8 * bg.width + bx / 8];
                let mut color =
                    tile_color(&bg.tiles, &self.front.palette, bg_word, bx % 8, by % 8)?
                        .unwrap_or(self.front.palette[0]);
                let wx = camera_x + x as i32;
                let wy = camera_y + y as i32;
                if let Some(back) = self.back.pixel(wx, wy)? {
                    color = back;
                }
                if let Some(front) = self.front.pixel(wx, wy)? {
                    color = front;
                }
                let start = (y * width + x) * 4;
                output[start..start + 4].copy_from_slice(&color);
            }
        }
        Ok(())
    }
}
