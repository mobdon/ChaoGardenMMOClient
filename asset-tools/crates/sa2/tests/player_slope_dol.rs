//! `ninja_vector::{scalor, unit_vector}` and `player_slope` against main.dol.
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    ninja_vector,
    player_analog::AnalogContext,
    player_slope::{self, SlopePlayer},
    player_turn::TurnPlayer,
};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f(&v[k]))
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_slope_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in doc["vectors"].as_array().unwrap() {
        let v = v3(&c[0]);
        assert_eq!(ninja_vector::scalor(v).to_bits(), u(&c[1]), "njScalor {c}");
        let (n, ret) = ninja_vector::unit_vector(v);
        assert_eq!(
            n.map(f32::to_bits),
            [0, 1, 2].map(|k| u(&c[2][k])),
            "njUnitVector {c}"
        );
        assert_eq!(ret.to_bits(), u(&c[3]), "njUnitVector return {c}");
    }
    let mut stack = MatrixStack::new();
    let mut moved = 0;
    for (n, c) in doc["cases"].as_array().unwrap().iter().enumerate() {
        let mut p = SlopePlayer {
            status: i(&c["status"]) as i16,
            angles: [0, 1, 2].map(|k| i(&c["angles"][k])),
            nocon: i(&c["nocon"]) as i16,
            surface: u(&c["surface"]),
            speed: v3(&c["speed"]),
            jog_speed: f(&c["jog"]),
            motion: v3(&c["motion"]),
            aim: i(&c["aim"]),
        };
        let before = (p.angles, p.aim);
        let gravity = v3(&c["gravity"]);
        if c["fn"].as_u64() == Some(0x80047CBC) {
            player_slope::slope_turn_strong(&mut stack, &mut p, gravity);
        } else {
            let turn = TurnPlayer {
                status: p.status,
                nocon: p.nocon,
                confuse: i(&c["confuse"]) as i16,
                up: f(&c["up"]),
                scale: f(&c["scale"]),
            };
            let analog = AnalogContext {
                magnitude: f(&c["magnitude"]),
                ..Default::default()
            };
            player_slope::slope_turn(
                &mut stack,
                &mut p,
                gravity,
                i(&c["stage"]) as i16,
                turn,
                analog,
            );
        }
        assert_eq!(
            p.angles,
            [0, 1, 2].map(|k| i(&c["outAngles"][k])),
            "case {n}: {c}"
        );
        assert_eq!(
            p.speed.map(f32::to_bits),
            [0, 1, 2].map(|k| u(&c["outSpeed"][k])),
            "case {n}: {c}"
        );
        assert_eq!(p.aim, i(&c["outAim"]), "case {n}: {c}");
        assert_eq!(stack.depth(), 0);
        moved += (before != (p.angles, p.aim)) as usize;
    }
    eprintln!("1,500 njScalor/njUnitVector cases and 2,400 slope turns ({moved} changed) matched main.dol");
}
