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
    let mode = args.next();
    if args.next().is_some() || mode.as_ref().is_some_and(|v| v != "--title" && v != "--act2") {
        return Err(Error("expected one local ROM path".into()));
    }
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN)?;
    if mode.as_ref().is_some_and(|v| v == "--title") {
        let mut title = sonic_advance_assets::title::Title::load_asoe(&rom)?;
        let mut checkpoints = Vec::new();
        for target in [0,25,49,50,62,90,122,123,154,155,187] {
            while title.snapshot().frame < target { title.step(0,1)?; }
            let rgba=title.render()?;
            let colors: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| {
                let color = u16::from(p[0] >> 3) | (u16::from(p[1] >> 3) << 5) | (u16::from(p[2] >> 3) << 10);
                color.to_le_bytes()
            }).collect();
            checkpoints.push(serde_json::json!({"title":title.snapshot(),"rgbaSha256":binary::sha256(&rgba),"bgr555Sha256":binary::sha256(&colors)}));
        }
        println!("{}",serde_json::Value::Array(checkpoints));
        return Ok(());
    }
    let mut runtime = Runtime::load(rom)?;
    if mode.is_some() { runtime.select_act(1)?; }
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
