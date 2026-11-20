//! Compare `walker_act` with main.dol (tests/walker_act_ppc.py) on whole
//! native work areas.
use sa2_asset_tools::{
    native_work::Work,
    walker_act::{
        button_held, button_pressed, go_stand, queue_stand_requests, reset_f48, walker_command,
        ActGlobals, ActSteps,
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
    }
}

fn calls_log(calls: Vec<EngineCall>, log: &mut Vec<Value>) {
    for c in calls {
        log.push(match c {
            EngineCall::PlayVoice { id } => json!(["voice", id]),
            EngineCall::Deferred { addr, arg } => json!(["deferred", addr, arg]),
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
}
