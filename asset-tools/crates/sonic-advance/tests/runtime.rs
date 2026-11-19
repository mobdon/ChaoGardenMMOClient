use sonic_advance_assets::{binary, runtime};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PIXELS: &str = "7ca7b26477218ef12ee0984ed877a1d28aeb29e32d6ff263c56f245a97dcd1b8";
const TRACE: &str = "29cd8b446f1d464ba39847e14b737d8674e04343163e74e5ff5c4c55df46e898";

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
    assert_eq!(receipt["observations"]["landings"], 4);
    assert_eq!(receipt["finalState"]["localRings"], 1);
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
