//! The routines `stage_objects` ports are the same code in every stage REL
//! that carries them: their relocation-masked bytes in stg42D.rel hash to
//! the recorded value, and the same bytes (with module-local branch
//! displacements masked too, `branch_masked_text`) appear in the other
//! GameCube stage RELs. Needs SA2_GC_REL_DIR (the RELs from the user's own
//! disc, never committed).
use sa2_asset_tools::{manifest::sha256, stage_objects};
use std::path::PathBuf;

fn rel_dir() -> Option<PathBuf> {
    std::env::var_os("SA2_GC_REL_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn routines_are_shared_across_stage_rels() {
    let dir = rel_dir().expect("SA2_GC_REL_DIR");
    let rel = std::fs::read(dir.join("stg42D.rel")).unwrap();
    let plain = stage_objects::masked_text(&rel).unwrap();
    let reference = stage_objects::branch_masked_text(&rel).unwrap();
    let mut others = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".rel")
            && name != "stg42D.rel"
            && (name.starts_with("stg") || name.to_lowercase().starts_with("boss"))
        {
            let text = stage_objects::branch_masked_text(&std::fs::read(&p).unwrap()).unwrap();
            others.push((name, text));
        }
    }
    others.sort();
    let mut bad = Vec::new();
    let mut total = 0;
    for (module, routines) in stage_objects::MODULES {
        for r in *routines {
            let (a, b) = (r.stg42d as usize, (r.stg42d + r.len) as usize);
            let hash = sha256(&plain[a..b]);
            if hash != r.masked_sha256 {
                bad.push(format!("{} hash {hash}", r.name));
            }
            let code = &reference[a..b];
            let found = others
                .iter()
                .filter(|(_, t)| t.windows(code.len()).step_by(4).any(|w| w == code))
                .count();
            eprintln!("{module:30} {:50} in {found:2} other RELs", r.name);
            if found == 0 {
                bad.push(format!("{} found in no other REL", r.name));
            }
            total += 1;
        }
    }
    eprintln!("{total} routines checked");
    assert!(bad.is_empty(), "{bad:#?}");
}
