//! The KASOKU (dash panel) callback against the installed stage modules.
use sa2_asset_tools::dash_panel::{Call, DashPanel, Observed, Program};
use serde_json::Value;
use std::{env, fs, path::PathBuf, process::Command};

fn gd() -> PathBuf {
    PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH")).join("resource/gd_PC")
}

#[test]
#[ignore = "requires the installed stage modules"]
fn reads_city_escape_and_every_module_with_the_same_callback() {
    let p = Program::read("stg13D.rel", &fs::read(gd().join("stg13D.rel")).unwrap()).unwrap();
    assert_eq!(p.update, 0x22c8c);
    assert_eq!(
        (p.default_speed, p.default_frames, p.floor),
        (14.0, 60.0, 0.0)
    );
    assert_eq!(p.velocity_yz, [0.0, 0.0]);
    assert_eq!(
        (p.initial_slot, p.sound, p.rumble),
        (13, 0x1003, [4, 15, 0])
    );
    // Every installed stage module that carries a KASOKU row runs this code.
    let mut found = vec![];
    for e in fs::read_dir(gd()).unwrap() {
        let path = e.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if !(name.starts_with("stg") && name.ends_with("D.rel")) {
            continue;
        }
        if let Ok(q) = Program::read(&name, &fs::read(&path).unwrap()) {
            assert_eq!(
                (q.default_speed, q.default_frames, q.sound, q.rumble),
                (p.default_speed, p.default_frames, p.sound, p.rumble)
            );
            found.push(name);
        }
    }
    assert!(found.contains(&"stg13D.rel".to_string()));
    eprintln!("{} modules: {found:?}", found.len());
}

fn call_json(c: &Call) -> Value {
    match *c {
        Call::InitCollision => serde_json::json!({"call": "initCollision"}),
        Call::EntryCollision => serde_json::json!({"call": "entryCollision"}),
        Call::Sound(code) => serde_json::json!({"call": "sound", "code": code}),
        Call::Rumble { slot, args } => {
            serde_json::json!({"call": "rumble", "slot": slot, "args": args})
        }
        Call::Accelerate {
            slot,
            velocity,
            angles,
            frames,
        } => serde_json::json!({
            "call": "accelerate",
            "slot": slot,
            "velocity": velocity.map(f32::to_bits),
            "angles": angles,
            "frames": frames,
        }),
    }
}

#[test]
#[ignore = "requires the installed City Escape module and Python with Unicorn PPC"]
fn matches_installed_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let rel = gd().join("stg13D.rel");
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/dash_panel_ppc.py"))
        .arg(&rel)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let p = Program::read("stg13D.rel", &fs::read(&rel).unwrap()).unwrap();
    assert_eq!(doc["update"], p.update);
    let mut updates = 0;
    let mut fired = 0;
    for (ci, case) in doc["cases"].as_array().unwrap().iter().enumerate() {
        let angles: Vec<i32> = serde_json::from_value(case["angles"].clone()).unwrap();
        let scale: Vec<u32> = serde_json::from_value(case["scale"].clone()).unwrap();
        let mut panel = DashPanel::new(
            [angles[0], angles[1], angles[2]],
            [0, 1, 2].map(|i| f32::from_bits(scale[i])),
        );
        let frames = case["frames"].as_array().unwrap();
        let native = case["native"].as_array().unwrap();
        for (fi, (f, n)) in frames.iter().zip(native).enumerate() {
            let obs = Observed {
                clipped: f["clipped"].as_bool().unwrap(),
                hit: f["hit"].as_u64().map(|h| h as u8),
                ground: [
                    f["ground"][0].as_bool().unwrap(),
                    f["ground"][1].as_bool().unwrap(),
                ],
                frames: f["frames"].as_f64().unwrap() as f32,
            };
            let calls: Vec<Value> = panel.step(&p, obs).iter().map(call_json).collect();
            fired += calls.iter().filter(|c| c["call"] == "accelerate").count();
            let rust = serde_json::json!({
                "calls": calls,
                "mode": panel.mode,
                "lastSlot": panel.last_slot,
                "timer": panel.timer,
                "scale": panel.scale.map(f32::to_bits),
            });
            let mut n = n.clone();
            n.as_object_mut().unwrap().remove("displaySet");
            assert_eq!(rust, n, "case {ci} update {fi}: {f}");
            updates += 1;
        }
    }
    assert!(fired > 100);
    eprintln!("{updates} KASOKU updates matched installed PowerPC ({fired} firings)");
}
