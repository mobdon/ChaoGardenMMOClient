//! Shared by the oracle comparisons: the scripted callback host, the task ops
//! and the JSON views of events and task state.
#![allow(dead_code)]
use sa2_runtime::{DisplayEngine, Event, Natives, Runtime, TaskId, WorkKind};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, process::Command};

/// Run an oracle script; None when it skipped (no SA2_MAIN_DOL).
pub fn oracle(script: &str) -> Option<Value> {
    if std::env::var_os("SA2_MAIN_DOL").is_none() {
        eprintln!("skipped: SA2_MAIN_DOL is not set");
        return None;
    }
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join(script),
        )
        .output()
        .expect("python");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    Some(serde_json::from_slice(&o.stdout).expect("json"))
}

/// The scripted callbacks of a scenario.
pub fn harness(sc: &Value) -> Harness {
    let scripts = sc["cbs"]
        .as_object()
        .expect("cbs")
        .iter()
        .map(|(k, v)| {
            (
                k.parse().expect("addr"),
                v.as_array().expect("list").clone(),
            )
        })
        .collect();
    Harness { scripts }
}

pub struct Harness {
    pub scripts: BTreeMap<u32, Vec<Value>>,
}

pub fn u(v: &Value) -> u32 {
    v.as_u64().expect("number") as u32
}

impl Natives for Harness {
    fn run(&mut self, rt: &mut Runtime, addr: u32, task: TaskId) -> bool {
        let actions = self.scripts.get(&addr).cloned().unwrap_or_default();
        for a in actions {
            match a[0].as_str().expect("action") {
                "free_self" => rt.free_task(task),
                "destroy_self" => rt.destroy_task(self, task),
                "freec_self" => rt.free_task_c(self, task),
                "destroy" => rt.destroy_task(self, TaskId(u(&a[1]))),
                "create" => {
                    rt.create_elemental_task(u(&a[1]) as u16, u(&a[2]), u(&a[3]), 0)
                        .expect("create");
                }
                "child" => {
                    rt.create_child_task(u(&a[1]) as u16, u(&a[2]), task)
                        .expect("child");
                }
                "set_exec" => rt.task_mut(task).expect("task").slots[0] = u(&a[1]),
                other => panic!("unknown action {other}"),
            }
        }
        true
    }
}

pub struct Engine {
    pub mode: u8,
    pub cam: i32,
    pub active: bool,
}

impl DisplayEngine for Engine {
    fn screen_mode(&mut self) -> u8 {
        self.mode
    }
    fn camera_index(&mut self) -> i32 {
        self.cam
    }
    fn sort_queue_active(&mut self) -> bool {
        self.active
    }
    fn view_z(&mut self, pos: [f32; 3]) -> f32 {
        pos[2]
    }
}

pub fn setup(rt: &mut Runtime, id: TaskId, op: &Value) {
    let t = rt.task_mut(id).expect("task");
    if let Some(slots) = op["slots"].as_object() {
        for (off, v) in slots {
            let off: u32 = off.parse().expect("offset");
            t.slots[((off - 0x10) / 4) as usize] = u(v);
        }
    }
    if let Some(s) = op["set"].as_u64() {
        t.set_entry = Some(s as usize);
    }
    if let Some(words) = op["twp"].as_object() {
        let twp = t.twp.as_mut().expect("twp");
        for (off, v) in words {
            let off: usize = off.parse().expect("offset");
            twp[off..off + 4].copy_from_slice(&u(v).to_be_bytes());
        }
    }
}

pub fn event_json(e: &Event) -> Value {
    let off = |k: &WorkKind| k.offset();
    match e {
        Event::Call { addr, task } => json!(["call", addr, task.0]),
        Event::Unported { addr, task } => json!(["unported", addr, task.0]),
        Event::FreeWork { task, kind } => json!(["free_work", task.0, off(kind)]),
        Event::FreeTask { task } => json!(["free_task", task.0]),
        Event::Engine { addr, args } => json!(["engine", addr, args]),
        Event::Anomaly(s) => json!(["anomaly", s]),
    }
}

pub fn state_json(rt: &Runtime) -> Value {
    let id = |t: Option<TaskId>| t.map(|t| t.0);
    let tasks: Vec<Value> = rt
        .live()
        .into_iter()
        .map(|t| {
            let x = rt.task(t).expect("live");
            let has = |w: &Option<Vec<u8>>| w.is_some() as u32;
            json!({
                "id": t.0, "next": id(x.next), "prev": id(x.prev),
                "parent": id(x.parent), "child": id(x.child),
                "slots": x.slots.to_vec(),
                "set": x.set_entry,
                "work": [has(&x.twp), has(&x.w38), has(&x.w3c), has(&x.w40)],
                "twp": x.twp.as_ref().map(|w| w.iter().map(|b| format!("{b:02x}")).collect::<String>()),
            })
        })
        .collect();
    json!({
        "btp": rt.btp.iter().map(|&t| id(t)).collect::<Vec<_>>(),
        "makethread": rt.makethread,
        "set_flags": rt.set_flags,
        "tasks": tasks,
    })
}

pub fn apply(rt: &mut Runtime, h: &mut Harness, op: &Value) {
    match op["op"].as_str().expect("op") {
        "create" => {
            let id = rt
                .create_elemental_task(u(&op["im"]) as u16, u(&op["level"]), u(&op["exec"]), 0)
                .expect("create");
            setup(rt, id, op);
        }
        "child" => {
            let id = rt
                .create_child_task(
                    u(&op["im"]) as u16,
                    u(&op["exec"]),
                    TaskId(u(&op["parent"])),
                )
                .expect("child");
            setup(rt, id, op);
        }
        "free" => rt.free_task(TaskId(u(&op["task"]))),
        "destroy" => rt.destroy_task(h, TaskId(u(&op["task"]))),
        "freec" => rt.free_task_c(h, TaskId(u(&op["task"]))),
        "exec" => rt.exec_pass(h),
        "display" => {
            let mut e = Engine {
                mode: u(&op["mode"]) as u8,
                cam: op["cam"].as_i64().expect("cam") as i32,
                active: u(&op["active"]) != 0,
            };
            rt.display_pass(h, &mut e);
        }
        "shadow" => rt.shadow_pass(h),
        "destroy_all" => rt.destroy_all(h),
        "destroy_stage" => rt.destroy_stage_levels(h),
        "clear" => rt.clear_btp(),
        other => panic!("unknown op {other}"),
    }
}
