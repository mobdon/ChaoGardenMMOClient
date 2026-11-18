//! Owner-local A2NE garden background composition. Decoders stay in the
//! independently published Rust asset crate; no native bytes are embedded.
#![forbid(unsafe_code)]

use sonic_advance_2_assets::{VerifiedRom, gba, graphics};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const WIDTH_TILES: usize = 22;
const HEIGHT_TILES: usize = 20;
const MAP_OFFSET: usize = 0x21bc4;
const PALETTE_OFFSET: usize = 0x233fc;
const SIDEBAR_MAP_OFFSET: usize = 0x24618;
const SIDEBAR_WIDTH_TILES: usize = 9;
const SIDEBAR_MAP_BYTES: usize = SIDEBAR_WIDTH_TILES * HEIGHT_TILES * 2;
const UPLOADS: [(usize, usize); 4] = [
    (0x235fc, 0x1c00),
    (0x22c44, 0x41e0),
    (0x220ec, 0x55e0),
    (0x211c8, 0x8000),
];

fn copy_at(destination: &mut [u8], offset: usize, source: &[u8]) -> Result<()> {
    let end = offset
        .checked_add(source.len())
        .ok_or("VRAM range overflow")?;
    destination
        .get_mut(offset..end)
        .ok_or("VRAM upload out of range")?
        .copy_from_slice(source);
    Ok(())
}

/// Mirrors the ten rows of composited tiles prepared by sub_02001c60.
/// These tiles service the adjacent native UI; recording all uploads avoids
/// pretending that each compressed stream is an independent tile set.
fn compose_ui_tiles(vram: &mut [u8]) -> Result<()> {
    for row in 0..10 {
        let base = vram.get(0x2980..0x2ac0).ok_or("UI base missing")?.to_vec();
        copy_at(vram, 0xf80 + row * 0x140, &base)?;
        let overlay_offset = 0x2ac0 + row * 32;
        let overlay: [u8; 32] = vram
            .get(overlay_offset..overlay_offset + 32)
            .ok_or("UI overlay missing")?
            .try_into()?;
        for column in 0..10 {
            let start = 0xf80 + row * 0x140 + column * 32;
            for (target, mask) in vram[start..start + 32].iter_mut().zip(overlay) {
                *target |= mask;
            }
        }
    }
    Ok(())
}

fn load_vram(rom: &VerifiedRom<'_>) -> Result<(Vec<u8>, Vec<serde_json::Value>)> {
    let garden = rom.garden_bytes();
    let mut vram = vec![0; 0x10000];
    let mut uploads = Vec::new();
    for (source, target) in UPLOADS {
        let stream = gba::decode_lz77(garden.get(source..).ok_or("Missing LZ source")?, 0x10000)?;
        copy_at(&mut vram, target, &stream.bytes)?;
        uploads.push(serde_json::json!({
            "gardenOffset": source, "vramOffset": target,
            "compressedLength": stream.consumed, "decodedLength": stream.bytes.len(),
            "compressedSha256": gba::sha256(rom.range("garden", source, stream.consumed)?),
            "decodedSha256": gba::sha256(&stream.bytes),
        }));
    }
    compose_ui_tiles(&mut vram)?;
    Ok((vram, uploads))
}

fn native_map(
    raw_map: &[u8],
    width_tiles: usize,
    tile_base: u16,
) -> Result<Vec<graphics::TilemapEntry>> {
    let mut entries = graphics::parse_tilemap(
        raw_map,
        width_tiles,
        HEIGHT_TILES,
        width_tiles * HEIGHT_TILES,
    )?;
    for entry in &mut entries {
        // sub_02001718 adds to the complete native halfword, retaining its
        // flip and palette-bank bits. Exact-source maps cannot overflow.
        let word = entry
            .raw
            .checked_add(tile_base)
            .ok_or("Map tile-base overflow")?;
        *entry = graphics::TilemapEntry::from_word(word);
    }
    Ok(entries)
}

