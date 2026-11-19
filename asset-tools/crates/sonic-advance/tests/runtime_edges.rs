//! Real-source integration edges; no ROM or decoded fixture is distributed.
use sonic_advance_assets::{binary, runtime::Runtime};

fn source() -> Vec<u8> {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap()
}

#[test]
#[ignore = "requires local SONIC_ADVANCE_ASOE_ROM; standalone practice only"]
fn batched_input_and_individual_ticks_have_identical_state_and_pixels() {
    let rom = source();
    let mut batch = Runtime::load(rom.clone()).unwrap();
    let mut individual = Runtime::load(rom).unwrap();
    for mask in [0, 0x10, 0x10, 0x11, 0x10, 0, 0x20, 0x31, 0, 1, 1, 0] {
        batch.step(mask, 4).unwrap();
        for _ in 0..4 {
            individual.step(mask, 1).unwrap();
        }
        assert_eq!(batch.snapshot(), individual.snapshot());
        assert_eq!(batch.pixels(), individual.pixels());
    }
}

#[test]
#[ignore = "requires local SONIC_ADVANCE_ASOE_ROM; jump rules are reference-derived"]
fn terrain_integration_preserves_native_jump_transition_then_release_order() {
    let mut game = Runtime::load(source()).unwrap();
    for _ in 0..20 {
        game.step(0, 1).unwrap();
    }
    let before = game.snapshot();
    assert!(before.grounded);
    game.step(1, 1).unwrap();
    let jump = game.snapshot();
    assert_eq!(jump.x_fixed, before.x_fixed);
    assert_eq!(jump.y_fixed, before.y_fixed);
    assert_eq!(jump.vy_fixed, -1248);
    assert!(!jump.grounded);
    game.step(0, 1).unwrap();
    let release = game.snapshot();
    assert_eq!(release.y_fixed, before.y_fixed - 768);
    assert_eq!(release.vy_fixed, -726);
}
