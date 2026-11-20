//! GBA graphics representations, without game-specific offsets or attribution
//! of any particular byte range to a sprite or background.
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitsPerPixel {
    Four,
    Eight,
}

/// Decode little-endian BGR555 palette words. Bit 15 is ignored. Channels use
/// bit replication `(v << 3) | (v >> 2)`, not a claim about LCD color response.
/// Palette entries are opaque; index-zero transparency is a renderer policy.
pub fn decode_palette(input: &[u8], max_colors: usize) -> Result<Vec<[u8; 4]>> {
    if input.is_empty() || !input.len().is_multiple_of(2) {
        return Err(Error("palette must contain complete 16-bit colors".into()));
    }
    let colors = input.len() / 2;
    if colors > max_colors || colors > 256 {
        return Err(Error(
            "palette exceeds color budget or 256-color hardware limit".into(),
        ));
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(colors)
        .map_err(|_| Error("palette allocation failed".into()))?;
    for bytes in input.as_chunks::<2>().0 {
        let word = u16::from_le_bytes([bytes[0], bytes[1]]);
        let expand = |value: u16| ((value << 3) | (value >> 2)) as u8;
        output.push([
            expand(word & 31),
            expand((word >> 5) & 31),
            expand((word >> 10) & 31),
            255,
        ]);
    }
    Ok(output)
}

/// Return tile-major pixels: 64 row-major indices for each 8x8 tile. In 4bpp
/// storage the left pixel occupies the low nibble. No palette is inferred.
pub fn decode_tile_indices(
    input: &[u8],
    depth: BitsPerPixel,
    max_pixels: usize,
) -> Result<Vec<u8>> {
    let bytes_per_tile = match depth {
        BitsPerPixel::Four => 32,
        BitsPerPixel::Eight => 64,
    };
    if input.is_empty() || !input.len().is_multiple_of(bytes_per_tile) {
        return Err(Error("tiles must contain complete 8x8 tiles".into()));
    }
    let pixels = (input.len() / bytes_per_tile)
        .checked_mul(64)
        .ok_or_else(|| Error("tile pixel count overflow".into()))?;
    if pixels > max_pixels {
        return Err(Error("tile pixels exceed output budget".into()));
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(pixels)
        .map_err(|_| Error("tile allocation failed".into()))?;
    for &byte in input {
        match depth {
            BitsPerPixel::Four => {
                output.push(byte & 15);
                output.push(byte >> 4);
            }
            BitsPerPixel::Eight => output.push(byte),
        }
    }
    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEntry {
    pub tile: u16,
    pub flip_x: bool,
    pub flip_y: bool,
    pub palette_bank: u8,
}

impl TextEntry {
    pub fn decode(word: u16) -> Self {
        Self {
            tile: word & 0x3ff,
            flip_x: word & 0x400 != 0,
            flip_y: word & 0x800 != 0,
            palette_bank: (word >> 12) as u8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMapLayout {
    /// Asset tables may store arbitrary rectangular maps in row order.
    Linear,
    /// GBA VRAM text maps use 32x32-entry screen blocks. Only the four hardware
    /// dimensions (32/64 tiles on each axis) are accepted for this layout.
    ScreenBlocks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMap {
    pub width_tiles: usize,
    pub height_tiles: usize,
    /// Always row-major, even if the input used hardware screen blocks.
    pub entries: Vec<TextEntry>,
}

pub fn decode_text_map(
    input: &[u8],
    width_tiles: usize,
    height_tiles: usize,
    layout: TextMapLayout,
    max_tiles: usize,
) -> Result<TextMap> {
    if width_tiles == 0 || height_tiles == 0 {
        return Err(Error("map dimensions must be nonzero".into()));
    }
    if layout == TextMapLayout::ScreenBlocks
        && (!matches!(width_tiles, 32 | 64) || !matches!(height_tiles, 32 | 64))
    {
        return Err(Error("unsupported text screen-block dimensions".into()));
    }
    let count = width_tiles
        .checked_mul(height_tiles)
        .ok_or_else(|| Error("map dimensions overflow".into()))?;
    if count > max_tiles {
        return Err(Error("map entries exceed output budget".into()));
    }
    let bytes = count
        .checked_mul(2)
        .ok_or_else(|| Error("map byte count overflow".into()))?;
    if input.len() != bytes {
        return Err(Error("map byte count does not match dimensions".into()));
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| Error("map allocation failed".into()))?;
    for y in 0..height_tiles {
        for x in 0..width_tiles {
            let index = match layout {
                TextMapLayout::Linear => y * width_tiles + x,
                TextMapLayout::ScreenBlocks => {
                    ((y / 32) * (width_tiles / 32) + x / 32) * 1024 + (y % 32) * 32 + x % 32
                }
            };
            entries.push(TextEntry::decode(u16::from_le_bytes([
                input[index * 2],
                input[index * 2 + 1],
            ])));
        }
    }
    Ok(TextMap {
        width_tiles,
        height_tiles,
        entries,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub depth: BitsPerPixel,
    /// Transparent source index zero, including banked 4bpp palettes. This is
    /// explicit presentation policy, not palette-byte alpha information.
    pub transparent_zero: bool,
    pub max_pixels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>,
}

/// Render decoded tile indices and text entries. 8bpp ignores palette-bank
/// bits, as the text-background representation requires. Affine maps, OAM
/// sprites, blending and window priorities are outside this renderer.
pub fn render_text_background(
    tiles: &[u8],
    palette: &[[u8; 4]],
    map: &TextMap,
    options: RenderOptions,
) -> Result<RgbaImage> {
    if tiles.is_empty() || !tiles.len().is_multiple_of(64) {
        return Err(Error("decoded tiles must contain 64 indices each".into()));
    }
    if palette.is_empty() || palette.len() > 256 {
        return Err(Error("palette must contain 1 through 256 colors".into()));
    }
    if options.depth == BitsPerPixel::Four && tiles.iter().any(|&value| value > 15) {
        return Err(Error("4bpp tile index exceeds 15".into()));
    }
    if map.width_tiles == 0 || map.height_tiles == 0 {
        return Err(Error("map dimensions must be nonzero".into()));
    }
    let entry_count = map
        .width_tiles
        .checked_mul(map.height_tiles)
        .ok_or_else(|| Error("map dimensions overflow".into()))?;
    if entry_count != map.entries.len() {
        return Err(Error("decoded map entry count mismatch".into()));
    }
    let width = map
        .width_tiles
        .checked_mul(8)
        .ok_or_else(|| Error("image width overflow".into()))?;
    let height = map
        .height_tiles
        .checked_mul(8)
        .ok_or_else(|| Error("image height overflow".into()))?;
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| Error("image pixel count overflow".into()))?;
    if pixels > options.max_pixels {
        return Err(Error("render exceeds output pixel budget".into()));
    }
    // Validate all references before allocating the output; public map fields
    // permit callers to construct data without using decode_text_map.
    for entry in &map.entries {
        if entry.tile > 1023
            || entry.palette_bank > 15
            || usize::from(entry.tile) >= tiles.len() / 64
        {
            return Err(Error("map contains an invalid tile or palette bank".into()));
        }
        let bank = match options.depth {
            BitsPerPixel::Four => usize::from(entry.palette_bank) * 16,
            BitsPerPixel::Eight => 0,
        };
        let start = usize::from(entry.tile) * 64;
        if tiles[start..start + 64]
            .iter()
            .any(|&value| bank + usize::from(value) >= palette.len())
        {
            return Err(Error("tile references an unavailable palette color".into()));
        }
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(pixels)
        .map_err(|_| Error("image allocation failed".into()))?;
    output.resize(pixels, [0; 4]);
    for (position, entry) in map.entries.iter().enumerate() {
        let tile_x = position % map.width_tiles;
        let tile_y = position / map.width_tiles;
        let bank = match options.depth {
            BitsPerPixel::Four => usize::from(entry.palette_bank) * 16,
            BitsPerPixel::Eight => 0,
        };
        for y in 0..8 {
            for x in 0..8 {
                let source_x = if entry.flip_x { 7 - x } else { x };
                let source_y = if entry.flip_y { 7 - y } else { y };
                let index = tiles[usize::from(entry.tile) * 64 + source_y * 8 + source_x];
                let mut color = palette[bank + usize::from(index)];
                if options.transparent_zero && index == 0 {
                    color[3] = 0;
                }
                output[(tile_y * 8 + y) * width + tile_x * 8 + x] = color;
            }
        }
    }
    Ok(RgbaImage {
        width,
        height,
        pixels: output,
    })
}
