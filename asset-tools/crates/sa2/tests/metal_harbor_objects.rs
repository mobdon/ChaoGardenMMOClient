//! Plain checks of the Metal Harbor stage objects ported without an oracle
//! (mob's rule): every routine decodes its constants from stg10D.rel and
//! gives the expected shape of result for a few inputs. Needs the GameCube
//! disc REL (SA2_GC_REL_DIR) and, when set, the Steam copy (SA2_GAME_PATH);
//! without them the test returns early.
use sa2_asset_tools::metal_harbor::replay;
use serde_json::{json, Value};
use std::path::PathBuf;

fn rels() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("SA2_GC_REL_DIR") {
        out.push(std::fs::read(PathBuf::from(dir).join("stg10D.rel")).unwrap());
    }
    if let Some(game) = std::env::var_os("SA2_GAME_PATH") {
        out.push(std::fs::read(PathBuf::from(game).join("resource/gd_PC/stg10D.rel")).unwrap());
    }
    out
}

fn run(rel: &[u8], routine: &str, input: Value) -> Value {
    replay(rel, routine, &input).unwrap_or_else(|e| panic!("{routine} {input}: {e}"))
}

fn engine_calls(v: &Value) -> Vec<String> {
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

#[test]
fn containers() {
    let one = 1f32.to_bits();
    for rel in rels() {
        for loader in [0x2414, 0x2d4c, 0x36ac, 0x9a00] {
            let v = run(
                &rel,
                "container_load",
                json!({ "loader": loader, "rangeOut": false, "alloc": true,
                    "rot": [0, 0x4000, 0], "pos": [one, one, one], "child": true }),
            );
            assert_eq!(v["state"]["dyn"][6], json!(0x4000));
            assert_eq!(engine_calls(&v).len(), 4);
        }
        for exec in [0x26d4, 0x300c, 0x396c, 0x9cc0] {
            for mode in [0, 2] {
                for timer in [0, 31] {
                    let v = run(
                        &rel,
                        "container_exec",
                        json!({ "exec": exec, "rangeOut": false, "mode": mode, "flags": 0,
                            "timer": timer, "rot": 0, "range": 100f32.to_bits(),
                            "accel": one, "speed": 200f32.to_bits(), "off": [0, 0, 0],
                            "child": true, "obj": true, "within": true }),
                    );
                    assert_eq!(v["state"]["flags"], json!(0x100));
                    let off = &v["state"]["off"];
                    assert!(off.as_array().unwrap().iter().any(|x| x != &json!(0)));
                }
            }
        }
        for display in [0x2cb8, 0x35f0, 0x3df4, 0xa250] {
            let v = run(
                &rel,
                "container_display",
                json!({ "display": display, "rot": 0 }),
            );
            assert!(engine_calls(&v).len() >= 8);
        }
        for dest in [0x2670, 0x2fa8, 0x3908, 0x9c5c] {
            run(&rel, "container_dest", json!({ "dest": dest, "obj": true }));
        }
    }
}
