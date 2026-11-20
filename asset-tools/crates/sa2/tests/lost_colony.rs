//! Lost Colony (stg27D) stage program: decoded values and routines on the
//! GameCube disc REL (SA2_GC_REL_DIR) and the installed Steam REL
//! (SA2_GAME_PATH), which must give identical results. Skips without them.
use sa2_asset_tools::lost_colony::{self, LostColony};
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(d) = std::env::var_os("SA2_GC_REL_DIR") {
        out.push(std::fs::read(PathBuf::from(d).join("stg27D.rel")).unwrap());
    }
    if let Some(g) = std::env::var_os("SA2_GAME_PATH") {
        out.push(std::fs::read(PathBuf::from(g).join("resource/gd_PC/stg27D.rel")).unwrap());
    }
    out
}

fn cases() -> Vec<(&'static str, Value)> {
    let one = 1.0f32.to_bits();
    vec![
        (
            "setup",
            json!({"handle": 7, "task": true, "slots": [0, 5, 0]}),
        ),
        (
            "teardown",
            json!({"slots": ["ours", 5, "ours"], "handle": 7}),
        ),
        (
            "exec",
            json!({"level": 0, "target": 1.5f32.to_bits(), "pause": [0, 0],
            "lights": 8, "bias": 0.25f32.to_bits(), "count": 3}),
        ),
        (
            "exec",
            json!({"level": one, "target": 0, "pause": [1, 0],
            "lights": 0, "bias": 0, "count": 0}),
        ),
        (
            "blend",
            json!({"a": "s6+0x94", "b": "s6+0x2a4", "weight": one}),
        ),
        ("task_start", json!(null)),
        (
            "task_exec",
            json!({"players": [true, true], "pending": [1, 0], "mode": [1, 3],
            "saved": [0, 2], "latched": [0, 1], "timer": 10}),
        ),
        ("task_dest", json!({"own": true})),
        (
            "display",
            json!({"screen": 0, "player": 0x4000, "pos": [0, 0, 0],
            "dir": [0, 0, 0], "point": [0, 0, 0], "cam": [0, 0, 0], "clip": false,
            "model": 0, "masks": [0, 0], "depth": vec![100.0f32.to_bits(); 15]}),
        ),
        (
            "display2",
            json!({"screen": 1, "masks": [0, 0], "timer": 27,
            "glow_depth": vec![(-50.0f32).to_bits(); 110],
            "anim_depth": vec![(-50.0f32).to_bits(); 3]}),
        ),
        (
            "hook",
            json!({"pos": [0, 0, 0], "strength": 5.0f32.to_bits(), "cam_dist": 0, "bias": 0,
            "lights": 0, "slots": [null, null], "created": true}),
        ),
        (
            "light_exec",
            json!({"slot": 3, "shown": 0, "level": 0.05f32.to_bits(), "lights": 0}),
        ),
        ("light_dest", json!({"slot": 3, "lights": 0x18})),
        (
            "light_display",
            json!({"slot": 4, "shown": 2.0f32.to_bits(), "pos": [0, 0, 0]}),
        ),
    ]
}

#[test]
fn routines_decode_and_steam_matches_disc() {
    let rels = rels();
    if rels.is_empty() {
        eprintln!("skipped: set SA2_GC_REL_DIR and/or SA2_GAME_PATH");
        return;
    }
    let mut first: Option<Vec<Value>> = None;
    for rel in &rels {
        let got: Vec<Value> = cases()
            .iter()
            .map(|(r, i)| lost_colony::replay(rel, r, i).unwrap_or_else(|e| panic!("{r}: {e}")))
            .collect();
        // setup: 37 calls with a task, slots +0/+0x28 filled, hook installed.
        assert_eq!(got[0]["calls"].as_array().unwrap().len(), 37);
        assert_eq!(got[0]["state"]["hook"], "s1+0x6ae0");
        assert_eq!(
            got[0]["state"]["slots"],
            json!(["s1+0x3513c", 5, "s1+0xe668"])
        );
        assert_eq!(got[1]["state"]["slots"], json!([0, 5, 0]));
        // exec: level = 0 + 0.38 * 1.5; target decays by 0.96, adds bias and 0.8 * count.
        assert_eq!(got[2]["state"]["level"], (0.38f32 * 1.5).to_bits());
        let t = 1.5f32 * 0.96 + (0.25 + 0.8 * 3.0);
        assert_eq!(got[2]["state"]["target"], 2.0f32.min(t).to_bits());
        assert_eq!(got[3]["state"]["blend"]["to"], "s6+0x94");
        // task exec: player 0 switches to mode 3, player 1 restores mode 2.
        assert_eq!(got[6]["state"]["saved"], json!([1, 2]));
        assert_eq!(got[6]["state"]["latched"], json!([1, 0]));
        assert_eq!(got[6]["state"]["timer"], 11);
        // light task falls below 0.1 and destroys itself; dest clears bits 0..=3.
        assert_eq!(got[11]["calls"][0]["engine"], "0x80071af0");
        assert_eq!(got[12]["state"]["lights"], 0x10);
        // hook with no lights spawns into slot 3.
        assert_eq!(got[10]["state"]["spawn"]["slot"], 3);
        if let Some(f) = &first {
            assert_eq!(f, &got, "Steam and disc stg27D differ");
        }
        first = Some(got);
    }
}

#[test]
fn rejects_other_modules() {
    assert!(LostColony::read(&[0; 16]).is_err());
    assert!(LostColony::read(&vec![0; lost_colony::REL_LEN]).is_err());
}
