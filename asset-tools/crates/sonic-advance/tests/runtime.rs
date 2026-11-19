use sonic_advance_assets::{binary, runtime};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PIXELS: &str = "0e987956075c9482f117bd87d58d4705879e1f70ac18deeda24f9b3f791eae0f";
const TRACE: &str = "ab9fc4e27419236a7ffd9d1f7fd93ad63946a7d951695c3f151f5fb40b542506";

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "asoe-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        fs::create_dir(base.join(".local")).unwrap();
        Self(base)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn replay(rom: &Path, directory: &Path, frames: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sonic-advance-native"))
        .arg("replay")
        .arg(rom)
        .arg(directory)
        .arg(frames)
        .output()
        .unwrap()
}

#[test]
fn invalid_source_paths_and_budgets_create_no_outputs() {
    let scratch = Scratch::new("negative");
    let local = scratch.0.join(".local");
    let bogus = scratch.0.join("bogus");
    fs::write(&bogus, b"ASOE").unwrap();
    for (dir, frames, message) in [
        (&local, "0", "frame budget"),
        (&local, "36001", "frame budget"),
        (&local, "-1", "invalid replay frame"),
        (&scratch.0, "1", "private directory"),
        (&local, "1", "expected 8 MiB ASOE ROM"),
    ] {
        let result = replay(&bogus, dir, frames);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains(message));
    }
    assert_eq!(fs::read_dir(local).unwrap().count(), 0);
    assert!(runtime::frame_sha256(&[0]).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM"]
fn reference_replay_receipt_pixels_trace_and_collision_rollback() {
    let source = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let source = Path::new(&source);
    let scratch = Scratch::new("replay");
    let local = scratch.0.join(".local");
    let stem = format!("asoe-native-600-{}", &PIXELS[..16]);
    let file = |ext| local.join(format!("{stem}.{ext}"));
    fs::write(file("receipt.json"), b"keep existing").unwrap();
    assert!(!replay(source, &local, "600").status.success());
    assert_eq!(fs::read(file("receipt.json")).unwrap(), b"keep existing");
    assert_eq!(fs::read_dir(&local).unwrap().count(), 1);
    fs::remove_file(file("receipt.json")).unwrap();
    let result = replay(source, &local, "600");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        receipt,
        serde_json::from_slice::<serde_json::Value>(&fs::read(file("receipt.json")).unwrap())
            .unwrap()
    );
    assert_eq!(receipt["sourceSha256"], binary::ROM_SHA256);
    assert_eq!(receipt["frames"], 600);
    assert_eq!(receipt["initial"]["xQ8"], 104 * 256);
    assert_eq!(receipt["initial"]["yQ8"], 444 * 256);
    assert_eq!(receipt["observations"]["jumps"], 3);
    assert_eq!(receipt["observations"]["landings"], 3);
    assert_eq!(receipt["finalState"]["localRings"], 14);
    let trace = fs::read(file("trace.json")).unwrap();
    assert_eq!(binary::sha256(&trace), TRACE);
    assert_eq!(receipt["traceSha256"], TRACE);
    let frames: Vec<serde_json::Value> = serde_json::from_slice(&trace).unwrap();
    assert_eq!(frames.len(), 600);
    assert_eq!(frames.last().unwrap(), &receipt["finalState"]);
    let ppm = fs::read(file("ppm")).unwrap();
    let rgb = ppm.strip_prefix(b"P6\n240 160\n255\n").unwrap();
    assert_eq!(rgb.len(), runtime::WIDTH * runtime::HEIGHT * 3);
    assert_eq!(binary::sha256(rgb), PIXELS);
    assert_eq!(receipt["finalFrameRgbSha256"], PIXELS);
}

fn reference_fields(s: &runtime::Snapshot) -> [i32; 8] {
    [
        s.x_q8,
        s.y_q8,
        s.vx_q8,
        s.vy_q8,
        s.camera_x,
        s.camera_y,
        s.collision_layer.try_into().unwrap(),
        s.local_rings.try_into().unwrap(),
    ]
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM"]
fn warmed_runtime_matches_600_retail_mgba_frames() {
    let rom = fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let mut run = runtime::NativeRun::load(&rom).unwrap();
    for _ in 0..25 {
        run.step(Default::default()).unwrap();
    }
    assert_eq!(
        reference_fields(&run.snapshot()),
        [26624, 113920, 0, 0, 0, 375, 1, 0]
    );
    // validation/retail_trace.py hashes retail frames 1800..=2399 in this order.
    let mut trace = Vec::with_capacity(600 * 8 * 4);
    for frame in 0..600 {
        run.step(runtime::exercise_input(frame)).unwrap();
        for field in reference_fields(&run.snapshot()) {
            trace.extend_from_slice(&field.to_le_bytes());
        }
    }
    assert_eq!(
        reference_fields(&run.snapshot()),
        [489457, 120154, 1249, 0, 1779, 399, 1, 14]
    );
    assert_eq!(
        binary::sha256(&trace),
        "f3191a348b928b9fcf28f22314a091a750675941f28b35707dbb5e42338f8f34"
    );
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM"]
fn twelve_ordinary_acts_load_step_and_render() {
    let rom = fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let mut pictures = std::collections::BTreeSet::new();
    for index in 0..12 {
        let mut run = runtime::NativeRun::load_stage(&rom, index).unwrap();
        assert_eq!(run.stage_evidence().stage_index, index);
        run.step(Default::default()).unwrap();
        let snapshot = run.snapshot();
        assert_eq!((snapshot.stage_index, snapshot.frame), (index, 1));
        let mut pixels = vec![0; runtime::WIDTH * runtime::HEIGHT];
        run.render(&mut pixels).unwrap();
        assert!(
            pixels.iter().any(|&color| color != pixels[0]),
            "empty stage {index}"
        );
        assert!(
            pictures.insert(runtime::frame_sha256(&pixels).unwrap()),
            "duplicate stage {index}"
        );
    }
    let error = runtime::NativeRun::load_stage(&rom, 12)
        .err()
        .expect("slot 12 is not an ordinary act");
    assert!(error.to_string().contains("0 through 11"));
}
