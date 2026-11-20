//! Stage 19's stage program, stage task and director against the real
//! stg19D.rel (GC disc copy from SA2_GC_REL_DIR, Steam copy from
//! SA2_GAME_PATH). Without either file the tests skip.
use sa2_asset_tools::stage_code::sonic_vs_shadow1 as s19;
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(d) = std::env::var_os("SA2_GC_REL_DIR") {
        out.extend(std::fs::read(PathBuf::from(d).join("stg19D.rel")).ok());
    }
    if let Some(g) = std::env::var_os("SA2_GAME_PATH") {
        out.extend(std::fs::read(PathBuf::from(g).join("resource/gd_PC/stg19D.rel")).ok());
    }
    if out.is_empty() {
        eprintln!("skipped: set SA2_GC_REL_DIR and/or SA2_GAME_PATH");
    }
    out
}

fn engines(v: &Value) -> Vec<String> {
    v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["engine"]
                .as_str()
                .or(c["module"].as_str())
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn rejects_other_modules() {
    assert!(s19::read(&[0; 16]).is_err());
    assert!(s19::read(&vec![0; s19::IDENT.len]).is_err());
}

#[test]
fn setup_and_teardown() {
    for rel in rels() {
        let v = s19::replay(
            &rel,
            "setup",
            &json!({ "player2": true, "slots": [0, 5, 0],
                "returns": { "0x8001dda8": [0], "0x80065048": [77], "0x80071e0c": [1] } }),
        )
        .unwrap();
        let e = engines(&v);
        assert_eq!(e[0], "0x8001dda8");
        assert_eq!(e[1], "0x80086e3c");
        assert!(e.contains(&"s1+0xd30".to_string()));
        assert_eq!(e.last().unwrap(), "0x8006d500");
        let st = &v["state"];
        assert_eq!(st["handle"], 77);
        assert_eq!(st["task"], "s1+0x13b8");
        assert_eq!(st["slots"], json!(["s1+0x1c284", 5, "s1+0x95a4"]));
        assert_eq!(st["trail"][0], 0x800e014c_u32);
        assert_eq!(st["trail"][1], "s1+0x7fe8");
        // The lights: block, block+0x20, block, block.
        let lights: Vec<_> = v["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["engine"] == "0x8002506c")
            .map(|c| c["args"][1].clone())
            .collect();
        assert_eq!(
            lights,
            vec![
                json!("s5+0x6b5c"),
                json!("s5+0x6b7c"),
                json!("s5+0x6b5c"),
                json!("s5+0x6b5c")
            ]
        );
        let t = s19::replay(
            &rel,
            "teardown",
            &json!({ "slots": ["ours", 9, "ours"], "handle": 77 }),
        )
        .unwrap();
        assert_eq!(t["state"]["slots"], json!([0, 9, 0]));
        assert!(engines(&t).contains(&"0x8001a2b0".to_string()));
    }
}

#[test]
fn program_exec_and_stage_task() {
    for rel in rels() {
        let v = s19::replay(&rel, "exec", &json!({ "mode": 0, "pos": [0.0, 0.0, 0.0] })).unwrap();
        assert_eq!(v["state"]["mode"], 1);
        assert_eq!(v["calls"][0]["args"][1]["0x4"], 50f32.to_bits());
        let v = s19::replay(
            &rel,
            "exec",
            &json!({ "mode": 1, "pos": [0.0, 0.0, 0.0], "player2": [1.0, 2.0, 3.0] }),
        )
        .unwrap();
        assert_eq!(v["state"]["pos"], json!([1.0, 12.0, 3.0]));
        let v = s19::replay(
            &rel,
            "task_exec",
            &json!({ "counter": 13, "texture": 0xc005 }),
        )
        .unwrap();
        assert_eq!(v["state"]["counter"], 0);
        assert_eq!(v["state"]["texture"], 0xc000);
        let v = s19::replay(&rel, "task_display", &json!({ "camera": [5.0, 1.0, 6.0] })).unwrap();
        assert_eq!(
            v["calls"][4]["args"][0],
            json!([5f32.to_bits(), 920f32.to_bits(), 6f32.to_bits()])
        );
    }
}

fn director(mode: i8, timer: i16, returns: Value) -> Value {
    let body = json!({ "action": 0, "flags4": 0, "yaw": 0, "pos": [0.0, 0.0, 0.0] });
    let data = json!({ "byte1": 0, "character": 0, "flags10": 0, "field1c": 1,
        "gauge": 3.0, "field68": 0.0, "field1b4": 0.0 });
    json!({ "g37": 0, "g39": 0, "g42": 0, "g7c": 5, "bodies": [body, body], "data": [data, data],
        "mode": mode, "flag": 0, "timer": timer, "limit": 100, "point": [0.0, 0.0, 0.0],
        "child": 1, "player2Task": 9, "returns": returns })
}

#[test]
fn director_modes() {
    for rel in rels() {
        // Mode 0 waits for fn_8001CD40 == 16, then opens the title card.
        let v = s19::replay(
            &rel,
            "director_exec",
            &director(0, 0, json!({ "0x8001cd40": [16, 0], "0x8011b65c": [10.0] })),
        )
        .unwrap();
        assert_eq!(v["state"]["mode"], 1);
        assert_eq!(v["state"]["limit"], 360);
        assert_eq!(v["state"]["point"], json!([0.0, -38.0, 0.0]));
        assert!(engines(&v).contains(&"s1+0x24584".to_string()));
        // Mode 1 past its limit: roam mode 5 with limit 240 + 120 * rand / 32768.
        let v = s19::replay(
            &rel,
            "director_exec",
            &director(
                1,
                101,
                json!({ "0x8001dda0": [1], "0x8010dc88": [16384],
                "0x800c3bdc": [0], "0x8001cd40": [0], "0x8011b65c": [10.0] }),
            ),
        )
        .unwrap();
        assert_eq!(v["state"]["mode"], 5);
        assert_eq!(v["state"]["limit"], 300);
        assert_eq!(v["state"]["timer"], 1);
        // Mode 7 after 60 frames goes back to the chase.
        let v = s19::replay(
            &rel,
            "director_exec",
            &director(7, 61, json!({ "0x8001cd40": [0], "0x8011b65c": [10.0] })),
        )
        .unwrap();
        assert_eq!(v["state"]["mode"], 2);
        // A restart (game word +0x7C zero) resets and destroys the child.
        let mut d = director(3, 1, json!({ "0x8011b65c": [10.0], "task+0xc": [0] }));
        d["g7c"] = json!(0);
        let v = s19::replay(&rel, "director_exec", &d).unwrap();
        assert_eq!(v["state"]["mode"], 0);
        assert_eq!(v["state"]["child"], 0);
        assert!(engines(&v).contains(&"0x80071af0".to_string()));
        // Outside the arena the rival is pulled back (fn_800399BC).
        let mut d = director(
            9,
            0,
            json!({ "0x8011b65c": [200.0], "s1+0x138": [100.0],
            "0x8001cd40": [0] }),
        );
        d["bodies"][1]["pos"] = json!([100.0, 0.0, 0.0]);
        let v = s19::replay(&rel, "director_exec", &d).unwrap();
        let pull = v["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["engine"] == "0x800399bc")
            .unwrap();
        assert_eq!(pull["args"][1], (129f32 * 0.01 * 100.0).to_bits());
    }
}

#[test]
fn director_create_by_character() {
    for rel in rels() {
        let v = s19::replay(
            &rel,
            "director_create",
            &json!({ "character": 1, "returns": { "0x80071e0c": [4], "0x801197fc": [8] } }),
        )
        .unwrap();
        assert_eq!(v["state"]["display"], "s1+0xd08");
        assert_eq!(v["state"]["gauge"], 3.0);
        let v = s19::replay(
            &rel,
            "director_create",
            &json!({ "character": 0, "returns": { "0x80071e0c": [4], "0x801197fc": [0] } }),
        )
        .unwrap();
        assert_eq!(engines(&v).last().unwrap(), "0x80071ae0");
    }
}
