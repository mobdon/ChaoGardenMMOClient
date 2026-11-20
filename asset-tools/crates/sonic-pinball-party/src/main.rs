use sonic_pinball_party::{
    catalog::{self, Catalog},
    graphics::{self, ColorDepth},
    sha256,
};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("input exceeds limit".into());
    }
    Ok(bytes)
}

fn write_local(directory: &Path, filename: &str, bytes: &[u8]) -> Result<()> {
    let dir = fs::canonicalize(directory)?;
    let mut local = false;
    for parent in dir.ancestors() {
        if parent.file_name().is_some_and(|n| n == ".local") {
            local = true;
            break;
        }
        if parent.join(".git").exists() {
            break;
        }
    }
    if !dir.is_dir() || !local {
        return Err("output directory must already exist beneath .local".into());
    }
    // Fixed/generated flat names, create_new: never follow output symlinks or overwrite.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(filename))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn pinned_catalog() -> Result<Catalog> {
    Ok(serde_json::from_str(include_str!("../catalog.json"))?)
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().and_then(|s| s.to_str()).unwrap_or("help");
    if command == "help" || command == "--help" {
        println!("spp-assets catalog|verify|audio|tables ROM\nspp-assets save ROM FLASH_OR_RECORD\nspp-assets extract ROM ASSET_ID EXISTING_LOCAL_DIR\nspp-assets tiles ROM TILE_ID PALETTE_ID 4|8 EXISTING_LOCAL_DIR\nAll ROM commands require exact retail A3VE rev0. Native outputs only beneath .local; files are never overwritten.");
        return Ok(());
    }
    let expected = match command {
        "catalog" | "verify" | "audio" | "tables" => 2,
        "save" => 3,
        "extract" => 4,
        "tiles" => 6,
        _ => return Err("unknown command; use --help".into()),
    };
    if args.len() != expected {
        return Err("incorrect arguments; use --help".into());
    }
    let rom = read_bounded(Path::new(&args[1]), catalog::ROM_LEN)?;
    catalog::validate_rom(&rom)?;
    match command {
        "catalog" => println!(
            "{}",
            serde_json::to_string_pretty(&catalog::build_catalog(&rom)?)?
        ),
        "audio" => println!(
            "{}",
            serde_json::to_string_pretty(&sonic_pinball_party::audio::parse_songs(&rom)?)?
        ),
        "tables" => println!(
            "{}",
            serde_json::to_string_pretty(&sonic_pinball_party::tables::parse_tables(&rom)?)?
        ),
        "save" => {
            let source = catalog::VerifiedRom::new(&rom)?;
            sonic_pinball_party::garden::verify_payload_layout(source.garden())?;
            let bytes = read_bounded(
                Path::new(&args[2]),
                sonic_pinball_party::garden::FLASH_LENGTH,
            )?;
            let (sector, record) = if bytes.len() == sonic_pinball_party::garden::FLASH_LENGTH {
                let selected = sonic_pinball_party::garden::select_flash_record(&bytes)?;
                (Some(selected.sector), selected.record)
            } else {
                (None, sonic_pinball_party::garden::read_record(&bytes)?)
            };
            println!(
                "{}",
                serde_json::json!({"sha256":sha256(&bytes),"sector":sector,"generation":record.generation,"recordSha256":sha256(record.bytes),"recordLength":record.bytes.len(),"opaqueBytesPreserved":true,"authority":"local native inspection only; no MMO import"})
            );
        }
        "verify" => {
            let pinned = pinned_catalog()?;
            if pinned.source != catalog::validate_rom(&rom)? || pinned.schema_version != 1 {
                return Err("catalog source/schema mismatch".into());
            }
            let source = catalog::VerifiedRom::new(&rom)?;
            sonic_pinball_party::garden::verify_payload_layout(source.garden())?;
            for asset in &pinned.assets {
                source.extract(asset)?;
            }
            println!(
                "{}",
                serde_json::json!({"sourceSha256":catalog::ROM_SHA256,"verifiedAssets":pinned.assets.len(),"songEntries":sonic_pinball_party::audio::parse_songs(&rom)?.len()})
            );
        }
        "extract" | "tiles" => {
            let pinned = pinned_catalog()?;
            let id = args[2].to_str().ok_or("asset ID must be UTF-8")?;
            let asset = pinned
                .assets
                .iter()
                .find(|a| a.id == id)
                .ok_or("unknown pinned asset ID")?;
            let native = catalog::extract_asset(&rom, asset)?;
            if command == "extract" {
                write_local(Path::new(&args[3]), &format!("{}.bin", asset.id), &native)?;
                println!(
                    "{}",
                    serde_json::json!({"id":asset.id,"length":native.len(),"sha256":sha256(&native)})
                );
            } else {
                let palette_id = args[3].to_str().ok_or("palette ID must be UTF-8")?;
                let palette_asset = pinned
                    .assets
                    .iter()
                    .find(|a| a.id == palette_id)
                    .ok_or("unknown pinned palette ID")?;
                let palette = graphics::decode_palette_le(
                    &catalog::extract_asset(&rom, palette_asset)?,
                    256,
                )?;
                let depth = match args[4].to_str() {
                    Some("4") => ColorDepth::Bpp4,
                    Some("8") => ColorDepth::Bpp8,
                    _ => return Err("depth must be 4 or 8".into()),
                };
                let tiles = graphics::decode_tiles(&native, depth, 8192)?;
                if tiles.is_empty() {
                    return Err("empty tile resource".into());
                }
                let width = 128;
                let height = tiles.len().div_ceil(16) * 8;
                let mut rgba = vec![0; width * height * 4];
                for (i, tile) in tiles.iter().enumerate() {
                    for (p, &index) in tile.iter().enumerate() {
                        if index == 0 {
                            continue;
                        }
                        let color = palette
                            .get(usize::from(index))
                            .ok_or("tile index exceeds explicit palette")?;
                        let at = (((i / 16) * 8 + p / 8) * width + (i % 16) * 8 + p % 8) * 4;
                        rgba[at..at + 4].copy_from_slice(color);
                    }
                }
                let header = format!("P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n");
                let mut pam = header.into_bytes();
                pam.extend_from_slice(&rgba);
                write_local(
                    Path::new(&args[5]),
                    &format!("{}-{}.pam", asset.id, palette_asset.id),
                    &pam,
                )?;
                println!(
                    "{}",
                    serde_json::json!({"tileId":asset.id,"paletteId":palette_asset.id,"width":width,"height":height,"rgbaSha256":sha256(&rgba),"pamSha256":sha256(&pam),"interpretation":"explicit depth/palette, tile sheet only; no sprite assembly or animation inference"})
                );
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("spp-assets: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_rejects_nonlocal_directory() {
        assert!(write_local(
            &std::env::temp_dir(),
            "spp-should-not-exist.bin",
            b"synthetic"
        )
        .is_err());
        // A checkout itself may live below another checkout's .local/worktrees.
        assert!(write_local(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
            "spp-should-not-exist.pam",
            b"synthetic"
        )
        .is_err());
    }
}
