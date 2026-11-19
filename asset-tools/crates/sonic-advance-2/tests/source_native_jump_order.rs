use sonic_advance_2_assets::{
    VerifiedRom,
    native_game::{HarnessCallback, NativeHarness},
    native_runtime::A_BUTTON,
};

#[test]
#[ignore = "requires the local verified A2NE_ROM"]
fn jump_request_defers_impulse_but_release_and_gravity_run_on_impulse_frame() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    // Pinned shared/stage/player.c:9653, 6980–7003, 7024–7044.
    // Flat, dry ground: native impulse is -4.875 pixels/frame, release caps
    // it at -3, and gravity adds 42/256 before the position update.
    for (second_input, expected_dy) in [(0, -726), (A_BUTTON, -1206)] {
        let mut game = NativeHarness::new(&rom).unwrap();
        let grounded = game.tick(0).unwrap();
        assert_eq!(grounded.player.rotation, 0);
        let request = game.tick(A_BUTTON).unwrap();
        assert_eq!(request.callback, HarnessCallback::InitJump);
        assert_eq!(request.player.q_world_y, grounded.player.q_world_y);
        let impulse = game.tick(second_input).unwrap();
        assert!(impulse.halted.is_none(), "{:?}", impulse.halted);
        assert_eq!(impulse.callback, HarnessCallback::Jumping);
        assert_eq!(i32::from(impulse.player.q_speed_air_y), expected_dy);
        assert_eq!(
            impulse.player.q_world_y - request.player.q_world_y,
            expected_dy
        );
    }
}
