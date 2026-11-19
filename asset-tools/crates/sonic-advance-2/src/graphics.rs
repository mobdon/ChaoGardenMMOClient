//! Bounded GBA palette, character-tile and text-background conversion.
//!
//! These formats describe source data, not a claim that a caller's bytes are
//! reachable art. Bit 15 of a palette word is preserved and has no invented
//! alpha meaning. Transparency is an explicit composition choice.

use std::fmt;

pub const MAX_PALETTE_COLORS: usize = 256;
pub const MAX_TILES: usize = 16_384;
pub const MAX_PIXELS: usize = 4_194_304;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsError {
    MisalignedLength { actual: usize, multiple: usize },
    LimitExceeded { actual: usize, limit: usize },
    InvalidDimensions,
    WrongLength { actual: usize, expected: usize },
    MissingTile { index: usize, available: usize },
    MissingPaletteColor { index: usize, available: usize },
    InvalidPixelIndex { index: u8, depth: TileDepth },
    AllocationFailed,
}

impl fmt::Display for GraphicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GraphicsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteColor {
    pub raw: u16,
    pub red5: u8,
    pub green5: u8,
    pub blue5: u8,
}

impl PaletteColor {
    pub fn from_word(raw: u16) -> Self {
        Self {
            raw,
            red5: (raw & 31) as u8,
            green5: ((raw >> 5) & 31) as u8,
            blue5: ((raw >> 10) & 31) as u8,
        }
    }

    /// Expands each five-bit channel by bit replication. Raw channel values
    /// remain available; hardware color response is not modeled.
    pub fn rgb8(self) -> [u8; 3] {
        [self.red5, self.green5, self.blue5].map(|v| (v << 3) | (v >> 2))
    }
}

pub fn parse_palette(bytes: &[u8], max_colors: usize) -> Result<Vec<PaletteColor>, GraphicsError> {
    aligned(bytes.len(), 2)?;
    let count = bytes.len() / 2;
    bounded(count, max_colors.min(MAX_PALETTE_COLORS))?;
    let mut output = allocated(count)?;
    output.extend(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| PaletteColor::from_word(u16::from_le_bytes([pair[0], pair[1]]))),
    );
    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileDepth {
    FourBpp,
    EightBpp,
}

pub type Tile = [u8; 64];

/// Produces row-major 8x8 pixel indices. The low nibble is the left pixel.
pub fn decode_4bpp_tiles(bytes: &[u8], max_tiles: usize) -> Result<Vec<Tile>, GraphicsError> {
    decode_tiles(bytes, TileDepth::FourBpp, max_tiles)
}

pub fn decode_8bpp_tiles(bytes: &[u8], max_tiles: usize) -> Result<Vec<Tile>, GraphicsError> {
    decode_tiles(bytes, TileDepth::EightBpp, max_tiles)
}

