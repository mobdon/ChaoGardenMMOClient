//! Compare `player_motion::motion_step` with main.dol's fn_8003D8D0
//! (tests/player_motion_ppc.py).
use sa2_asset_tools::player_motion::{
    motion_settle, motion_step, Action, BlendEvent, MotionTables, MotionWork,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}

#[test]
#[ignore = "requires SA2_MAIN_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn motion_step_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_motion_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut modes = [0u32; 17];
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let frames: Vec<u32> = case["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| i(x) as u32)
            .collect();
        let keys: Vec<u32> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| i(x) as u32)
            .collect();
        let mut actions: Vec<Action> = case["acts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| Action {
                motion: i(&a[0]) as i16,
                model: i(&a[1]) as i16,
                mode: i(&a[2]) as i16,
                next: i(&a[3]) as i16,
                blend: f(&a[4]),
                speed: f(&a[5]),
            })
            .collect();
        actions.resize(0x100, Action::default());
        let low = actions[..10].to_vec();
        actions.extend(low);
        let fr = |m: i16| frames[m as usize];
        let mk = |m: i16| keys[m as usize];
        let t = MotionTables {
            actions: &actions,
            frames: &fr,
            model_key: &mk,
        };
        let w0 = &case["w"];
        let mut w = MotionWork {
            mode: i(&w0["mode"]) as i16,
            request: i(&w0["req"]) as i16,
            action: i(&w0["act"]) as i16,
            last_action: i(&w0["last"]) as i16,
            next: i(&w0["next"]) as i16,
            timer: i(&w0["timer"]) as i16,
            flags: i(&w0["flags"]) as i16,
            frame: f(&w0["frame"]),
            last_frame: f(&w0["last_frame"]),
            start_frame: f(&w0["start"]),
            scale_a: f(&w0["sa"]),
            scale_b: f(&w0["sb"]),
            has_blend: w0["buf"].as_bool().unwrap(),
        };
        modes[(w.mode as usize).min(16)] += 1;
        let ev = if case["fn"].as_u64() == Some(0x8003_D7D4) {
            motion_settle(&mut w, &t)
        } else {
            motion_step(&mut w, &t)
        };
        let mut log = vec![];
        for e in ev {
            match e {
                BlendEvent::Create => {
                    log.push(json!(["malloc", 0xC]));
                    log.push(json!(["calloc", 0x6F, 0x18]));
                }
                BlendEvent::Blend { from, to } => log.push(json!(["blend", from, to])),
                BlendEvent::FreeKeys { request } => log.push(json!(["free", request])),
                BlendEvent::Destroy => {
                    log.push(json!(["syfree", 0x8114_0100u32]));
                    log.push(json!(["syfree", 0x8114_0000u32]));
                }
            }
        }
        let got = json!([
            w.mode,
            w.request,
            w.action,
            w.last_action,
            w.next,
            w.timer,
            w.flags,
            w.frame.to_bits(),
            w.last_frame.to_bits(),
            w.start_frame.to_bits()
        ]);
        assert_eq!(got, case["out"], "case {ci}: {case}");
        assert_eq!(
            w.has_blend,
            case["buf_after"].as_bool().unwrap(),
            "case {ci}"
        );
        assert_eq!(Value::from(log), case["log"], "case {ci}: {case}");
    }
    eprintln!("{modes:?}");
}
