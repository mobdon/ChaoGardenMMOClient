//! main.dol MOV_Control against the GameCube PowerPC (needs SA2_MAIN_DOL, the
//! local Gekko tables and SA2_PPC_PYTHON; returns early when the oracle skips).
use sa2_asset_tools::mov_control::{Mover, mov_control};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn v3(v: &Value) -> [f32; 3] {
    std::array::from_fn(|k| f32::from_bits(v[k].as_u64().unwrap() as u32))
}
fn bits(a: [f32; 3]) -> Vec<u64> {
    a.iter().map(|x| u64::from(x.to_bits())).collect()
}
fn want(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

#[test]
#[ignore = "requires the user's GameCube main.dol, the local Gekko tables and Python with Unicorn"]
fn mov_control_matches_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mov_control_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in doc.as_array().unwrap() {
        let mut m = Mover {
            velocity: v3(&c["v"]),
            accel: v3(&c["a"]),
            flags: c["flags"].as_u64().unwrap() as u16,
            gravity: f32::from_bits(c["g"].as_u64().unwrap() as u32),
        };
        let mut p = v3(&c["p"]);
        mov_control(&mut m, &mut p, c["pushed"].as_u64().unwrap() != 0);
        assert_eq!(bits(m.velocity), want(&c["vOut"]), "velocity {c}");
        assert_eq!(bits(m.accel), want(&c["aOut"]), "accel {c}");
        assert_eq!(bits(p), want(&c["pOut"]), "position {c}");
        assert_eq!(
            u64::from(m.flags),
            c["flagsOut"].as_u64().unwrap(),
            "flags {c}"
        );
    }
}

#[test]
#[ignore = "requires the user's GameCube main.dol and Python with Unicorn"]
fn turn_toward_matches_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/turn_toward_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in doc.as_array().unwrap() {
        let (aim, yaw) = sa2_asset_tools::mov_control::turn_toward(
            v3(&c["p"]),
            v3(&c["t"]),
            c["yaw"].as_i64().unwrap() as i32,
            c["step"].as_i64().unwrap() as i32,
        );
        assert_eq!(i64::from(aim), c["aim"].as_i64().unwrap(), "aim {c}");
        assert_eq!(i64::from(yaw), c["yawOut"].as_i64().unwrap(), "yaw {c}");
    }
}
