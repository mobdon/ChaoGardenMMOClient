//! Ninja trigonometry against main.dol run under tests/dol_ppc.py.
use sa2_asset_tools::{manifest, ninja};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

#[test]
fn table_landmarks() {
    let t = ninja::SinCos::new();
    assert_eq!((t.sin(0), t.cos(0)), (0.0, 1.0));
    assert_eq!((t.sin(0x4000), t.sin(-0x4000)), (1.0, -1.0));
    assert_eq!(t.sin(0x10000), t.sin(0));
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ninja_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let table = ninja::SinCos::new();
    let samples = doc["samples"].as_array().unwrap();
    for s in samples {
        let x = f32::from_bits(s[0].as_u64().unwrap() as u32);
        assert_eq!(
            ninja::sinf(x).to_bits() as u64,
            s[1].as_u64().unwrap(),
            "sinf {x:e}"
        );
        assert_eq!(
            ninja::cosf(x).to_bits() as u64,
            s[2].as_u64().unwrap(),
            "cosf {x:e}"
        );
    }
    assert_eq!(doc["tableSha256"], manifest::sha256(&table.to_be_bytes()));
    eprintln!(
        "njSinCosTbl (65,536 pairs) and {} sinf/cosf samples matched main.dol",
        samples.len()
    );
}
