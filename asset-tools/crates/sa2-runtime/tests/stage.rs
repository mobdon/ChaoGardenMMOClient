//! The stage load against main.dol's fn_8001EB80 and the GC stg11D.rel's
//! fn_11_A434 under Unicorn (tests/stage_ppc.py), after the session init:
//! events, tasks and every stage global are compared after each op. Skips
//! without SA2_MAIN_DOL, SA2_STG11D_REL or SA2_GAME_PATH.
mod common;
use common::*;
use sa2_runtime::player::{steam_physics_rows, PlayerHost, Players, TEX_BUFFER_SIZE};
use sa2_runtime::session::{session_init, Session, SessionHost, MESSAGE_LOAD, SAVE_PROBE};
use sa2_runtime::stage::{stage_load, StageHost, StageState, Stg11Layout};
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
    objlist: Vec<(i16, i16, u32)>,
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

impl StageHost for Host {
    fn object_list(&mut self) -> Vec<(i16, i16, u32)> {
        self.objlist.clone()
    }
}

fn stage_json(st: &StageState, s: &Session) -> Value {
    let set = &st.set;
    json!({
        "b81e": st.b81e, "b824": st.b824, "cleared_5870": st.cleared_5870,
        "cleared_5630": st.cleared_5630,
        "set": {
            "ada08": set.ada08, "ad468": set.ad468, "ada0c": set.ada0c, "ada18": set.ada18,
            "ada1c": set.ada1c, "rows": set.rows, "file": set.file, "file_arg": set.file_arg,
            "object_list": set.object_list, "task": set.task.map(|t| t.0), "ada14": set.ada14,
            "entries_cleared": set.entries_cleared,
        },
        "object_table": st.object_table.iter().map(|(v, p)| json!([v, p])).collect::<Vec<_>>(),
        "callbacks": st.callbacks, "table_77d8": st.table_77d8, "adc30": st.adc30,
        "ad718": st.ad718, "death_task": st.death_task.map(|t| t.0), "death_data": st.death_data,
        "bss20": st.bss20.map(|t| t.0), "bss24": st.bss24, "bss4d0": st.bss4d0.map(|t| t.0),
        "slots_5e534": st.slots_5e534.map(|t| t.map(|t| t.0)), "chars": s.players.chars,
    })
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
fn stage_load_matches_main_dol() {
    let Some(game) = std::env::var_os("SA2_GAME_PATH") else {
        eprintln!("skipped: SA2_GAME_PATH is not set (PhysicsArray comes from Steam)");
        return;
    };
    let Some(out) = oracle("stage_ppc.py") else {
        return;
    };
    let exe = std::fs::read(std::path::Path::new(&game).join("sonic2app.exe")).expect("exe");
    let physics = steam_physics_rows(&exe).expect("rows");
    let mut tables = [0; 6];
    for (i, v) in out["tables"].as_array().expect("tables").iter().enumerate() {
        tables[i] = v.as_u64().expect("u32") as u32;
    }
    let lay = &out["layout"];
    let u = |k: &str| lay[k].as_u64().expect("u32") as u32;
    let layout = Stg11Layout {
        text: u("text"),
        rodata: u("rodata"),
        data: u("data"),
        bss: u("bss"),
    };
    let objlist: Vec<(i16, i16, u32)> = out["object_list"]
        .as_array()
        .expect("list")
        .iter()
        .map(|e| {
            (
                e[0].as_i64().expect("i") as i16,
                e[1].as_i64().expect("v") as i16,
                e[2].as_u64().expect("p") as u32,
            )
        })
        .collect();
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
            objlist: objlist.clone(),
        };
        let mut st = StageState::new(layout);
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
                    if let Some(t) = op["objtable"].as_object() {
                        for (i, e) in t {
                            st.object_table[i.parse::<usize>().expect("idx")] = (
                                e[0].as_i64().expect("v") as i16,
                                e[1].as_u64().expect("p") as u32,
                            );
                        }
                    }
                }
                "session" => ret = json!(session_init(&mut rt, &mut s, &mut host)),
                "stage" => stage_load(&mut rt, &mut n, &mut s, &mut st, &mut host),
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
            let _ = ret;
            let got = stage_json(&st, &s);
            for (key, v) in got.as_object().expect("obj") {
                assert_eq!(v, &step["stage"][key], "{name} op {k} {op}: stage {key}");
            }
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared >= 7);
}