fn decode_tiles(
    bytes: &[u8],
    depth: TileDepth,
    max_tiles: usize,
) -> Result<Vec<Tile>, GraphicsError> {
    let stride = match depth {
        TileDepth::FourBpp => 32,
        TileDepth::EightBpp => 64,
    };
    aligned(bytes.len(), stride)?;
    let count = bytes.len() / stride;
    bounded(count, max_tiles.min(MAX_TILES))?;
    let mut output = allocated(count)?;
    for source in bytes.chunks_exact(stride) {
        let mut tile = [0; 64];
        match depth {
            TileDepth::FourBpp => {
                for (index, byte) in source.iter().enumerate() {
                    tile[index * 2] = byte & 15;
                    tile[index * 2 + 1] = byte >> 4;
                }
            }
            TileDepth::EightBpp => tile.copy_from_slice(source),
        }
        output.push(tile);
    }
    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilemapEntry {
    pub raw: u16,
    pub tile_index: u16,
    pub horizontal_flip: bool,
    pub vertical_flip: bool,
    pub palette_bank: u8,
}

impl TilemapEntry {
    pub fn from_word(raw: u16) -> Self {
        Self {
            raw,
            tile_index: raw & 0x3ff,
            horizontal_flip: raw & 0x400 != 0,
            vertical_flip: raw & 0x800 != 0,
            palette_bank: (raw >> 12) as u8,
        }
    }
}

/// Parses row-major text entries. A GBA screen-block layout must first be
/// arranged into rows by its caller; affine byte maps are a separate format.
pub fn parse_tilemap(
    bytes: &[u8],
    width_tiles: usize,
    height_tiles: usize,
    max_cells: usize,
) -> Result<Vec<TilemapEntry>, GraphicsError> {
    let count = cell_count(width_tiles, height_tiles)?;
    bounded(count, max_cells.min(MAX_PIXELS / 64))?;
    let expected = count
        .checked_mul(2)
        .ok_or(GraphicsError::InvalidDimensions)?;
    if bytes.len() != expected {
        return Err(GraphicsError::WrongLength {
            actual: bytes.len(),
            expected,
        });
    }
    let mut output = allocated(count)?;
    output.extend(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| TilemapEntry::from_word(u16::from_le_bytes([pair[0], pair[1]]))),
    );
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMap {
    pub width: usize,
    pub height: usize,
    /// Row-major RGBA8 bytes. No color correction or filtering is applied.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub width_tiles: usize,
    pub height_tiles: usize,
    pub depth: TileDepth,
    /// Index zero is transparent within each four-bit bank. Set false when
    /// generating a palette-colored background with its own backdrop.
    pub transparent_zero: bool,
    pub max_pixels: usize,
}

/// Resolves tile indices, flips and four-bit palette banks. Eight-bit maps
/// ignore the bank bits, matching GBA text-background color selection.
pub fn render_text_map(
    tiles: &[Tile],
    entries: &[TilemapEntry],
    palette: &[PaletteColor],
    options: RenderOptions,
) -> Result<RenderedMap, GraphicsError> {
    let count = cell_count(options.width_tiles, options.height_tiles)?;
    let pixel_count = count
        .checked_mul(64)
        .ok_or(GraphicsError::InvalidDimensions)?;
    bounded(pixel_count, options.max_pixels.min(MAX_PIXELS))?;
    if entries.len() != count {
        return Err(GraphicsError::WrongLength {
            actual: entries.len(),
            expected: count,
        });
    }
    let width = options
        .width_tiles
        .checked_mul(8)
        .ok_or(GraphicsError::InvalidDimensions)?;
    let height = options
        .height_tiles
        .checked_mul(8)
        .ok_or(GraphicsError::InvalidDimensions)?;
    let byte_count = pixel_count
        .checked_mul(4)
        .ok_or(GraphicsError::InvalidDimensions)?;
    let mut rgba = allocated(byte_count)?;
    rgba.resize(byte_count, 0);
    for (cell, entry) in entries.iter().enumerate() {
        let tile = tiles
            .get(usize::from(entry.tile_index))
            .ok_or(GraphicsError::MissingTile {
                index: usize::from(entry.tile_index),
                available: tiles.len(),
            })?;
        for y in 0..8 {
            for x in 0..8 {
                let source_x = if entry.horizontal_flip { 7 - x } else { x };
                let source_y = if entry.vertical_flip { 7 - y } else { y };
                let index = tile[source_y * 8 + source_x];
                if options.depth == TileDepth::FourBpp && index > 15 {
                    return Err(GraphicsError::InvalidPixelIndex {
                        index,
                        depth: options.depth,
                    });
                }
                let palette_index = usize::from(index)
                    + if options.depth == TileDepth::FourBpp {
                        usize::from(entry.palette_bank) * 16
                    } else {
                        0
                    };
                let color =
                    palette
                        .get(palette_index)
                        .ok_or(GraphicsError::MissingPaletteColor {
                            index: palette_index,
                            available: palette.len(),
                        })?;
                let output_x = (cell % options.width_tiles) * 8 + x;
                let output_y = (cell / options.width_tiles) * 8 + y;
                let offset = (output_y * width + output_x) * 4;
                rgba[offset..offset + 3].copy_from_slice(&color.rgb8());
                rgba[offset + 3] = if options.transparent_zero && index == 0 {
                    0
                } else {
                    255
                };
            }
        }
    }
    Ok(RenderedMap {
        width,
        height,
        rgba,
    })
}

fn aligned(actual: usize, multiple: usize) -> Result<(), GraphicsError> {
    if !actual.is_multiple_of(multiple) {
        return Err(GraphicsError::MisalignedLength { actual, multiple });
    }
    Ok(())
}

fn bounded(actual: usize, limit: usize) -> Result<(), GraphicsError> {
    if actual > limit {
        return Err(GraphicsError::LimitExceeded { actual, limit });
    }
    Ok(())
}

fn cell_count(width: usize, height: usize) -> Result<usize, GraphicsError> {
    if width == 0 || height == 0 {
        return Err(GraphicsError::InvalidDimensions);
    }
    width
        .checked_mul(height)
        .ok_or(GraphicsError::InvalidDimensions)
}

fn allocated<T>(count: usize) -> Result<Vec<T>, GraphicsError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| GraphicsError::AllocationFailed)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(depth: TileDepth) -> RenderOptions {
        RenderOptions {
            width_tiles: 1,
            height_tiles: 1,
            depth,
            transparent_zero: true,
            max_pixels: 64,
        }
    }

    fn palette() -> Vec<PaletteColor> {
        (0..256).map(PaletteColor::from_word).collect()
    }

    #[test]
    fn colors_preserve_raw_word_and_channel_order() {
        let colors = parse_palette(&[0x1f, 0x80, 0xe0, 3, 0, 0x7c], 3).unwrap();
        assert_eq!(colors[0].raw, 0x801f);
        assert_eq!(colors[0].rgb8(), [255, 0, 0]);
        assert_eq!(colors[1].rgb8(), [0, 255, 0]);
        assert_eq!(colors[2].rgb8(), [0, 0, 255]);
        assert!(parse_palette(&[0], 1).is_err());
        assert!(parse_palette(&[0; 4], 1).is_err());
        assert!(parse_palette(&[0; 514], usize::MAX).is_err());
    }

    #[test]
    fn tile_indices_preserve_nibble_and_row_order() {
        let packed: Vec<u8> = (0..32).map(|i| i as u8).collect();
        let decoded = decode_4bpp_tiles(&packed, 1).unwrap();
        for i in 0..32 {
            assert_eq!(decoded[0][i * 2], packed[i] & 15);
            assert_eq!(decoded[0][i * 2 + 1], packed[i] >> 4);
        }
        let direct: Vec<u8> = (0..64).collect();
        assert_eq!(decode_8bpp_tiles(&direct, 1).unwrap()[0], direct[..]);
        assert!(decode_4bpp_tiles(&[0; 31], 1).is_err());
        assert!(decode_8bpp_tiles(&[0; 63], 1).is_err());
        assert!(decode_4bpp_tiles(&[0; 32], 0).is_err());
    }

    #[test]
    fn map_words_preserve_flips_bank_and_tile_bits() {
        let entries = parse_tilemap(&0xac37_u16.to_le_bytes(), 1, 1, 1).unwrap();
        assert_eq!(entries[0].raw, 0xac37);
        assert_eq!(entries[0].tile_index, 0x37);
        assert!(entries[0].horizontal_flip && entries[0].vertical_flip);
        assert_eq!(entries[0].palette_bank, 10);
        assert!(parse_tilemap(&[], 0, 1, 1).is_err());
        assert!(parse_tilemap(&[], usize::MAX, 2, usize::MAX).is_err());
        assert!(parse_tilemap(&[0; 1], 1, 1, 1).is_err());
        assert!(parse_tilemap(&[0; 4], 1, 1, 1).is_err());
        assert!(parse_tilemap(&[0; 2], 1, 1, 0).is_err());
    }

    #[test]
    fn rendering_resolves_both_flips_banks_and_explicit_transparency() {
        let mut tile = [0; 64];
        tile[0] = 1;
        tile[7] = 2;
        tile[56] = 3;
        tile[63] = 4;
        let colors = palette();
        for flags in [0, 0x400, 0x800, 0xc00] {
            let entry = TilemapEntry::from_word(flags | 0x2000);
            let image =
                render_text_map(&[tile], &[entry], &colors, options(TileDepth::FourBpp)).unwrap();
            let source = match flags {
                0 => 1,
                0x400 => 2,
                0x800 => 3,
                _ => 4,
            };
            assert_eq!(&image.rgba[..3], &colors[32 + source].rgb8());
            assert_eq!(image.rgba[3], 255);
            assert_eq!(image.rgba[4 * 9 + 3], 0);
        }
        let mut opaque = options(TileDepth::FourBpp);
        opaque.transparent_zero = false;
        let image =
            render_text_map(&[[0; 64]], &[TilemapEntry::from_word(0)], &colors, opaque).unwrap();
        assert_eq!(image.rgba[3], 255);
    }

    #[test]
    fn eight_bit_maps_ignore_palette_bank() {
        let mut tile = [0; 64];
        tile[0] = 200;
        let colors = palette();
        let image = render_text_map(
            &[tile],
            &[TilemapEntry::from_word(0xf000)],
            &colors,
            options(TileDepth::EightBpp),
        )
        .unwrap();
        assert_eq!(&image.rgba[..3], &colors[200].rgb8());
    }

    #[test]
    fn malformed_rendering_is_rejected_before_indexing() {
        let colors = palette();
        let entry = TilemapEntry::from_word(0);
        assert!(render_text_map(&[], &[entry], &colors, options(TileDepth::FourBpp)).is_err());
        assert!(render_text_map(&[[0; 64]], &[], &colors, options(TileDepth::FourBpp)).is_err());
        assert!(
            render_text_map(&[[16; 64]], &[entry], &colors, options(TileDepth::FourBpp)).is_err()
        );
        assert!(render_text_map(&[[0; 64]], &[entry], &[], options(TileDepth::FourBpp)).is_err());
        assert!(
            render_text_map(
                &[[0; 64]],
                &[TilemapEntry::from_word(0xf000)],
                &colors[..16],
                options(TileDepth::FourBpp)
            )
            .is_err()
        );
        let mut limited = options(TileDepth::FourBpp);
        limited.max_pixels = 63;
        assert!(render_text_map(&[[0; 64]], &[entry], &colors, limited).is_err());
        limited.width_tiles = usize::MAX;
        limited.height_tiles = 2;
        assert!(render_text_map(&[[0; 64]], &[entry], &colors, limited).is_err());
    }
}
