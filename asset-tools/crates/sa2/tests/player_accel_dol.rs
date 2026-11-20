//! `player_accel` against main.dol (tests/dol_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    player_accel::{self, GroundPlayer, MoveInput, SteerInput},
    player_analog::AnalogContext,
    player_path::{Path, Points},
    player_physics::PlayerParameters,
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

fn path(p: &Value) -> Path {
    let recs = p["records"].as_array().unwrap();
    let points = if p["kind"] == 1 {
        Points::Angled(
            recs.iter()
                .map(|r| (i(&r[0]) as i16, i(&r[1]) as i16, f(&r[2]), v3(&r[3])))
                .collect(),
        )
    } else {
        Points::Vectored(
            recs.iter()
                .map(|r| (v3(&r[0]), f(&r[1]), v3(&r[2])))
                .collect(),
        )
    };
    Path {
        flags: i(&p["flags"]) as i16,
        count: i(&p["count"]) as i16,
        total: f(&p["total"]),
        points,
    }
}

/// Player work +0xC4..+0x144, in `PlayerParameters` order.
fn physics(w: &Value) -> PlayerParameters {
    let g = |k: usize| f(&w[k]);
    PlayerParameters {
        jump2_timer: 0,
        pos_error: g(0),
        lim_h_spd: g(1),
        lim_v_spd: g(2),
        max_x_spd: g(3),
        max_psh_spd: g(4),
        jmp_y_spd: g(5),
        nocon_speed: g(6),
        slide_speed: g(7),
        jog_speed: g(8),
        run_speed: g(9),
        rush_speed: g(10),
        crash_speed: g(11),
        dash_speed: g(12),
        jmp_addit: g(13),
        run_accel: g(14),
        air_accel: g(15),
        slow_down: g(16),
        run_break: g(17),
        air_break: g(18),
        air_resist_air: g(19),
        air_resist: g(20),
        air_resist_y: g(21),
        air_resist_z: g(22),
        grd_frict: g(23),
        grd_frict_z: g(24),
        lim_frict: g(25),
        rat_bound: g(26),
        rad: g(27),
        height: g(28),
        weight: g(29),
        eyes_height: g(30),
        center_height: g(31),
    }
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_accel_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut stack = MatrixStack::new();
    let mut turned = 0;
    for (n, c) in doc["plain"].as_array().unwrap().iter().enumerate() {
        let mut p = GroundPlayer {
            status: i(&c["status"]) as i16,
            angles: [0, 1, 2].map(|k| i(&c["angles"][k])),
            base: v3(&c["base"]),
            aim: i(&c["aim"]),
            nocon: i(&c["nocon"]) as i16,
            confuse: i(&c["confuse"]) as i16,
            up: f(&c["up"]),
            scale: f(&c["scale"]),
            speed: v3(&c["speed"]),
            ..Default::default()
        };
        let gravity = v3(&c["gravity"]);
        let analog = AnalogContext {
            angle: i(&c["stick"]),
            magnitude: f(&c["magnitude"]),
            frame: u(&c["frame"]),
            stage: i(&c["stage"]) as i16,
            gravity_y: gravity[1],
            ..Default::default()
        };
        let before = p.angles;
        player_accel::accelerate(&mut stack, &mut p, &physics(&c["phys"]), gravity, analog);
        let bits = |v: [f32; 3]| v.map(f32::to_bits);
        let want = |key: &str| [0, 1, 2].map(|k| u(&c[key][k]));
        assert_eq!(
            p.angles,
            [0, 1, 2].map(|k| i(&c["outAngles"][k])),
            "case {n}: {c}"
        );
        assert_eq!(bits(p.speed), want("outSpeed"), "case {n}: {c}");
        assert_eq!(bits(p.accel), want("outAccel"), "case {n}: {c}");
        assert_eq!(p.excess.to_bits(), u(&c["outExcess"]), "case {n}: {c}");
        assert_eq!(p.push.to_bits(), u(&c["outPush"]), "case {n}: {c}");
        assert_eq!(p.aim, i(&c["outAim"]), "case {n}: {c}");
        assert_eq!(stack.depth(), 0);
        turned += (before != p.angles) as usize;
    }
    for c in doc["rotate"].as_array().unwrap() {
        let mut stack = MatrixStack::new();
        let m: Vec<f32> = (0..12).map(|k| f(&c["m"][k])).collect();
        *stack.current_mut() = [0, 1, 2].map(|r| [0, 1, 2, 3].map(|k| m[4 * r + k]));
        stack.rotate_axis(v3(&c["axis"]), i(&c["angle"]));
        let out: Vec<u32> = stack
            .current()
            .iter()
            .flatten()
            .map(|x| x.to_bits())
            .collect();
        let want: Vec<u32> = (0..12).map(|k| u(&c["out"][k])).collect();
        assert_eq!(out, want, "njRotate {c}");
    }
    let mut turned_by = [0, 0];
    for (set, key) in ["steered", "move"].into_iter().enumerate() {
        for (n, c) in doc[key].as_array().unwrap().iter().enumerate() {
            let mut p = GroundPlayer {
                status: i(&c["status"]) as i16,
                angles: [0, 1, 2].map(|k| i(&c["angles"][k])),
                pos: v3(&c["pos"]),
                motion: v3(&c["motion"]),
                base: v3(&c["base"]),
                aim: i(&c["aim"]),
                nocon: i(&c["nocon"]) as i16,
                confuse: i(&c["confuse"]) as i16,
                path_dist: f(&c["pathDist"]),
                up: f(&c["up"]),
                scale: f(&c["scale"]),
                speed: v3(&c["speed"]),
                down: v3(&c["down"]),
                surface: u(&c["surface"]),
                normal: v3(&c["normal"]),
                character: i(&c["character"]) as i8,
                wall: v3(&c["wall"]),
                ..Default::default()
            };
            let gravity = v3(&c["gravity"]);
            let raw = [0, 1].map(|k| i(&c["raw"][k]) as i16);
            let input = SteerInput {
                analog: AnalogContext {
                    angle: i(&c["stick"]),
                    magnitude: f(&c["magnitude"]),
                    frame: u(&c["frame"]),
                    stage: i(&c["stage"]) as i16,
                    stick_y: raw[1],
                    gravity_y: gravity[1],
                    ..Default::default()
                },
                gravity,
                stage: i(&c["stage"]) as i16,
                raw_stick: raw,
                camera: v3(&c["camera"]),
            };
            let path = path(&c["path"]);
            let before = (p.angles, p.aim);
            let phys = physics(&c["phys"]);
            if set == 0 {
                player_accel::accelerate_steered(&mut stack, &mut p, &phys, &input, Some(&path));
            } else {
                let input = MoveInput {
                    steer: input,
                    walker_sharp: u(&c["sharp"]) as u8,
                };
                player_accel::ground_move(&mut stack, &mut p, &phys, &input, Some(&path));
            }
            let bits = |v: [f32; 3]| v.map(f32::to_bits);
            let want = |key: &str| [0, 1, 2].map(|k| u(&c[key][k]));
            assert_eq!(
                p.status as i64,
                c["outStatus"].as_i64().unwrap(),
                "{key} {n}: {c}"
            );
            assert_eq!(
                p.angles,
                [0, 1, 2].map(|k| i(&c["outAngles"][k])),
                "{key} {n}: {c}"
            );
            assert_eq!(bits(p.speed), want("outSpeed"), "{key} {n}: {c}");
            assert_eq!(bits(p.accel), want("outAccel"), "{key} {n}: {c}");
            assert_eq!(p.excess.to_bits(), u(&c["outExcess"]), "{key} {n}: {c}");
            assert_eq!(p.push.to_bits(), u(&c["outPush"]), "{key} {n}: {c}");
            assert_eq!(
                p.path_dist.to_bits(),
                u(&c["outPathDist"]),
                "{key} {n}: {c}"
            );
            assert_eq!(p.aim, i(&c["outAim"]), "{key} {n}: {c}");
            assert_eq!(stack.depth(), 0);
            turned_by[set] += (before != (p.angles, p.aim)) as usize;
        }
    }
    eprintln!(
        "3,000 fn_8004B568 frames ({turned} turned), 800 njRotate cases and 3,000 fn_8004BA74 frames ({} turned) and 6,000 \
         fn_8004C734 frames ({} turned) matched main.dol",
        turned_by[0], turned_by[1]
    );
}
