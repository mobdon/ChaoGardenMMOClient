//! Plain checks of the Metal Harbor stage objects ported without an oracle
//! (mob's rule): every routine decodes its constants from stg10D.rel and
//! gives the expected shape of result for a few inputs, and the Steam REL
//! (which the port follows wherever the copies differ) gives the same
//! results as the disc one. Needs the GameCube disc REL (SA2_GC_REL_DIR)
//! and/or the Steam copy (SA2_GAME_PATH); without them the test returns
//! early.
use sa2_asset_tools::metal_harbor::replay;
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(game) = std::env::var_os("SA2_GAME_PATH") {
        out.push(std::fs::read(PathBuf::from(game).join("resource/gd_PC/stg10D.rel")).unwrap());
    }
    if let Some(dir) = std::env::var_os("SA2_GC_REL_DIR") {
        out.push(std::fs::read(PathBuf::from(dir).join("stg10D.rel")).unwrap());
    }
    out
}

fn calls(v: &Value) -> Vec<String> {
    v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c.get("engine")
                .or_else(|| c.get("module"))
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

/// Every case, with a check on its result.
fn cases() -> Vec<(&'static str, Value, fn(&Value))> {
    let one = 1f32.to_bits();
    let mut out: Vec<(&'static str, Value, fn(&Value))> = Vec::new();
    for loader in [0x2414, 0x2d4c, 0x36ac, 0x9a00] {
        out.push((
            "container_load",
            json!({ "loader": loader, "rangeOut": false, "alloc": true,
                "rot": [0, 0x4000, 0], "pos": [one, one, one], "child": true }),
            |v| {
                assert_eq!(v["state"]["dyn"][6], json!(0x4000));
                assert_eq!(calls(v).len(), 4);
            },
        ));
    }
    for exec in [0x26d4, 0x300c, 0x396c, 0x9cc0] {
        for mode in [0, 2] {
            for timer in [0, 31] {
                out.push((
                    "container_exec",
                    json!({ "exec": exec, "rangeOut": false, "mode": mode, "flags": 0,
                        "timer": timer, "rot": 0, "range": 100f32.to_bits(),
                        "accel": one, "speed": 200f32.to_bits(), "off": [0, 0, 0],
                        "child": true, "obj": true, "within": true }),
                    |v| {
                        assert_eq!(v["state"]["flags"], json!(0x100));
                        let off = v["state"]["off"].as_array().unwrap();
                        assert!(off.iter().any(|x| x != &json!(0)));
                    },
                ));
            }
        }
    }
    for display in [0x2cb8, 0x35f0, 0x3df4, 0xa250] {
        out.push((
            "container_display",
            json!({ "display": display, "rot": 0 }),
            |v| assert!(calls(v).len() >= 8),
        ));
    }
    for dest in [0x2670, 0x2fa8, 0x3908, 0x9c5c] {
        out.push((
            "container_dest",
            json!({ "dest": dest, "obj": true }),
            |v| assert_eq!(calls(v).len(), 2),
        ));
    }
    for loader in [0x8fec, 0x93f8, 0x97dc] {
        out.push((
            "slider_load",
            json!({ "loader": loader, "rangeOut": false, "alloc": true,
                "rot": 0x4000, "pos": [one, one, one] }),
            |v| assert_eq!(calls(v).len(), 9),
        ));
    }
    for exec in [0x8f40, 0x934c, 0x9730] {
        out.push((
            "slider_exec",
            json!({ "exec": exec, "rangeOut": false, "dynSet": true, "obj": true,
                "within": false, "flags": 0x101 }),
            |v| assert_eq!(v["state"]["flags"], json!(1)),
        ));
    }
    for display in [0x8de8, 0x9210, 0x961c] {
        out.push((
            "slider_display",
            json!({ "display": display, "rot": 0, "clock": 30 }),
            |v| assert!(calls(v).len() >= 7),
        ));
    }
    for dest in [0x8ed8, 0x92d4, 0x96c8] {
        out.push((
            "slider_dest",
            json!({ "dest": dest, "dynSet": true, "obj": false }),
            |v| assert_eq!(calls(v).len(), 1),
        ));
    }
    out
}

#[test]
fn stage_objects() {
    let rels = rels();
    for (routine, input, check) in cases() {
        let mut first: Option<Value> = None;
        for rel in &rels {
            let v =
                replay(rel, routine, &input).unwrap_or_else(|e| panic!("{routine} {input}: {e}"));
            check(&v);
            match &first {
                Some(f) => assert_eq!(f, &v, "Steam and disc differ: {routine} {input}"),
                None => first = Some(v),
            }
        }
    }
}
