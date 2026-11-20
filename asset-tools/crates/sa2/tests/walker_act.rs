//! Compare `walker_act` with main.dol (tests/walker_act_ppc.py) on whole
//! native work areas.
use sa2_asset_tools::walker_act::walker_jump;
use sa2_asset_tools::{
    native_work::Work,
    ninja_matrix::MatrixStack,
    walker_act::{
        button_held, button_pressed, go_stand, lock_in_range, queue_button_requests,
        queue_event_requests, queue_ground_request, queue_stand_requests, reset_f48,
        walker_command, ActGlobals, ActSteps, LockTarget, LockWorld,
    },
    walker_exec::EngineCall,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn hex(v: &Value) -> Work {
    let s = v.as_str().unwrap();
    Work(
        (0..s.len() / 2)
            .map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap())
            .collect(),
    )
}

struct Log(Vec<Value>);

impl ActSteps for Log {
    fn stand(&mut self, _: &mut Work, _: &mut Work, _: &mut Work) {
        self.0.push(json!(["stand"]));
    }
}

fn globals(g: &Value) -> ActGlobals {
    let arr = |k: &str| {
        let mut a = [0i8; 8];
        for (j, x) in g[k].as_array().unwrap().iter().enumerate() {
            a[j] = x.as_i64().unwrap() as i8;
        }
        a
    };
    ActGlobals {
        stage: g["stage"].as_i64().unwrap() as i32,
        gs_b49: g["b49"].as_i64().unwrap() as i8,
        pressed: arr("pressed"),
        held: arr("held"),
        motion_56: g["m56"].as_bool().unwrap(),
        gs: hex(&g["gs"]),
    }
}

fn calls_log(calls: Vec<EngineCall>, log: &mut Vec<Value>) {
    for c in calls {
        log.push(match c {
            EngineCall::PlayVoice { id } => json!(["voice", id]),
            EngineCall::Deferred { addr, arg } => json!(["deferred", addr, arg]),
            EngineCall::Sound {
                addr,
                id,
                args,
                pos,
            } => json!(["se", addr, id, args, pos.map(|v| v.map(f32::to_bits))]),
            other => json!([format!("{other:?}")]),
        });
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn walker_act_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_act_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let g = globals(&case["g"]);
        let (mut t, mut m, mut p) = (hex(&case["t"]), hex(&case["m"]), hex(&case["p"]));
        let mut steps = Log(vec![]);
        let mut calls = vec![];
        let ret: Option<bool> = match case["fn"].as_str().unwrap() {
            "command" => Some(walker_command(
                &mut t, &mut m, &mut p, &g, &mut steps, &mut calls,
            )),
            "go_stand" => {
                go_stand(&mut t, &mut m, &mut p, &g, &mut steps);
                None
            }
            "pressed" => Some(button_pressed(&t, &p, &g)),
            "held" => Some(button_held(&t, &mut p, &g)),
            "queue" => Some(queue_stand_requests(&t, &mut p)),
            "events" => Some(queue_event_requests(&t, &mut p, &g)),
            "buttons" => Some(queue_button_requests(&t, &mut p)),
            "ground" => Some(queue_ground_request(&t, &mut p)),
            "jump" | "jump41" => {
                let ride = case["g"]["ride"].as_array().map(|_| v3(&case["g"]["ride"]));
                let act = (case["fn"] == "jump41").then_some(0x41);
                walker_jump(&mut t, &mut m, &mut p, ride, act, &mut calls);
                None
            }
            "f48" => {
                reset_f48(&mut p, &g);
                None
            }
            f => panic!("{f}"),
        };
        let mut log = steps.0;
        calls_log(calls, &mut log);
        if let Some(r) = ret {
            assert_eq!(
                r as u64,
                case["ret"].as_u64().unwrap(),
                "case {ci}: {}",
                case["fn"]
            );
        }
        assert_eq!(
            t,
            hex(&case["t_after"]),
            "case {ci}: {} task work",
            case["fn"]
        );
        assert_eq!(
            m,
            hex(&case["m_after"]),
            "case {ci}: {} motion work",
            case["fn"]
        );
        assert_eq!(
            p,
            hex(&case["p_after"]),
            "case {ci}: {} player work",
            case["fn"]
        );
        assert_eq!(Value::from(log), case["log"], "case {ci}: {}", case["fn"]);
    }
    struct World<'a>(&'a Value, Vec<Value>);
    impl LockWorld for World<'_> {
        fn partner(&mut self, _: i8) -> Option<[f32; 3]> {
            (self.0["partner"].as_i64().unwrap() != -1).then(|| v3(&self.0["pq"]))
        }
        fn targets(&mut self, list: usize) -> Vec<LockTarget> {
            self.0["lists"][list]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["valid"].as_bool().unwrap())
                .map(|e| LockTarget {
                    kind: e["kind"].as_u64().unwrap() as u8,
                    center: v3(&e["c"]),
                })
                .collect()
        }
    }
    let mut stack = MatrixStack::new();
    let mut hits = 0;
    for (ci, case) in c["lock"].as_array().unwrap().iter().enumerate() {
        let (t, p) = (hex(&case["t"]), hex(&case["p"]));
        let mut w = World(case, vec![]);
        let r = lock_in_range(&mut stack, &t, &p, &mut w);
        hits += r as u32;
        assert_eq!(
            r as u64,
            case["ret"].as_u64().unwrap(),
            "lock case {ci}: {case}"
        );
    }
    assert!(hits > 80, "{hits}");
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f32::from_bits(v[k].as_u64().unwrap() as u32))
}
