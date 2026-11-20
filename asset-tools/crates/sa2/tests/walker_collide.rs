//! Compare `walker_collide` with main.dol (tests/walker_collide_ppc.py).
use sa2_asset_tools::land_contact::ContactHit;
use sa2_asset_tools::land_nodes::Candidate;
use sa2_asset_tools::walker_collide::{
    apply_surface, slide_airborne, slide_grounded, sphere_lift, LiftInput, PhysicsRow, SlideInput,
    SurfaceWork,
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn v3(x: &Value) -> [f32; 3] {
    [f(&x[0]), f(&x[1]), f(&x[2])]
}

#[test]
#[ignore = "requires SA2_MAIN_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_collide_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in v["lift"].as_array().unwrap() {
        let i = LiftInput {
            motion: v3(&c["motion"]),
            height: f(&c["height"]),
            last_attr: c["last"].as_u64().unwrap() as u32,
            spd_x: f(&c["spd"][0]),
            spd_y: f(&c["spd"][1]),
            status: c["status"].as_u64().unwrap() as u16 as i16,
            run_speed: f(&c["run"]),
            speed_f4: f(&c["f4"]),
            up: v3(&c["up"]),
            global: v3(&c["global"]),
        };
        let got = sphere_lift(&i, f(&c["out_y"])).map(f32::to_bits);
        let want: Vec<u32> = c["out"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(got[..], want[..], "{c}");
    }
    let u = |x: &Value| x.as_u64().unwrap() as u32;
    let rows: Vec<PhysicsRow> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| PhysicsRow {
            run_accel: f(&r[0]),
            f58: f(&r[1]),
            weight: f(&r[2]),
        })
        .collect();
    for c in v["surface"].as_array().unwrap() {
        let b = &c["before"];
        let mut w = SurfaceWork {
            character: c["ch"].as_i64().unwrap() as i8,
            accel_row: c["b3"].as_i64().unwrap() as i8,
            attr: u(&b[0]),
            f40: f(&b[1]),
            run_accel: f(&b[2]),
            f118: f(&b[3]),
            weight: f(&b[4]),
            f134: f(&c["f134"]),
            flags_1a8: u(&c["flags"]),
            height_1b0: f(&c["h1b0"]),
        };
        apply_surface(
            &mut w,
            f(&c["pos_y"]),
            u(&c["attr"]),
            c["stage"].as_i64().unwrap() as i32,
            &rows,
        );
        let got = [
            w.attr,
            w.f40.to_bits(),
            w.run_accel.to_bits(),
            w.f118.to_bits(),
            w.weight.to_bits(),
        ];
        let want: Vec<u32> = c["after"].as_array().unwrap().iter().map(u).collect();
        assert_eq!(got[..], want[..], "{c}");
    }
    let r74 = &v["row74"];
    for c in v["slide"].as_array().unwrap() {
        let i = SlideInput {
            task_mode: c["mode"].as_i64().unwrap() as i8,
            character: c["ch"].as_i64().unwrap() as i8,
            f134: f(&c["f134"]),
            row0_74: f(&r74[0]),
            row1_74: f(&r74[1]),
            entry_has_task: c["task"].as_bool().unwrap(),
            center: v3(&c["center"]),
            up: v3(&c["up"]),
            global: v3(&c["global"]),
        };
        let hits: Vec<ContactHit> = c["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| ContactHit {
                candidate: Candidate { node: 0, tri: 0 },
                angles: (0, 0),
                normal: [0.0; 3],
                facing: v3(&h[0]),
                foot: [0.0; 3],
                head: v3(&h[1]),
                along: [0.0; 3],
                across: [0.0; 3],
                start_height: 0.0,
                end_height: 0.0,
            })
            .collect();
        let mut motion = v3(&c["motion"]);
        let mut vel = v3(&c["vel"]);
        let r = if c["air"].as_bool().unwrap() {
            slide_airborne(&i, &hits, &mut motion, &mut vel)
        } else {
            slide_grounded(&i, &hits, &mut motion, &mut vel, f(&c["f26"]))
        };
        assert_eq!(
            (r.hit as u64, r.stuck as u64),
            (c["ret"].as_u64().unwrap(), c["stuck"].as_u64().unwrap()),
            "{c}"
        );
        assert_eq!(
            motion.map(f32::to_bits),
            v3(&c["motion_after"]).map(f32::to_bits),
            "{c}"
        );
        assert_eq!(
            vel.map(f32::to_bits),
            v3(&c["vel_after"]).map(f32::to_bits),
            "{c}"
        );
    }
}
