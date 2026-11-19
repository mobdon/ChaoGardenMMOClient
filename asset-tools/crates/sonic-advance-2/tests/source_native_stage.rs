use serde::Deserialize;
use sonic_advance_2_assets::{VerifiedRom, gba, native_stage::NativeStage};

#[derive(Deserialize)]
struct Resources {
    resources: Vec<Resource>,
}
#[derive(Deserialize)]
struct Resource {
    name: String,
    offset: usize,
    length: usize,
    sha256: String,
    reference_git_blob_sha1: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    seed: u32,
    queries: usize,
    output_bytes: usize,
    output_sha256: String,
}

fn next(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^ (x << 5)
}

#[test]
#[ignore = "A2NE_ROM actual source; native_stage_oracle.py records independently compiled original C results"]
fn original_stage_collision_routines_match_rust() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let resources: Resources =
        serde_json::from_str(include_str!("../catalog/native-stage-resources.json")).unwrap();
    let mut independent_banks = 0;
    for resource in resources.resources {
        let bytes = rom.range("rom", resource.offset, resource.length).unwrap();
        assert_eq!(gba::sha256(bytes), resource.sha256, "{}", resource.name);
        if let Some(expected_blob) = resource.reference_git_blob_sha1 {
            let mut blob = format!("blob {}\0", bytes.len()).into_bytes();
            blob.extend_from_slice(bytes);
            assert_eq!(gba::sha1(&blob), expected_blob, "{}", resource.name);
            independent_banks += 1;
        }
    }
    assert_eq!(independent_banks, 6);
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-stage-parity.json")).unwrap();
    assert_eq!(expected.queries, 4096);
    let mut seed = expected.seed;
    let mut result = Vec::new();
    for _ in 0..expected.queries {
        seed = next(seed);
        let x = (seed % 19840) as i32 - 32;
        seed = next(seed);
        let y = (seed % 3136) as i32 - 32;
        seed = next(seed);
        let layer = (seed & 1) as u8 + ((seed >> 1 & 1) * 128) as u8;
        let step = if seed & 4 != 0 { 8 } else { -8 };
        let v = stage.vertical_sample(x, y, layer).unwrap();
        let h = stage.horizontal_sample(x, y, layer).unwrap();
        let pv = stage.probe(x, y, layer, step, true).unwrap();
        let ph = stage.probe(x, y, layer, step, false).unwrap();
        let lv = stage.long_probe(x, y, layer, step, true).unwrap();
        let lh = stage.long_probe(x, y, layer, step, false).unwrap();
        let row = [
            v.extent,
            v.rotation as i32,
            h.extent,
            h.rotation as i32,
            pv.distance,
            pv.rotation.unwrap_or(0x5a) as i32,
            ph.distance,
            ph.rotation.unwrap_or(0x5a) as i32,
            lv.distance,
            lv.rotation.unwrap_or(0x5a) as i32,
            lh.distance,
            lh.rotation.unwrap_or(0x5a) as i32,
        ];
        for word in row {
            result.extend_from_slice(&word.to_le_bytes());
        }
    }
    assert_eq!(result.len(), expected.output_bytes);
    assert_eq!(gba::sha256(&result), expected.output_sha256);
    assert!(stage.tile_at(-1, 0, 0).is_err());
    assert!(stage.tile_at(0, 0, 2).is_err());
    assert!(stage.probe(96, 671, 0, 0, true).is_err());
    assert!(stage.render_view(0, 0, usize::MAX, 2).is_err());
}
