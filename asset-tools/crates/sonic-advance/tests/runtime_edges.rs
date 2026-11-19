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
    // The curled body is five pixels shorter; the native shift preserves the feet.
    assert_eq!(jump.y_fixed, before.y_fixed + 5 * 256);
    assert_eq!(jump.vy_fixed, -1248);
    assert!(!jump.grounded);
    game.step(0, 1).unwrap();
    let release = game.snapshot();
    assert_eq!(release.y_fixed, jump.y_fixed - 768);
    assert_eq!(release.vy_fixed, -726);
}

// A cold-boot retail replay settles at frame 1799. The practice runtime skips
// the menu and landing intro; align at the same settled position, then drive
// identical input. Compare every tick, not just the final coordinate.
#[test]
#[ignore = "requires local ASOE; reference from original game running in mGBA"]
fn retail_crouch_charge_release_and_first_thirteen_roll_ticks() {
    let mut game = Runtime::load(source()).unwrap();
    for _ in 0..5 { game.step(0, 4).unwrap(); }
    let mut trace = Vec::new();
    for (i, mask) in std::iter::once(None)
        .chain([0x80,0x81,0x80,0x81,0x80,0x81,0x80,0x80,0x80].into_iter().map(Some))
        .chain(std::iter::repeat_n(Some(0), 14)).enumerate() {
        if let Some(mask) = mask { game.step(mask, 1).unwrap(); }
        let p = game.snapshot();
        for field in [p.x_fixed,p.y_fixed,p.vx_fixed,p.vy_fixed,p.ground_speed,
                      p.body_half_height,i32::from(p.animation),i32::from(p.variant)] {
            trace.extend_from_slice(&field.to_le_bytes());
        }
        if (2..=9).contains(&i) {
            assert_eq!(p.charge, [0,0,2048,1984,2434,2358,2285,2214][i-2], "retail frame {}", 1799+i);
        }
        assert_eq!(p.dust_animation, match i { 2..=3 => Some(746), 4..=9 => Some(747), _ => None });
        if i == 10 {
            assert_eq!((p.x_fixed,p.vx_fixed,p.ground_speed), (26624,0,2304));
            assert!(p.rolling && !p.spindashing);
        }
    }
    assert_eq!(binary::sha256(&trace), "3cc0a91c9d8c4d7f4e501f77ae43e6ba7bb0ec3a03cc63220c4689d069c56b63");
}

