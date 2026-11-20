//! Compare `metal_harbor` with stg10D.rel's own stage program and stage task
//! run under the main.dol harness (tests/metal_harbor_ppc.py), case by case:
//! the ordered engine and module calls and every location the routine writes.
//! With SA2_GAME_PATH set, the installed Steam stg10D.rel is replayed too and
//! must give the same GSNE8P results.
use sa2_asset_tools::metal_harbor;
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn oracle() -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/metal_harbor_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    Some(serde_json::from_slice(&o.stdout).unwrap())
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn stage_program_and_task_match_stg10d() {
    let Some(c) = oracle() else { return };
    let mut rels = vec![std::fs::read(
        PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").unwrap()).join("stg10D.rel"),
    )
    .unwrap()];
    if let Some(game) = std::env::var_os("SA2_GAME_PATH") {
        rels.push(std::fs::read(PathBuf::from(game).join("resource/gd_PC/stg10D.rel")).unwrap());
    }
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 965);
    for rel in &rels {
        for case in cases {
            let routine = case["fn"].as_str().unwrap();
            if routine == "speed_probe" {
                continue; // the oracle itself checked +0xAB7C writes nothing
            }
            let got = metal_harbor::replay(rel, routine, &case["input"])
                .unwrap_or_else(|e| panic!("{routine}: {e}"));
            assert_eq!(got["calls"], case["calls"], "{case}");
            assert_eq!(got["state"], case["state"], "{case}");
        }
    }
}

#[test]
fn rejects_other_modules() {
    assert!(metal_harbor::MetalHarbor::read(&[0; 16]).is_err());
    assert!(metal_harbor::MetalHarbor::read(&vec![0; metal_harbor::REL_LEN]).is_err());
}
