//! Synthetic end-to-end evaluation/query checks. No retail fixture bytes.
use sa2_asset_tools::manifest;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local")
            .join(format!(
                "runtime-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sa2-assets"));
        c.current_dir(env!("CARGO_MANIFEST_DIR"));
        c
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let allowed = Path::new(env!("CARGO_MANIFEST_DIR")).join(".local");
        if self.0.parent() == Some(allowed.as_path()) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn succeeded(o: Output) {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn sidecar(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".manifest.json");
    PathBuf::from(p)
}
fn f32_at(b: &mut [u8], at: usize, v: f32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn u32_at(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn model() -> Vec<u8> {
    let mut b = vec![0; 68];
    b[..7].copy_from_slice(b"SA2MDL\0");
    u32_at(&mut b, 8, 16);
    for (i, v) in [3.0, 4.0, 5.0].iter().enumerate() {
        f32_at(&mut b, 24 + i * 4, *v);
    }
    for i in 0..3 {
        f32_at(&mut b, 48 + i * 4, 1.0);
    }
    b
}
fn motion() -> Vec<u8> {
    let mut b = vec![0; 72];
    b[..7].copy_from_slice(b"SAANIM\0");
    b[7] = 1;
    u32_at(&mut b, 8, 20);
    u32_at(&mut b, 16, 1);
    u32_at(&mut b, 20, 32);
    u32_at(&mut b, 24, 2);
    b[28] = 1;
    b[30] = 1;
    u32_at(&mut b, 32, 40);
    u32_at(&mut b, 36, 2);
    u32_at(&mut b, 56, 1);
    f32_at(&mut b, 64, 2.0);
    b
}
fn stage() -> Vec<u8> {
    let mut b = vec![0; 238];
    b[..7].copy_from_slice(b"SA2LVL\0");
    u32_at(&mut b, 8, 16);
    b[16] = 1;
    b[18] = 255;
    b[19] = 255;
    u32_at(&mut b, 32, 48);
    u32_at(&mut b, 64, 80);
    u32_at(&mut b, 76, 1);
    u32_at(&mut b, 84, 132);
    for (i, v) in [1.0, 2.0, 3.0].iter().enumerate() {
        f32_at(&mut b, 88 + i * 4, *v);
    }
    for i in 0..3 {
        f32_at(&mut b, 112 + i * 4, 1.0);
    }
    u32_at(&mut b, 132, 172);
    u32_at(&mut b, 140, 3);
    u32_at(&mut b, 144, 208);
    b[152] = 1;
    f32_at(&mut b, 168, 1.0);
    for (i, v) in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]
        .iter()
        .flatten()
        .enumerate()
    {
        f32_at(&mut b, 172 + i * 4, *v);
    }
    b[210] = 1;
    u32_at(&mut b, 212, 232);
    b[234] = 1;
    b[236] = 2;
    b
}

#[test]
fn native_pose_retains_rest_units_and_records_motion_source() {
    let c = Case::new();
    let m = model();
    let input = c.file("source.sa2mdl", &m);
    let output = c.0.join("rest.json");
    succeeded(
        c.command()
            .arg("pose")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap(),
    );
    let rest = read(&output);
    let rest_manifest = read(&sidecar(&output));
    assert_eq!(rest["nodes"][0]["worldMatrix"][0][3], 3.0);
    assert_eq!(rest["nodes"][0]["worldMatrix"][1][3], 4.0);
    let a = motion();
    let animation = c.file("move.saanim", &a);
    let output = c.0.join("sampled.json");
    succeeded(
        c.command()
            .arg("pose")
            .arg(&input)
            .arg(&output)
            .arg(&animation)
            .arg("0.5")
            .output()
            .unwrap(),
    );
    let sampled = read(&output);
    assert_eq!(sampled["nodes"][0]["worldMatrix"][1][3], 1.0);
    assert_eq!(sampled["requestedFrame"], 0.5);
    assert_eq!(sampled["schemaVersion"], 1);
    let m = read(&sidecar(&output));
    assert!(m["assets"][0]["id"]
        .as_str()
        .unwrap()
        .ends_with(".native-evaluation.native-pose-json"));
    assert_eq!(m["assets"][0]["id"], rest_manifest["assets"][0]["id"]);
    assert_eq!(
        m["sources"][0]["sha256"],
        rest_manifest["sources"][0]["sha256"]
    );
    assert_ne!(
        m["assets"][0]["output"]["sha256"],
        rest_manifest["assets"][0]["output"]["sha256"]
    );
    assert_eq!(m["sources"].as_array().unwrap().len(), 2);
    assert_eq!(m["assets"][0]["dependencies"][0]["role"], "native-motion");
    assert_eq!(
        m["assets"][0]["dependencies"][0]["sha256"],
        manifest::sha256(&a)
    );
    assert_eq!(fs::read(input).unwrap(), model());
    assert_eq!(fs::read(animation).unwrap(), a);
}

#[test]
fn native_collision_bake_and_distance_limited_query_use_real_transforms() {
    let c = Case::new();
    let s = stage();
    let input = c.file("stage.sa2lvl", &s);
    let output = c.0.join("collision.json");
    succeeded(
        c.command()
            .arg("collision")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap(),
    );
    let world = read(&output);
    assert_eq!(
        world["meshes"][0]["vertices"][0],
        serde_json::json!([1.0, 2.0, 3.0])
    );
    assert_eq!(
        world["meshes"][0]["triangles"][0],
        serde_json::json!([0, 1, 2])
    );
    let output = c.0.join("scaled.json");
    succeeded(
        c.command()
            .arg("collision")
            .arg(&input)
            .arg(&output)
            .arg("0.1")
            .output()
            .unwrap(),
    );
    assert_eq!(read(&output)["uniformScale"], 0.1);
    let query = c.file(
        "ray.json",
        br#"{"origin":[1.25,10,3.25],"direction":[0,-2,0],"maxDistance":9}"#,
    );
    let output = c.0.join("hit.json");
    succeeded(
        c.command()
            .arg("raycast")
            .arg(&input)
            .arg(&output)
            .arg(&query)
            .output()
            .unwrap(),
    );
    let hit = read(&output);
    assert_eq!(hit["hit"]["distance"], 8.0);
    assert_eq!(hit["hit"]["position"][1], 2.0);
    let m = read(&sidecar(&output));
    assert_eq!(m["assets"][0]["dependencies"][0]["role"], "ray-request");
    let query = c.file(
        "short.json",
        br#"{"origin":[1.25,10,3.25],"direction":[0,-1,0],"maxDistance":7}"#,
    );
    let output = c.0.join("miss.json");
    succeeded(
        c.command()
            .arg("raycast")
            .arg(&input)
            .arg(&output)
            .arg(&query)
            .output()
            .unwrap(),
    );
    assert!(read(&output)["hit"].is_null());
    assert_eq!(fs::read(input).unwrap(), s);
}

#[test]
fn invalid_runtime_arguments_and_queries_produce_no_output() {
    let c = Case::new();
    let input = c.file("source.sa2mdl", &model());
    let animation = c.file("move.saanim", &motion());
    for value in ["NaN", "inf"] {
        let output = c.0.join(format!("bad-{value}.json"));
        let result = c
            .command()
            .arg("pose")
            .arg(&input)
            .arg(&output)
            .arg(&animation)
            .arg(value)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!output.exists());
    }
    let stage = c.file("stage.sa2lvl", &stage());
    for value in ["NaN", "inf", "0", "-1"] {
        let output = c.0.join(format!("scale-{value}.json"));
        assert!(!c
            .command()
            .arg("collision")
            .arg(&stage)
            .arg(&output)
            .arg(value)
            .output()
            .unwrap()
            .status
            .success());
        assert!(!output.exists());
    }
    let query = c.file(
        "invalid.json",
        br#"{"origin":[0,1,0],"direction":[0,0,0],"maxDistance":10,"trustMe":true}"#,
    );
    let output = c.0.join("invalid-ray.json");
    assert!(!c
        .command()
        .arg("raycast")
        .arg(&stage)
        .arg(&output)
        .arg(query)
        .output()
        .unwrap()
        .status
        .success());
    assert!(!output.exists());
    let output = c.0.join("arity.json");
    assert!(!c
        .command()
        .arg("pose")
        .arg(input)
        .arg(&output)
        .arg(animation)
        .output()
        .unwrap()
        .status
        .success());
    assert!(!output.exists());
}
