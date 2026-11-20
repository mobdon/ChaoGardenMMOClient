//! Compare `land_select::select_static` with main.dol's fn_8002334C on the
//! relocated GameCube stg11D.rel (Iron Gate's objLandTable0011).
use sa2_asset_tools::land_select::{select_static, Counters, LandCol, LandTableHead};
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STG11_REL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol_on_iron_gate() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/land_select_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let f = |x: &serde_json::Value| f32::from_bits(x.as_u64().unwrap() as u32);
    let head = LandTableHead {
        count: v["head"][0].as_i64().unwrap() as i16,
        start: v["head"][1].as_i64().unwrap() as i16,
    };
    let cols: Vec<LandCol> = v["cols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| LandCol {
            center: [f(&c[0]), f(&c[1]), f(&c[2])],
            radius: f(&c[3]),
            flags: c[4].as_u64().unwrap() as u32,
        })
        .collect();
    let mut hits = 0;
    for c in v["cases"].as_array().unwrap() {
        let positions: Vec<[f32; 3]> = c["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [f(&p[0]), f(&p[1]), f(&p[2])])
            .collect();
        let mut counters = Counters::default();
        let got = select_static(head, &cols, &positions, f(&c["range"]), &mut counters);
        let want: Vec<usize> = c["picked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(got, want, "{c}");
        assert_eq!(counters.statics as u64, c["statics"].as_u64().unwrap());
        assert_eq!(counters.total as i64, c["total"].as_i64().unwrap());
        hits += got.len();
    }
    assert!(hits > 300, "too few selections exercised: {hits}");
}
