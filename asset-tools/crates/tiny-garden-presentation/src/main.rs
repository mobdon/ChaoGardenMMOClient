//! Local original-art exporter. No gameplay, save, credential or network access.
use sonic_advance_2_assets::{VerifiedRom, catalog, gba, graphics};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Component, Path},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn resource(rom: &VerifiedRom<'_>, id: &str) -> Result<Vec<u8>> {
    let item = catalog::resources()?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("Resource not cataloged")?;
    let bytes = rom.range(&item.source, item.offset, item.length)?;
    Ok(if item.format == "lz77" {
        gba::decode_lz77(bytes, 96 * 1024)?.bytes
    } else {
        bytes.to_vec()
    })
}

fn registers(rom: &VerifiedRom<'_>) -> Result<BTreeMap<u16, u16>> {
    let mut result = BTreeMap::new();
    for (offset, length) in [(0x20e9c, 0x34), (0x20ed0, 0xc)] {
        for row in rom.range("garden", offset, length)?.as_chunks::<4>().0 {
            result.insert(
                u16::from_le_bytes([row[0], row[1]]),
                u16::from_le_bytes([row[2], row[3]]),
            );
        }
    }
    // InitGarden schedules this display word after setup (register 0).
    result.insert(0, 0x1f40);
    Ok(result)
}