fn render(rom: &VerifiedRom<'_>) -> Result<(graphics::RenderedMap, serde_json::Value)> {
    // Native register table: BG3CNT=0x1f83 => 8bpp, character base 0,
    // screen block 31. InitGarden copies a 22x20 map with tile base +0x200.
    let registers = rom.range("garden", 0x20e9c, 0x34)?;
    if registers[40..44] != [0x0e, 0x00, 0x83, 0x1f] {
        return Err("Unexpected native BG3 control register".into());
    }
    let (vram, uploads) = load_vram(rom)?;
    let raw_map = rom.range("garden", MAP_OFFSET, WIDTH_TILES * HEIGHT_TILES * 2)?;
    let entries = native_map(raw_map, WIDTH_TILES, 0x200)?;
    let palette_bytes = rom.range("garden", PALETTE_OFFSET, 512)?;
    let palette = graphics::parse_palette(palette_bytes, 256)?;
    let tiles = graphics::decode_8bpp_tiles(&vram, 1024)?;
    let image = graphics::render_text_map(
        &tiles,
        &entries,
        &palette,
        graphics::RenderOptions {
            width_tiles: WIDTH_TILES,
            height_tiles: HEIGHT_TILES,
            depth: graphics::TileDepth::EightBpp,
            // The hardware background uses palette[0] as its backdrop. This is
            // a complete garden background crop, not a transparent OBJ sprite.
            transparent_zero: false,
            max_pixels: WIDTH_TILES * HEIGHT_TILES * 64,
        },
    )?;
    let receipt = serde_json::json!({
        "schemaVersion": 1,
        "kind": "original-a2ne-garden-background",
        "romSha256": sonic_advance_2_assets::ROM_SHA256,
        "gardenSha256": sonic_advance_2_assets::GARDEN_SHA256,
        "referenceRepository": "SAT-R/sa2",
        "referenceCommit": sonic_advance_2_assets::REFERENCE_COMMIT,
        "referenceFunctions": ["InitGarden", "sub_02001c60", "sub_02001718"],
        "decoder": "sonic-advance-2-assets path dependency; VerifiedRom + gba + graphics",
        "map": {"gardenOffset": MAP_OFFSET, "length": raw_map.len(), "sha256": gba::sha256(raw_map),
            "widthTiles": WIDTH_TILES, "heightTiles": HEIGHT_TILES, "tileBase": 512,
            "bg3cnt": 8067, "depth": 8},
        "palette": {"gardenOffset": PALETTE_OFFSET, "length": 512, "sha256": gba::sha256(palette_bytes),
            "conversion": "BGR555 channels expanded by bit replication; no color correction"},
        "uploads": uploads,
        "width": image.width, "height": image.height,
        "rgbaSha256": gba::sha256(&image.rgba),
        "alpha": "opaque native background with palette-zero backdrop",
        "scope": "22x20 garden map only; no sidebar, OBJ/Chao/item sprites, OAM, animation, save, mechanics or progression",
        "presentation": "Pixel-faithful tile/map/palette composition; using it beneath the canonical MMO Chao is an adaptation",
    });
    Ok((image, receipt))
}

