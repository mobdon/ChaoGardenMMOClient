//! Compare `walker_move` with main.dol (tests/walker_move_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    walker_move::{
        accel, accel_air, accel_gravity, accel_mode43, accel_run, air_level, AccelWork,
    },
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn v3(x: &Value) -> [f32; 3] {
    [f(&x[0]), f(&x[1]), f(&x[2])]
}

fn i3(x: &Value) -> [i32; 3] {
    [0, 1, 2].map(|j| x[j].as_i64().unwrap() as i32)
}

fn bits(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_move_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut stack = MatrixStack::new();
    for c in v["level"].as_array().unwrap() {
        let mut ang = i3(&c["ang"]);
        let mut pos = v3(&c["pos"]);
        let mut spd = v3(&c["spd"]);
        let t = &c["target"];
        air_level(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            &mut ang,
            &mut pos,
            &mut spd,
            c["w24"].as_u64().unwrap() as u32,
            f(&c["f4"]),
            f(&c["f134"]),
            (t[0].as_i64().unwrap() as i32, t[1].as_i64().unwrap() as i32),
        );
        assert_eq!(ang, i3(&c["ang_after"]), "{c}");
        assert_eq!(bits(pos), bits(v3(&c["pos_after"])), "{c}");
        assert_eq!(bits(spd), bits(v3(&c["spd_after"])), "{c}");
    }
    for c in v["acc"].as_array().unwrap() {
        let g = |k: &str| f(&c["f"][k]);
        let w = AccelWork {
            spd: v3(&c["spd"]),
            f40: g("0x40"),
            f_d0: g("0xd0"),
            air_resist: [g("0x114"), g("0x118"), g("0x11c")],
            decel_x: g("0x120"),
            decel_z: g("0x124"),
            stop_share: g("0x128"),
            weight: g("0x138"),
        };
        let mut across = v3(&c["across"]);
        let got = accel(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            i3(&c["ang"]),
            &w,
            v3(&c["mwacc"]),
            v3(&c["global"]),
            &mut across,
        );
        let want = v3(&c["acc_after"]);
        match got {
            Some(a) => assert_eq!(bits(a), bits(want), "{c}"),
            None => assert_eq!(bits(want), bits([1.5, 2.5, 3.5]), "{c}"),
        }
        assert_eq!(bits(across), bits(v3(&c["across_after"])), "{c}");
    }
    for c in v["run"].as_array().unwrap() {
        let g = |k: &str| f(&c["f"][k]);
        let w = AccelWork {
            spd: v3(&c["spd"]),
            f40: g("0x40"),
            f_d0: 0.0,
            air_resist: [g("0x114"), g("0x118"), g("0x11c")],
            decel_x: g("0x120"),
            decel_z: g("0x124"),
            stop_share: g("0x128"),
            weight: g("0x138"),
        };
        let got = accel_run(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            i3(&c["ang"]),
            &w,
            g("0x108"),
            v3(&c["mwacc"]),
            v3(&c["global"]),
            &mut || {},
        );
        let want = v3(&c["acc_after"]);
        match got {
            Some(a) => assert_eq!(bits(a), bits(want), "{c}"),
            None => assert_eq!(bits(want), bits([1.5, 2.5, 3.5]), "{c}"),
        }
    }
    for c in v["air"].as_array().unwrap() {
        let g = |k: &str| f(&c["f"][k]);
        let w = AccelWork {
            spd: v3(&c["spd"]),
            f40: 0.0,
            f_d0: 0.0,
            air_resist: [0.0, g("0x118"), g("0x11c")],
            decel_x: 0.0,
            decel_z: 0.0,
            stop_share: 0.0,
            weight: g("0x138"),
        };
        let st = &c["stick"];
        let held = st[0].as_u64().unwrap() != 0;
        let stick = (st[1].as_i64().unwrap() as i32, f(&st[2]));
        let mut aim = c["aim"].as_i64().unwrap() as i32;
        let a = accel_air(
            &mut stack,
            i3(&c["ang"]),
            &w,
            g("0x110"),
            g("0xe8"),
            g("0x100"),
            g("0x10c"),
            v3(&c["mwacc"]),
            f(&c["mwspd_y"]),
            &mut aim,
            v3(&c["global"]),
            &mut || held.then_some(stick),
            &mut |_| {},
        );
        assert_eq!(bits(a), bits(v3(&c["acc_after"])), "{c}");
        assert_eq!(aim as i64, c["aim_after"].as_i64().unwrap(), "{c}");
    }
    for c in v["grav"].as_array().unwrap() {
        let got = accel_gravity(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            i3(&c["ang"]),
            f(&c["f"]["0x138"]),
            v3(&c["mwacc"]),
            v3(&c["global"]),
        );
        let want = v3(&c["acc_after"]);
        match got {
            Some(a) => assert_eq!(bits(a), bits(want), "{c}"),
            None => assert_eq!(bits(want), bits([1.5, 2.5, 3.5]), "{c}"),
        }
    }
    for c in v["m43"].as_array().unwrap() {
        let g = |k: &str| f(&c["f"][k]);
        let w = AccelWork {
            spd: v3(&c["spd"]),
            f40: 0.0,
            f_d0: g("0xd0"),
            air_resist: [g("0x114"), g("0x118"), g("0x11c")],
            decel_x: 0.0,
            decel_z: 0.0,
            stop_share: 0.0,
            weight: g("0x138"),
        };
        let mut across = v3(&c["across"]);
        let got = accel_mode43(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            i3(&c["ang"]),
            &w,
            g("0x34"),
            g("0xe8"),
            v3(&c["mwacc"]),
            v3(&c["global"]),
            &mut across,
        );
        let want = v3(&c["acc_after"]);
        match got {
            Some(a) => assert_eq!(bits(a), bits(want), "{c}"),
            None => assert_eq!(bits(want), bits([1.5, 2.5, 3.5]), "{c}"),
        }
        assert_eq!(bits(across), bits(v3(&c["across_after"])), "{c}");
    }
}
