//! Compare `land_probe::six_way_probe` with main.dol's fn_800CFE6C on
//! synthetic collision nodes (tests/land_probe_xyz_ppc.py).
use sa2_asset_tools::{
    land_nodes::{LandNodes, Node, Tri},
    land_probe::{six_way_probe, PROBE_REACH as LIMIT, RAYS},
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}

#[test]
#[ignore = "requires SA2_MAIN_DOL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn six_way_probe_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/land_probe_xyz_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let f = |v: &Value| f32::from_bits(u(v));
    let mut hits = 0;
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let tris = case["tris"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| Tri {
                center: [0.0; 3],
                radius_sq: 0.0,
                v: [0, 1, 2].map(|q| [0, 1, 2].map(|k| f(&t[q][k]))),
                attr: 0,
            })
            .collect();
        let mut nodes = LandNodes::new();
        nodes.nodes.push(Node {
            obj: 0,
            flags: u(&case["flags"]),
            max_dz: 0.0,
            center: [0.0; 3],
            tris,
            cand_first: None,
            cand_count: 0,
        });
        let p = [0, 1, 2].map(|k| f(&case["p"][k]));
        let found = u(&case["found"]) != 0;
        let got = six_way_probe(&nodes, if found { 0 } else { 1 }, p);
        let block: Vec<u32> = case["block"].as_array().unwrap().iter().map(u).collect();
        let mut want_ret = 0;
        let init = [
            LIMIT + p[0],
            p[0] - LIMIT,
            LIMIT + p[1],
            p[1] - LIMIT,
            LIMIT + p[2],
            p[2] - LIMIT,
        ];
        for r in 0..RAYS {
            let w = &block[3 + 8 * r..3 + 8 * r + 8];
            let (hit, coord) = match &got {
                Some(g) => (g.hits[r], g.coords[r]),
                None => (None, init[r]),
            };
            match hit {
                Some(h) => {
                    want_ret = 1;
                    hits += 1;
                    assert_eq!(
                        w.to_vec(),
                        vec![
                            1,
                            h.flags,
                            h.angle_x as u32,
                            h.angle_z as u32,
                            coord.to_bits(),
                            h.normal[0].to_bits(),
                            h.normal[1].to_bits(),
                            h.normal[2].to_bits(),
                        ],
                        "case {ci} ray {r}"
                    );
                }
                None => {
                    assert_eq!(w[0], 0, "case {ci} ray {r}: main.dol hit");
                    assert_eq!(w[4], coord.to_bits(), "case {ci} ray {r}: coordinate");
                }
            }
        }
        assert_eq!(u(&case["ret"]), want_ret, "case {ci}: return");
    }
    assert!(hits > 500, "{hits}");
}
