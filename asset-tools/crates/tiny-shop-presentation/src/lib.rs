//! Static original-art export only. Purchases, care, progression, identity and
//! saves belong to the independent Rust authority. No native bytes are embedded.
use serde::Deserialize;
pub mod font;
use sonic_advance_2_assets::{VerifiedRom, gba, graphics};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const ROM_BYTES: usize = 16 * 1024 * 1024;
pub const SOURCE_METADATA: &str = include_str!("../catalog/source.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCatalog {
    pub schema_version: u8,
    pub game_code: String,
    pub rom_sha256: String,
    pub garden_sha1: String,
    pub garden_length: usize,
    pub reference_commit: String,
    pub spans: Vec<Span>,
    pub subjects: Vec<Subject>,
    pub font_outputs: Vec<FontOutput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub id: String,
    pub offset: usize,
    pub length: usize,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Fruit,
    Egg,
    Toy,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fruit => "fruit",
            Self::Egg => "egg",
            Self::Toy => "toy",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub kind: Kind,
    pub native_id: usize,
    pub label: String,
    pub filename: String,
    pub native_tile_index: u16,
    pub palette_bank: u8,
    pub palette_source: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontOutput {
    pub filename: String,
    pub text: String,
    pub width: usize,
    pub height: usize,
    pub sha256: String,
}

pub struct RenderedSubject {
    pub subject: Subject,
    pub png: Vec<u8>,
    pub rgba_sha256: String,
}

pub struct RenderedExport {
    pub catalog: SourceCatalog,
    pub subjects: Vec<RenderedSubject>,
    pub weed_png: Vec<u8>,
    pub font: font::RenderedFont,
}

pub fn catalog() -> Result<SourceCatalog> {
    let catalog: SourceCatalog = serde_json::from_str(SOURCE_METADATA)?;
    if catalog.schema_version != 1
        || catalog.game_code != "A2NE"
        || catalog.garden_length != 191_340
        || catalog.subjects.len() != 22
        || catalog.font_outputs.len() != 3
    {
        return Err("Unsupported static source catalog".into());
    }
    let mut filenames = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for subject in &catalog.subjects {
        let name = Path::new(&subject.filename);
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
            || name.extension().is_none_or(|extension| extension != "png")
            || !filenames.insert(subject.filename.clone())
            || !ids.insert((subject.kind.label(), subject.native_id))
        {
            return Err("Invalid or duplicate subject identity/filename".into());
        }
    }
    for (kind, count) in [("fruit", 7), ("egg", 12), ("toy", 3)] {
        if (0..count).any(|id| !ids.contains(&(kind, id))) {
            return Err("Incomplete native subject set".into());
        }
    }
    for output in &catalog.font_outputs {
        let name = Path::new(&output.filename);
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
            || name.extension().is_none_or(|extension| extension != "png")
            || !filenames.insert(output.filename.clone())
        {
            return Err("Invalid or duplicate font output filename".into());
        }
    }
    Ok(catalog)
}

pub fn read_source(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() != ROM_BYTES as u64 {
        return Err("Only the supplied 16-MiB A2NE USA revision 0 ROM is supported".into());
    }
    Ok(fs::read(path)?)
}

fn encode_png(image: &graphics::RenderedMap) -> Result<Vec<u8>> {
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

fn sprite(
    tiles: &[graphics::Tile],
    palette: &[graphics::PaletteColor],
    native_tile: u16,
    bank: u8,
) -> Result<graphics::RenderedMap> {
    // InitGarden's second native OBJ stream starts at VRAM byte 0x1580.
    // DISPCNT selects 1D OBJ mapping. Record 0x205E4 is a 16x16 square,
    // whose four 8x8 character tiles follow in original row order.
    let first = native_tile
        .checked_sub(0x1580 / 32)
        .ok_or("Subject tile is below the loaded OBJ stream")?;
    let entries = (0..4)
        .map(|index| graphics::TilemapEntry::from_word(first + index + (u16::from(bank) << 12)))
        .collect::<Vec<_>>();
    Ok(graphics::render_text_map(
        tiles,
        &entries,
        palette,
        graphics::RenderOptions {
            width_tiles: 2,
            height_tiles: 2,
            depth: graphics::TileDepth::FourBpp,
            transparent_zero: true,
            max_pixels: 256,
        },
    )?)
}

pub fn render(source: &[u8]) -> Result<RenderedExport> {
    let catalog = catalog()?;
    if source.len() != ROM_BYTES || gba::sha256(source) != catalog.rom_sha256 {
        return Err("Unsupported or modified A2NE source fingerprint".into());
    }
    // Reuse the existing complete cartridge/header/payload/CRC gate. This
    // exporter introduces no alternate ROM or compression reader.
    let rom = VerifiedRom::parse(source)?;
    for span in &catalog.spans {
        let bytes = rom.range("garden", span.offset, span.length)?;
        if gba::sha256(bytes) != span.sha256 {
            return Err(format!("Pinned source span changed: {}", span.id).into());
        }
    }
    let decoded = gba::decode_lz77(rom.range("garden", 0x268b4, 3812)?, 6144)?.bytes;
    if decoded.len() != 6144 {
        return Err("Unexpected native OBJ stream length".into());
    }
    let tiles = graphics::decode_4bpp_tiles(&decoded, 192)?;
    let palette = graphics::parse_palette(rom.range("garden", 0x246e4, 512)?, 256)?;
    let mut subjects = Vec::with_capacity(catalog.subjects.len());
    for subject in &catalog.subjects {
        let image = if subject.kind == Kind::Egg {
            // Native InitGarden copies this exact row to OBJ palette bank 14.
            // A standalone subject can address the same 16 words as bank 0.
            let egg_palette =
                graphics::parse_palette(rom.range("garden", subject.palette_source, 32)?, 16)?;
            sprite(&tiles, &egg_palette, subject.native_tile_index, 0)?
        } else {
            sprite(
                &tiles,
                &palette,
                subject.native_tile_index,
                subject.palette_bank,
            )?
        };
        let png = encode_png(&image)?;
        if gba::sha256(&png) != subject.sha256 {
            return Err(format!(
                "Subject pixel/encoding parity changed: {}",
                subject.filename
            )
            .into());
        }
        subjects.push(RenderedSubject {
            subject: subject.clone(),
            rgba_sha256: gba::sha256(&image.rgba),
            png,
        });
    }
    let font = font::render(&rom)?;
    for (image, expected) in font.images.iter().zip(&catalog.font_outputs) {
        if image.filename != expected.filename
            || image.text != expected.text
            || image.width != expected.width
            || image.height != expected.height
            || gba::sha256(&image.png) != expected.sha256
        {
            return Err(format!("Font output parity changed: {}", image.filename).into());
        }
    }
    let weed_png = encode_png(&sprite(&tiles,&palette,0x109,1)?)?;
    Ok(RenderedExport {
        weed_png,
        catalog,
        subjects,
        font,
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

/// The caller chooses the location explicitly. Existing directories/files
/// are never overwritten; source validation and pixel parity happen first.
pub fn write_export(output: &Path, export: &RenderedExport) -> Result<()> {
    fs::create_dir(output)?;
    write_new(&output.join("tiny-weed-a2ne.png"),&export.weed_png)?;
    for image in &export.subjects {
        write_new(&output.join(&image.subject.filename), &image.png)?;
    }
    for image in &export.font.images {
        write_new(&output.join(image.filename), &image.png)?;
    }
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "kind": "static-original-a2ne-tiny-shop-subjects",
        "source": { "gameCode": export.catalog.game_code, "romSha256": export.catalog.rom_sha256,
            "gardenSha1": export.catalog.garden_sha1, "gardenLength": export.catalog.garden_length,
            "referenceCommit": export.catalog.reference_commit, "spans": serde_json::from_str::<serde_json::Value>(SOURCE_METADATA)?["spans"] },
        "tool": { "version": env!("CARGO_PKG_VERSION"), "sourceMetadataSha256": gba::sha256(SOURCE_METADATA.as_bytes()),
            "librarySourceSha256": gba::sha256(include_bytes!("lib.rs")), "cliSourceSha256": gba::sha256(include_bytes!("main.rs")),
            "fontSourceSha256": gba::sha256(include_bytes!("font.rs")) },
        "gardenWeed":{"filename":"tiny-weed-a2ne.png","nativeTile":265,"paletteBank":1,"descriptorOffset":132580,"sha256":gba::sha256(&export.weed_png)},
        "presentation": { "width": 16, "height": 16, "transparentNativeIndexZero": true,
            "recolored": false, "filtered": false, "nativeGeometryChanged": false },
        "outputs": export.subjects.iter().map(|image| serde_json::json!({
            "kind": image.subject.kind.label(), "nativeId": image.subject.native_id,
            "label": image.subject.label, "filename": image.subject.filename,
            "nativeTileIndex": image.subject.native_tile_index, "paletteBank": image.subject.palette_bank,
            "paletteSource": image.subject.palette_source, "width":16, "height":16,
            "sha256": image.subject.sha256, "rgbaSha256": image.rgba_sha256,
        })).collect::<Vec<_>>(),
        "font": { "source": "original ASCII tiles: character code minus 32, stream 0x220EC, native VRAM byte 0x55E0",
            "paletteBinding": "Original glyph indices 1/2/3 map to original English sidebar palette 0 indices 9/7/5; original background index 4 becomes transparent",
            "spacing": "Horizontal ink bounds with one original-pixel gap; original eight-pixel baseline preserved",
            "matchedBakedNativeLabels": export.font.matched_native_labels,
            "adaptation": "Luck and Intelligence are newly composed labels from original native glyphs, not native baked hidden-stat labels; ASCII atlas uses the same native palette binding",
            "outputs": export.font.images.iter().map(|image| serde_json::json!({
                "filename": image.filename, "text": image.text, "width":image.width, "height":image.height,
                "sha256":gba::sha256(&image.png), "rgbaSha256": image.rgba_sha256,
            })).collect::<Vec<_>>() },
        "limits": "Static source subjects and native-glyph presentation only. Native stock, hatching, toy actions, care, progression, transfer and Rust service acceptance are separate. Egg labels describe visible colors, not verified jewel/genetics codes."
    });
    write_new(
        &output.join("provenance.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_covers_the_exact_native_subject_set_without_asset_bytes() {
        let catalog = catalog().unwrap();
        assert_eq!(catalog.subjects.len(), 22);
        assert_eq!(
            catalog
                .subjects
                .iter()
                .filter(|s| s.kind == Kind::Fruit)
                .count(),
            7
        );
        assert_eq!(
            catalog
                .subjects
                .iter()
                .filter(|s| s.kind == Kind::Egg)
                .count(),
            12
        );
        assert_eq!(
            catalog
                .subjects
                .iter()
                .filter(|s| s.kind == Kind::Toy)
                .count(),
            3
        );
        assert!(!SOURCE_METADATA.contains("base64"));
        assert!(!SOURCE_METADATA.contains("rgbaBytes"));
    }

    #[test]
    fn rgba_png_roundtrip_preserves_transparency_and_original_color_channels() {
        let image = graphics::RenderedMap {
            width: 2,
            height: 1,
            rgba: vec![12, 34, 56, 0, 78, 90, 123, 255],
        };
        let bytes = encode_png(&image).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut decoded = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(decoded, image.rgba);
    }

    #[test]
    fn unrelated_short_and_oversized_inputs_are_rejected_before_graphics() {
        assert!(render(&[]).is_err());
        assert!(render(&[0; 192]).is_err());
        assert!(render(&vec![0; ROM_BYTES + 1]).is_err());
    }
}
