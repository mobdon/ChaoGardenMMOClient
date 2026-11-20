//! Run every ported stg07D routine on the GameCube disc REL (SA2_GC_REL_DIR)
//! and the installed Steam REL (SA2_GAME_PATH): engine calls are checked
//! against the GC module's imports, and both modules must give the same
//! results. Without those files the file-backed tests skip.
use sa2_asset_tools::aquatic_mine::{self, AquaticMine};
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("SA2_GC_REL_DIR") {
        out.push(std::fs::read(PathBuf::from(dir).join("stg07D.rel")).unwrap());
    }
    if let Some(game) = std::env::var_os("SA2_GAME_PATH") {
        out.push(std::fs::read(PathBuf::from(game).join("resource/gd_PC/stg07D.rel")).unwrap());
    }
    if out.is_empty() {
        eprintln!("skipped: set SA2_GC_REL_DIR and/or SA2_GAME_PATH");
    }
    out
}

fn cases() -> Vec<(&'static str, Value)> {
    let f = |v: f32| v.to_bits();
    let pos = [f(10.0), f(-5.0), f(200.0)];
    let mut c = vec![
        (
            "setup",
            json!({ "handle": 7, "task": true, "slots": [0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            "effect": 0, "buffer": 9, "second": 1 }),
        ),
        (
            "setup",
            json!({ "handle": 7, "task": false, "slots": vec![1; 10],
            "effect": 3, "buffer": 9, "second": 0 }),
        ),
        (
            "teardown",
            json!({ "buffer": 9, "effect": 3, "handle": 7,
            "slots": ["ours", 1, "ours", "ours", "ours", 5, 0, "ours", 0, "ours"] }),
        ),
        (
            "teardown",
            json!({ "buffer": 0, "effect": 0, "handle": 0, "slots": vec![0; 10] }),
        ),
        (
            "task_exec",
            json!({ "player": true, "clock": 3, "under": 0, "pos": pos,
            "rands": [0, 32767, 0, 16384] }),
        ),
        (
            "task_exec",
            json!({ "player": true, "clock": 6, "under": 0, "pos": pos, "rands": [9831] }),
        ),
        (
            "task_exec",
            json!({ "player": true, "clock": 4, "under": 0, "pos": pos, "rands": [] }),
        ),
        ("clear_flags", json!({})),
        ("area_spawn", json!({ "existing": false, "created": true })),
        (
            "area_warmup",
            json!({ "timer": 3, "player": true, "hook": 0 }),
        ),
        (
            "area_warmup",
            json!({ "timer": 1, "player": true, "hook": 0 }),
        ),
        ("area_dest", json!({ "own": true, "hook_ours": false })),
        ("prop_exec", json!({ "range_out": false, "colli": true })),
        ("nocolskull", json!({ "range_out": false })),
        ("sign", json!({})),
        ("cmdoor_display", json!({ "rot_y": 0x4000, "pos": pos })),
    ];
    for name in aquatic_mine::PROP_NAMES {
        c.push(("prop_load", json!({ "name": name, "range_out": false })));
    }
    for mode in [0, 1, 2, 3] {
        c.push((
            "prop_display",
            json!({ "mode": mode, "flag": mode & 1, "timer": 5,
            "rot": [1, 2, 3], "pos": pos, "clock": 100, "sin": f(0.5), "tex": "t", "model": "m" }),
        ));
    }
    for frame in 0..5 {
        c.push((
            "frame_load",
            json!({ "frame": frame, "work1": 0, "rangeOut": false,
            "alloc": true, "rot": [1, 2, 3], "pos": pos }),
        ));
        c.push((
            "frame_exec",
            json!({ "frame": frame, "work1": 0, "rangeOut": false,
            "obj": true, "within": true, "flags": 0, "dead": frame == 1, "colli": true }),
        ));
    }
    for masked in [false, true] {
        c.push((
            "frame_display",
            json!({ "masked": masked, "split": 0, "screen": 0,
            "rot": [1, 2, 3], "pos": pos, "tex": "t", "model": "m" }),
        ));
    }
    for area in 0..3 {
        c.push((
            "water",
            json!({ "area": area, "cam": pos, "player": true, "status": 2, "effect": true }),
        ));
    }
    c
}

#[test]
fn routines_run_on_both_modules_alike() {
    let rels = rels();
    let mut first: Option<Vec<Value>> = None;
    for rel in &rels {
        let got: Vec<Value> = cases()
            .iter()
            .map(|(r, i)| aquatic_mine::replay(rel, r, i).unwrap_or_else(|e| panic!("{r}: {e}")))
            .collect();
        match &first {
            Some(f) => assert_eq!(f, &got),
            None => first = Some(got),
        }
    }
}

#[test]
fn stage_program_reads_native_values() {
    for rel in rels() {
        let am = AquaticMine::read(&rel).unwrap();
        let setup = aquatic_mine::replay(&rel, "setup", &cases()[0].1).unwrap();
        let calls = setup["calls"].as_array().unwrap();
        // fn_7_324(0, 1270) runs while .bss+0x10 is empty.
        assert!(calls.contains(&json!({ "module": "s1+0x324", "args": [0, 1270] })));
        assert_eq!(setup["state"]["task"], json!("s1+0x7e6c"));
        // Steam behaviour: slot +0x14 is never installed.
        assert_eq!(setup["state"]["slots"][5], json!(0));
        assert_eq!(setup["state"]["slots"][1], json!(1));
        assert_eq!(setup["state"]["slots"][0], json!("s1+0x36618"));
        // rand probability 0.3 at scale 1/32768: 9831/32768 >= 0.3 stops after one rand.
        let t = aquatic_mine::replay(&rel, "task_exec", &cases()[5].1).unwrap();
        assert_eq!(t["calls"].as_array().unwrap().len(), 1);
        let t = aquatic_mine::replay(&rel, "task_exec", &cases()[4].1).unwrap();
        let last = &t["calls"][4]["args"][4];
        let p: Vec<f32> = last
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f32::from_bits(v.as_u64().unwrap() as u32))
            .collect();
        // x: 10 + 300*(32767/32768 - 0.5), y: -5 + 200*(0 - 0.5), z: 200 + 300*(0.5 - 0.5)
        assert!((p[0] - 159.99084).abs() < 1e-3);
        assert_eq!(p[1], -105.0);
        assert_eq!(p[2], 200.0);
        let boxes = aquatic_mine::water_boxes(&am).unwrap();
        assert_eq!(boxes.len(), 16);
        assert!(boxes.iter().all(|b| (0..3).contains(&b.area)));
        // Inside the first box the level is its centre height.
        let b = &boxes[0];
        let inside = [b.centre[0].to_bits(), 0, b.centre[2].to_bits()];
        assert_eq!(
            aquatic_mine::water_level(&am, b.area, inside).unwrap(),
            boxes
                .iter()
                .find(|x| x.area == b.area && x.half[0] > 0.0)
                .map_or(-100000.0, |_| b.centre[1])
        );
        assert_eq!(
            aquatic_mine::water_level(&am, 9, inside).unwrap(),
            -100000.0
        );
    }
}

#[test]
fn steam_code_is_gc_code_without_the_slot_hook() {
    let rels = rels();
    let Some(steam) = rels.iter().find(|r| r.len() == aquatic_mine::STEAM_REL_LEN) else {
        return;
    };
    let am = AquaticMine::read(steam).unwrap();
    assert!(am.is_steam());
    assert_eq!(am.code(0x7c88).unwrap(), 0x7c88 - 0x4c);
    assert_eq!(am.code(0x7900).unwrap(), 0x7900 - 0x24);
    assert_eq!(am.code(0x78c0).unwrap(), 0x78c0);
    assert!(am.code(0x78c4).is_err());
    assert_eq!(am.gc_code(0x7c88 - 0x4c), 0x7c88);
}

#[test]
fn rejects_other_modules() {
    assert!(AquaticMine::read(&[0; 16]).is_err());
    assert!(AquaticMine::read(&vec![0; aquatic_mine::GC_REL_LEN]).is_err());
}
