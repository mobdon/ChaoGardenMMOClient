//! The session init against main.dol's fn_80021828 under Unicorn
//! (tests/session_ppc.py): events, tasks, player and camera state and the
//! session globals are compared after every op. Skips without SA2_MAIN_DOL.
mod common;
use common::*;
use sa2_runtime::player::{steam_physics_rows, PlayerHost, Players, TEX_BUFFER_SIZE};
use sa2_runtime::session::{session_init, Session, SessionHost, MESSAGE_LOAD, SAVE_PROBE};
use sa2_runtime::{Runtime, TaskId};
use serde_json::{json, Value};

struct Host {
    physics: Vec<[u8; 0x84]>,
    upgrades: [u8; 0x1D],
    tex_size: u32,
    coll: u32,
    value: u32,
    tables: [u32; 6],
    w420: i32,
    probe: i32,
}

impl PlayerHost for Host {
    fn physics_row(&mut self, row: i32) -> [u8; 0x84] {
        self.physics[row as usize]
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

impl SessionHost for Host {
    fn message_tables(&mut self) -> [u32; 6] {
        self.tables
    }
    fn word_803ad420(&mut self) -> i32 {
        self.w420
    }
    fn value(&mut self, addr: u32, _args: &[u32]) -> u32 {
        match addr {
            MESSAGE_LOAD => 0,
            SAVE_PROBE => self.probe as u32,
            _ => 0,
        }
    }
}

fn session_json(s: &Session) -> Value {
    json!({
        "mode": s.mode, "exit_request": s.exit_request, "b818": s.b818,
        "pause_cursor": s.pause_cursor, "pause_flag": s.pause_flag, "h830": s.h830,
        "messages": s.messages, "rand_next": s.rand_next, "buffer_cleared": s.buffer_cleared,
        "g": s.players.g.iter().map(|b| format!("{b:02x}")).collect::<String>(),
    })
}

#[test]
fn session_init_matches_main_dol() {
    let Some(game) = std::env::var_os("SA2_GAME_PATH") else {
        eprintln!("skipped: SA2_GAME_PATH is not set (PhysicsArray comes from Steam)");
        return;
    };
    let Some(out) = oracle("session_ppc.py") else {
        return;
    };
    let exe = std::fs::read(std::path::Path::new(&game).join("sonic2app.exe")).expect("exe");
    let physics = steam_physics_rows(&exe).expect("rows");
    let mut tables = [0; 6];
    for (i, v) in out["tables"].as_array().expect("tables").iter().enumerate() {
        tables[i] = v.as_u64().expect("u32") as u32;
    }
    let mut compared = 0;
    for sc in out["scenarios"].as_array().expect("scenarios") {
        let name = sc["name"].as_str().expect("name");
        let mut n = harness(sc);
        let mut rt = Runtime::new();
        rt.set_flags = vec![3; 4];
        let mut s = Session::new();
        s.players = Players {
            g: vec![0; 0xF0],
            ..Players::default()
        };
        let mut host = Host {
            physics: physics.clone(),
            upgrades: [0; 0x1D],
            tex_size: out["tex_size"].as_u64().expect("size") as u32,
            coll: out["coll"].as_u64().expect("coll") as u32,
            value: 0,
            tables,
            w420: 0,
            probe: 0,
        };
        for (k, (op, step)) in sc["ops"]
            .as_array()
            .expect("ops")
            .iter()
            .zip(sc["steps"].as_array().expect("steps"))
            .enumerate()
        {
            rt.events.clear();
            let mut ret = Value::Null;
            match op["op"].as_str().expect("op") {
                "state" => {
                    if let Some(g) = op["g"].as_object() {
                        for (o, v) in g {
                            s.players.g[o.parse::<usize>().expect("off")] =
                                v.as_u64().expect("b") as u8;
                        }
                    }
                    if let Some(v) = op["stage"].as_i64() {
                        s.players.stage = v as i16;
                    }
                    if let Some(c) = op["chars"].as_array() {
                        for (i, v) in c.iter().enumerate() {
                            s.players.chars[i] = v.as_i64().expect("char") as i32;
                        }
                    }
                    if let Some(u) = op["upgrades"].as_array() {
                        for (i, v) in u.iter().enumerate() {
                            host.upgrades[i] = v.as_u64().expect("b") as u8;
                        }
                    }
                    if let Some(v) = op["w420"].as_i64() {
                        host.w420 = v as i32;
                    }
                    if let Some(v) = op["probe"].as_i64() {
                        host.probe = v as i32;
                    }
                }
                "session" => ret = json!(session_init(&mut rt, &mut s, &mut host)),
                _ => apply(&mut rt, &mut n, op),
            }
            let events: Vec<Value> = rt.events.iter().map(event_json).collect();
            assert_eq!(
                Value::Array(events),
                step["events"],
                "{name} op {k} {op}: events"
            );
            assert_eq!(state_json(&rt), step["state"], "{name} op {k} {op}: tasks");
            assert_eq!(
                session_json(&s),
                step["session"],
                "{name} op {k} {op}: session"
            );
            let cam = &s.camera;
            let camera = json!({
                "task": cam.task.map(|t| t.0), "viewports": cam.viewports, "index": cam.index,
                "adb1c": cam.adb1c, "adb00": cam.adb00, "adb04": cam.adb04, "adafc": cam.adafc,
                "table": cam.table, "blocks": cam.blocks,
            });
            assert_eq!(camera, step["load"]["camera"], "{name} op {k} {op}: camera");
            assert_eq!(
                json!(s.players.allocs),
                step["load"]["allocs"],
                "{name} op {k}: allocs"
            );
            assert_eq!(ret, step["ret"], "{name} op {k} {op}: return");
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared >= 12);
}
