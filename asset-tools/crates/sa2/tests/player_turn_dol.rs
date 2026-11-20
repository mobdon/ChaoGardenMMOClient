//! `player_turn` against main.dol (tests/dol_ppc.py).
use sa2_asset_tools::{ninja_matrix::MatrixStack, player_turn};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_turn_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in doc["angles"].as_array().unwrap() {
        let (a, b, step) = (i(&c[0]), i(&c[1]), i(&c[2]));
        assert_eq!(player_turn::sub_angle(a, b), i(&c[3]), "SubAngle {c}");
        assert_eq!(player_turn::diff_angle(a, b), i(&c[4]), "DiffAngle {c}");
        assert_eq!(
            player_turn::adjust_angle(a, b, step),
            i(&c[5]),
            "AdjustAngle {c}"
        );
    }
    let mut stack = MatrixStack::new();
    for (n, c) in doc["turns"].as_array().unwrap().iter().enumerate() {
        let mut angles = [0, 1, 2].map(|k| i(&c["angles"][k]));
        let mut speed = [0, 1, 2].map(|k| f(&c["speed"][k]));
        player_turn::turn_toward(
            &mut stack,
            &mut angles,
            &mut speed,
            f(&c["dash"]),
            f(&c["up"]),
            i(&c["target"]),
        );
        assert_eq!(
            angles,
            [0, 1, 2].map(|k| i(&c["outAngles"][k])),
            "turn {n}: {c}"
        );
        assert_eq!(
            speed.map(f32::to_bits),
            [0, 1, 2].map(|k| c["outSpeed"][k].as_u64().unwrap() as u32),
            "turn {n}: {c}"
        );
        assert_eq!(stack.depth(), 0);
    }
    eprintln!("3,000 angle-helper cases and 1,500 fn_800473B4 turns matched main.dol");
}
