//! Compare `walker_collide` with main.dol (tests/walker_collide_ppc.py).
use sa2_asset_tools::land_contact::ContactHit;
use sa2_asset_tools::land_nodes::Candidate;
use sa2_asset_tools::ninja_matrix::MatrixStack;
use sa2_asset_tools::walker_collide::{
    adjust_angle, apply_surface, diff_angle, plane_intersection, settle_contact, slide_airborne,
    slide_grounded, sphere_lift, sub_angle, tilt_to_contact, wall_push, wall_slide, LiftInput,
    Line, MotionWork, PhysicsRow, Plane, PushSpeeds, SlideInput, SlideQuery, SurfaceWork,
    TiltSpeeds, TurnWork, WallSlideInput,
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
    let bits3 = |x: [f32; 3]| x.map(f32::to_bits);
    for c in v["planes"].as_array().unwrap() {
        let pl = |p: &Value| Plane {
            point: v3(&p[0]),
            normal: v3(&p[1]),
        };
        let mut line = Line {
            point: [f32::from_bits(0xeeee_eeee); 3],
            dir: [f32::from_bits(0xeeee_eeee); 3],
        };
        let with = c["with_line"].as_bool().unwrap();
        let r = plane_intersection(
            &pl(&c["a"]),
            &pl(&c["b"]),
            if with { Some(&mut line) } else { None },
        );
        assert_eq!(r.to_bits() as u64, c["ret"].as_u64().unwrap(), "{c}");
        if with {
            let got: Vec<u32> = bits3(line.point)
                .into_iter()
                .chain(bits3(line.dir))
                .collect();
            let want: Vec<u32> = c["line"].as_array().unwrap().iter().map(u).collect();
            assert_eq!(got, want, "{c}");
        }
    }
    for c in v["wall"].as_array().unwrap() {
        let i = WallSlideInput {
            slide: SlideInput {
                task_mode: 0,
                character: c["ch"].as_i64().unwrap() as i8,
                f134: f(&c["f134"]),
                row0_74: f(&r74[0]),
                row1_74: f(&r74[1]),
                entry_has_task: false,
                center: v3(&c["center"]),
                up: v3(&c["up"]),
                global: [0.0; 3],
            },
            entry_flags: u(&c["eflags"]),
            last_attr: u(&c["last"]),
            motion: v3(&c["motion"]),
            head: v3(&c["head"]),
            across: v3(&c["across"]),
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
                foot: v3(&h[1]),
                head: v3(&h[2]),
                along: [0.0; 3],
                across: [0.0; 3],
                start_height: 0.0,
                end_height: 0.0,
            })
            .collect();
        let mut along = v3(&c["along"]);
        let mut vel = v3(&c["vel"]);
        let mut wall = [1.5, 2.5, 3.5];
        let r = wall_slide(&i, &hits, &mut along, &mut vel, &mut wall);
        assert_eq!(r, c["ret"].as_u64().unwrap() != 0, "{c}");
        assert_eq!(bits3(along), bits3(v3(&c["along_after"])), "{c}");
        assert_eq!(bits3(vel), bits3(v3(&c["vel_after"])), "{c}");
        assert_eq!(bits3(wall), bits3(v3(&c["wall_after"])), "{c}");
    }
    for c in v["angles"].as_array().unwrap() {
        let g = |k: usize| c[k].as_i64().unwrap() as i32;
        let (a, b, st) = (g(0), g(1), g(2));
        let got = [diff_angle(a, b), sub_angle(a, b), adjust_angle(a, b, st)].map(|x| x as u32);
        let want = [3, 4, 5].map(|k| c[k].as_u64().unwrap() as u32);
        assert_eq!(got, want, "{c}");
    }
    let mut stack = MatrixStack::new();
    for c in v["tilt"].as_array().unwrap() {
        let g = |x: &Value| x.as_i64().unwrap() as i32;
        let mut ang = [g(&c["ang"][0]), g(&c["ang"][1]), g(&c["ang"][2])];
        let sp = &c["spd"];
        let speeds = TiltSpeeds {
            spd_x: f(&sp[0]),
            jog: f(&sp[1]),
            run: f(&sp[2]),
            f_ec: f(&sp[3]),
            f_f4: f(&sp[4]),
        };
        tilt_to_contact(
            &mut stack,
            &mut ang,
            c["status"].as_u64().unwrap() as u16 as i16,
            &speeds,
            (g(&c["qa"][0]), g(&c["qa"][1])),
            v3(&c["facing"]),
            v3(&c["up"]),
            v3(&c["global"]),
            u(&c["flags"]),
        );
        let want = [g(&c["after"][0]), g(&c["after"][1]), g(&c["after"][2])];
        assert_eq!(ang, want, "{c}");
    }
    for c in v["settle"].as_array().unwrap() {
        let g = |x: &Value| x.as_i64().unwrap() as i32;
        let w = TurnWork {
            status: c["status"].as_u64().unwrap() as u16 as i16,
            spd: v3(&c["spd"]),
            jog: f(&c["jog"]),
            character: c["ch"].as_i64().unwrap() as i8,
            f134: f(&c["f134"]),
            row0_74: f(&r74[0]),
            row1_74: f(&r74[1]),
        };
        let mut q = SlideQuery {
            center: v3(&c["center"]),
            motion: v3(&c["motion"]),
            up: v3(&c["up"]),
            facing: v3(&c["facing"]),
            head: v3(&c["head"]),
            along: v3(&c["along"]),
        };
        let mut mw = MotionWork {
            spd: v3(&c["mw"]),
            ang_aim: g(&c["aim"]),
        };
        let mut pos = v3(&c["pos"]);
        let mut ang_y = g(&c["ang_y"]);
        let mut vel = v3(&c["vel"]);
        let mut wall = [1.5, 2.5, 3.5];
        let r = settle_contact(
            c["tflag"].as_u64().unwrap() as u16 as i16,
            &mut pos,
            &mut ang_y,
            &mut vel,
            &w,
            &mut wall,
            &mut q,
            &mut mw,
            v3(&c["point"]),
            v3(&c["offset"]),
            g(&c["ret_in"]),
            u(&c["flags"]),
            u(&c["stuck"]) != 0,
        );
        assert_eq!(r as u32, u(&c["ret"]), "{c}");
        assert_eq!(bits3(pos), bits3(v3(&c["pos_after"])), "pos {c}");
        assert_eq!(ang_y, g(&c["ang_y_after"]), "ang {c}");
        assert_eq!(bits3(vel), bits3(v3(&c["vel_after"])), "vel {c}");
        assert_eq!(bits3(wall), bits3(v3(&c["wall_after"])), "wall {c}");
        assert_eq!(bits3(q.center), bits3(v3(&c["center_after"])), "center {c}");
        assert_eq!(bits3(q.motion), bits3(v3(&c["motion_after"])), "motion {c}");
        assert_eq!(bits3(q.along), bits3(v3(&c["along_after"])), "along {c}");
        assert_eq!(bits3(mw.spd), bits3(v3(&c["mw_after"])), "mw {c}");
        assert_eq!(mw.ang_aim, g(&c["aim_after"]), "aim {c}");
    }
    for c in v["push"].as_array().unwrap() {
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
        let sp = &c["speeds"];
        let speeds = PushSpeeds {
            spd_x: f(&sp[0]),
            f_f0: f(&sp[1]),
            jog: f(&sp[2]),
            run: f(&sp[3]),
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
        let (code, stuck) = wall_push(&i, u(&c["eflags"]), &speeds, &hits, &mut motion, &mut vel);
        assert_eq!(
            (code as u32, stuck as u32),
            (u(&c["ret"]), u(&c["stuck"])),
            "{c}"
        );
        assert_eq!(bits3(motion), bits3(v3(&c["motion_after"])), "motion {c}");
        assert_eq!(bits3(vel), bits3(v3(&c["vel_after"])), "vel {c}");
    }
}
