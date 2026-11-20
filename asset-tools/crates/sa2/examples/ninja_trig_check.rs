//! Compare `ninja_trig` with an oracle run of main.dol (JSON from #426's
//! tests/ninja_trig_ppc.py: the njSinCos table SHA-256 and MSL sinf/cosf
//! samples).
//!
//!     cargo run --release --example ninja_trig_check -- ORACLE.json
use sa2_asset_tools::ninja_trig::{msl_cosf, msl_sinf, nj_cos, nj_sin};

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut bad = 0;
    let mut smallest = f32::MAX;
    let samples = v["samples"].as_array().unwrap();
    for s in samples {
        let [x, sin, cos] = [0, 1, 2].map(|i| s[i].as_u64().unwrap() as u32);
        let xf = f32::from_bits(x);
        let (gs, gc) = (msl_sinf(xf).to_bits(), msl_cosf(xf).to_bits());
        let same = |a: u32, b: u32| a == b || (f32::from_bits(a).is_nan() && f32::from_bits(b).is_nan());
        if !same(gs, sin) || !same(gc, cos) {
            bad += 1;
            smallest = smallest.min(xf.abs());
            if bad <= 10 {
                println!("x {x:08x}: sin {gs:08x} vs {sin:08x}, cos {gc:08x} vs {cos:08x}");
            }
        }
    }
    let mut table = Vec::with_capacity(65536 * 8);
    for a in 0..65536 {
        table.extend_from_slice(&nj_sin(a).to_be_bytes());
        table.extend_from_slice(&nj_cos(a).to_be_bytes());
    }
    let sha = sa2_asset_tools::manifest::sha256(&table);
    println!(
        "samples {} mismatches {bad} (smallest |x| {smallest:e}); table sha {} ({})",
        samples.len(),
        if sha == v["tableSha256"].as_str().unwrap() { "matches" } else { "DIFFERS" },
        sha
    );
}
