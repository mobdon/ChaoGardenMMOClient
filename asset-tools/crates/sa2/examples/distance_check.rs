//! Compare `player_range::distance` with an oracle run of main.dol
//! 0x8011B65C (and its sqrt 0x8002F15C).
//!
//!     cargo run --release --example distance_check -- ORACLE.json
use sa2_asset_tools::player_range::distance;

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let f = |x: &serde_json::Value| f32::from_bits(x.as_u64().unwrap() as u32);
    let samples = v["samples"].as_array().unwrap();
    let mut bad = 0;
    for s in samples {
        let a = [0, 1, 2].map(|k| f(&s[0][k]));
        let b = [0, 1, 2].map(|k| f(&s[1][k]));
        let want = s[2].as_u64().unwrap() as u32;
        let got = distance(a, b).to_bits();
        if got != want {
            bad += 1;
            if bad <= 10 {
                println!("{a:?} {b:?}: {got:08x} vs {want:08x}");
            }
        }
    }
    println!("samples {} mismatches {bad}", samples.len());
}
