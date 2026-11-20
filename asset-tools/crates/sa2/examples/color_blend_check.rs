//! Compare `color_blend::blend_argb` with an oracle run of main.dol 0x800334B0.
//!
//!     cargo run --release --example color_blend_check -- ORACLE.json
use sa2_asset_tools::color_blend::blend_argb;

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let samples = v["samples"].as_array().unwrap();
    let mut bad = 0;
    for s in samples {
        let [a, b, t, want] = [0, 1, 2, 3].map(|i| s[i].as_u64().unwrap() as u32);
        let got = blend_argb(a, b, f32::from_bits(t));
        if got != want {
            bad += 1;
            if bad <= 10 {
                println!(
                    "{a:08x} {b:08x} t {}: {got:08x} vs {want:08x}",
                    f32::from_bits(t)
                );
            }
        }
    }
    println!("samples {} mismatches {bad}", samples.len());
}
