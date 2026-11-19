//! Hash-only reference for the browser's reset framebuffer.
use sonic_advance_assets::{
    binary,
    runtime::{NativeRun, HEIGHT, WIDTH},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: spawn_hash LOCAL_ROM")?;
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN)?;
    let run = NativeRun::load(&rom)?;
    let mut frame = vec![0; WIDTH * HEIGHT];
    run.render(&mut frame)?;
    let rgba: Vec<_> = frame
        .iter()
        .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8, 255])
        .collect();
    println!("{}", binary::sha256(&rgba));
    Ok(())
}
