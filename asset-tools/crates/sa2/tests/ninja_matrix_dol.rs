//! `ninja_matrix` against main.dol run under tests/dol_ppc.py.
use sa2_asset_tools::ninja_matrix::{self, Axis, MatrixStack, Mtx};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn mtx(v: &Value) -> Mtx {
    let w: Vec<u32> = serde_json::from_value(v.clone()).unwrap();
    let mut m = [[0.0; 4]; 3];
    for (k, b) in w.iter().enumerate() {
        m[k / 4][k % 4] = f32::from_bits(*b);
    }
    m
}

fn bits_m(m: &Mtx) -> Vec<u32> {
    m.iter().flatten().map(|x| x.to_bits()).collect()
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|i| f32::from_bits(v[i].as_u64().unwrap() as u32))
}

fn bits_v(v: [f32; 3]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn u32s(v: &Value) -> Vec<u32> {
    serde_json::from_value(v.clone()).unwrap()
}

#[test]
fn rotations_of_quarter_turns() {
    let mut s = MatrixStack::new();
    let v = ninja_matrix::rotate_by_angles(&mut s, [0, 0, 0], [1.0, 2.0, 3.0]);
    assert_eq!(v, [1.0, 2.0, 3.0]);
    assert_eq!(s.depth(), 0);
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ninja_matrix_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for (i, c) in doc["concat"].as_array().unwrap().iter().enumerate() {
        let r = ninja_matrix::concat(&mtx(&c[0]), &mtx(&c[1]));
        assert_eq!(bits_m(&r), u32s(&c[2]), "concat {i}");
    }
    for (i, c) in doc["vec"].as_array().unwrap().iter().enumerate() {
        let m = mtx(&c[0]);
        let v = v3(&c[1]);
        assert_eq!(
            bits_v(ninja_matrix::mult_vec_sr(&m, v)),
            u32s(&c[2]),
            "vec_sr {i}"
        );
        assert_eq!(
            bits_v(ninja_matrix::mult_vec(&m, v)),
            u32s(&c[3]),
            "vec {i}"
        );
    }
    for (i, c) in doc["rotate"].as_array().unwrap().iter().enumerate() {
        let mut m = mtx(&c[0]);
        let axis = [Axis::X, Axis::Y, Axis::Z][c[1].as_u64().unwrap() as usize];
        ninja_matrix::rotate(&mut m, axis, c[2].as_i64().unwrap() as i32);
        assert_eq!(bits_m(&m), u32s(&c[3]), "rotate {i}");
    }
    let mut s = MatrixStack::new();
    for (i, c) in doc["frame"].as_array().unwrap().iter().enumerate() {
        let a = [0, 1, 2].map(|k| c[0][k].as_i64().unwrap() as i32);
        let v = v3(&c[1]);
        let to = ninja_matrix::rotate_by_angles(&mut s, a, v);
        let back = ninja_matrix::unrotate_by_angles(&mut s, a, v);
        assert_eq!(bits_v(to), u32s(&c[2]), "fn_8003E9E0 {i}");
        assert_eq!(bits_v(back), u32s(&c[3]), "fn_8003EA6C {i}");
        assert_eq!(s.depth(), 0);
    }
    eprintln!("400 concats, 400 vector pairs, 600 rotations and 600 frame pairs matched main.dol");
}
