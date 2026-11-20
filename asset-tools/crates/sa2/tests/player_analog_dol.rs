//! `ninja_vector` and `player_analog` against main.dol (tests/dol_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    ninja_vector,
    player_analog::{self, AnalogContext, AnalogPlayer},
};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|i| f32::from_bits(v[i].as_u64().unwrap() as u32))
}

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_analog_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for (i, c) in doc["vec"].as_array().unwrap().iter().enumerate() {
        let (a, b) = (v3(&c[0]), v3(&c[1]));
        assert_eq!(
            ninja_vector::inner_product(a, b).to_bits(),
            u(&c[3]),
            "inner {i}"
        );
        let cross = ninja_vector::outer_product(a, b).map(f32::to_bits);
        assert_eq!(cross, [0, 1, 2].map(|k| u(&c[4][k])), "outer {i}");
        let x = f32::from_bits(u(&c[6]));
        assert_eq!(ninja_vector::acosf(x).to_bits(), u(&c[7]), "acosf {x}");
    }
    for (i, c) in doc["vectorAngle"].as_array().unwrap().iter().enumerate() {
        let side = (!c[2].is_null()).then(|| v3(&c[2]));
        let got = ninja_vector::vector_angle(v3(&c[0]), v3(&c[1]), side);
        assert_eq!(got as u32, u(&c[3]), "VectorAngle {i}");
    }
    let mut stack = MatrixStack::new();
    let (mut active, mut steered) = (0, 0);
    for (i, c) in doc["analog"].as_array().unwrap().iter().enumerate() {
        let player = AnalogPlayer {
            status: c["status"].as_i64().unwrap() as i16,
            angles: [0, 1, 2].map(|k| c["angles"][k].as_i64().unwrap() as i32),
            nocon: c["nocon"].as_i64().unwrap() as i16,
            confuse: c["confuse"].as_i64().unwrap() as i16,
        };
        let ctx = AnalogContext {
            angle: c["angle"].as_i64().unwrap() as i32,
            magnitude: f32::from_bits(u(&c["magnitude"])),
            frame: u(&c["frame"]),
            stage: c["stage"].as_i64().unwrap() as i16,
            ground_object: (!c["object"].is_null())
                .then(|| [0, 1, 2].map(|k| c["object"][k].as_i64().unwrap() as i32)),
            stick_y: c["stickY"].as_i64().unwrap() as i16,
            gravity_y: f32::from_bits(u(&c["gravityY"])),
        };
        let r = player_analog::get_analog(
            &mut stack,
            player,
            ctx,
            c["wantAngle"].as_bool().unwrap(),
            c["wantMagnitude"].as_bool().unwrap(),
        );
        assert_eq!(r.active as u64, c["ret"].as_u64().unwrap(), "case {i}: {c}");
        let out_a = r
            .angle
            .map(|a| a as i64)
            .unwrap_or(0xDEADBEEFu32 as i32 as i64);
        assert_eq!(out_a, c["outAngle"].as_i64().unwrap(), "angle {i}: {c}");
        let out_m = r.magnitude.map(f32::to_bits).unwrap_or(0xDEADBEEF);
        assert_eq!(out_m, u(&c["outMagnitude"]), "magnitude {i}: {c}");
        assert_eq!(stack.depth(), 0);
        active += r.active as usize;
        steered += (r.angle.is_some() && ctx.ground_object.is_some()) as usize;
    }
    eprintln!(
        "1,500 vector-product/acosf cases, 600 VectorAngle cases and 1,500 fn_8004E560 cases \
         ({active} active, {steered} steered by an object) matched main.dol"
    );
}
