//! Compare `land_probe::vertical_probe` with an oracle run of main.dol's
//! fn_800D05D0 on synthetic nodes (scratch oracle probe_ppc.py).
//!
//!     cargo run --release --example land_probe_check -- ORACLE.json
use sa2_asset_tools::land_nodes::{LandNodes, Node, Tri};
use sa2_asset_tools::land_probe::{vertical_probe, Hit};

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let u = |x: &serde_json::Value| x.as_u64().unwrap() as u32;
    let f = |x: &serde_json::Value| f32::from_bits(u(x));
    let cases = v["cases"].as_array().unwrap();
    let (mut bad, mut hits) = (0, 0);
    for c in cases {
        let mut nodes = LandNodes::new();
        for n in c["nodes"].as_array().unwrap() {
            let tris = n["tris"]
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
            nodes.nodes.push(Node {
                obj: n["obj"].as_u64().unwrap() as usize,
                flags: u(&n["flags"]),
                max_dz: 0.0,
                center: [0.0; 3],
                tris,
                cand_first: None,
                cand_count: 0,
            });
        }
        let p = [0, 1, 2].map(|k| f(&c["p"][k]));
        let got = vertical_probe(&nodes, c["obj"].as_u64().unwrap() as usize, p);
        let b: Vec<u32> = c["block"].as_array().unwrap().iter().map(u).collect();
        let rec = |h: Option<Hit>, y: f32| -> Vec<u32> {
            match h {
                Some(h) => vec![
                    1,
                    h.flags,
                    h.angle_x as u32,
                    h.angle_z as u32,
                    y.to_bits(),
                    h.normal[0].to_bits(),
                    h.normal[1].to_bits(),
                    h.normal[2].to_bits(),
                ],
                None => vec![0, b[4], b[5], b[6], y.to_bits(), b[8], b[9], b[10]],
            }
        };
        let (want_below, want_above): (Vec<u32>, Vec<u32>) =
            (b[3..11].to_vec(), b[11..19].to_vec());
        let (got_below, got_above, ret) = match got {
            Some(pr) => {
                let mut ab = rec(pr.above, pr.above_y);
                if pr.above.is_none() {
                    ab[1..4].copy_from_slice(&b[12..15]);
                    ab[5..8].copy_from_slice(&b[16..19]);
                }
                (
                    rec(pr.below, pr.below_y),
                    ab,
                    u32::from(pr.below.is_some() || pr.above.is_some()),
                )
            }
            None => {
                let mut be = want_below.clone();
                be[0] = 0;
                be[4] = (p[1] + 220.0).to_bits();
                let mut ab = want_above.clone();
                ab[0] = 0;
                ab[4] = (p[1] - 220.0).to_bits();
                (be, ab, 0)
            }
        };
        hits += ret;
        if ret != u(&c["ret"]) || got_below != want_below || got_above != want_above {
            bad += 1;
            if bad <= 5 {
                println!("ret {ret} vs {}\n below {got_below:08x?}\n  want {want_below:08x?}\n above {got_above:08x?}\n  want {want_above:08x?}", c["ret"]);
            }
        }
    }
    println!("cases {} ({hits} with hits) mismatches {bad}", cases.len());
}
