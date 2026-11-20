use sonic_advance_2_assets::{VerifiedRom, graphics::RenderedMap, native_garden_home_art};
use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let bytes = std::fs::read(&args[1])?;
    let rom = VerifiedRom::parse(&bytes)?;
    let mut input = std::fs::File::open(&args[2])?;
    let mut output = std::fs::File::create(&args[3])?;
    let mut record = vec![0; 0x858];
    loop {
        match input.read_exact(&mut record) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }
        let mut frame = RenderedMap {
            width: 240,
            height: 160,
            rgba: vec![0; 240 * 160 * 4],
        };
        let report = native_garden_home_art::paint(&rom, &record, &mut frame)?;
        let json = serde_json::to_vec(&report)?;
        output.write_all(&(json.len() as u32).to_le_bytes())?;
        output.write_all(&json)?;
        output.write_all(&frame.rgba)?;
    }
    Ok(())
}
