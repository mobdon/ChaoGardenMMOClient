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

fn replay(rom: &[u8], output: &Path, frames: u32, stage: usize) -> Result<()> {
    if frames == 0 || frames > runtime::MAX_REPLAY_FRAMES {
        return Err(Error("replay frame budget must be 1 through 36000".into()));
    }
    let directory = local_directory(output)?;
    let mut run = runtime::NativeRun::load_stage(rom, stage)?;
    let initial = run.snapshot();
    let mut trace = Vec::new();
    let mut observations = Observations::default();
    for frame in 0..frames {
        let outcome = run.step(runtime::exercise_input(frame))?;
        observations.jumps += u32::from(outcome.jumped);
        observations.landings += u32::from(outcome.landed);
        observations.horizontal_blocks += u32::from(outcome.blocked_x);
        let snapshot = run.snapshot();
        let stopped = snapshot.death.is_some() || snapshot.goal.is_some_and(|g| g.results_ready);
        trace.push(snapshot);
        if stopped {
            break;
        }
    }
    let mut image = vec![0; runtime::WIDTH * runtime::HEIGHT];
    run.render(&mut image)?;
    let digest = runtime::frame_sha256(&image)?;
    let trace = serde_json::to_vec(&trace).map_err(|e| Error(e.to_string()))?;
    let receipt = Receipt {
        source_sha256: binary::ROM_SHA256,
        status: "local-native-stage-practice",
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
    let stem = if stage == 0 {
        format!("asoe-native-{frames}-{}", &digest[..16])
    } else {
        format!("asoe-native-stage{stage}-{frames}-{}", &digest[..16])
    };
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
fn play(rom: &[u8], demo_frames: Option<u32>, stage: usize) -> Result<()> {
    use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
    let mut run = runtime::NativeRun::load_stage(rom, stage)?;
    let mut window = Window::new(
        "Sonic Advance | Rust stage practice",
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
            run = runtime::NativeRun::load_stage(rom, run.snapshot().stage_index)?;
            status.clear();
            paused = false;
        }
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::Enter, KeyRepeat::No) && !status.is_empty() {
            match run.continue_stage() {
                Ok(()) => {
                    status.clear();
                    paused = false;
                }
                Err(error) => status = error.to_string(),
            }
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
            let down = demo_frames.is_none()
                && (window.is_key_down(Key::Down) || window.is_key_down(Key::S));
            match run.step_controls(input, down) {
                Ok(_) => {
                    let s = run.snapshot();
                    if s.death.is_some() {
                        status = if s.lives == 0 {
                            "Game over | R restarts"
                        } else {
                            "Life lost | Enter to retry"
                        }
                        .into();
                    } else if s.goal.is_some_and(|g| g.results_ready) {
                        status = if s.stage_index + 1 < runtime::PRACTICE_ACTS {
                            "Goal reached | Enter for the next act"
                        } else {
                            "Goal reached | End of supported acts"
                        }
                        .into();
                    }
                }
                Err(error) => status = format!("Unsupported path: {error}; R restarts"),
            }
        }
        run.render(&mut pixels)?;
        let snapshot = run.snapshot();
        window.set_title(&format!("Sonic Advance | Zone {} Act {} | x={} y={} rings={} lives={} | {} | Arrows/AD Space/Z R P Esc",
            snapshot.stage_index / 2 + 1, snapshot.stage_index % 2 + 1, snapshot.x_q8 >> 8, snapshot.y_q8 >> 8, snapshot.local_rings, snapshot.lives,
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
fn play(_: &[u8], _: Option<u32>, _: usize) -> Result<()> {
    Err(Error(
        "build with --features native-window to open the Rust window".into(),
    ))
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err(Error("usage: sonic-advance-native play ROM [STAGE] | demo ROM FRAMES [STAGE] | replay ROM EXISTING_LOCAL_DIR FRAMES [STAGE]".into()));
    }
    let stage_arg = |at: usize| -> Result<usize> {
        match args.get(at) {
            None => Ok(0),
            Some(arg) => arg
                .to_str()
                .and_then(|s| s.parse().ok())
                .filter(|&n| n < runtime::PRACTICE_ACTS)
                .ok_or_else(|| Error("practice stage index must be 0 through 11".into())),
        }
    };
    let rom = binary::read_bounded(Path::new(&args[1]), binary::ROM_LEN)?;
    if args[0] == "play" && (2..=3).contains(&args.len()) {
        play(&rom, None, stage_arg(2)?)
    } else if args[0] == "demo" && (3..=4).contains(&args.len()) {
        let frames: u32 = args[2]
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error("invalid demo frame count".into()))?;
        if frames == 0 || frames > runtime::MAX_REPLAY_FRAMES {
            return Err(Error("demo frame budget must be 1 through 36000".into()));
        }
        play(&rom, Some(frames), stage_arg(3)?)
    } else if args[0] == "replay" && (4..=5).contains(&args.len()) {
        let frames = args[3]
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error("invalid replay frame count".into()))?;
        replay(&rom, Path::new(&args[2]), frames, stage_arg(4)?)
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
