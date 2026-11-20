//! The task system against main.dol's own routines under Unicorn
//! (tests/task_system_ppc.py): every scenario op is replayed and the events
//! (callback calls, frees, engine calls) and the full list state are compared
//! after each one. Skips without SA2_MAIN_DOL.
mod common;
use common::*;
use sa2_runtime::Runtime;
use serde_json::Value;

#[test]
fn task_system_matches_main_dol() {
    let Some(out) = common::oracle("task_system_ppc.py") else {
        return;
    };
    let mut compared = 0;
    for sc in out["scenarios"].as_array().expect("scenarios") {
        let name = sc["name"].as_str().expect("name");
        let mut h = harness(sc);
        let mut rt = Runtime::new();
        rt.set_flags = vec![3; 4];
        let ops = sc["ops"].as_array().expect("ops");
        let steps = sc["steps"].as_array().expect("steps");
        for (i, (op, step)) in ops.iter().zip(steps).enumerate() {
            rt.events.clear();
            apply(&mut rt, &mut h, op);
            let events: Vec<Value> = rt.events.iter().map(event_json).collect();
            assert_eq!(
                Value::Array(events),
                step["events"],
                "{name} op {i} {op}: events"
            );
            assert_eq!(state_json(&rt), step["state"], "{name} op {i} {op}: state");
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared > 40);
}
