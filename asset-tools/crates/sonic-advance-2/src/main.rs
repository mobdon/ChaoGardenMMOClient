use sonic_advance_2_assets::{
    VerifiedRom, catalog, gba, graphics, native_campaign_save, native_save, save_layout, transfer,
};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds {limit} byte limit").into());
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

/// Asset writes require an explicit .local directory and reject symlink/junction
/// ancestors. Generated source data cannot accidentally enter a normal source path.
fn local_output(path: &Path) -> Result<()> {
    if path.components().any(|p| matches!(p, Component::ParentDir))
        || !path.components().any(|p| p.as_os_str() == ".local")
    {
        return Err("Extracted assets must use an explicit .local directory without '..'".into());
    }
    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if parent.as_os_str().is_empty() {
            break;
        }
        if let Ok(metadata) = fs::symlink_metadata(parent) {
            if metadata.file_type().is_symlink() {
                return Err("Asset output has a symlink ancestor".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Asset output has a reparse-point ancestor".into());
                }
            }
        }
        ancestor = parent.parent();
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn json(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(value)?)
}
fn find_resource(id: &str) -> Result<catalog::Resource> {
    catalog::resources()?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Unknown resource ID {id}").into())
}
fn materialize(rom: &VerifiedRom<'_>, item: &catalog::Resource) -> Result<Vec<u8>> {
    let bytes = rom.range(&item.source, item.offset, item.length)?;
    Ok(if item.format == "lz77" {
        gba::decode_lz77(bytes, 4 * 1024 * 1024)?.bytes
    } else {
        bytes.to_vec()
    })
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("catalog") if (2..=3).contains(&args.len()) => {
            let bytes = read_limited(Path::new(&args[1]), sonic_advance_2_assets::ROM_LEN)?;
            let report = json(&catalog::manifest(&VerifiedRom::parse(&bytes)?)?)?;
            if let Some(path) = args.get(2) { write_new(Path::new(path), &report)?; } else { println!("{}", String::from_utf8(report)?); }
        }
        Some("extract") if args.len() == 4 => {
            let bytes = read_limited(Path::new(&args[1]), sonic_advance_2_assets::ROM_LEN)?;
            let rom = VerifiedRom::parse(&bytes)?;
            let item = find_resource(&args[2])?;
            let directory = Path::new(&args[3]);
            let path = directory.join(format!("{}.raw.bin", item.id));
            local_output(&path)?;
            let decoded = catalog::decode_resource(&rom, &item)?;
            write_new(&path, rom.range(&item.source, item.offset, item.length)?)?;
            if let Some((decoded, _, _)) = decoded {
                write_new(&directory.join(format!("{}.decoded.bin", item.id)), &decoded)?;
            }
            println!("Extracted {} locally", item.id);
        }
        Some("render-map") if args.len() == 8 => {
            let bytes = read_limited(Path::new(&args[1]), sonic_advance_2_assets::ROM_LEN)?;
            let rom = VerifiedRom::parse(&bytes)?;
            let map = materialize(&rom, &find_resource(&args[2])?)?;
            let tiles = materialize(&rom, &find_resource(&args[3])?)?;
            let palette = materialize(&rom, &find_resource(&args[4])?)?;
            let width: usize = args[5].parse()?;
            let height: usize = args[6].parse()?;
            let entries = graphics::parse_tilemap(&map, width, height, 65536)?;
            let tiles = graphics::decode_4bpp_tiles(&tiles, 16384)?;
            let palette = graphics::parse_palette(&palette, 256)?;
            let output = graphics::render_text_map(&tiles, &entries, &palette, graphics::RenderOptions {
                width_tiles: width, height_tiles: height, depth: graphics::TileDepth::FourBpp,
                transparent_zero: true, max_pixels: graphics::MAX_PIXELS,
            })?;
            let mut pam = format!("P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", output.width, output.height).into_bytes();
            pam.extend_from_slice(&output.rgba);
            local_output(Path::new(&args[7]))?;
            write_new(Path::new(&args[7]), &pam)?;
            println!("Rendered {}x{} RGBA map locally", output.width, output.height);
        }
        Some("campaign-save") if args.len() == 2 => {
            let bytes = read_limited(Path::new(&args[1]), 16 * native_campaign_save::SECTOR_BYTES)?;
            let inspection = native_campaign_save::inspect_flash(&bytes)?;
            let mut session = native_campaign_save::CampaignSaveSession::new(&bytes, true)?;
            let recovery = session.load_latest(&native_campaign_save::MemoryFlashFaults::default());
            let profile = recovery.success.then_some(&session.loaded);
            println!("{}", String::from_utf8(json(&serde_json::json!({
                "inspection": inspection, "inMemoryRecovery": recovery, "profile": profile
            }))?)?);
        }
        Some("save") if args.len() == 2 => {
            let bytes = read_limited(Path::new(&args[1]), 128 * 1024)?;
            let inspection = native_save::inspect_flash(&bytes)?;
            println!("{}", String::from_utf8(json(&serde_json::json!({
                "sectors": inspection.sectors,
                "selectedPhysicalSector": inspection.selected_physical_sector,
                "trailingBytesAfterFirstBank": inspection.trailing_bytes_after_first_bank,
                "selected": inspection.selected.as_ref().map(save_layout::inspect_record),
            }))?)?);
        }
        Some("transfer") if args.len() == 3 => {
            let command = u32::from_str_radix(args[1].trim_start_matches("0x"), 16)?;
            let command = transfer::TransferCommand::parse(command)?;
            let bytes = read_limited(Path::new(&args[2]), transfer::MAX_TRANSFER_BYTES)?;
            let report = transfer::TransferPayload::parse(command, &bytes)?.report();
            println!("{}", String::from_utf8(json(&report)?)?);
        }
        _ => return Err("Usage: a2ne-assets catalog ROM [METADATA.json] | extract ROM ID .local/OUTPUT_DIR | render-map ROM MAP_ID TILES_ID PALETTE_ID WIDTH_TILES HEIGHT_TILES .local/OUTPUT.pam | save FLASH.sav | campaign-save FLASH.sav | transfer HEX_COMMAND PAYLOAD_FILE".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("a2ne-assets: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_require_local_directory_and_no_traversal() {
        assert!(local_output(Path::new("output.bin")).is_err());
        assert!(local_output(Path::new(".local/../output.bin")).is_err());
    }
    #[test]
    fn missing_arguments_are_reported() {
        assert!(run(&[]).is_err());
    }
}
