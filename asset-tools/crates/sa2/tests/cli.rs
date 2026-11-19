//! End-to-end checks use synthetic resources only. Generated fixtures and
//! converter outputs stay beneath this crate's ignored .local directory.
use sa2_asset_tools::manifest;
use serde_json::{json, Value};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct Case {
    path: PathBuf,
}
impl Case {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local")
            .join(format!(
                "cli-test-{}-{}",
                std::process::id(),
                NEXT_CASE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sa2-assets"));
        command.current_dir(env!("CARGO_MANIFEST_DIR"));
        command
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let allowed = Path::new(env!("CARGO_MANIFEST_DIR")).join(".local");
        // This fixture owns a unique direct child, never a caller-chosen path.
        if self.path.parent() == Some(allowed.as_path()) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn succeeded(output: Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn json_file(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".manifest.json");
    PathBuf::from(name)
}
fn adx() -> Vec<u8> {
    let mut b = vec![0; 50];
    b[..4].copy_from_slice(&[0x80, 0, 0, 28]);
    b[4..8].copy_from_slice(&[3, 18, 4, 1]);
    b[8..12].copy_from_slice(&32000u32.to_be_bytes());
    b[12..16].copy_from_slice(&3u32.to_be_bytes());
    b[16..18].copy_from_slice(&500u16.to_be_bytes());
    b[18..20].copy_from_slice(&0x0300u16.to_be_bytes());
    b[26..32].copy_from_slice(b"(c)CRI");
    b[34..36].copy_from_slice(&[0x12, 0x30]);
    b
}
fn ci4_gvr() -> Vec<u8> {
    let mut b = vec![0x01; 48];
    b[..4].copy_from_slice(b"GVRT");
    b[4..8].copy_from_slice(&40u32.to_le_bytes());
    b[8..11].fill(0);
    b[11] = 8;
    b[12..14].copy_from_slice(&8u16.to_be_bytes());
    b[14..16].copy_from_slice(&8u16.to_be_bytes());
    b
}
fn palette() -> Vec<u8> {
    let mut b = vec![0; 16 + 32 * 4];
    b[..4].copy_from_slice(b"GVPL");
    b[4..8].copy_from_slice(&136u32.to_le_bytes());
    b[8..10].copy_from_slice(&6u16.to_be_bytes());
    b[14..16].copy_from_slice(&32u16.to_be_bytes());
    for i in 0..32 {
        let at = 16 + i * 4;
        b[at..at + 4].copy_from_slice(&[((i % 16) * 17) as u8, i as u8, 30, 99]);
    }
    b
}

#[test]
fn adx_conversion_preserves_source_hash_and_refuses_overwrite() {
    let case = Case::new();
    let original = adx();
    let input = case.file("source.adx", &original);
    let output = case.path.join("converted.wav");
    succeeded(
        case.command()
            .arg("adx")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap(),
    );
    let generated = fs::read(&output).unwrap();
    assert_eq!(&generated[44..], &[1, 0, 3, 0, 7, 0]);
    let manifest = json_file(sidecar(&output));
    assert_eq!(
        manifest["sources"][0]["sha256"],
        manifest::sha256(&original)
    );
    assert_eq!(
        manifest["assets"][0]["output"]["sha256"],
        manifest::sha256(&generated)
    );
    assert_eq!(manifest["assets"][0]["fidelity"], "unknown");
    let retry = case
        .command()
        .arg("adx")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!retry.status.success());
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(fs::read(&output).unwrap(), generated);
}

#[test]
fn rejects_unknown_or_extra_arguments_before_creating_outputs() {
    let case = Case::new();
    let input = case.file("source.adx", &adx());
    for command in [
        "catalog",
        "inspect",
        "prs",
        "extract",
        "texture",
        "adx",
        "transporter",
        "validate",
        "not-a-command",
    ] {
        let output = case.path.join(format!("{command}.out"));
        let result = case
            .command()
            .arg(command)
            .arg(&input)
            .arg(&output)
            .arg("--unexpected")
            .output()
            .unwrap();
        assert!(!result.status.success(), "{command}");
        assert!(!output.exists(), "{command}");
        assert!(!sidecar(&output).exists(), "{command}");
    }
}

#[test]
fn rejects_output_outside_the_nearest_checkouts_local_directory() {
    let case = Case::new();
    let input = case.file("source.adx", &adx());
    // This fixture works both in a normal checkout and in a worktree nested
    // beneath a different checkout's .local directory.
    let tracked_target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("cli-refused-{}.wav", std::process::id()));
    let result = case
        .command()
        .arg("adx")
        .arg(input)
        .arg(&tracked_target)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!tracked_target.exists());
}

