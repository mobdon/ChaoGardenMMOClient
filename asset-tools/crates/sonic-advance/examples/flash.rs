use sonic_advance_assets::{binary, flash};
use std::{error::Error, path::Path};
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let rom = args.next().ok_or("usage: flash ASOE_ROM RAW_64K_SAVE")?;
    let save = args.next().ok_or("expected raw flash image")?;
    if args.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    binary::verify_asoe(&binary::read_bounded(Path::new(&rom), binary::ROM_LEN)?)?;
    let bytes = binary::read_bounded(Path::new(&save), flash::FLASH_BYTES)?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &flash::inspect(&bytes)?)?;
    Ok(())
}
