//! The routines `stage_objects` ports are the same code in every stage REL
//! that carries them: their relocation-masked bytes in stg42D.rel hash to
//! the recorded value, and the same bytes appear in the other GameCube
//! stage RELs. Needs SA2_GC_REL_DIR (the RELs from the user's own disc,
//! never committed).
use sa2_asset_tools::{manifest::sha256, stage_objects};
use std::path::PathBuf;

fn rel_dir() -> Option<PathBuf> {
    std::env::var_os("SA2_GC_REL_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn routines_are_shared_across_stage_rels() {
    let dir = rel_dir().expect("SA2_GC_REL_DIR");
    let reference =
        stage_objects::masked_text(&std::fs::read(dir.join("stg42D.rel")).unwrap()).unwrap();
    let mut others = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".rel")
            && name != "stg42D.rel"
            && (name.starts_with("stg") || name.to_lowercase().starts_with("boss"))
        {
            others.push((
                name,
                stage_objects::masked_text(&std::fs::read(&p).unwrap()).unwrap(),
            ));
        }
    }
    others.sort();
    let mut bad = Vec::new();
    for r in stage_objects::routines() {
        let code = &reference[r.stg42d as usize..(r.stg42d + r.len) as usize];
        let hash = sha256(code);
        if hash != r.masked_sha256 {
            bad.push(format!("{} hash {hash}", r.name));
        }
        let found: Vec<&str> = others
            .iter()
            .filter(|(_, t)| {
                (0..t.len().saturating_sub(code.len()))
                    .step_by(4)
                    .any(|o| &t[o..o + code.len()] == code)
            })
            .map(|(n, _)| n.as_str())
            .collect();
        eprintln!("{:40} in {:2} other RELs", r.name, found.len());
        if found.is_empty() {
            bad.push(format!("{} found in no other REL", r.name));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
