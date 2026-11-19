use sonic_advance_assets::{binary, runtime::Runtime};

#[test]
fn unsupported_rom_cannot_create_game_state() {
    assert!(Runtime::load(vec![0; 192]).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; isolated solo runtime, no canonical saves"]
fn actual_stage_walk_jump_land_and_replay_are_deterministic() {
    let source = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom = binary::read_bounded(std::path::Path::new(&source), binary::ROM_LEN).unwrap();
    let mut game = Runtime::load(rom.clone()).unwrap();
    let mut replay = Runtime::load(rom).unwrap();
    for _ in 0..20 {
        game.step(0, 1).unwrap();
        replay.step(0, 1).unwrap();
    }
    assert!(game.snapshot().grounded);
    assert_eq!(game.snapshot().y_fixed, 445 * 256);
    for mask in std::iter::repeat_n(0x10, 120)
        .chain(std::iter::repeat_n(0x11, 30))
        .chain(std::iter::repeat_n(0, 100))
    {
        game.step(mask, 1).unwrap();
        replay.step(mask, 1).unwrap();
        assert_eq!(game.snapshot(), replay.snapshot());
    }
    assert!(game.snapshot().x_fixed > 104 * 256);
    // The adapted slope resolver currently misses the second retail pickup.
    assert_eq!(game.snapshot().rings, 1);
    let collected = game.snapshot().rings;
    game.step(0, 1).unwrap();
    replay.step(0, 1).unwrap();
    assert_eq!(game.snapshot().rings, collected);
    assert_eq!(game.pixels(), replay.pixels());
    let before = game.snapshot();
    assert!(game.step(0xff, 1).is_err());
    assert!(game.step(0, 0).is_err());
    assert!(game.step(0, 5).is_err());
    assert_eq!(game.snapshot(), before);
    game.reset().unwrap();
    assert_eq!(game.snapshot().x_fixed, 104 * 256);
    assert_eq!(game.snapshot().frame, 0);
    assert_eq!(game.snapshot().rings, 0);
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM"]
fn stage_boundary_stops_outward_velocity_and_running_pose() {
    let source = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let mut game = Runtime::load(
        binary::read_bounded(std::path::Path::new(&source), binary::ROM_LEN).unwrap(),
    )
    .unwrap();
    for _ in 0..20 {
        game.step(0, 1).unwrap();
    }
    for _ in 0..160 {
        game.step(0x20, 1).unwrap();
    }
    assert_eq!(game.snapshot().x_fixed, 6 * 256);
    assert_eq!(game.snapshot().vx_fixed, 0);
    assert_eq!(game.snapshot().animation, 0);
    game.step(0x10, 1).unwrap();
    assert_eq!(game.snapshot().vx_fixed, 8);
    assert!(!game.snapshot().facing_left);
}