fn render_sidebar(
    rom: &VerifiedRom<'_>,
    variant: usize,
) -> Result<(graphics::RenderedMap, serde_json::Value)> {
    if variant > 1 {
        return Err("Sidebar variant must be 0 or 1".into());
    }
    // BG2CNT=0x1e02: 4bpp text map, character base 0, screen block 30.
    // Native HOFS=-171 and VOFS=0 show source columns 0..68 at x171..239.
    let registers = rom.range("garden", 0x20e9c, 0x34)?;
    if registers[28..40]
        != [
            0x0c, 0x00, 0x02, 0x1e, 0x18, 0x00, 0x55, 0xff, 0x1a, 0x00, 0x00, 0x00,
        ]
    {
        return Err("Unexpected native BG2 control/scroll registers".into());
    }
    let (vram, uploads) = load_vram(rom)?;
    let maps = gba::decode_lz77(
        rom.garden_bytes()
            .get(SIDEBAR_MAP_OFFSET..)
            .ok_or("Missing sidebar map source")?,
        SIDEBAR_MAP_BYTES * 2,
    )?;
    if maps.bytes.len() != SIDEBAR_MAP_BYTES * 2 {
        return Err("Unexpected native sidebar map length".into());
    }
    let map_start = variant * SIDEBAR_MAP_BYTES;
    let raw_map = &maps.bytes[map_start..map_start + SIDEBAR_MAP_BYTES];
    let entries = native_map(raw_map, SIDEBAR_WIDTH_TILES, 0xe0)?;
    let palette_bytes = rom.range("garden", PALETTE_OFFSET, 512)?;
    let palette = graphics::parse_palette(palette_bytes, 256)?;
    // 4bpp text map indices address only the first 1024 32-byte tiles.
    let tiles = graphics::decode_4bpp_tiles(&vram[..0x8000], 1024)?;
    let image = graphics::render_text_map(
        &tiles,
        &entries,
        &palette,
        graphics::RenderOptions {
            width_tiles: SIDEBAR_WIDTH_TILES,
            height_tiles: HEIGHT_TILES,
            depth: graphics::TileDepth::FourBpp,
            transparent_zero: true,
            max_pixels: SIDEBAR_WIDTH_TILES * HEIGHT_TILES * 64,
        },
    )?;
    let receipt = serde_json::json!({
        "schemaVersion": 1,
        "kind": "original-a2ne-garden-sidebar",
        "romSha256": sonic_advance_2_assets::ROM_SHA256,
        "gardenSha256": sonic_advance_2_assets::GARDEN_SHA256,
        "referenceRepository": "SAT-R/sa2",
        "referenceCommit": sonic_advance_2_assets::REFERENCE_COMMIT,
        "referenceFunctions": ["InitGarden", "sub_02001c60", "sub_02001718"],
        "decoder": "sonic-advance-2-assets path dependency; VerifiedRom + gba + graphics",
        "mapStream": {"gardenOffset": SIDEBAR_MAP_OFFSET,
            "compressedLength": maps.consumed, "decodedLength": maps.bytes.len(),
            "compressedSha256": gba::sha256(rom.range("garden", SIDEBAR_MAP_OFFSET, maps.consumed)?),
            "decodedSha256": gba::sha256(&maps.bytes)},
        "map": {"variant": variant, "decodedOffset": map_start,
            "length": raw_map.len(), "sha256": gba::sha256(raw_map),
            "widthTiles": SIDEBAR_WIDTH_TILES, "heightTiles": HEIGHT_TILES,
            "tileBase": 224, "bg2cnt": 7682, "depth": 4},
        "palette": {"gardenOffset": PALETTE_OFFSET, "length": 512, "sha256": gba::sha256(palette_bytes),
            "conversion": "BGR555 channels expanded by bit replication; no color correction"},
        "uploads": uploads,
        "width": image.width, "height": image.height,
        "rgbaSha256": gba::sha256(&image.rgba),
        "alpha": "4bpp index zero transparent within each original palette bank",
        "nativePlacement": {"screenWidth": 240, "screenHeight": 160,
            "bg2HorizontalOffset": -171, "bg2VerticalOffset": 0,
            "screenX": 171, "screenY": 0, "visibleSourceWidth": 69,
            "note": "Full 72-pixel source is preserved; last three columns are offscreen on hardware"},
        "scope": "Static BG2 sidebar map only; no names, stat values, cursor/OBJ sprites, OAM, animation, menu state machine, save or mechanics",
        "presentation": "Original pixels/maps preserved; dynamic MMO data must be separately labeled and Rust-authoritative; variant language/mode semantics unverified",
    });
    Ok((image, receipt))
}

fn validate_output(path: &Path) -> Result<()> {
    if path.extension().and_then(|x| x.to_str()) != Some("png")
        || path.components().any(|x| matches!(x, Component::ParentDir))
        || !path.components().any(|x| x.as_os_str() == ".local")
    {
        return Err(
            "Output must be a .png inside an explicit ignored .local directory without '..'".into(),
        );
    }
    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if let Ok(metadata) = fs::symlink_metadata(parent) {
            if metadata.file_type().is_symlink() {
                return Err("Output has symlink ancestor".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Output has reparse-point ancestor".into());
                }
            }
        }
        ancestor = parent.parent();
    }
    Ok(())
}

