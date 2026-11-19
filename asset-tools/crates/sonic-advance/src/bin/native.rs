use sonic_advance_assets::{binary, runtime, Error, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

fn local_directory(path: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path).map_err(|e| Error(e.to_string()))?;
    if !path.is_dir() || path.file_name() != Some(std::ffi::OsStr::new(".local")) {
        return Err(Error(
            "output must be an existing private directory named .local".into(),
        ));
    }
    Ok(path)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    source_sha256: &'static str,
    status: &'static str,
    frames: u32,
    initial: runtime::Snapshot,
    final_state: runtime::Snapshot,
    final_frame_rgb_sha256: String,
    stage: &'a sonic_advance_assets::stage::StageEvidence,
    ring_decoded_sha256: &'a str,
    trace_sha256: String,
    observations: Observations,
}

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Observations {
    jumps: u32,
    landings: u32,
    horizontal_blocks: u32,
}

struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn publish(directory: &Path, stem: &str, files: &[(&str, &[u8])]) -> Result<()> {
    let mut published = Cleanup(Vec::new());
    for &(suffix, bytes) in files {
        let path = directory.join(format!("{stem}.{suffix}"));
        let temp = directory.join(format!("{stem}.{suffix}.partial-{}", std::process::id()));
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| Error(e.to_string()))?;
        let _temporary = Cleanup(vec![temp.clone()]);
        let written = out.write_all(bytes).and_then(|_| out.sync_all());
        drop(out);
        written.map_err(|e| Error(e.to_string()))?;
        std::fs::hard_link(&temp, &path).map_err(|e| Error(e.to_string()))?;
        published.0.push(path);
    }
    published.0.clear();
    Ok(())
}

fn replay(rom: &[u8], output: &Path, frames: u32) -> Result<()> {
    if frames == 0 || frames > runtime::MAX_REPLAY_FRAMES {
        return Err(Error("replay frame budget must be 1 through 36000".into()));
    }
    let directory = local_directory(output)?;
    let mut run = runtime::NativeRun::load(rom)?;
    let initial = run.snapshot();
    let mut trace = Vec::new();
    let mut observations = Observations::default();
    for frame in 0..frames {
        let outcome = run.step(runtime::exercise_input(frame))?;
        observations.jumps += u32::from(outcome.jumped);
        observations.landings += u32::from(outcome.landed);
        observations.horizontal_blocks += u32::from(outcome.blocked_x);
        trace.push(run.snapshot());
        if outcome.fell_out {
            break;
        }
    }
    let mut image = vec![0; runtime::WIDTH * runtime::HEIGHT];
    run.render(&mut image)?;
    let digest = runtime::frame_sha256(&image)?;
    let trace = serde_json::to_vec(&trace).map_err(|e| Error(e.to_string()))?;
    let receipt = Receipt {
        source_sha256: binary::ROM_SHA256,
        status: "local-native-first-stage-testbed; complete-game-parity-unfinished",
        frames: run.snapshot().frame as u32,
        initial,
        final_state: run.snapshot(),
        final_frame_rgb_sha256: digest.clone(),
        stage: run.stage_evidence(),
        ring_decoded_sha256: &run.ring_layout().decoded_sha256,
        trace_sha256: binary::sha256(&trace),
        observations,
    };
    let mut ppm = format!("P6\n{} {}\n255\n", runtime::WIDTH, runtime::HEIGHT).into_bytes();
    for p in image {
        ppm.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    let stem = format!("asoe-native-{frames}-{}", &digest[..16]);
    let json = serde_json::to_vec_pretty(&receipt).map_err(|e| Error(e.to_string()))?;
    publish(
        &directory,
        &stem,
        &[
            ("ppm", &ppm),
            ("trace.json", &trace),
            ("receipt.json", &json),
        ],
    )?;
    std::io::stdout()
        .lock()
        .write_all(&json)
        .map_err(|e| Error(e.to_string()))?;
    println!();
    Ok(())
}

#[cfg(feature = "native-window")]
fn play(rom: &[u8], demo_frames: Option<u32>) -> Result<()> {
    use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
    let mut run = runtime::NativeRun::load(rom)?;
    let mut window = Window::new(
        "Sonic Advance | Rust native first-stage testbed",
        runtime::WIDTH,
        runtime::HEIGHT,
        WindowOptions {
            scale: Scale::X4,
            resize: true,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| Error(e.to_string()))?;
    window.set_target_fps(60);
    let mut pixels = vec![0; runtime::WIDTH * runtime::HEIGHT];
    let mut paused = false;
    let mut status = String::new();
    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            run = runtime::NativeRun::load(rom)?;
            status.clear();
            paused = false;
        }
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
        }
        if !paused && status.is_empty() {
            let input = if demo_frames.is_some() {
                runtime::exercise_input(run.snapshot().frame as u32)
            } else {
                sonic_advance_assets::player::Input {
                    left: window.is_key_down(Key::Left) || window.is_key_down(Key::A),
                    right: window.is_key_down(Key::Right) || window.is_key_down(Key::D),
                    jump: window.is_key_down(Key::Space) || window.is_key_down(Key::Z),
                }
            };
            match run.step(input) {
                Ok(frame) if frame.fell_out => {
                    status = "Fell out - R restarts this local run".into()
                }
                Ok(_) => {}
                Err(error) => status = format!("Unsupported path: {error}; R restarts"),
            }
        }
        run.render(&mut pixels)?;
        let snapshot = run.snapshot();
        window.set_title(&format!("Sonic Advance | Rust testbed | x={} y={} local rings={} | {} | Arrows/AD Space/Z R P Esc",
            snapshot.x_q8 >> 8, snapshot.y_q8 >> 8, snapshot.local_rings,
            if status.is_empty() { if paused { "Paused" } else { "Offline practice" } } else { &status }));
        window
            .update_with_buffer(&pixels, runtime::WIDTH, runtime::HEIGHT)
            .map_err(|e| Error(e.to_string()))?;
        if let Some(limit) = demo_frames {
            if snapshot.frame >= u64::from(limit) || !status.is_empty() {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "native-window"))]
fn play(_: &[u8], _: Option<u32>) -> Result<()> {
    Err(Error(
        "build with --features native-window to open the Rust window".into(),
    ))
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err(Error("usage: sonic-advance-native play ROM | demo ROM FRAMES | replay ROM EXISTING_LOCAL_DIR FRAMES".into()));
    }
    let rom = binary::read_bounded(Path::new(&args[1]), binary::ROM_LEN)?;
    if args[0] == "play" && args.len() == 2 {
        play(&rom, None)
    } else if args[0] == "demo" && args.len() == 3 {
        let frames: u32 = args[2]
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error("invalid demo frame count".into()))?;
        if frames == 0 || frames > runtime::MAX_REPLAY_FRAMES {
            return Err(Error("demo frame budget must be 1 through 36000".into()));
        }
        play(&rom, Some(frames))
    } else if args[0] == "replay" && args.len() == 4 {
        let frames = args[3]
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error("invalid replay frame count".into()))?;
        replay(&rom, Path::new(&args[2]), frames)
    } else {
        Err(Error(
            "invalid native runner command or argument count".into(),
        ))
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
