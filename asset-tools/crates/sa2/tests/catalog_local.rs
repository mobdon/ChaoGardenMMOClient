//! Explicit owner-local inventory comparison; no source fixtures are tracked.
use sa2_asset_tools::manifest;
use std::{collections::BTreeMap, path::PathBuf};

#[test]
#[ignore = "requires SA2_GAME_PATH and SA2_REFERENCE_ROOT with verified full-inventory.json"]
fn complete_installation_catalog_matches_verified_hashes() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let root = PathBuf::from(std::env::var_os("SA2_REFERENCE_ROOT").expect("SA2_REFERENCE_ROOT"));
    let reference: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join(".local/full-inventory.json")).unwrap())
            .unwrap();
    assert_eq!(reference["complete"], true);
    let expected = reference["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["relativePath"].as_str().unwrap(), f))
        .collect::<BTreeMap<_, _>>();
    let actual = manifest::catalog(&game).unwrap();
    assert_eq!(actual.sources.len(), expected.len());
    let mut bytes = 0u64;
    for source in &actual.sources {
        let record = expected
            .get(source.logical_path.as_str())
            .expect("new installed file");
        assert_eq!(
            source.sha256,
            record["sha256"].as_str().unwrap(),
            "{}",
            source.logical_path
        );
        assert_eq!(
            source.byte_length,
            record["bytes"]
                .as_u64()
                .or_else(|| record["size"].as_u64())
                .unwrap(),
            "{}",
            source.logical_path
        );
        bytes += source.byte_length;
    }
    assert_eq!((actual.sources.len(), bytes), (4351, 3_253_731_852));
    println!("Verified {} installed source SHA-256 hashes, {bytes} bytes; all asset categories remain candidate-only",actual.sources.len());
}
