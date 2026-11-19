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
    let exe = fs::read(gd.join("../../sonic2app.exe")).unwrap();
    for id in [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30,
        32, 40, 43, 44,
    ] {
        let bytes = fs::read(gd.join(format!("stg{id:02}D.rel"))).unwrap();
        let program = Program::read(id, &bytes).unwrap();
        let pairs = program.pc_shape_templates(&exe).unwrap();
        for pair in pairs.as_array().unwrap() {
            assert_eq!(
                pair["matchesAfterEndianConversion"], true,
                "stage {id}: {pair}"
            );
        }
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
    let mut counts = [0; 18];
    let gd = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap());
    let program = Program::read(13, &fs::read(gd.join("stg13D.rel")).unwrap()).unwrap();
    let variants = [13, 16, 18]
        .map(|id| Program::read(id, &fs::read(gd.join(format!("stg{id}D.rel"))).unwrap()).unwrap());
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
        } else if v[0] == "COUNT" {
            assert_eq!(
                program.consumer_countdown(n(1), serde_json::from_str(v[2]).unwrap()),
                serde_json::from_str::<serde_json::Value>(v[3]).unwrap(),
                "{line}"
            );
            counts[17] += 1;
        } else if v[0] == "TAIL" {
            assert_eq!(
                program
                    .consumer_update(n(1), v[2].parse().unwrap(), n(3))
                    .unwrap(),
                serde_json::from_str::<serde_json::Value>(v[4]).unwrap(),
                "{line}"
            );
            counts[16] += 1;
        } else if v[0] == "TEX" {
            assert_eq!(
                program
                    .regular_texture_setup(n(1) as u32, n(2) as u32, n(3) as u32)
                    .unwrap(),
                serde_json::from_str::<serde_json::Value>(v[4]).unwrap(),
                "{line}"
            );
            counts[15] += 1;
        } else if v[0] == "FREE" {
            assert_eq!(
                program.regular_release(n(1) as u32, n(2) as u32).unwrap(),
                serde_json::from_str::<serde_json::Value>(v[3]).unwrap(),
                "{line}"
            );
            counts[14] += 1;
        } else if v[0] == "BIND" {
            let queries: Vec<i32> = serde_json::from_str(v[2]).unwrap();
            assert_eq!(
                program.regular_task_bind(n(1), &queries).unwrap(),
                serde_json::from_str::<serde_json::Value>(v[3]).unwrap(),
                "{line}"
            );
            counts[13] += 1;
        } else if v[0] == "G" {
            assert_eq!(
                program.regular_collision_setup(n(1)),
                serde_json::from_str::<serde_json::Value>(v[2]).unwrap(),
                "{line}"
            );
            counts[12] += 1;
        } else if v[0] == "Z" {
            assert_eq!(
                program
                    .child_start(n(1) as i8, serde_json::from_str(v[2]).unwrap())
                    .unwrap(),
                serde_json::from_str::<serde_json::Value>(v[3]).unwrap(),
                "{line}"
            );
            counts[11] += 1;
        } else if v[0] == "P" || v[0] == "Y" {
            let obs = serde_json::from_str(v[1]).unwrap();
            assert_eq!(
                program.child_contact(obs).unwrap(),
                serde_json::from_str::<serde_json::Value>(v[2]).unwrap(),
                "{line}"
            );
            counts[if v[0] == "P" { 9 } else { 10 }] += 1;
        } else if v[0] == "A" {
            let p = variants
                .iter()
                .find(|p| i32::from(p.level) == n(1))
                .unwrap();
            let gates: Vec<i8> = serde_json::from_str(v[4]).unwrap();
            assert_eq!(
                p.child_mode_one(v[2].parse().unwrap(), v[3].parse().unwrap(), &gates)
                    .unwrap(),
                serde_json::from_str::<serde_json::Value>(v[5]).unwrap(),
                "{line}"
            );
            counts[8] += 1;
        } else if v[0] == "H" {
            let r = program
                .child_dispatch(n(1) as i8, n(2) as i8, (n(2) == 0).then(|| n(3) != 0))
                .unwrap();
            let h = &r["handler"];
            let calls = h
                .get("calls")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            assert_eq!(
                calls,
                serde_json::from_str::<serde_json::Value>(v[6]).unwrap()
            );
            let mode = if r["resetModeAfterCall"] == true {
                0
            } else {
                h["modeWrite"].as_i64().unwrap_or(i64::from(n(1)))
            };
            assert_eq!(mode, i64::from(n(4)));
            assert_eq!(h["task60Write"].as_i64().unwrap_or(123), i64::from(n(5)));
            counts[7] += 1;
        } else if v[0] == "U" {
            let r = program
                .child_dispatch(n(1) as i8, n(2) as i8, (n(2) == 0).then(|| n(3) != 0))
                .unwrap();
            assert_eq!(r["rangeChecked"], n(4) != 0);
            let call = &r["taskCall"];
            let addr = call["engine"]
                .as_u64()
                .or(call["local"].as_u64())
                .unwrap_or(0);
            assert_eq!(addr, v[5].parse::<u64>().unwrap());
            let writes = &r["callbackWrites20And32"];
            for (i, default) in [7, 11].into_iter().enumerate() {
                let expected =
                    writes[i]
                        .as_u64()
                        .map_or(default, |v| if v == 0 { 0 } else { 0x81000000 + v });
                assert_eq!(expected, v[6 + i].parse::<u64>().unwrap());
            }
            let final_mode = if r["resetModeAfterCall"] == true {
                0
            } else if addr != 0 {
                93
            } else {
                n(1)
            };
            assert_eq!(final_mode, n(8));
            counts[6] += 1;
        } else if v[0] == "K" {
            let observed = sa2_asset_tools::goal_ring::ChildObservation {
                existing: n(1) != 0,
                allocation: (n(2) >= 0).then(|| n(2) == 1),
                linked_after: n(3) != 0,
            };
            let report = program
                .child_placement(observed, [1.5, -20., 3.75])
                .unwrap();
            if n(4) < 0 {
                assert!(report["create"].is_null());
            } else {
                assert_eq!(report["create"]["entry"], n(4));
            }
            if n(5) < 0 {
                assert!(report["initialized"].is_null());
            } else {
                assert_eq!(report["initialized"]["parentCallback32"], n(5));
                assert_eq!(
                    report["initialized"]["childWork32Bits"],
                    v[6].parse::<u32>().unwrap()
                );
                assert_eq!(report["initialized"]["childWorkByte1"], n(7));
            }
            assert_eq!(
                report["linkedChildPosition"],
                serde_json::json!((n(3) != 0).then_some([1.5, -20., 3.75]))
            );
            counts[5] += 1;
        } else if v[0] == "T" {
            use sa2_asset_tools::goal_ring::{IndexedMessage, MessageContext};
            let code = n(1) as u16;
            let count = n(2);
            let normal = i32::from(code) < count;
            let m = MessageContext {
                count,
                handle: if normal { 67 } else { 35 },
                work_address: 0x100100,
                section5_address: 0x82500000,
                indexed: normal.then_some(IndexedMessage {
                    pointer: 0x12340000 + u32::from(code),
                    game_byte_17: n(3) as i8,
                }),
            };
            let call = program.message_call(code, m).unwrap();
            assert_eq!(call["engine"], v[4].parse::<u32>().unwrap());
            assert_eq!(
                call["args"],
                serde_json::json!(v[5..]
                    .iter()
                    .map(|x| x.parse::<i64>().unwrap())
                    .collect::<Vec<_>>())
            );
            counts[4] += 1;
        } else if v[0] == "D" {
            let slot = n(1) as u8;
            let mut words = [41, 42];
            words[usize::from(slot)] = 2;
            let code = program.consume_goal_word(&mut words, slot, n(2)).unwrap();
            assert_eq!(code.map(i32::from).unwrap_or(-1), n(3));
            assert_eq!(words, [n(4) as u32, n(5) as u32]);
            counts[3] += 1;
        } else if v[0] == "M" {
            let mut m = sa2_asset_tools::goal_ring::RegularMotion {
                yaw: 2147483640,
                y: -11.625,
                timer: n(3) as u16,
            };
            m.tick(n(1) != 0, n(2) as i8, program.spin, program.rise);
            assert_eq!(u32::from(m.timer), n(5) as u32);
            assert_eq!(m.yaw, n(6));
            assert_eq!(m.y.to_bits(), v[7].parse::<u32>().unwrap());
            assert_eq!(
                m.cleanup_call(n(1) != 0, n(4) != 0).unwrap_or_default(),
                v[8].parse::<u32>().unwrap()
            );
            counts[2] += 1;
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
            let mut words = [12345, 54321];
            sa2_asset_tools::goal_ring::apply_slot_writes(&mut words, &calls, program.local_effect);
            assert_eq!(words, [n(8) as u32, n(9) as u32]);
            assert_eq!(names, v[7], "{line}");
            assert_eq!(s.latched, n(5) != 0);
            assert_eq!(v[6], if !old && s.latched { "0.0" } else { "7.0" });
            counts[1] += 1;
        }
    }
    assert_eq!(
        counts,
        [720, 280, 40, 194, 45, 5, 44, 6, 18, 96, 176, 1280, 12, 6, 2, 2, 100, 32]
    );
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
    fs::write(&request,r#"{"post_contact_consumer":{"slot":1,"query":13},"initial_slot_words":[91,72],"game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":[{"kind":"none"},{"kind":"player","slot":1},{"kind":"player","slot":1}]}"#).unwrap();
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
    assert_eq!(
        v["postContactConsumer"]["moduleSlotWords"],
        serde_json::json!([91, 0])
    );
    assert_eq!(v["radius"], 25.);
    for (i, expected) in [[91, 72], [91, 2], [91, 2]].iter().enumerate() {
        assert_eq!(
            v["frames"][i]["moduleSlotWords"],
            serde_json::json!(expected)
        );
    }
    assert_eq!(v["frames"][1]["resetScaleY"], true);
    assert_eq!(v["frames"][2]["calls"], serde_json::json!(["Register"]));
    assert_eq!(run("349").unwrap()["replay"]["init"], "ChildTask");
    assert!(run("7").is_none());
    fs::write(&request,r#"{"game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":[{"kind":"none"}],"child":{"existing":false,"allocation":true,"linked_after":true},"child_dispatch":{"mode":0,"byte1":1,"first_update":{"game_byte_55":1},"start":{"work32":2.0,"task_link":false,"parent_link":false,"query_returns":[]}}}"#).unwrap();
    let child = run("349").unwrap();
    assert_eq!(child["replay"]["childDispatch"]["start"]["mode"], 1);
    assert_eq!(child["replay"]["childDispatch"]["start"]["task60"], 2);
    assert_eq!(child["replay"]["childDispatch"]["rangeChecked"], false);
    assert!(child["replay"]["childDispatch"]["taskCall"]["local"].is_u64());
    assert_eq!(
        child["replay"]["childDispatch"]["firstUpdate"]["handlerFrames"][0]["mode"],
        2
    );
    let mut alternate: serde_json::Value =
        serde_json::from_slice(&fs::read(&request).unwrap()).unwrap();
    alternate["child_dispatch"]["start"]["task_link"] = true.into();
    alternate["child_dispatch"]["start"]["query_returns"] = serde_json::json!([1, 1]);
    alternate["child_dispatch"]["first_update"] = serde_json::json!({});
    fs::write(&request, serde_json::to_vec(&alternate).unwrap()).unwrap();
    let child = run("349").unwrap();
    let dispatch = &child["replay"]["childDispatch"];
    assert_eq!(dispatch["start"]["mode"], 4);
    assert_eq!(dispatch["firstUpdate"]["taskCall"]["engine"], 0x8000f4ecu32);
    assert_eq!(dispatch["firstUpdate"]["resetModeAfterCall"], false);
    let mut inputs = vec![serde_json::json!({"kind":"none"}); 64];
    inputs[0] = serde_json::json!({"kind":"player","slot":0});
    fs::write(
        &request,
        serde_json::to_vec(&serde_json::json!({
            "game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":inputs,
            "texture_setup":{"work56":4096,"allocation_return":8192},
            "lifecycle":{"initial_timer":0,"game_byte_37":0,"task_field_30_present":false,"release":{"work56":4096,"nested":8192}}
        }))
        .unwrap(),
    )
    .unwrap();
    let doc = run("198").unwrap();
    let frames = doc["replay"]["frames"].as_array().unwrap();
    assert_eq!(
        doc["replay"]["textureSetup"]["calls"][1],
        serde_json::json!([0x801194b4u32, 1, 4096])
    );
    assert_eq!(
        doc["replay"]["observedRelease"]["events"][0],
        serde_json::json!(["free", 0x801194e0u32, 8192])
    );
    assert_eq!(frames.len(), 61);
    assert_eq!(frames[0]["regularMotion"]["timer"], 0);
    assert_eq!(frames[59]["regularMotion"]["timer"], 59);
    assert!(frames[59]["cleanupCall"].is_null());
    assert_eq!(frames[60]["cleanupCall"], 0x80071878u32);
    fs::remove_file(request).unwrap();
    fs::remove_dir(dir).unwrap();
}
