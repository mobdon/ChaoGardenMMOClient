//! The frame step against main.dol's own frame routines under Unicorn
//! (tests/frame_step_ppc.py): every op is replayed, and the events, the task
//! state, the game state and the return value are compared after each one.
//! Skips without SA2_MAIN_DOL.
mod common;
use common::*;
use sa2_runtime::frame::{pause_frame, stage_frame};
use sa2_runtime::{FrameHost, GameState, Runtime, TaskId};
use serde_json::{json, Value};

#[derive(Default)]
struct Host {
    rings: i16,
    pressed: [u32; 2],
    held: [u32; 2],
    copy: u32,
    w53a8: u32,
    w5398: [u32; 2],
    flags: [Option<i16>; 2],
    end: f32,
}

impl FrameHost for Host {
    fn ring_count(&mut self, _player: u32) -> i16 {
        self.rings
    }
    fn pad_pressed(&mut self, slot: usize) -> u32 {
        self.pressed[slot]
    }
    fn pad_held(&mut self, slot: usize) -> u32 {
        self.held[slot]
    }
    fn pad_copy_pressed(&mut self) -> u32 {
        self.copy
    }
    fn word_801e53a8(&mut self) -> u32 {
        self.w53a8
    }
    fn word_801e5398(&mut self, i: usize) -> u32 {
        self.w5398[i]
    }
    fn player_flags(&mut self, i: usize) -> Option<i16> {
        self.flags[i]
    }
    fn frame_end_value(&mut self) -> f32 {
        self.end
    }
    fn sort_queue_active(&mut self) -> bool {
        false
    }
    fn view_z(&mut self, pos: [f32; 3]) -> f32 {
        pos[2]
    }
}

fn i(v: &Value) -> i64 {
    v.as_i64().expect("int")
}

fn set_state(st: &mut GameState, op: &Value) {
    if let Some(g) = op["g"].as_object() {
        for (k, v) in g {
            st.g[k.parse::<usize>().expect("off")] = i(v) as u8;
        }
    }
    if let Some(g) = op["g32"].as_object() {
        for (k, v) in g {
            let o: usize = k.parse().expect("off");
            st.g[o..o + 4].copy_from_slice(&(i(v) as u32).to_be_bytes());
        }
    }
    if let Some(sd) = op["sd"].as_object() {
        for (k, v) in sd {
            let v = i(v);
            match k.as_str() {
                "ticks" => st.ticks = v as i32,
                "tick_index" => st.tick_index = v as i32,
                "slow" => st.slow = v as u8,
                "mode" => st.mode = v as i16,
                "exit_request" => st.exit_request = v as i16,
                "stage" => st.stage = v as i16,
                "overlay" => st.overlay = v as i16,
                "pause_flag" => st.pause_flag = v as i8,
                "pause_cursor" => st.pause_cursor = v as u8,
                "pause_menu" => st.pause_menu = v as u8,
                "pause_timer" => st.pause_timer = v as i16,
                "viewports" => st.viewports = v as i32,
                "camera" => st.camera = v as i32,
                "gx_ae139" => st.gx_ae139 = v as u8,
                other => panic!("unknown field {other}"),
            }
        }
    }
    if let Some(items) = op["pause_items"].as_array() {
        for (k, v) in items.iter().enumerate() {
            st.pause_items[k] = i(v) as u32;
        }
    }
    if let Some(p) = op["player_tasks"].as_object() {
        for (k, v) in p {
            st.player_tasks[k.parse::<usize>().expect("index")] =
                v.as_u64().map(|t| TaskId(t as u32));
        }
    }
}

fn set_host(h: &mut Host, op: &Value) {
    let pair = |v: &Value| [i(&v[0]) as u32, i(&v[1]) as u32];
    for (k, v) in op.as_object().expect("op") {
        match k.as_str() {
            "op" => {}
            "rings" => h.rings = i(v) as i16,
            "pressed" => h.pressed = pair(v),
            "held" => h.held = pair(v),
            "copy" => h.copy = i(v) as u32,
            "w53a8" => h.w53a8 = i(v) as u32,
            "w5398" => h.w5398 = pair(v),
            "flags" => h.flags = [0, 1].map(|n| v[n].as_i64().map(|f| f as i16)),
            "end" => h.end = v.as_f64().expect("float") as f32,
            other => panic!("unknown host field {other}"),
        }
    }
}

fn frame_json(st: &GameState) -> Value {
    json!({
        "ticks": st.ticks, "tick_index": st.tick_index, "slow": st.slow, "mode": st.mode,
        "exit_request": st.exit_request, "stage": st.stage, "overlay": st.overlay,
        "pause_flag": st.pause_flag, "pause_cursor": st.pause_cursor,
        "pause_menu": st.pause_menu, "pause_timer": st.pause_timer,
        "viewports": st.viewports, "camera": st.camera, "gx_ae139": st.gx_ae139,
        "g": st.g.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "clock": st.clock, "pause_items": st.pause_items,
    })
}

#[test]
fn frame_step_matches_main_dol() {
    let Some(out) = oracle("frame_step_ppc.py") else {
        return;
    };
    let mut compared = 0;
    for sc in out["scenarios"].as_array().expect("scenarios") {
        let name = sc["name"].as_str().expect("name");
        let mut n = harness(sc);
        let mut rt = Runtime::new();
        rt.set_flags = vec![3; 4];
        let mut st = GameState::default();
        let mut host = Host::default();
        let ops = sc["ops"].as_array().expect("ops");
        let steps = sc["steps"].as_array().expect("steps");
        for (k, (op, step)) in ops.iter().zip(steps).enumerate() {
            rt.events.clear();
            let mut ret = Value::Null;
            match op["op"].as_str().expect("op") {
                "state" => set_state(&mut st, op),
                "host" => set_host(&mut host, op),
                "frame" => ret = json!(stage_frame(&mut rt, &mut n, &mut st, &mut host)),
                "pause" => ret = json!(pause_frame(&mut rt, &mut n, &mut st, &mut host)),
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
                frame_json(&st),
                step["frame"],
                "{name} op {k} {op}: game state"
            );
            assert_eq!(ret, step["ret"], "{name} op {k} {op}: return");
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared > 60);
}
