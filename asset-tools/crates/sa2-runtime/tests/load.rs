//! Player load and camera task creation against main.dol under Unicorn
//! (tests/load_ppc.py): tasks, events, player work (pointer words as
//! symbols), player tables, collision write and camera globals are compared
//! after every op. Skips without SA2_MAIN_DOL.
mod common;
use common::*;
use sa2_runtime::camera::{camera_init, Camera};
use sa2_runtime::player::{load_players, PlayerHost, Players, Ptr, PLAYER_WORK, TEX_BUFFER_SIZE};
use sa2_runtime::{Runtime, TaskId};
use serde_json::{json, Value};

struct Host {
    physics: Vec<Vec<u8>>,
    upgrades: [u8; 0x1D],
    tex_size: u32,
    coll: u32,
    value: u32,
}

impl PlayerHost for Host {
    fn physics_row(&mut self, row: i32) -> [u8; 0x84] {
        let mut r = [0; 0x84];
        r.copy_from_slice(&self.physics[row as usize]);
        r
    }
    fn upgrades(&mut self) -> [u8; 0x1D] {
        self.upgrades
    }
    fn engine_value(&mut self, addr: u32, _args: &[u32]) -> u32 {
        if addr == TEX_BUFFER_SIZE {
            return self.tex_size;
        }
        self.value += 1;
        0xC000_0000 + self.value - 1
    }
    fn collision(&mut self, task: TaskId) -> u32 {
        self.coll + 0x40 * task.0
    }
}

fn sym(p: Option<Ptr>) -> Value {
    match p {
        None => Value::Null,
        Some(Ptr::Alloc(n, 0)) => json!(format!("a{n}")),
        Some(Ptr::Alloc(n, off)) => json!(format!("a{n}+0x{off:x}")),
        Some(Ptr::Task(t)) => json!(format!("task{}", t.0)),
        Some(Ptr::Twp(t)) => json!(format!("twp{}", t.0)),
        Some(Ptr::Mwp(t)) => json!(format!("mwp{}", t.0)),
    }
}

fn load_state(rt: &Runtime, pl: &Players, cam: &Camera) -> Value {
    let mut works = serde_json::Map::new();
    let mut radius = vec![];
    for t in rt.live() {
        let Some(w) = rt.task(t).and_then(|x| x.w40.as_ref()) else {
            continue;
        };
        if w.len() != PLAYER_WORK {
            continue;
        }
        let words: Vec<Value> = (0..PLAYER_WORK)
            .step_by(4)
            .map(|k| match pl.ptrs.get(&(t.0, k)) {
                Some(p) => sym(Some(*p)),
                None => json!(format!(
                    "{:08x}",
                    u32::from_be_bytes([w[k], w[k + 1], w[k + 2], w[k + 3]])
                )),
            })
            .collect();
        works.insert(t.0.to_string(), Value::Array(words));
        let r = pl
            .collision_radius
            .iter()
            .rev()
            .find(|(id, _)| *id == t)
            .map_or(0, |x| x.1);
        radius.push(json!([t.0, r]));
    }
    json!({
        "pwp": pl.pwp.map(sym), "mwp": pl.mwp.map(sym), "twp": pl.twp.map(sym), "tp": pl.tp.map(sym),
        "works": works, "radius": radius, "allocs": pl.allocs,
        "camera": {
            "task": cam.task.map(|t| t.0), "viewports": cam.viewports, "index": cam.index,
            "adb1c": cam.adb1c, "adb00": cam.adb00, "adb04": cam.adb04, "adafc": cam.adafc,
            "table": cam.table, "blocks": cam.blocks,
        },
    })
}

#[test]
fn player_load_matches_main_dol() {
    let Some(out) = oracle("load_ppc.py") else {
        return;
    };
    let unhex = |s: &str| -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    };
    // The Steam install's PhysicsArray when present (identical to main.dol's, see
    // steam_physics.rs), else the rows the oracle read from main.dol.
    let physics: Vec<Vec<u8>> = match std::env::var_os("SA2_GAME_PATH") {
        Some(game) => {
            let exe =
                std::fs::read(std::path::Path::new(&game).join("sonic2app.exe")).expect("exe");
            eprintln!("physics rows from the Steam install");
            sa2_runtime::player::steam_physics_rows(&exe)
                .expect("rows")
                .iter()
                .map(|r| r.to_vec())
                .collect()
        }
        None => out["physics"]
            .as_array()
            .expect("physics")
            .iter()
            .map(|r| unhex(r.as_str().expect("row")))
            .collect(),
    };
    let mut compared = 0;
    for sc in out["scenarios"].as_array().expect("scenarios") {
        let name = sc["name"].as_str().expect("name");
        let mut n = harness(sc);
        let mut rt = Runtime::new();
        rt.set_flags = vec![3; 4];
        let mut pl = Players {
            g: vec![0; 0xF0],
            ..Players::default()
        };
        let mut cam = Camera::default();
        let mut host = Host {
            physics: physics.clone(),
            upgrades: [0; 0x1D],
            tex_size: out["tex_size"].as_u64().expect("size") as u32,
            coll: out["coll"].as_u64().expect("coll") as u32,
            value: 0,
        };
        let ops = sc["ops"].as_array().expect("ops");
        let steps = sc["steps"].as_array().expect("steps");
        for (k, (op, step)) in ops.iter().zip(steps).enumerate() {
            rt.events.clear();
            match op["op"].as_str().expect("op") {
                "state" => {
                    if let Some(g) = op["g"].as_object() {
                        for (o, v) in g {
                            pl.g[o.parse::<usize>().expect("off")] = v.as_u64().expect("b") as u8;
                        }
                    }
                    if let Some(s) = op["stage"].as_i64() {
                        pl.stage = s as i16;
                    }
                    if let Some(c) = op["chars"].as_array() {
                        for (i, v) in c.iter().enumerate() {
                            pl.chars[i] = v.as_i64().expect("char") as i32;
                        }
                    }
                    if let Some(u) = op["upgrades"].as_array() {
                        for (i, v) in u.iter().enumerate() {
                            host.upgrades[i] = v.as_u64().expect("b") as u8;
                        }
                    }
                    if let Some(a) = op["adb00"].as_u64() {
                        cam.adb00 = a as u32;
                    }
                }
                "players" => load_players(&mut rt, &mut pl, &mut host),
                "camera" => camera_init(
                    &mut rt,
                    &mut cam,
                    pl.stage,
                    op["count"].as_i64().expect("n") as i32,
                ),
                _ => apply(&mut rt, &mut n, op),
            }
            let events: Vec<Value> = rt.events.iter().map(event_json).collect();
            assert_eq!(
                Value::Array(events),
                step["events"],
                "{name} op {k} {op}: events"
            );
            assert_eq!(state_json(&rt), step["state"], "{name} op {k} {op}: tasks");
            let got = load_state(&rt, &pl, &cam);
            for key in [
                "pwp", "mwp", "twp", "tp", "works", "radius", "allocs", "camera",
            ] {
                assert_eq!(got[key], step["load"][key], "{name} op {k} {op}: {key}");
            }
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared >= 15);
}