fn copy_map(vram: &mut [u8], map: &[u8], screen: usize, width: usize, bias: u16) -> Result<()> {
    if width == 0 || width > 32 || !map.len().is_multiple_of(width * 2) {
        return Err("Incomplete map row".into());
    }
    for (cell, bytes) in map.as_chunks::<2>().0.iter().enumerate() {
        // The original loader adds to the whole 16-bit entry, retaining banks/flips.
        let value = u16::from_le_bytes([bytes[0], bytes[1]])
            .checked_add(bias)
            .ok_or("Tile bias overflow")?;
        let offset = screen * 0x800 + ((cell / width) * 32 + cell % width) * 2;
        vram.get_mut(offset..offset + 2)
            .ok_or("Map exceeds VRAM")?
            .copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn background(rom: &VerifiedRom<'_>, variant: usize) -> Result<Vec<u8>> {
    let mut vram = vec![0; 96 * 1024];
    // The native InitGarden loader's four independently verified source/destination pairs.
    for (id, offset) in [
        ("garden.bg.tiles-ui", 0x1c00),
        ("garden.bg.tiles-secondary", 0x41e0),
        ("garden.bg.tiles-ground", 0x55e0),
        ("garden.bg.tiles-main", 0x8000),
    ] {
        let bytes = resource(rom, id)?;
        vram.get_mut(offset..offset + bytes.len())
            .ok_or("Tiles exceed VRAM")?
            .copy_from_slice(&bytes);
    }
    // Native screen blocks are cleared before the two static maps are copied.
    vram[0xe000..].fill(0);
    copy_map(
        &mut vram,
        &resource(rom, "garden.bg.map-22x20")?,
        31,
        22,
        0x200,
    )?;
    let variants = resource(rom, "garden.bg.map-variants-lz77")?;
    let offset = variant * 9 * 20 * 2;
    copy_map(
        &mut vram,
        variants
            .get(offset..offset + 9 * 20 * 2)
            .ok_or("Short native panel map")?,
        30,
        9,
        0xe0,
    )?;
    Ok(vram)
}

fn render(
    rom: &VerifiedRom<'_>,
    variant: usize,
) -> Result<(graphics::RenderedMap, serde_json::Value)> {
    if variant > 1 {
        return Err("Native panel variant must be 0 or 1".into());
    }
    let vram = background(rom, variant)?;
    let registers = registers(rom)?;
    let display = *registers.get(&0).ok_or("Missing display control")?;
    if display & 7 != 0 {
        return Err("Only verified native text mode is supported".into());
    }
    let palette = graphics::parse_palette(&resource(rom, "garden.bg.palette")?, 256)?;
    let mut layers = Vec::new();
    for layer in 0..4u16 {
        if display & (0x100 << layer) == 0 {
            continue;
        }
        let control = *registers
            .get(&(8 + layer * 2))
            .ok_or("Missing BG control")?;
        if control & 0xc040 != 0 {
            return Err("Unsupported BG size/mosaic".into());
        }
        let tile_base = usize::from((control >> 2) & 3) * 0x4000;
        let screen = usize::from((control >> 8) & 31) * 0x800;
        let depth = if control & 0x80 != 0 {
            graphics::TileDepth::EightBpp
        } else {
            graphics::TileDepth::FourBpp
        };
        let tiles = match depth {
            graphics::TileDepth::FourBpp => {
                graphics::decode_4bpp_tiles(&vram[tile_base..0x10000], 2048)?
            }
            graphics::TileDepth::EightBpp => {
                graphics::decode_8bpp_tiles(&vram[tile_base..0x10000], 1024)?
            }
        };
        let entries = graphics::parse_tilemap(&vram[screen..screen + 0x800], 32, 32, 1024)?;
        let image = graphics::render_text_map(
            &tiles,
            &entries,
            &palette,
            graphics::RenderOptions {
                width_tiles: 32,
                height_tiles: 32,
                depth,
                transparent_zero: true,
                max_pixels: 256 * 256,
            },
        )?;
        let x = usize::from(*registers.get(&(0x10 + layer * 4)).unwrap_or(&0)) & 255;
        let y = usize::from(*registers.get(&(0x12 + layer * 4)).unwrap_or(&0)) & 255;
        layers.push((control & 3, layer, x, y, image));
    }
    // Back to front: lower priority number wins; lower BG index wins ties.
    layers.sort_by_key(|(priority, layer, ..)| {
        (std::cmp::Reverse(*priority), std::cmp::Reverse(*layer))
    });
    let [red, green, blue] = palette[0].rgb8();
    let mut output = graphics::RenderedMap {
        width: 240,
        height: 160,
        rgba: [red, green, blue, 255].repeat(240 * 160),
    };
    for (_, _, scroll_x, scroll_y, image) in &layers {
        for y in 0..160 {
            for x in 0..240 {
                let source = (((y + scroll_y) & 255) * 256 + ((x + scroll_x) & 255)) * 4;
                let target = (y * 240 + x) * 4;
                if image.rgba[source + 3] != 0 {
                    output.rgba[target..target + 4]
                        .copy_from_slice(&image.rgba[source..source + 4]);
                }
            }
        }
    }
    let metadata = serde_json::json!({
        "schemaVersion": 1, "source": "A2NE", "romSha256": sonic_advance_2_assets::ROM_SHA256,
        "gardenSha256": sonic_advance_2_assets::GARDEN_SHA256,
        "referenceCommit": sonic_advance_2_assets::REFERENCE_COMMIT,
        "width": 240, "height": 160, "panelVariant": variant, "registers": registers,
        "rgbaSha256": gba::sha256(&output.rgba),
        "scope": "Native initial static background layers; no OBJ, gameplay, native AI, audio or save import",
    });
    Ok((output, metadata))
}

fn png(image: &graphics::RenderedMap) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut bytes,
            image.width.try_into()?,
            image.height.try_into()?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&image.rgba)?;
    }
    Ok(bytes)
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "Usage: tiny-garden-presentation VERIFIED_A2NE_ROM EXISTING_LOCAL_OUTPUT_DIRECTORY"
                .into(),
        );
    }
    let directory = Path::new(&args[1]);
    let checkout = std::env::current_dir()?.canonicalize()?;
    let resolved = directory.canonicalize()?;
    if !directory.is_dir()
        || directory.components().any(|c| c == Component::ParentDir)
        || resolved
            .strip_prefix(checkout)
            .ok()
            .and_then(|relative| relative.components().next())
            != Some(Component::Normal(std::ffi::OsStr::new(".local")))
    {
        return Err(
            "Output must be inside this checkout's existing .local directory without traversal"
                .into(),
        );
    }
    // Reject symlink/junction ancestors so local art cannot escape through a reparse point.
    for ancestor in directory.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let metadata = std::fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() {
            return Err("Output cannot use symlink ancestors".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Output cannot use reparse-point ancestors".into());
            }
        }
    }
    for name in ["tiny-native-background.png", "tiny-native-background.json"] {
        if directory.join(name).try_exists()? {
            return Err("Native output already exists; choose a fresh local directory".into());
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[0])?
        .take(sonic_advance_2_assets::ROM_LEN as u64 + 1)
        .read_to_end(&mut bytes)?;
    let rom = VerifiedRom::parse(&bytes)?;
    let (image, mut metadata) = render(&rom, 1)?;
    let image = png(&image)?;
    metadata["pngSha256"] = gba::sha256(&image).into();
    for (name, bytes) in [
        ("tiny-native-background.png", image),
        (
            "tiny-native-background.json",
            serde_json::to_vec_pretty(&metadata)?,
        ),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        file.write_all(&bytes)?;
    }
    println!("{}", serde_json::to_string_pretty(&metadata)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tiny-garden-presentation: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_maps_keep_native_screen_stride_and_attributes() {
        let mut vram = vec![0; 0x1000];
        copy_map(&mut vram, &[3, 0x54, 7, 0xa0], 1, 1, 0x200).unwrap();
        assert_eq!(&vram[0x800..0x802], &[3, 0x56]);
        assert_eq!(&vram[0x840..0x842], &[7, 0xa2]);
        assert!(vram[0x802..0x840].iter().all(|b| *b == 0));
        assert!(copy_map(&mut vram, &[0; 3], 1, 2, 0).is_err());
        assert!(copy_map(&mut vram, &[0; 2], 1, 0, 0).is_err());
        assert!(copy_map(&mut vram, &[255; 2], 1, 1, 1).is_err());
        assert!(copy_map(&mut vram, &[0; 2], 2, 1, 0).is_err());
    }

    #[test]
    fn exported_png_preserves_pixels_and_alpha_without_filtering() {
        let pixels = graphics::RenderedMap {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 0],
        };
        let bytes = png(&pixels).unwrap();
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut output = vec![0; decoder.output_buffer_size()];
        let info = decoder.next_frame(&mut output).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(output, pixels.rgba);
    }

    #[test]
    #[ignore = "Set A2NE_ROM to the supplied local cartridge"]
    fn verified_source_renders_both_native_panels_and_rejects_modified_rom() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("Set A2NE_ROM")).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        let (english, _) = render(&rom, 1).unwrap();
        let (japanese, _) = render(&rom, 0).unwrap();
        assert_eq!((english.width, english.height), (240, 160));
        assert_eq!(
            gba::sha256(&english.rgba),
            "7f4af9cab17d2f44c57208630f46577a9488e599f57eda2673c0840a75d721ce"
        );
        assert_eq!(
            gba::sha256(&japanese.rgba),
            "85092d6efa1c9bba6394c1733de9f7e0b8d17e3442663438c6c1735ecd22dfca"
        );
        // The panels differ while the original garden field is identical.
        for y in 0..160 {
            let offset = y * 240 * 4;
            assert_eq!(
                &english.rgba[offset..offset + 171 * 4],
                &japanese.rgba[offset..offset + 171 * 4]
            );
        }
        assert!(render(&rom, 2).is_err());
        let mut modified = bytes;
        modified[0x200] ^= 1;
        assert!(VerifiedRom::parse(&modified).is_err());
    }
}
