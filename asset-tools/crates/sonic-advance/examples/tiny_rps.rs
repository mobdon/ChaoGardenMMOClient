use sonic_advance_assets::{
    binary,
    tiny_rps::{Round, FIRE, LEFT, RIGHT},
};
use std::{
    error::Error,
    io::{self, BufRead, Write},
    path::Path,
};
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let rom = args
        .next()
        .ok_or("usage: tiny_rps ASOE_ROM [decimal seed]")?;
    let seed = args
        .next()
        .map(|v| {
            v.into_string()
                .map_err(|_| "invalid seed")?
                .parse::<u32>()
                .map_err(|_| "invalid seed")
        })
        .transpose()?
        .unwrap_or(12345);
    let mut round = Round::from_rom(
        &binary::read_bounded(Path::new(&rom), binary::ROM_LEN)?,
        seed,
    )?;
    println!("a/d select a lane, f fires, . waits a frame. Enter a count to wait multiple frames or q to quit.");
    let mut lines = io::stdin().lock().lines();
    loop {
        let (s, f) = round.time();
        println!(
            "Time {s}s + {f}/60 · shots {} · points {} · wave {}",
            round.lives(),
            round.points(),
            u16::from(round.wave()) + 1
        );
        for (i, lane) in round.lanes().iter().enumerate() {
            let label = match lane.phase {
                0 => "ready",
                1 => "flying",
                9 => "empty",
                _ => "resolving",
            };
            println!(
                "{}lane {} at x={}: {:?} {label}",
                if i == round.selected() { "> " } else { "  " },
                i + 1,
                12 + 64 * i,
                lane.hand
            );
        }
        print!("Targets:");
        for t in round.targets().iter().filter(|t| t.phase == 0) {
            if let Some((x, y)) = t.position() {
                print!(" {:?}({x},{y})", t.hand);
            }
        }
        println!();
        if let Some(end) = round.end() {
            println!("{end:?}");
            if let Some(points) = round.result_points() {
                println!("Final points {points}");
            }
            break;
        }
        print!("> ");
        io::stdout().flush()?;
        let Some(line) = lines.next().transpose()? else {
            break;
        };
        let cmd = line.trim();
        if cmd == "q" {
            break;
        }
        if let Ok(n) = cmd.parse::<u16>() {
            for _ in 0..n {
                round.step(0);
                if round.end().is_some() {
                    break;
                }
            }
        } else {
            for ch in cmd.chars() {
                let key = match ch {
                    'a' => LEFT,
                    'd' => RIGHT,
                    'f' => FIRE,
                    '.' => 0,
                    _ => continue,
                };
                round.step(key);
                if key != 0 {
                    round.step(0);
                }
            }
        }
    }
    Ok(())
}