#[cfg(windows)]
#[test]
fn rejects_windows_junction_output_ancestors() {
    let case = Case::new();
    let input = case.file("source.adx", &adx());
    let target = case.path.join("junction-target");
    fs::create_dir(&target).unwrap();
    let junction = case.path.join("junction");
    succeeded(
        Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap(),
    );
    let result = case
        .command()
        .arg("adx")
        .arg(input)
        .arg(junction.join("refused.wav"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!target.join("refused.wav").exists());
}

#[test]
fn source_root_matches_catalog_ids_and_refuses_escaped_sources() {
    let case = Case::new();
    let root = case.path.join("install");
    fs::create_dir_all(root.join("ADX")).unwrap();
    let input = root.join("ADX/source.adx");
    fs::write(&input, adx()).unwrap();
    let output = case.path.join("converted.wav");
    succeeded(
        case.command()
            .arg("--source-root")
            .arg(&root)
            .arg("adx")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap(),
    );
    let catalog = case.path.join("catalog.json");
    succeeded(
        case.command()
            .arg("catalog")
            .arg(&root)
            .arg(&catalog)
            .output()
            .unwrap(),
    );
    let converted = json_file(sidecar(&output));
    let catalog = json_file(catalog);
    assert_eq!(converted["sources"][0]["id"], catalog["sources"][0]["id"]);
    assert_eq!(converted["sources"][0]["logicalPath"], "ADX/source.adx");
    let escaped = case.file("outside.adx", &adx());
    let refused = case.path.join("escaped.wav");
    let result = case
        .command()
        .arg("--source-root")
        .arg(root)
        .arg("adx")
        .arg(escaped)
        .arg(&refused)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!refused.exists());
}

#[test]
fn palette_dependency_records_exact_bank_bytes_and_preserves_alpha() {
    let case = Case::new();
    let gvr = case.file("texture.gvr", &ci4_gvr());
    let palette_bytes = palette();
    let gvp = case.file("palette.gvp", &palette_bytes);
    let output = case.path.join("texture.png");
    succeeded(
        case.command()
            .arg("texture")
            .arg(&gvr)
            .arg(&output)
            .arg(&gvp)
            .arg("1")
            .output()
            .unwrap(),
    );
    let manifest = json_file(sidecar(&output));
    let dependency = &manifest["assets"][0]["dependencies"][0];
    assert_eq!(dependency["sourceId"], manifest["sources"][1]["id"]);
    assert_eq!(dependency["role"], "external-palette");
    assert_eq!(dependency["bank"], 1);
    assert_eq!(dependency["entryCount"], 16);
    assert_eq!(dependency["offset"], 80);
    assert_eq!(dependency["byteLength"], 64);
    assert_eq!(
        dependency["sha256"],
        manifest::sha256(&palette_bytes[80..144])
    );
    let mut reader = png::Decoder::new(Cursor::new(fs::read(&output).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (8, 8));
    assert_eq!(&pixels[..8], &[16, 30, 99, 0, 17, 30, 99, 17]);
}

#[test]
fn rejects_palette_arguments_that_would_not_be_used() {
    let case = Case::new();
    let gvp = case.file("palette.gvp", &palette());
    let mut nonindexed = ci4_gvr();
    nonindexed[11] = 0;
    let input = case.file("intensity.gvr", &nonindexed);
    let output = case.path.join("unused.png");
    let result = case
        .command()
        .arg("texture")
        .arg(input)
        .arg(output)
        .arg(gvp)
        .arg("0")
        .output()
        .unwrap();
    assert!(!result.status.success());
}

#[test]
fn manifest_validation_rejects_contradictory_hashes_and_opaque_parents() {
    let case = Case::new();
    let input = case.file("source.adx", &adx());
    let output = case.path.join("converted.wav");
    succeeded(
        case.command()
            .arg("adx")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap(),
    );
    let original = json_file(sidecar(&output));
    let original_manifest = case.file("valid.json", &serde_json::to_vec(&original).unwrap());
    let report = case.path.join("valid-report.json");
    succeeded(
        case.command()
            .arg("validate")
            .arg(&original_manifest)
            .arg(&report)
            .output()
            .unwrap(),
    );
    let report = json_file(report);
    assert_eq!(report["structurallyValid"], true);
    assert_eq!(report["contentVerified"], false);
    let mut wrong_hash = original.clone();
    wrong_hash["assets"][0]["span"]["sha256"] = json!("0".repeat(64));
    let mut opaque_parent = original.clone();
    opaque_parent["assets"][0]["representation"] = json!("opaque");
    opaque_parent["assets"][0]["output"]["format"] = json!("prs-decoded");
    let mut child = opaque_parent["assets"][0].clone();
    child["id"] = json!("sa2.pc.asset.child");
    child["span"]["parentId"] = opaque_parent["assets"][0]["id"].clone();
    child["span"]["addressSpace"] = json!("decoded-container");
    child["span"]["offset"] = json!(0);
    child["span"]["byteLength"] = json!(1);
    child["output"] = Value::Null;
    opaque_parent["assets"].as_array_mut().unwrap().push(child);
    for (name, value) in [("wrong-hash", wrong_hash), ("opaque-parent", opaque_parent)] {
        let input = case.file(
            &format!("{name}.json"),
            &serde_json::to_vec(&value).unwrap(),
        );
        let output = case.path.join(format!("{name}-report.json"));
        let result = case
            .command()
            .arg("validate")
            .arg(input)
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success(), "{name}");
        assert!(!output.exists(), "{name}");
    }
}
