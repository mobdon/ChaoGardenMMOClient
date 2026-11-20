//! Deterministic input-only integration evidence, not full-course acceptance.
use sonic_advance_2_assets::{VerifiedRom, native_game::NativeHarness};

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn exact_input_routes_reach_recorded_leaf_forest_geometry() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let report: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-input-routes.json")).unwrap();
    for route in report["routes"].as_array().unwrap() {
        let level = route["level"].as_u64().unwrap() as u8;
        let mut game = NativeHarness::for_level(&rom, level).unwrap();
        for segment in route["segments"].as_array().unwrap() {
            let keys = segment["keys"].as_u64().unwrap() as u16;
            for _ in 0..segment["frames"].as_u64().unwrap() {
                let snapshot = game.tick(keys).unwrap();
                assert!(snapshot.halted.is_none());
                assert!(snapshot.game_over.is_none());
                assert_eq!(snapshot.lives, 3);
            }
        }
        let actual = serde_json::to_value(game.snapshot()).unwrap();
        for (key, expected) in route["expected"].as_object().unwrap() {
            assert_eq!(&actual[key], expected, "level{level}: {key}");
        }
    }
}
