use sa2_asset_tools::goal_ring::{initialize, Call, Contact, Hit, InitRoute, Program};
use std::{env, fs, path::PathBuf, process::Command};
#[test]
fn goal_contact_cannot_repeat() {
    let mut state = Contact::default();
    let calls = state.step(0, 0, Hit::Player { slot: 0 }, 123).unwrap();
    assert!(matches!(
        calls.last(),
        Some(Call::Local {
            offset: 123,
            slot: 0,
            argument: 2
        })
    ));
    assert_eq!(
        state.step(0, 0, Hit::Player { slot: 0 }, 123).unwrap(),
        [Call::Register]
    );
    assert!(matches!(
        initialize(1, 0, 0, None, false, true),
        InitRoute::ChildTask
    ));
    assert!(matches!(
        initialize(0, 4, 1, Some(119), false, true),
        InitRoute::Deferred
    ));
}
#[test]
#[ignore = "requires installed Steam stage modules"]
fn installed_goal_programs() {
    let gd = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap());
    for id in [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30,
        32, 40, 43, 44,
    ] {
        let bytes = fs::read(gd.join(format!("stg{id:02}D.rel"))).unwrap();
        let program = Program::read(id, &bytes).unwrap();
        assert_eq!(program.radii, [25., 8.]);
        assert!(Program::read(if id == 3 { 4 } else { 3 }, &bytes).is_err());
    }
}
#[test]
#[ignore = "requires installed stage and Unicorn PPC"]
fn compare_installed_init_and_contact() {
    let o = Command::new("python")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goal_ring_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let stdout = String::from_utf8(o.stdout).unwrap();
    let mut counts = [0, 0];
    for line in stdout.lines() {
        let v: Vec<_> = line.split_whitespace().collect();
        let n = |i: usize| v[i].parse::<i32>().unwrap();
        if v[0] == "I" {
            let r = initialize(
                n(1),
                n(2) as i8,
                n(3) as i8,
                (n(4) >= 0).then(|| n(4) as u16),
                n(5) != 0,
                n(6) != 0,
            );
            let name = match r {
                InitRoute::Contact { angle_x } => {
                    assert_eq!(angle_x, n(8));
                    "contact"
                }
                InitRoute::ChildTask => "child",
                _ => "return",
            };
            assert_eq!(name, v[7], "{line}");
            counts[0] += 1;
        } else {
            let mut s = Contact { latched: n(3) != 0 };
            let hit = match n(4) {
                -2 => Hit::None,
                -1 => Hit::Unidentified,
                i => Hit::Player { slot: i as u8 },
            };
            let old = s.latched;
            let calls = s.step(n(1), n(2) as i8, hit, 0x390dc).unwrap();
            let names = calls
                .iter()
                .map(|c| match c {
                    Call::CheckHit => "check",
                    Call::Identify => "identify",
                    Call::Register => "register",
                    Call::Main {
                        address: 0x8001baa4,
                        ..
                    } => "main2",
                    Call::Main {
                        address: 0x8001c78c,
                        ..
                    } => "main1",
                    Call::Local {
                        offset: 0x390dc,
                        argument: 2,
                        ..
                    } => "local",
                    Call::Sound {
                        code: 0x1013,
                        priority: 30,
                        duration: 180,
                    } => "4115",
                    Call::Sound {
                        code: 0x1014,
                        priority: 30,
                        duration: 180,
                    } => "4116",
                    _ => panic!(),
                })
                .collect::<Vec<_>>()
                .join(",");
            for call in &calls {
                match call {
                    Call::Main { slot, .. } | Call::Local { slot, .. } => {
                        assert_eq!(i32::from(*slot), n(4))
                    }
                    _ => {}
                }
            }
            assert_eq!(names, v[7], "{line}");
            assert_eq!(s.latched, n(5) != 0);
            assert_eq!(v[6], if !old && s.latched { "0.0" } else { "7.0" });
            counts[1] += 1;
        }
    }
    assert_eq!(counts, [720, 280]);
}
#[test]
#[ignore = "requires installed Green Forest and official mapping files"]
fn mounted_goal_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root
        .ancestors()
        .nth(3)
        .unwrap()
        .join(".local")
        .join(format!("goal-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let game = PathBuf::from(env::var_os("SA2_GAME_PATH").unwrap());
    let config = PathBuf::from(env::var_os("SA2_PC_CONFIG").unwrap());
    let request = dir.join("request.json");
    fs::write(&request,r#"{"game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":[{"kind":"none"},{"kind":"player","slot":1},{"kind":"player","slot":1}]}"#).unwrap();
    let run = |row: &str| {
        let out = dir.join(format!("{row}.json"));
        let result = Command::new(env!("CARGO_BIN_EXE_sa2-assets"))
            .arg("--source-root")
            .arg(&game)
            .arg("fan-goal")
            .arg(game.join("sonic2app.exe"))
            .arg(&out)
            .arg(&config)
            .args(["Hero Story Sequence", "13", "0", row])
            .arg(&request)
            .output()
            .unwrap();
        if !result.status.success() {
            assert!(!out.exists());
            return None;
        }
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(out.with_extension("json.manifest.json")).unwrap())
                .unwrap();
        assert!(
            receipt["assets"][0]["dependencies"]
                .as_array()
                .unwrap()
                .len()
                >= 6
        );
        let doc: serde_json::Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
        fs::remove_file(out.with_extension("json.manifest.json")).unwrap();
        fs::remove_file(out).unwrap();
        Some(doc)
    };
    let doc = run("198").unwrap();
    let v = &doc["replay"];
    assert_eq!(v["radius"], 25.);
    assert_eq!(v["frames"][1]["resetScaleY"], true);
    assert_eq!(v["frames"][2]["calls"], serde_json::json!(["Register"]));
    assert_eq!(run("349").unwrap()["replay"]["init"], "ChildTask");
    assert!(run("7").is_none());
    fs::remove_file(request).unwrap();
    fs::remove_dir(dir).unwrap();
}
