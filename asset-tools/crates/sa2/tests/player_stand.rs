//! Compare `player_stand` with main.dol's fn_80054D04 and fn_8005118C
//! (tests/player_stand_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    player_stand::{stand_check, StandGame, StandWork, VoiceTables},
    walker_exec::EngineCall,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f(&v[k]))
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn stand_check_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_stand_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let any: Vec<(i16, i16, i16)> = c["any"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (i(&e[0]) as i16, i(&e[1]) as i16, i(&e[2]) as i16))
        .collect();
    let by: Vec<(i8, i8, i16, i16)> = c["by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                i(&e[0]) as i8,
                i(&e[1]) as i8,
                i(&e[2]) as i16,
                i(&e[3]) as i16,
            )
        })
        .collect();
    let tables = VoiceTables {
        any_stage: &any,
        by_stage: &by,
    };
    let mut stack = MatrixStack::new();
    let mut voices = 0;
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let w0 = &case["w"];
        let g0 = &case["g"];
        let mut w = StandWork {
            pnum: i(&w0["pnum"]) as i8,
            character: i(&w0["c"]) as i8,
            voice_char: i(&w0["vc"]) as i8,
            idle: i(&w0["idle"]) as i16,
            f44: 7.0,
            normal: v3(&case["n"]),
            attr: i(&w0["attr"]) as u32,
            action: i(&w0["action"]) as i16,
            h17c: 5,
        };
        let game = StandGame {
            b1e: i(&g0["b1e"]) as i8,
            b39: i(&g0["b39"]) as i8,
            frame: i(&g0["frame"]) as u32,
            stage: i(&g0["stage"]) as i32,
        };
        let mut rands: Vec<i32> = case["rands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| i(x) as i32)
            .collect();
        rands.reverse();
        let mut log = vec![];
        let hook_val = i(&case["hook"]) != 0;
        let loaded = case["loaded"].as_bool().unwrap();
        let mut hooked = false;
        let mut used = vec![];
        let ang = [0, 1, 2].map(|j| i(&case["ang"][j]) as i32);
        let calls = stand_check(
            &mut stack,
            &mut w,
            ang,
            i(&case["aim"]) as i32,
            v3(&case["grav"]),
            &game,
            &tables,
            &mut || {
                let v = rands.pop().unwrap();
                used.push(v);
                v
            },
            &mut || {
                if !loaded {
                    return false;
                }
                hooked = true;
                hook_val
            },
        );
        if hooked {
            log.push(json!(["hook"]));
        }
        for v in used {
            log.push(json!(["rand", v]));
        }
        for call in calls {
            let EngineCall::PlayVoice { id } = call else {
                panic!("{call:?}")
            };
            voices += 1;
            log.push(json!(["voice", id]));
        }
        let got = json!([w.idle, w.f44.to_bits(), w.action, w.h17c]);
        assert_eq!(got, case["out"], "case {ci}: {case}");
        assert_eq!(Value::from(log), case["log"], "case {ci}: {case}");
    }
    assert!(voices > 20, "{voices}");
}
