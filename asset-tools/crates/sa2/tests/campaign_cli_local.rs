//! Invoke the shipped executable. All retained fixtures are hashes of locally split sources.
use sa2_asset_tools::manifest::{self, Manifest};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct Case {
    dir: PathBuf,
    game: PathBuf,
}
impl Case {
    fn call(&self, command: &str, input: &Path, label: &str, args: &[&str]) -> (PathBuf, Output) {
        let output = self.dir.join(format!("{label}.json"));
        let result = Command::new(env!("CARGO_BIN_EXE_sa2-assets"))
            .arg("--source-root")
            .arg(&self.game)
            .arg(command)
            .arg(input)
            .arg(&output)
            .args(args)
            .output()
            .unwrap();
        (output, result)
    }
    fn good(
        &self,
        command: &str,
        input: &Path,
        label: &str,
        args: &[&str],
        deps: &[(&str, &Path)],
    ) -> Value {
        let (path, run) = self.call(command, input, label, args);
        assert!(
            run.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let bytes = fs::read(&path).unwrap();
        let m: Manifest =
            serde_json::from_slice(&fs::read(path.with_extension("json.manifest.json")).unwrap())
                .unwrap();
        m.validate().unwrap();
        assert_eq!(m.assets.len(), 1);
        let a = &m.assets[0];
        let expected = manifest::file_hash(input).unwrap();
        assert_eq!(
            (a.span.byte_length, &a.span.sha256),
            (expected.0, &expected.1)
        );
        let out = a.output.as_ref().unwrap();
        assert_eq!(out.format, format!("{command}-json"));
        assert_eq!(
            out.logical_path,
            path.file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(out.byte_length, bytes.len() as u64);
        assert_eq!(out.sha256, manifest::sha256(&bytes));
        assert_eq!(m.sources.len(), deps.len() + 1);
        assert_eq!(a.dependencies.len(), deps.len());
        for (binding, (role, dep_path)) in a.dependencies.iter().zip(deps) {
            let (len, hash) = manifest::file_hash(dep_path).unwrap();
            assert_eq!(binding.role, *role);
            assert_eq!((binding.offset, binding.byte_length), (0, len));
            assert_eq!(binding.sha256, hash);
            let source = m
                .sources
                .iter()
                .find(|s| s.id == binding.source_id)
                .unwrap();
            assert_eq!((source.byte_length, &source.sha256), (len, &hash));
            assert_eq!(
                Path::new(&source.logical_path).file_name(),
                dep_path.file_name()
            );
        }
        serde_json::from_slice(&bytes).unwrap()
    }
    fn bad(&self, command: &str, input: &Path, label: &str, args: &[&str], reason: &str) {
        let (path, run) = self.call(command, input, label, args);
        assert!(!run.status.success());
        let err = String::from_utf8_lossy(&run.stderr);
        assert!(err.contains(reason), "{label}: {err}");
        assert!(!path.exists());
        assert!(!path.with_extension("json.manifest.json").exists());
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        // A fresh test case contains only JSON files produced by this case.
        if let Ok(files) = fs::read_dir(&self.dir) {
            for f in files.flatten() {
                let _ = fs::remove_file(f.path());
            }
        }
        let _ = fs::remove_dir(&self.dir);
    }
}

fn camera(packet: &Value, global: u32, scene: u32, local: u32, motion: i32, hash: &str) {
    assert_eq!(packet["timing"]["global_frame"], global);
    assert_eq!(packet["timing"]["scene"], scene);
    assert_eq!(packet["timing"]["scene_frame"], local);
    assert_eq!(packet["cameraSlot"], 0);
    assert_eq!(packet["motionIndex"], motion);
    let c = &packet["camera"];
    assert_eq!(c["motion_index"], motion);
    assert_eq!(c["frame"].as_f64().unwrap(), f64::from(local));
    assert!(c["frame_count"].as_u64().unwrap() > u64::from(local));
    let mut bytes = Vec::new();
    for name in ["position", "target"] {
        let v = c[name].as_array().unwrap();
        assert_eq!(v.len(), 3);
        for x in v {
            bytes.extend_from_slice(&(x.as_f64().unwrap() as f32).to_le_bytes());
        }
    }
    for name in ["roll_bams", "fov_bams"] {
        bytes.extend_from_slice(
            &i32::try_from(c[name].as_i64().unwrap())
                .unwrap()
                .to_le_bytes(),
        );
    }
    // Independent SAModel NJS_MOTION.GetPosition/GetTarget/GetRoll/GetAngle results from the SA Tools split.
    assert_eq!(manifest::sha256(&bytes), hash);
}

#[test]
#[ignore = "requires SA2_GAME_PATH and SA2_PC_CONFIG with original matching PC sources"]
fn opening_cameras_cut_and_city_stage_entry() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let ini = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap()).join("ADVERTISE.ini");
    let ini_arg = ini.to_str().unwrap();
    let exe = game.join("sonic2app.exe");
    let gd = game.join("resource/gd_PC");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join(".local");
    fs::create_dir_all(&root).unwrap();
    let dir = root.join(format!(
        "campaign-cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let case = Case { dir, game };
    let config_dep = ("native-table-mapping", ini.as_path());
    let story = case.good("story", &exe, "story", &[ini_arg], &[config_dep]);
    assert_eq!(story.as_object().unwrap().len(), 15);
    for (name, id, track, expected) in [
        (
            "Hero Story Sequence",
            0,
            3,
            "c120d15c43d392e583629a30a210042764296d8a1b40d8206ceea5c16162fdbe",
        ),
        (
            "Dark Story Sequence",
            100,
            1,
            "00c9ecb2924c8441c8503a8c6a6b13b2b2f85cec172b4b5257bfc5b24edc838e",
        ),
        (
            "Last Story Sequence",
            200,
            13,
            "af6b0c528a8393ae1ad68522fbfa1573badf987d69dd5901a226ddc619c7c6c8",
        ),
    ] {
        let prs = gd.join(format!("event/e{id:04}.prs"));
        let bank = gd.join(format!("event/e{id:04}motion.bin"));
        let deps = [
            config_dep,
            ("story-event-image", prs.as_path()),
            ("event-motion-bank", bank.as_path()),
        ];
        assert_eq!(story[name]["entries"][0]["eventWords"][0], id);
        let d = case.good(
            "campaign-camera",
            &exe,
            &format!("opening-{id}"),
            &[ini_arg, name, "0", "0", "0"],
            &deps,
        );
        assert_eq!(d["sequence"], name);
        assert_eq!(d["action"]["event"], id);
        assert_eq!(d["actionIndex"], 0);
        camera(&d["content"], 0, 1, 0, track, expected);
        if id == 0 {
            for (frame, scene, local, track, digest) in [
                (
                    994,
                    1,
                    994,
                    3,
                    "5045432993c36517ccccc6367f36b701cf1ea6d855aa62e56b1130ea97e2eec2",
                ),
                (
                    995,
                    2,
                    0,
                    18,
                    "5bd5d4f4f6080fdb673696fea120b6e398e1a01d97093ef44fb3a82426260662",
                ),
            ] {
                let f = frame.to_string();
                let at = case.good(
                    "campaign-camera",
                    &exe,
                    &format!("cut-{f}"),
                    &[ini_arg, name, "0", &f, "0"],
                    &deps,
                );
                camera(&at["content"], frame, scene, local, track, digest);
                if frame == 995 {
                    let standalone = case.good(
                        "event-camera",
                        &prs,
                        "standalone",
                        &[&f, bank.to_str().unwrap(), "0"],
                        &[deps[2]],
                    );
                    assert_eq!(standalone, at["content"]);
                }
            }
            case.bad(
                "event-camera",
                &prs,
                "outside-slot",
                &["0", bank.to_str().unwrap(), "999"],
                "camera slot outside",
            );
        }
    }
    let hero = "Hero Story Sequence";
    let solid = gd.join("set0013_s.bin");
    let other = gd.join("set0013_u.bin");
    let city = case.good(
        "campaign-frame",
        &exe,
        "city-entry",
        &[ini_arg, hero, "1", "0"],
        &[
            config_dep,
            ("normal-city-layout", &solid),
            ("normal-city-layout", &other),
        ],
    );
    assert_eq!(
        city["action"],
        serde_json::json!({"kind":"level","level":13,"character":0})
    );
    let layers = city["content"]["normalCityLayers"].as_array().unwrap();
    assert_eq!(layers.len(), 2);
    for (layer, (name, count)) in layers
        .iter()
        .zip([("set0013_s.bin", 516), ("set0013_u.bin", 898)])
    {
        assert_eq!(layer["source"], name);
        assert_eq!(layer["placements"], count);
    }
    assert!(!city["content"]["entryPoses"].as_array().unwrap().is_empty());
    case.bad(
        "campaign-frame",
        &exe,
        "movement",
        &[ini_arg, hero, "1", "1"],
        "entry only",
    );
    case.bad(
        "campaign-camera",
        &exe,
        "level-camera",
        &[ini_arg, hero, "1", "0", "0"],
        "is a level",
    );
    case.bad(
        "campaign-frame",
        &exe,
        "unknown-story",
        &[ini_arg, "Foreign Sequence", "0", "0"],
        "unknown story sequence",
    );
    case.bad(
        "campaign-frame",
        &exe,
        "unknown-action",
        &[ini_arg, hero, "999", "0"],
        "action outside",
    );
}