fn png_bytes(image: &graphics::RenderedMap) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut bytes,
            image.width.try_into()?,
            image.height.try_into()?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.rgba)?;
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    if !(2..=3).contains(&args.len()) {
        return Err(
            "Usage: tiny-garden-render A2NE_ROM .local/OUTPUT.png [garden|sidebar-0|sidebar-1]"
                .into(),
        );
    }
    let kind = args.get(2).map_or("garden", String::as_str);
    if !matches!(kind, "garden" | "sidebar-0" | "sidebar-1") {
        return Err("Unknown export kind (expected garden, sidebar-0 or sidebar-1)".into());
    }
    let output = Path::new(&args[1]);
    validate_output(output)?;
    let receipt_path = output.with_extension("png.provenance.json");
    if output.exists() || receipt_path.exists() {
        return Err("Refusing to overwrite existing native image or receipt".into());
    }
    let mut bytes = Vec::new();
    File::open(&args[0])?
        .take(sonic_advance_2_assets::ROM_LEN as u64 + 1)
        .read_to_end(&mut bytes)?;
    let rom = VerifiedRom::parse(&bytes)?;
    let (image, mut receipt) = match kind {
        "sidebar-0" => render_sidebar(&rom, 0)?,
        "sidebar-1" => render_sidebar(&rom, 1)?,
        _ => render(&rom)?,
    };
    let encoded = png_bytes(&image)?;
    receipt["pngSha256"] = serde_json::json!(gba::sha256(&encoded));
    receipt["filename"] = serde_json::json!(
        output
            .file_name()
            .ok_or("Missing filename")?
            .to_str()
            .ok_or("Non-UTF8 filename")?
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    write_new(output, &encoded)?;
    write_new(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    println!(
        "Rendered {}x{} original {kind}; PNG SHA-256 {}",
        image.width,
        image.height,
        gba::sha256(&encoded)
    );
    println!("RGBA SHA-256 {}", gba::sha256(&image.rgba));
    Ok(())
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("tiny-garden-render: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_stays_private_and_uploads_bounded() {
        assert!(validate_output(Path::new("public/garden.png")).is_err());
        assert!(validate_output(Path::new(".local/../public/garden.png")).is_err());
        assert!(copy_at(&mut [0; 2], 1, &[1, 2]).is_err());
        assert!(copy_at(&mut [0; 2], usize::MAX, &[1]).is_err());
    }

    #[test]
    fn native_overlay_repeats_row_mask_across_ten_tiles() {
        let mut vram = vec![0; 0x10000];
        vram[0x2980..0x2ac0].fill(1);
        for row in 0..10 {
            vram[0x2ac0 + row * 32..0x2ac0 + (row + 1) * 32].fill((row + 1) as u8 * 2);
        }
        compose_ui_tiles(&mut vram).unwrap();
        for row in 0..10 {
            assert!(
                vram[0xf80 + row * 0x140..0xf80 + (row + 1) * 0x140]
                    .iter()
                    .all(|x| *x == (1 | ((row + 1) as u8 * 2)))
            );
        }
    }

    #[test]
    fn native_map_adds_base_without_losing_palette_or_flips() {
        let mut raw = vec![0; SIDEBAR_MAP_BYTES];
        raw[..2].copy_from_slice(&0xac12_u16.to_le_bytes());
        let entries = native_map(&raw, SIDEBAR_WIDTH_TILES, 0xe0).unwrap();
        assert_eq!(entries.len(), 180);
        assert_eq!(entries[0].raw, 0xacf2);
        assert_eq!(entries[0].tile_index, 0xf2);
        assert_eq!(entries[0].palette_bank, 10);
        assert!(entries[0].horizontal_flip);
        assert!(entries[0].vertical_flip);
        assert_eq!(entries[1].raw, 0xe0);
        raw[..2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(native_map(&raw, SIDEBAR_WIDTH_TILES, 0xe0).is_err());
        assert!(native_map(&raw[..raw.len() - 2], SIDEBAR_WIDTH_TILES, 0xe0).is_err());
    }
}
