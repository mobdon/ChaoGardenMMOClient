//! Offline stdin/button host. Never opens a listener or writes its input flash.
use sonic_advance_2_assets::{
    ROM_LEN, VerifiedRom, native_garden_session::GardenSession, native_shop_state::NativeShopItem,
};
use std::{
    error::Error,
    fs::File,
    io::{self, BufRead, Read, Write},
    path::Path,
};
type R<T> = Result<T, Box<dyn Error>>;
fn read(path: &str, max: usize) -> R<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err("Oversized input".into());
    }
    Ok(bytes)
}
fn run() -> R<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if !(1..=3).contains(&a.len()) {
        return Err("Usage: a2ne-garden ROM [FLASH.sav|-] [SEED]".into());
    }
    let bytes = read(&a[0], ROM_LEN)?;
    let rom = VerifiedRom::parse(&bytes)?;
    let flash = match a.get(1).map(String::as_str) {
        Some(s) if s != "-" => read(s, 128 * 1024)?,
        _ => vec![255; 65536],
    };
    let seed = a.get(2).map(|s| s.parse::<u32>()).transpose()?.unwrap_or(0);
    let mut game = GardenSession::load(&rom, &flash, seed)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    writeln!(out, "{}", game.view())?;
    out.flush()?;
    let mut input = io::stdin().lock();
    loop {
        let mut line = String::new();
        if input.by_ref().take(4097).read_line(&mut line)? == 0 {
            break;
        }
        if line.len() > 4096 {
            return Err("Command exceeds 4096 bytes".into());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" {
            break;
        }
        let result: R<()> = (|| {
            if let Some(path) = line.strip_prefix("save ") {
                if !game.at_garden() {
                    return Err("Return to the Garden before saving".into());
                }
                let path = Path::new(path.trim());
                export::local_output(path)?;
                game.export_flash(|data| {
                    let mut f = File::create_new(path)?;
                    let result = f.write_all(data).and_then(|()| f.sync_all());
                    if result.is_err() {
                        drop(f);
                        let _ = std::fs::remove_file(path);
                    }
                    result
                })?;
                writeln!(out, "{}", game.view())?;
                return Ok(());
            }
            if line == "frame" || line.starts_with("frame ") {
                let frame = game.picture()?;
                if let Some(path) = line.strip_prefix("frame ") {
                    let path = Path::new(path.trim());
                    export::local_output(path)?;
                    let mut f = File::create_new(path)?;
                    writeln!(
                        f,
                        "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR",
                        frame.width, frame.height
                    )?;
                    f.write_all(&frame.rgba)?;
                    writeln!(out, "{}", game.view())?;
                } else {
                    writeln!(out, "P6\n{} {}\n255", frame.width, frame.height)?;
                    for pixel in frame.rgba.as_chunks::<4>().0 {
                        out.write_all(&pixel[..3])?;
                    }
                }
                return Ok(());
            }
            let words: Vec<_> = line.split_ascii_whitespace().collect();
            match words.as_slice() {
                ["status"] => {}
                [name @ ("memory" | "bounce")] => game.start(name)?,
                ["hatch"] => game.hatch()?,
                ["place", x, y] => game.place(x.parse()?, y.parse()?)?,
                ["feed", slot] => game.feed(slot.parse()?)?,
                ["pull", slot] => game.pull(slot.parse()?)?,
                ["shop"] => {
                    writeln!(out, "{}", game.stock()?)?;
                    return Ok(());
                }
                ["buy", kind, id] => {
                    let id = id.parse()?;
                    let item = match *kind {
                        "fruit" => NativeShopItem::Fruit(id),
                        "egg" => NativeShopItem::Egg(id),
                        "toy" => NativeShopItem::Toy(id),
                        _ => return Err("Unknown item class".into()),
                    };
                    game.buy(item)?;
                }
                ["pad", mask] | ["pad", mask, _] => {
                    let held = mask.parse()?;
                    let frames = words
                        .get(2)
                        .map(|s| s.parse::<u32>())
                        .transpose()?
                        .unwrap_or(1);
                    if !(1..=36000).contains(&frames) {
                        return Err("Frame count must be 1..36000".into());
                    }
                    let started_home = game.at_garden();
                    for _ in 0..frames {
                        game.tick(held)?;
                        if !started_home && game.at_garden() {
                            break;
                        }
                    }
                }
                _ => {
                    return Err(concat!(
                        "Commands: status, hatch, memory, bounce, pad MASK [FRAMES], shop, ",
                        "buy fruit|egg|toy ID, place X Y, feed SLOT, pull SLOT, ",
                        "frame [LOCAL_PATH], save LOCAL_PATH, quit"
                    )
                    .into());
                }
            }
            writeln!(out, "{}", game.view())?;
            Ok(())
        })();
        if let Err(e) = result {
            writeln!(out, "{}", serde_json::json!({"error":e.to_string()}))?;
        }
        out.flush()?;
    }
    Ok(())
}
#[path = "../local_asset_output.rs"]
mod export;
fn main() {
    if let Err(e) = run() {
        eprintln!("a2ne-garden: {e}");
        std::process::exit(1);
    }
}
