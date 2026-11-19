//! Bounded GBA graphics primitives. These describe hardware storage, not an
//! inferred Sonic Advance or A3VE resource-table layout.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsError {
    IncompletePalette,
    IncompleteTile { tile_bytes: usize },
    LimitExceeded,
    AllocationFailed,
    InvalidMapDimensions,
    MapLength { expected: usize, actual: usize },
    TileIndex { index: usize, available: usize },
    PaletteIndex { index: usize, available: usize },
    InvalidPixelIndex { index: u8, depth: ColorDepth },
    OamLength { actual: usize },
    ReservedObjectShape,
}

impl fmt::Display for GraphicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GraphicsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    Bpp4,
    Bpp8,
}

/// Palette entries retain bit-15-independent BGR555 color. Transparency belongs
/// to the tile/background lookup, not to the stored palette color.
pub fn decode_palette_le(input: &[u8], max_colors: usize) -> Result<Vec<[u8; 4]>, GraphicsError> {
    if !input.len().is_multiple_of(2) {
        return Err(GraphicsError::IncompletePalette);
    }
    let count = input.len() / 2;
    if count > max_colors {
        return Err(GraphicsError::LimitExceeded);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| GraphicsError::AllocationFailed)?;
    for pair in input.as_chunks::<2>().0 {
        let value = u16::from_le_bytes([pair[0], pair[1]]);
        let expand = |shift: u32| {
            let channel = ((value >> shift) & 31_u16) as u8;
            (channel << 3) | (channel >> 2)
        };
        output.push([expand(0), expand(5), expand(10), 255]);
    }
    Ok(output)
}

