//! Compare src/hunter.rs with main.dol's fn_80091454, fn_80093270 and
//! fn_8009B708 run under the Unicorn DOL harness (tests/hunter_ppc.py).
use sa2_asset_tools::{
    hunter::{climb_request, collision_center, drop_check, ClimbRequest, DropCheck},
    ninja_matrix::MatrixStack,
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn u(v: &Value) -> u64 {
    v.as_u64().unwrap()
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

#[test]
#[ignore = "requires SA2_MAIN_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol_bit_for_bit() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/hunter_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cases: Value = serde_json::from_slice(&o.stdout).unwrap();

    let drop = cases["drop"].as_array().unwrap();
    for c in drop {
        let x = &c["in"];
        let check = DropCheck {
            action: u(&x[0]) as u8,
            flag: i(&x[1]) as i16,
            info_center_y: f(&x[2]),
            spd_y: f(&x[3]),
            flags_1a8: u(&x[4]) as u32,
            height_1b0: f(&x[5]),
            height_1b4: f(&x[6]),
            table_894: u(&x[7]) as u8 as i8,
            table_898: u(&x[8]) as u8 as i8,
        };
        assert_eq!(drop_check(&check) as u64, u(&c["out"]), "drop {c}");
    }

    let climb = cases["climb"].as_array().unwrap();
    for c in climb {
        let x = &c["in"];
        let mut w = ClimbRequest {
            motion_action: i(&x[1]) as i16,
            spd_x: f(&x[2]),
            spd_z: f(&x[3]),
            request: i(&x[4]) as i16,
            field_44: f(&x[5]),
        };
        climb_request(i(&x[0]) as i16, &mut w);
        assert_eq!(w.request as i64, i(&c["request"]), "climb {c}");
        assert_eq!(w.field_44.to_bits() as u64, u(&c["f44"]), "climb {c}");
    }

    let center = cases["center"].as_array().unwrap();
    let mut stack = MatrixStack::new();
    for c in center {
        let x = &c["in"];
        let angles = std::array::from_fn(|k| i(&x[1][k]) as i32);
        let pos = std::array::from_fn(|k| f(&x[2][k]));
        let out = collision_center(&mut stack, u(&x[0]) as u8, angles, pos, f(&x[3]), f(&x[4]));
        let want: Vec<u64> = (0..3).map(|k| u(&c["out"][k])).collect();
        let got: Vec<u64> = out.iter().map(|v| v.to_bits() as u64).collect();
        assert_eq!(got, want, "center {c}");
    }
    eprintln!(
        "hunter: {} drop, {} climb, {} center cases match",
        drop.len(),
        climb.len(),
        center.len()
    );
}
