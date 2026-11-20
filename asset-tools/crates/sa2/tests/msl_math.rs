//! Compare `msl_math` with main.dol's own routines run under Unicorn.
use sa2_asset_tools::msl_math::{asinf, atan2f, atanf, inv_sqrtf, normal_angles};
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires SA2_GSNE8P_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol_bit_for_bit() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/msl_math_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let f = |s: &str| f32::from_bits(s.parse().unwrap());
    let mut counts = std::collections::BTreeMap::new();
    for line in String::from_utf8(o.stdout).unwrap().lines() {
        let v: Vec<&str> = line.split_whitespace().collect();
        let got = match v[0] {
            "atanf" => vec![atanf(f(v[1])).to_bits() as i64],
            "asinf" => vec![asinf(f(v[1])).to_bits() as i64],
            "inv_sqrtf" => vec![inv_sqrtf(f(v[1])).to_bits() as i64],
            "atan2f" => vec![atan2f(f(v[1]), f(v[2])).to_bits() as i64],
            "angles" => {
                let (x, z) = normal_angles(f(v[1]), f(v[2]), f(v[3]));
                vec![x as i64, z as i64]
            }
            k => panic!("unknown case {k}"),
        };
        let want: Vec<i64> = v[v.len() - got.len()..]
            .iter()
            .map(|s| s.parse::<i64>().unwrap())
            .collect();
        assert_eq!(got, want, "{line}");
        *counts.entry(v[0].to_string()).or_insert(0) += 1;
    }
    assert!(counts.values().all(|&n| n > 100), "{counts:?}");
}
