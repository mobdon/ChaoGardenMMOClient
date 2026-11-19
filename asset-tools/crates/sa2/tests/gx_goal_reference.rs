use sa2_asset_tools::goal_ring::Program;
use std::{env, fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires installed City Escape, the local GX reference and a C compiler"]
fn compare_gx_reference_goal_texture() {
    let gd = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap());
    let program = Program::read(13, &fs::read(gd.join("stg13D.rel")).unwrap()).unwrap();
    let out = Command::new("python")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/gx_goal_ref.py"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<[u32; 9]> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 4);
    for row in cases {
        assert_eq!(program.gx_goal_object_words(row[0]), row[1..]);
    }
}
