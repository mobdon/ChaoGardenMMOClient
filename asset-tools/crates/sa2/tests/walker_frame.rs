//! Compare `walker_frame::player_speed_from_world` with main.dol's
//! fn_8003E668 run under the Unicorn DOL harness.
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    walker_frame::{
        clamp_speed, player_speed_from_world, player_world_speed, FrameInput, SpeedTask, SpeedWork,
    },
};
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires SA2_MAIN_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol_bit_for_bit() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_frame_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cases: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let f = |v: &serde_json::Value| f32::from_bits(v.as_u64().unwrap() as u32);
    let v3 = |v: &serde_json::Value| [f(&v[0]), f(&v[1]), f(&v[2])];
    let i3 = |v: &serde_json::Value| std::array::from_fn(|k| v[k].as_i64().unwrap() as i32);
    let mut stack = MatrixStack::new();
    for c in cases["speed"].as_array().unwrap() {
        let i = &c["in"];
        let mut w = SpeedWork {
            spd: v3(&i[0]),
            acc: v3(&i[1]),
            dotp: f(&i[2]),
            attr: i[3].as_u64().unwrap() as u32,
            floor_normal: v3(&i[4]),
            jog_speed: f(&i[5]),
            run_speed: f(&i[6]),
            run_break: f(&i[7]),
            lim_h_spd: f(&i[8][0]),
            lim_v_spd: f(&i[8][1]),
            flags_1a8: i[9].as_u64().unwrap() as u32,
            height_1b0: f(&i[10]),
        };
        let task = SpeedTask {
            mode: i[11].as_u64().unwrap() as u8,
            angles: i3(&i[12]),
            pos_y: f(&i[13]),
        };
        let mut world = [0.0, f(&i[14]), 0.0];
        let active = i[15].as_u64().unwrap() != 0;
        let mut calls = 0;
        let mut stack = MatrixStack::new();
        player_world_speed(&mut stack, &mut w, &task, &mut world, |_| {
            calls += 1;
            active
        });
        let bits = |v: &serde_json::Value| -> Vec<u32> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u32)
                .collect()
        };
        assert_eq!(w.spd.map(f32::to_bits).to_vec(), bits(&c["spd"]), "spd {c}");
        assert_eq!(
            world.map(f32::to_bits).to_vec(),
            bits(&c["world"]),
            "world {c}"
        );
        assert_eq!(
            calls,
            c["calls"].as_u64().unwrap() as i32,
            "analog calls {c}"
        );
    }
    let clamps = cases["clamp"].as_array().unwrap();
    for c in clamps {
        let got = clamp_speed(v3(&c[0]), f(&c[1][0]), f(&c[1][1]));
        let want: Vec<u32> = c[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(got.map(f32::to_bits).to_vec(), want, "clamp {c}");
    }
    assert_eq!(clamps.len(), 600);
    let cases = cases["frame"].as_array().unwrap();
    for c in cases {
        let i = &c["in"];
        let input = FrameInput {
            task_flag: i[0].as_i64().unwrap() as i16,
            task_angles: i3(&i[1]),
            world: v3(&i[2]),
            vector_angles: i3(&i[3]),
            attr: i[4].as_u64().unwrap() as u32,
            last_attr: i[5].as_u64().unwrap() as u32,
            floor_normal: v3(&i[6]),
            run_accel: f(&i[7]),
            weight: f(&i[8]),
            global: v3(&i[9]),
            stage: i[10].as_i64().unwrap() as i32,
        };
        let got = player_speed_from_world(&mut stack, &input);
        assert_eq!(stack.depth(), 0, "matrix stack left pushed");
        let want_spd: Vec<u32> = c["spd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(got.spd.map(f32::to_bits).to_vec(), want_spd, "spd {c}");
        assert_eq!(
            got.dotp.to_bits(),
            c["dotp"].as_u64().unwrap() as u32,
            "dotp {c}"
        );
        assert_eq!(got.task_angles, i3(&c["task"]), "task {c}");
        assert_eq!(got.vector_angles, i3(&c["vec"]), "vec {c}");
    }
    assert_eq!(cases.len(), 1500);
}
