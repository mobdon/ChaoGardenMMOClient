//! Compare `walker_move` with main.dol (tests/walker_move_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    walker_move::{
        accel, accel_air, accel_gravity, accel_hover, accel_mode43, accel_run, aim_carry,
        aim_toward, air_level, jump_assist, slope_aim, steer, turn_back, AccelWork, JumpTarget,
        JumpWork, SteerCalls, SteerInput,
    },
};
use serde_json::{json, Value};
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

struct Log {
    stick: Option<i32>,
    mag: f32,
    calls: Vec<Value>,
}

impl SteerCalls for Log {
    fn flagged_turn(&mut self) {
        self.calls.push(json!(["flagged", 0]));
    }
    fn analog(&mut self) -> Option<i32> {
        self.stick
    }
    fn analog_magnitude(&mut self) -> Option<(i32, f32)> {
        self.stick.map(|a| (a, self.mag))
    }
    fn slope_turn(&mut self) {
        self.calls.push(json!(["slope", 0]));
    }
    fn turn_toward(&mut self, aim: &mut i32) {
        self.calls.push(json!(["toward", *aim as u32]));
    }
    fn turn_smoothly(&mut self, aim: &mut i32) {
        self.calls.push(json!(["smooth", *aim as u32]));
    }
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
    for c in v["steer"].as_array().unwrap() {
        let i = |x: &Value| x.as_i64().unwrap() as i32;
        let inp = SteerInput {
            task_flag: c["flag"].as_u64().unwrap() as i16,
            angles: i3(&c["ang"]),
            attr: c["attr"].as_u64().unwrap() as u32,
            normal: v3(&c["n"]),
            stick: (i(&c["stk"][0]) as i16, i(&c["stk"][1]) as i16),
            camera: (i(&c["cam"][0]), i(&c["cam"][1])),
        };
        let mut log = Log {
            stick: (c["stick"][0].as_u64().unwrap() != 0).then(|| i(&c["stick"][1])),
            mag: 0.0,
            calls: vec![],
        };
        let mut aim = i(&c["aim"]);
        steer(&mut stack, &inp, &mut aim, &mut log);
        assert_eq!(aim, i(&c["aim_after"]), "{c}");
        assert_eq!(Value::from(log.calls), c["log"], "{c}");
    }
    for c in v["jump"].as_array().unwrap() {
        let i = |x: &Value| x.as_i64().unwrap() as i32;
        let g = |k: &str| f(&c["f"][k]);
        let mut yaw = i(&c["yaw"]);
        let inp = SteerInput {
            task_flag: 0,
            angles: [0, yaw, 0],
            attr: 0,
            normal: [0.0; 3],
            stick: (0, 0),
            camera: (0, 0),
        };
        let w = JumpWork {
            task_pos: v3(&c["pos"]),
            mw_spd: v3(&c["mwspd"]),
            air_resist: [g("0x110"), g("0x118"), g("0x11c")],
            f_cc: g("0xcc"),
            f_e8: g("0xe8"),
            run_accel: g("0xfc"),
            weight: g("0x138"),
        };
        let mut tg: Vec<JumpTarget> = c["tg"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| JumpTarget {
                pos: [f(&t[0]), f(&t[1]), f(&t[2])],
                reach: f(&t[3]),
                rank: f(&t[4]),
                dist: f(&t[5]),
                turn: i(&t[6]),
                dir: i(&t[7]),
            })
            .collect();
        let mut log = Log {
            stick: (c["stick"][0].as_u64().unwrap() != 0).then(|| i(&c["stick"][1])),
            mag: 0.0,
            calls: vec![],
        };
        let mut aim = i(&c["aim"]);
        let mut acc = v3(&c["mwacc"]);
        let mut a43 = vec![];
        let ret = jump_assist(
            &mut stack,
            &inp,
            &mut yaw,
            &mut aim,
            &mut acc,
            &w,
            &mut tg,
            &mut log,
            &mut |_, y, a| a43.push(json!(["accel43", y, bits(a)])),
        );
        log.calls.extend(a43);
        assert_eq!(ret as u64, c["ret"].as_u64().unwrap(), "{c}");
        assert_eq!(yaw, i(&c["yaw_after"]), "{c}");
        assert_eq!(aim, i(&c["aim_after"]), "{c}");
        assert_eq!(bits(acc), bits(v3(&c["acc_after"])), "{c}");
        assert_eq!(Value::from(log.calls), c["log"], "{c}");
        for (t, want) in tg.iter().zip(c["tg_after"].as_array().unwrap()) {
            let got = [
                t.pos[0].to_bits() as i64,
                t.pos[1].to_bits() as i64,
                t.pos[2].to_bits() as i64,
                t.reach.to_bits() as i64,
                t.rank.to_bits() as i64,
                t.dist.to_bits() as i64,
                t.turn as i64,
                t.dir as i64,
            ];
            assert_eq!(Value::from(got.to_vec()), *want, "{c}");
        }
    }
    for c in v["small"].as_array().unwrap() {
        let i = |x: &Value| x.as_i64().unwrap() as i32;
        let g = |k: &str| f(&c["f"][k]);
        let flag = c["flag"].as_u64().unwrap() as i16;
        let mut ang = i3(&c["ang"]);
        let mut spd = v3(&c["spd"]);
        let mut aim = i(&c["aim"]);
        let mut log = Log {
            stick: (c["stick"][0].as_u64().unwrap() != 0).then(|| i(&c["stick"][1])),
            mag: f(&c["stick"][2]),
            calls: vec![],
        };
        let touch = c["touch"].as_bool().unwrap();
        let mut acc = [1.5, 2.5, 3.5];
        let mut ret = 0u64;
        match c["fn"].as_u64().unwrap() {
            0x8004E490 => {
                let mut touched = vec![];
                let r = turn_back(flag, ang[1], &mut log, &mut |k| {
                    touched.push(k);
                    touch
                });
                for k in touched {
                    log.calls.push(json!(["touch", k]));
                }
                ret = r as u64;
            }
            0x80048400 => aim_toward(flag, ang[1], &mut aim, &mut log),
            0x80048370 => aim_carry(
                &mut stack,
                flag,
                c["mode2"].as_u64().unwrap() as u8,
                &mut ang,
                g("0x34"),
                &mut spd,
                &mut aim,
                &mut log,
            ),
            _ => {
                acc = accel_hover(
                    &mut stack,
                    ang,
                    spd,
                    [g("0x110"), g("0x118"), g("0x11c")],
                    g("0xe8"),
                    g("0x100"),
                    g("0x10c"),
                    v3(&c["mwacc"]),
                    &mut aim,
                    &mut log,
                )
            }
        }
        if c["fn"].as_u64().unwrap() == 0x8004E490 {
            assert_eq!(ret, c["ret"].as_u64().unwrap(), "{c}");
        }
        assert_eq!(ang, i3(&c["ang_after"]), "{c}");
        assert_eq!(bits(spd), bits(v3(&c["spd_after"])), "{c}");
        assert_eq!(bits(acc), bits(v3(&c["acc_after"])), "{c}");
        assert_eq!(aim, i(&c["aim_after"]), "{c}");
        assert_eq!(Value::from(log.calls), c["log"], "{c}");
    }
    for c in v["slope"].as_array().unwrap() {
        let i = |x: &Value| x.as_i64().unwrap() as i32;
        let mut aim = i(&c["aim"]);
        let mut log = Log {
            stick: None,
            mag: 0.0,
            calls: vec![],
        };
        let r = slope_aim(
            &mut stack,
            c["flag"].as_u64().unwrap() as i16,
            i3(&c["ang"]),
            v3(&c["spd"]),
            &mut aim,
            i(&c["t14"]) as i16,
            c["attr"].as_u64().unwrap() as u32,
            f(&c["fe4"]),
            v3(&c["global"]),
            &mut log,
        );
        assert_eq!(r.to_bits() as u64, c["ret"].as_u64().unwrap(), "{c}");
        assert_eq!(aim, i(&c["aim_after"]), "{c}");
        assert_eq!(Value::from(log.calls), c["log"], "{c}");
    }
}
