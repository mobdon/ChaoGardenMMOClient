//! Compare `chao_pipe::outlet` with an oracle run of main.dol (njUnitMatrix,
//! njTranslateV, njRotateZ/X/Y, njCalcPoint of (0, 3, 2)).
//!
//!     cargo run --release --example chao_pipe_check -- ORACLE.json
use sa2_asset_tools::chao_pipe::outlet;

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let samples = v["samples"].as_array().unwrap();
    let mut bad = 0;
    for s in samples {
        let pos = [0, 1, 2].map(|i| f32::from_bits(s["pos"][i].as_u64().unwrap() as u32));
        let ang = [0, 1, 2].map(|i| s["ang"][i].as_i64().unwrap() as i32);
        let got = outlet(pos, ang).map(f32::to_bits);
        let want = [0, 1, 2].map(|i| s["out"][i].as_u64().unwrap() as u32);
        if got != want {
            bad += 1;
            if bad <= 10 {
                println!("{pos:?} {ang:?}: {got:08x?} vs {want:08x?}");
            }
        }
    }
    println!("samples {} mismatches {bad}", samples.len());
}
