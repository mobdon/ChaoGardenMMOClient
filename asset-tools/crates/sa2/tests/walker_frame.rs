//! Compare `walker_frame::player_speed_from_world` with main.dol's
//! fn_8003E668 run under the Unicorn DOL harness.
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    walker_frame::{player_speed_from_world, FrameInput},
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
    let cases = cases.as_array().unwrap();
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
