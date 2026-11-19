//! Hash-only native oracle for comparison with the same Rust core in WASM.
use serde::{Deserialize, Serialize};
use sonic_advance_assets::{
    binary,
    runtime::{Runtime, Snapshot},
    Error, Result,
};

#[derive(Deserialize)]
struct Segment {
    mask: u32,
    ticks: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    snapshot: Snapshot,
    rgba_sha256: String,
}

fn replay() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or_else(|| Error("usage: runtime-replay LOCAL_ASOE_ROM".into()))?;
    if args.next().is_some() {
        return Err(Error("expected one local ROM path".into()));
    }
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN)?;
    let mut runtime = Runtime::load(rom)?;
    let plan: Vec<Segment> = serde_json::from_str(include_str!("../../tools/replay-plan.json"))
        .map_err(|e| Error(e.to_string()))?;
    let mut checkpoints = vec![checkpoint(&runtime)];
    for segment in plan {
        if segment.ticks == 0 || segment.ticks > 4096 {
            return Err(Error("replay tick budget exceeded".into()));
        }
        for _ in 0..segment.ticks {
            runtime.step(segment.mask, 1)?;
        }
        checkpoints.push(checkpoint(&runtime));
    }
    println!(
        "{}",
        serde_json::to_string(&checkpoints).map_err(|e| Error(e.to_string()))?
    );
    Ok(())
}

fn checkpoint(runtime: &Runtime) -> Checkpoint {
    Checkpoint {
        snapshot: runtime.snapshot(),
        rgba_sha256: binary::sha256(runtime.pixels()),
    }
}

fn main() {
    if let Err(error) = replay() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
