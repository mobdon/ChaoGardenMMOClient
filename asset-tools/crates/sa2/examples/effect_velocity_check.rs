//! Compare `effect_emitter::turn_velocity` with an oracle run of main.dol
//! (njUnitMatrix, njTranslateV, njRotateY, njRotateZ, njCalcVector).
//!
//!     cargo run --release --example effect_velocity_check -- ORACLE.json
use sa2_asset_tools::effect_emitter::turn_velocity;

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let f = |x: &serde_json::Value| f32::from_bits(x.as_u64().unwrap() as u32);
    let samples = v["samples"].as_array().unwrap();
    let mut bad = 0;
    for s in samples {
        let vin = [0, 1, 2].map(|i| f(&s["v"][i]));
        let ry = s["ry"].as_i64().unwrap() as i32;
        let rz = s["rz"].as_i64().unwrap() as i32;
        let got = turn_velocity(vin, ry, rz).map(f32::to_bits);
        let want = [0, 1, 2].map(|i| s["out"][i].as_u64().unwrap() as u32);
        if got != want {
            bad += 1;
            if bad <= 10 {
                println!("{vin:?} ry {ry} rz {rz}: {got:08x?} vs {want:08x?}");
            }
        }
    }
    println!("samples {} mismatches {bad}", samples.len());
}
