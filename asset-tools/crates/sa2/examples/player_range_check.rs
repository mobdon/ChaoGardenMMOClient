//! Compare `player_range` with an oracle run of main.dol: the range test
//! (0x800398D8) and vector length (0x801151F4) under Unicorn, with Gekko's
//! `frsqrte` and `fres` estimates in place of the emulator's exact ones.
//!
//!     cargo run --release --example player_range_check -- ORACLE.json
use sa2_asset_tools::player_range::{player_in_range, vector_length};

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let f = |x: &serde_json::Value| f32::from_bits(x.as_u64().unwrap() as u32);
    let v3 = |x: &serde_json::Value| [f(&x[0]), f(&x[1]), f(&x[2])];
    let samples = v["samples"].as_array().unwrap();
    let (mut bad_hit, mut bad_len, mut hits) = (0, 0, 0);
    for s in samples {
        let players = [0, 1].map(|i| (!s["players"][i].is_null()).then(|| v3(&s["players"][i])));
        let got = player_in_range(&players, v3(&s["query"]), f(&s["radius"]));
        let want = s["hit"].as_u64().unwrap() as u32;
        hits += u32::from(want != 0);
        if got != want {
            bad_hit += 1;
            if bad_hit <= 10 {
                println!("hit {got} vs {want}: {s}");
            }
        }
        let len = vector_length(v3(&s["vec"])).to_bits();
        let want = s["length"].as_u64().unwrap() as u32;
        if len != want {
            bad_len += 1;
            if bad_len <= 10 {
                println!("length {len:08x} vs {want:08x}: {}", s["vec"]);
            }
        }
    }
    println!(
        "samples {} ({hits} in range): range mismatches {bad_hit}, length mismatches {bad_len}",
        samples.len()
    );
}
