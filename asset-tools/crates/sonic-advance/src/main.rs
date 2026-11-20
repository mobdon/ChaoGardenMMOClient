use sonic_advance_assets::{binary, catalog, flash, Error, Result};
use std::io::Write;
use std::path::Path;

struct PartialFile(std::path::PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_new_blob(output: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = output.with_extension(format!("partial-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| Error(e.to_string()))?;
    let _cleanup = PartialFile(temporary.clone());
    file.write_all(bytes).map_err(|e| Error(e.to_string()))?;
    file.sync_all().map_err(|e| Error(e.to_string()))?;
    drop(file);
    // Publish a complete file without replacing an existing path. A hard link
    // fails closed on filesystems that cannot provide this guarantee.
    std::fs::hard_link(&temporary, output).map_err(|e| Error(e.to_string()))?;
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if matches!(args.len(), 2 | 3) && args[0] == "inspect-save" {
        if let Some(rom_path) = args.get(2) {
            let rom = binary::read_bounded(Path::new(rom_path), binary::ROM_LEN)?;
            binary::verify_asoe(&rom)?;
        }
        let bytes = binary::read_bounded(Path::new(&args[1]), flash::FLASH_BYTES)?;
        print_json(&flash::inspect(&bytes)?)?;
    } else if args.len() == 2 && args[0] == "inspect" {
        let rom = binary::read_bounded(Path::new(&args[1]), binary::ROM_LEN)?;
        let report = catalog::verify(&rom)?;
        print_json(&report)?;
    } else if args.len() == 3 && args[0] == "extract-garden" {
        let rom = binary::read_bounded(Path::new(&args[1]), binary::ROM_LEN)?;
        // Require an explicit existing private .local directory. Derive the
        // filename from the fingerprint; never use ROM/archive-provided paths.
        let directory = std::fs::canonicalize(&args[2]).map_err(|e| Error(e.to_string()))?;
        if !directory.is_dir() || directory.file_name() != Some(std::ffi::OsStr::new(".local")) {
            return Err(Error(
                "output must be an existing directory named .local".into(),
            ));
        }
        let decoded = catalog::garden(&rom)?;
        let output = directory.join(format!(
            "sonic-advance-asoe-garden-{}.bin",
            catalog::GARDEN_SHA256
        ));
        write_new_blob(&output, &decoded.bytes)?;
        println!("{}", output.display());
    } else {
        return Err(Error("usage: sonic-advance-assets inspect ROM | inspect-save RAW_SAVE [ASOE_ROM] | extract-garden ROM EXISTING_LOCAL_DIR".into()));
    }
    Ok(())
}

fn print_json(report: &impl serde::Serialize) -> Result<()> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, report).map_err(|e| Error(e.to_string()))?;
    writeln!(output).map_err(|e| Error(e.to_string()))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