/// Returns one row-major array of 64 indices per 8x8 tile. Four-bit pixels use
/// the low nibble first. No palette or object mapping mode is inferred here.
pub fn decode_tiles(
    input: &[u8],
    depth: ColorDepth,
    max_tiles: usize,
) -> Result<Vec<[u8; 64]>, GraphicsError> {
    let tile_bytes = match depth {
        ColorDepth::Bpp4 => 32,
        ColorDepth::Bpp8 => 64,
    };
    if !input.len().is_multiple_of(tile_bytes) {
        return Err(GraphicsError::IncompleteTile { tile_bytes });
    }
    let count = input.len() / tile_bytes;
    if count > max_tiles {
        return Err(GraphicsError::LimitExceeded);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| GraphicsError::AllocationFailed)?;
    for source in input.chunks_exact(tile_bytes) {
        let mut tile = [0; 64];
        match depth {
            ColorDepth::Bpp4 => {
                for (i, byte) in source.iter().copied().enumerate() {
                    tile[i * 2] = byte & 15;
                    tile[i * 2 + 1] = byte >> 4;
                }
            }
            ColorDepth::Bpp8 => tile.copy_from_slice(source),
        }
        output.push(tile);
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    /// RGBA8 row-major bytes; transparent index zero is [0, 0, 0, 0].
    pub pixels: Vec<u8>,
}

/// Render a text background in native 32x32 screen-block order. Dimensions are
/// (width, height) in tiles, each either 32 or 64. This is not an affine map or
/// an object compositor. No tile-bank start address or palette is guessed.
pub fn render_text_bg(
    tiles: &[[u8; 64]],
    map: &[u8],
    palette: &[[u8; 4]],
    depth: ColorDepth,
    dimensions: (usize, usize),
    max_pixels: usize,
) -> Result<RgbaImage, GraphicsError> {
    let (width_tiles, height_tiles) = dimensions;
    if !matches!(width_tiles, 32 | 64) || !matches!(height_tiles, 32 | 64) {
        return Err(GraphicsError::InvalidMapDimensions);
    }
    let expected = width_tiles * height_tiles * 2;
    if map.len() != expected {
        return Err(GraphicsError::MapLength {
            expected,
            actual: map.len(),
        });
    }
    let width = width_tiles * 8;
    let height = height_tiles * 8;
    let count = width * height;
    if count > max_pixels {
        return Err(GraphicsError::LimitExceeded);
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count * 4)
        .map_err(|_| GraphicsError::AllocationFailed)?;
    pixels.resize(count * 4, 0);
    for ty in 0..height_tiles {
        for tx in 0..width_tiles {
            let screen_block = (ty / 32) * (width_tiles / 32) + tx / 32;
            let cell = screen_block * 1024 + (ty % 32) * 32 + tx % 32;
            let entry = u16::from_le_bytes([map[cell * 2], map[cell * 2 + 1]]);
            let index = usize::from(entry & 1023);
            let tile = tiles.get(index).ok_or(GraphicsError::TileIndex {
                index,
                available: tiles.len(),
            })?;
            let bank = match depth {
                ColorDepth::Bpp4 => usize::from(entry >> 12) * 16,
                ColorDepth::Bpp8 => 0,
            };
            for y in 0..8 {
                for x in 0..8 {
                    let sx = if entry & 0x400 != 0 { 7 - x } else { x };
                    let sy = if entry & 0x800 != 0 { 7 - y } else { y };
                    let value = tile[sy * 8 + sx];
                    if depth == ColorDepth::Bpp4 && value > 15 {
                        return Err(GraphicsError::InvalidPixelIndex {
                            index: value,
                            depth,
                        });
                    }
                    if value == 0 {
                        continue;
                    }
                    let index = bank + usize::from(value);
                    let color = palette.get(index).ok_or(GraphicsError::PaletteIndex {
                        index,
                        available: palette.len(),
                    })?;
                    let at = ((ty * 8 + y) * width + tx * 8 + x) * 4;
                    pixels[at..at + 4].copy_from_slice(color);
                }
            }
        }
    }
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OamAttributes {
    /// The fourth halfword is preserved: it may be an affine matrix element.
    pub raw: [u16; 4],
    pub x: u16,
    pub y: u8,
    pub width: u8,
    pub height: u8,
    pub affine: bool,
    pub double_size: bool,
    pub disabled: bool,
    pub object_mode: u8,
    pub mosaic: bool,
    pub depth: ColorDepth,
    pub affine_index: Option<u8>,
    pub horizontal_flip: bool,
    pub vertical_flip: bool,
    /// Native 32-byte tile units, including when depth is 8bpp.
    pub tile_number: u16,
    pub priority: u8,
    pub palette_bank: u8,
}

/// Interpret one eight-byte hardware OAM slot. Coordinates remain their native
/// wrapping unsigned values; affine matrices and 1D/2D tile mapping are separate.
pub fn parse_oam(input: &[u8]) -> Result<OamAttributes, GraphicsError> {
    let bytes: &[u8; 8] = input.try_into().map_err(|_| GraphicsError::OamLength {
        actual: input.len(),
    })?;
    let raw = std::array::from_fn(|i| u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
    let shape = usize::from(raw[0] >> 14);
    let size = usize::from(raw[1] >> 14);
    const SIZES: [[(u8, u8); 4]; 3] = [
        [(8, 8), (16, 16), (32, 32), (64, 64)],
        [(16, 8), (32, 8), (32, 16), (64, 32)],
        [(8, 16), (8, 32), (16, 32), (32, 64)],
    ];
    let &(width, height) = SIZES
        .get(shape)
        .and_then(|row| row.get(size))
        .ok_or(GraphicsError::ReservedObjectShape)?;
    let affine = raw[0] & 0x100 != 0;
    Ok(OamAttributes {
        raw,
        x: raw[1] & 0x1ff,
        y: raw[0] as u8,
        width,
        height,
        affine,
        double_size: affine && raw[0] & 0x200 != 0,
        disabled: !affine && raw[0] & 0x200 != 0,
        object_mode: ((raw[0] >> 10) & 3) as u8,
        mosaic: raw[0] & 0x1000 != 0,
        depth: if raw[0] & 0x2000 != 0 {
            ColorDepth::Bpp8
        } else {
            ColorDepth::Bpp4
        },
        affine_index: affine.then_some(((raw[1] >> 9) & 31) as u8),
        horizontal_flip: !affine && raw[1] & 0x1000 != 0,
        vertical_flip: !affine && raw[1] & 0x2000 != 0,
        tile_number: raw[2] & 1023,
        priority: ((raw[2] >> 10) & 3) as u8,
        palette_bank: (raw[2] >> 12) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgr555_channels_endianness_and_unused_bit() {
        assert_eq!(
            decode_palette_le(&[0x1f, 0, 0xe0, 3, 0, 0x7c, 0xff, 0xff], 4).unwrap(),
            [
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [255, 255, 255, 255]
            ]
        );
        assert_eq!(
            decode_palette_le(&[1], 4),
            Err(GraphicsError::IncompletePalette)
        );
        assert_eq!(
            decode_palette_le(&[0; 4], 1),
            Err(GraphicsError::LimitExceeded)
        );
    }

    #[test]
    fn four_bit_pixels_low_nibble_first_and_eight_bit_unmodified() {
        let mut packed = [0; 32];
        packed[0] = 0x21;
        packed[31] = 0xfe;
        let tile = decode_tiles(&packed, ColorDepth::Bpp4, 1).unwrap()[0];
        assert_eq!(&tile[..2], &[1, 2]);
        assert_eq!(&tile[62..], &[14, 15]);
        let source = std::array::from_fn::<_, 64, _>(|i| i as u8);
        assert_eq!(
            decode_tiles(&source, ColorDepth::Bpp8, 1).unwrap()[0],
            source
        );
        assert!(matches!(
            decode_tiles(&[0; 31], ColorDepth::Bpp4, 1),
            Err(GraphicsError::IncompleteTile { .. })
        ));
        assert_eq!(
            decode_tiles(&packed, ColorDepth::Bpp4, 0),
            Err(GraphicsError::LimitExceeded)
        );
    }

    fn palette() -> Vec<[u8; 4]> {
        (0..256).map(|i| [i as u8, 0, 0, 255]).collect()
    }

    fn image_pixel(image: &RgbaImage, x: usize, y: usize) -> &[u8] {
        let offset = (y * image.width + x) * 4;
        &image.pixels[offset..offset + 4]
    }

    #[test]
    fn text_bg_flips_palette_banks_and_transparency() {
        let mut tile = [0; 64];
        tile[0] = 1;
        tile[7] = 2;
        tile[56] = 3;
        tile[63] = 4;
        let mut map = [0; 2048];
        // First cell: bank 3 and both flips, second: horizontal, third: vertical.
        map[..2].copy_from_slice(&0x3c00_u16.to_le_bytes());
        map[2..4].copy_from_slice(&0x0400_u16.to_le_bytes());
        map[4..6].copy_from_slice(&0x0800_u16.to_le_bytes());
        let image =
            render_text_bg(&[tile], &map, &palette(), ColorDepth::Bpp4, (32, 32), 65536).unwrap();
        assert_eq!(image_pixel(&image, 0, 0), &[52, 0, 0, 255]);
        assert_eq!(image_pixel(&image, 7, 7), &[49, 0, 0, 255]);
        assert_eq!(image_pixel(&image, 8, 0), &[2, 0, 0, 255]);
        assert_eq!(image_pixel(&image, 16, 0), &[3, 0, 0, 255]);
        assert_eq!(image_pixel(&image, 1, 1), &[0, 0, 0, 0]);
    }

    #[test]
    fn text_bg_uses_screen_blocks_and_ignores_bank_for_eight_bit() {
        let tiles = [[1; 64], [2; 64], [3; 64], [4; 64]];
        let mut map = [0; 8192];
        for block in 0..4 {
            let entry = (0xf000 | block as u16).to_le_bytes();
            map[block * 2048..block * 2048 + 2].copy_from_slice(&entry);
        }
        let image =
            render_text_bg(&tiles, &map, &palette(), ColorDepth::Bpp8, (64, 64), 262144).unwrap();
        for (x, y, color) in [(0, 0, 1), (256, 0, 2), (0, 256, 3), (256, 256, 4)] {
            assert_eq!(image_pixel(&image, x, y), &[color, 0, 0, 255]);
        }
    }

    #[test]
    fn text_bg_rejects_dimensions_truncation_limits_and_bad_indices() {
        let map = [0; 2048];
        let tiles = [[1; 64]];
        assert_eq!(
            render_text_bg(
                &tiles,
                &map,
                &palette(),
                ColorDepth::Bpp4,
                (1, 32),
                usize::MAX
            ),
            Err(GraphicsError::InvalidMapDimensions)
        );
        assert!(matches!(
            render_text_bg(
                &tiles,
                &map[..2047],
                &palette(),
                ColorDepth::Bpp4,
                (32, 32),
                65536
            ),
            Err(GraphicsError::MapLength { .. })
        ));
        assert_eq!(
            render_text_bg(&tiles, &map, &palette(), ColorDepth::Bpp4, (32, 32), 65535),
            Err(GraphicsError::LimitExceeded)
        );
        assert!(matches!(
            render_text_bg(&[], &map, &palette(), ColorDepth::Bpp4, (32, 32), 65536),
            Err(GraphicsError::TileIndex { .. })
        ));
        assert!(matches!(
            render_text_bg(&tiles, &map, &[], ColorDepth::Bpp4, (32, 32), 65536),
            Err(GraphicsError::PaletteIndex { .. })
        ));
        assert!(matches!(
            render_text_bg(
                &[[16; 64]],
                &map,
                &palette(),
                ColorDepth::Bpp4,
                (32, 32),
                65536
            ),
            Err(GraphicsError::InvalidPixelIndex { .. })
        ));
    }

    fn oam_words(words: [u16; 4]) -> [u8; 8] {
        std::array::from_fn(|i| words[i / 2].to_le_bytes()[i % 2])
    }

    #[test]
    fn oam_shapes_sizes_and_fields() {
        let attributes = parse_oam(&oam_words([0x5005, 0xb12c, 0xb955, 0xface])).unwrap();
        assert_eq!((attributes.width, attributes.height), (32, 16));
        assert_eq!((attributes.x, attributes.y), (300, 5));
        assert!(attributes.mosaic && attributes.horizontal_flip && attributes.vertical_flip);
        assert!(!attributes.affine && !attributes.disabled);
        assert_eq!(attributes.tile_number, 0x155);
        assert_eq!((attributes.priority, attributes.palette_bank), (2, 11));
        assert_eq!(attributes.raw[3], 0xface);
        for (shape, expected) in [(0, (64, 64)), (1, (64, 32)), (2, (32, 64))] {
            let item = parse_oam(&oam_words([shape << 14, 3 << 14, 0, 0])).unwrap();
            assert_eq!((item.width, item.height), expected);
        }
    }

    #[test]
    fn affine_bits_do_not_become_flips_or_disable() {
        let affine = parse_oam(&oam_words([0x2300, 0x3e00, 0, 0])).unwrap();
        assert!(affine.affine && affine.double_size);
        assert!(!affine.disabled && !affine.horizontal_flip && !affine.vertical_flip);
        assert_eq!(affine.affine_index, Some(31));
        assert_eq!(affine.depth, ColorDepth::Bpp8);
        assert!(parse_oam(&oam_words([0x0200, 0, 0, 0])).unwrap().disabled);
        assert_eq!(
            parse_oam(&[0; 7]),
            Err(GraphicsError::OamLength { actual: 7 })
        );
        assert_eq!(
            parse_oam(&[0; 9]),
            Err(GraphicsError::OamLength { actual: 9 })
        );
        assert_eq!(
            parse_oam(&oam_words([0xc000, 0, 0, 0])),
            Err(GraphicsError::ReservedObjectShape)
        );
    }
}
