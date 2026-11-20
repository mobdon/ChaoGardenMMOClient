//! Original A2NE glyph pixels with the original English sidebar's ink palette.
//! Hidden-stat words are newly composed presentation, not baked native labels.
use crate::{Result, encode_png};
use sonic_advance_2_assets::{VerifiedRom, gba, graphics};

pub struct FontImage {
    pub filename: &'static str,
    pub text: &'static str,
    pub width: usize,
    pub height: usize,
    pub png: Vec<u8>,
    pub rgba_sha256: String,
}

pub struct RenderedFont {
    pub images: Vec<FontImage>,
    pub matched_native_labels: Vec<&'static str>,
}

fn ink_bounds(tile: &graphics::Tile) -> Option<(usize, usize)> {
    let columns = (0..8).filter(|&x| (0..8).any(|y| tile[y * 8 + x] != 4));
    Some((columns.clone().min()?, columns.max()?))
}

fn pixel(palette: &[graphics::PaletteColor], index: u8) -> Result<[u8; 4]> {
    // These original palette roles reproduce the English baked sidebar text.
    // Glyph geometry is untouched; cream background becomes transparent.
    let entry = match index {
        4 => return Ok([0, 0, 0, 0]),
        1 => 9,
        2 => 7,
        3 => 5,
        _ => return Err(format!("Unexpected native ASCII glyph palette index {index}").into()),
    };
    let [r, g, b] = palette[entry].rgb8();
    Ok([r, g, b, 255])
}

fn compose(
    tiles: &[graphics::Tile],
    palette: &[graphics::PaletteColor],
    text: &str,
) -> Result<graphics::RenderedMap> {
    if text.is_empty() || !text.bytes().all(|code| (33..=126).contains(&code)) {
        return Err(
            "Only nonempty, non-space original printable ASCII labels are supported".into(),
        );
    }
    let glyphs = text
        .bytes()
        .map(|code| {
            let tile = &tiles[usize::from(code - 32)];
            let (left, right) = ink_bounds(tile).ok_or("Native label glyph is empty")?;
            Ok((tile, left, right))
        })
        .collect::<Result<Vec<_>>>()?;
    let width = glyphs
        .iter()
        .map(|(_, left, right)| right - left + 1)
        .sum::<usize>()
        + glyphs.len()
        - 1;
    let mut image = graphics::RenderedMap {
        width,
        height: 8,
        rgba: vec![0; width * 8 * 4],
    };
    let mut cursor = 0;
    for (tile, left, right) in glyphs {
        for y in 0..8 {
            for x in left..=right {
                let at = (y * width + cursor + x - left) * 4;
                image.rgba[at..at + 4].copy_from_slice(&pixel(palette, tile[y * 8 + x])?);
            }
        }
        cursor += right - left + 2;
    }
    Ok(image)
}

fn native_sidebar(
    rom: &VerifiedRom<'_>,
    palette: &[graphics::PaletteColor],
) -> Result<graphics::RenderedMap> {
    let decoded = gba::decode_lz77(rom.range("garden", 0x235fc, 4124)?, 16384)?.bytes;
    let tiles = graphics::decode_4bpp_tiles(&decoded, 512)?;
    let maps = gba::decode_lz77(rom.range("garden", 0x24618, 204)?, 720)?.bytes;
    if maps.len() != 720 {
        return Err("Unexpected native sidebar map length".into());
    }
    let entries = maps[360..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|word| graphics::TilemapEntry::from_word(u16::from_le_bytes([word[0], word[1]])))
        .collect::<Vec<_>>();
    Ok(graphics::render_text_map(
        &tiles,
        &entries,
        palette,
        graphics::RenderOptions {
            width_tiles: 9,
            height_tiles: 20,
            depth: graphics::TileDepth::FourBpp,
            transparent_zero: false,
            max_pixels: 72 * 160,
        },
    )?)
}

pub fn render(rom: &VerifiedRom<'_>) -> Result<RenderedFont> {
    let decoded = gba::decode_lz77(rom.range("garden", 0x220ec, 2904)?, 10784)?.bytes;
    if decoded.len() != 10784 {
        return Err("Unexpected native font stream length".into());
    }
    let tiles = graphics::decode_4bpp_tiles(&decoded, 337)?;
    let palette = graphics::parse_palette(rom.range("garden", 0x233fc, 512)?, 256)?;
    let sidebar = native_sidebar(rom, &palette)?;
    let mut matched_native_labels = Vec::new();
    for (text, y) in [("Mood", 32), ("Swim", 64), ("Stamina", 128)] {
        let label = compose(&tiles, &palette, text)?;
        for row in 0..8 {
            for x in 0..label.width {
                let at = (row * label.width + x) * 4;
                let native_at = ((y + row) * sidebar.width + 10 + x) * 4;
                let expected = if label.rgba[at + 3] == 0 {
                    let [r, g, b] = palette[4].rgb8();
                    [r, g, b, 255]
                } else {
                    label.rgba[at..at + 4].try_into()?
                };
                if sidebar.rgba[native_at..native_at + 4] != expected {
                    return Err(format!(
                        "Native English sidebar parity differs: {text} at {x},{row}"
                    )
                    .into());
                }
            }
        }
        matched_native_labels.push(text);
    }
    let mut images = Vec::new();
    for (text, filename) in [
        ("Luck", "tiny-native-label-luck-a2ne.png"),
        ("Intelligence", "tiny-native-label-intelligence-a2ne.png"),
    ] {
        let image = compose(&tiles, &palette, text)?;
        images.push(FontImage {
            filename,
            text,
            width: image.width,
            height: image.height,
            rgba_sha256: gba::sha256(&image.rgba),
            png: encode_png(&image)?,
        });
    }
    let mut atlas = graphics::RenderedMap {
        width: 128,
        height: 48,
        rgba: vec![0; 128 * 48 * 4],
    };
    for (index, tile) in tiles.iter().take(95).enumerate() {
        for y in 0..8 {
            for x in 0..8 {
                let at = (((index / 16) * 8 + y) * 128 + (index % 16) * 8 + x) * 4;
                atlas.rgba[at..at + 4].copy_from_slice(&pixel(&palette, tile[y * 8 + x])?);
            }
        }
    }
    images.push(FontImage {
        filename: "tiny-native-sidebar-ascii-a2ne.png",
        text: "ASCII 32..126",
        width: atlas.width,
        height: atlas.height,
        rgba_sha256: gba::sha256(&atlas.rgba),
        png: encode_png(&atlas)?,
    });
    Ok(RenderedFont {
        images,
        matched_native_labels,
    })
}
