//! Run `dry_lagoon`'s routines on the GameCube disc stg18D.rel and the Steam
//! copy (SA2_GC_REL_DIR, SA2_GAME_PATH). Every engine call is checked against
//! the GC module's own relocation, and both copies must give the same result.
//! Without the files the tests skip.
use sa2_asset_tools::dry_lagoon::{self, region, DryLagoon};
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("SA2_GC_REL_DIR") {
        out.extend(std::fs::read(PathBuf::from(dir).join("stg18D.rel")));
    }
    if let Some(game) = std::env::var_os("SA2_GAME_PATH") {
        out.extend(std::fs::read(
            PathBuf::from(game).join("resource/gd_PC/stg18D.rel"),
        ));
    }
    if out.is_empty() {
        eprintln!("skipped: set SA2_GC_REL_DIR and/or SA2_GAME_PATH");
    }
    out
}

fn cases() -> Vec<(&'static str, Value)> {
    let cam = |x: f32, y: f32, z: f32| json!({ "pos": [x, y, z], "rot_x": 0x1000 });
    let mut v = vec![
        (
            "setup",
            json!({ "handle": 7, "players": 0, "task": true, "bg": 0,
                "slots": [0, 5, 0, 0, 0, 0, 0, 0, 0, 0], "water": 0, "sound": 9 }),
        ),
        (
            "setup",
            json!({ "handle": 7, "players": 1, "task": false, "bg": 3,
                "slots": [1, 1, 1, 1, 1, 1, 1, 1, 1, 1], "water": 4, "sound": 9 }),
        ),
        (
            "teardown",
            json!({ "slots": ["ours", 1, "ours", "ours", "ours", 0, 0, "ours", 0, "ours"],
                "sound": 2, "handle": 3, "water": 4 }),
        ),
        (
            "teardown",
            json!({ "slots": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "sound": 0, "handle": 0, "water": 0 }),
        ),
        ("exec", json!({ "task": true, "bg2": 0 })),
        ("exec", json!({ "task": false, "bg2": 1 })),
        ("exec_after", json!({})),
    ];
    for (x, y, z) in [
        (0.0, 0.0, 0.0),
        (700.0, 0.0, 0.0),
        (-300.0, 50.0, 600.0),
        (800.0, 300.0, 600.0),
        (100.0, 300.0, 600.0),
        (200.0, 300.0, 1300.0),
        (200.0, 100.0, 1300.0),
        (200.0, 300.0, 1000.0),
    ] {
        v.push(("bg_clip", json!({ "cam": [x, y, z] })));
        for clock in [0u32, 90, 91] {
            v.push((
                "ambient",
                json!({ "cam": [x, y, z], "player": [0.0, 0.0, 1000.0],
                    "clock": clock, "rand": 3000 }),
            ));
        }
        v.push((
            "ambient",
            json!({ "cam": [x, y, z], "player": [0.0, 0.0, 0.0], "clock": 5 }),
        ));
        for (flags, py) in [(2u32, 0.0f32), (0x400, 150.0), (0x20_0000, 0.0), (0, 0.0)] {
            v.push((
                "display",
                json!({ "cam": cam(x, y, z), "player": [200.0, py, 1000.0], "water": true,
                    "flags": flags, "clock": 37, "tan": 0.5, "point_y": -10.0 }),
            ));
        }
    }
    v.push((
        "surface_visible",
        json!({ "cam": cam(0.0, 200.0, 0.0), "level": 180.0, "tan": 0.5, "point_y": 0.0 }),
    ));
    for (twp, task, block, entries) in [
        (false, false, false, false),
        (true, false, false, false),
        (true, true, false, false),
        (true, true, true, false),
        (true, true, true, true),
    ] {
        v.push((
            "water_spawn",
            json!({ "player": 0, "count": 3, "out": true, "twp": twp, "task": task,
                "block": block, "entries": entries }),
        ));
    }
    v
}

#[test]
fn routines_agree_on_both_copies() {
    let rels = rels();
    let mut first: Option<Vec<Value>> = None;
    for rel in &rels {
        let got: Vec<Value> = cases()
            .iter()
            .map(|(r, i)| {
                dry_lagoon::replay(rel, r, i).unwrap_or_else(|e| panic!("{r} {i}: {e:?}"))
            })
            .collect();
        match &first {
            None => first = Some(got),
            Some(f) => assert_eq!(f, &got),
        }
    }
}

#[test]
fn setup_follows_steam_and_regions_are_native() {
    for rel in rels() {
        let dl = DryLagoon::read(&rel).unwrap();
        let out = dry_lagoon::replay(
            &rel,
            "setup",
            &json!({ "handle": 1, "players": 0, "task": true, "bg": 0,
                "slots": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "water": 0, "sound": 2 }),
        )
        .unwrap();
        // Slot +0x14 (index 5) stays as it was: Steam has no install for it.
        assert_eq!(out["state"]["slots"][5], 0);
        assert_eq!(out["state"]["slots"][9], "s1+0x12d60");
        assert_eq!(out["state"]["game+0x3b"], 1);
        // The two music tracks come from the module's strings.
        assert_eq!(
            dry_lagoon::camera_region(&dl, [0.0, 0.0, 0.0]).unwrap(),
            region::LAGOON
        );
        assert_eq!(
            dry_lagoon::camera_region(&dl, [700.0, 0.0, 0.0]).unwrap(),
            region::NORTH_EAST
        );
        assert_eq!(
            dry_lagoon::camera_region(&dl, [-300.0, 0.0, 600.0]).unwrap(),
            region::WEST
        );
        assert_eq!(dry_lagoon::water_level(&dl, [0.0; 3]).unwrap(), 0.0);
        assert!(dry_lagoon::store_region(&dl, 2, [0.0; 3]).is_err());
    }
}

#[test]
fn rejects_other_modules() {
    assert!(DryLagoon::read(&[0; 16]).is_err());
}
