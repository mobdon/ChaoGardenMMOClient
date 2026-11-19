//! ROM-backed integration regression. Primitive native parity is tested in
//! separate original-C tests; this golden does not imply full retail-loop parity.
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_game::NativeHarness,
    native_player::{FACING_LEFT, IN_AIR},
};

#[test]
#[ignore = "Requires local verified A2NE_ROM"]
fn leaf_forest_replay_integrates_original_movement_sensors_and_animation() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let input = include_bytes!("../examples/leaf-forest-inputs.json");
    let replay: serde_json::Value = serde_json::from_slice(input).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-harness-replay.json")).unwrap();
    assert_eq!(gba::sha256(input), expected["input_sha256"]);
    let mut game = NativeHarness::new(&rom).unwrap();
    let initial = serde_json::to_value(game.snapshot()).unwrap();
    assert!(game.tick(2).is_err());
    assert_eq!(serde_json::to_value(game.snapshot()).unwrap(), initial);
    let mut transitions = vec![serde_json::json!([0, "Ground", 0, 0])];
    let mut previous = (game.snapshot().callback, 0, 0);
    let mut airborne = false;
    let mut landed = false;
    for segment in replay["segments"].as_array().unwrap() {
        let keys = segment["keys"].as_u64().unwrap() as u16;
        for _ in 0..segment["frames"].as_u64().unwrap() {
            let state = game.tick(keys).unwrap();
            assert!(state.halted.is_none(), "{:?}", state.halted);
            if state.player.move_state & IN_AIR != 0 {
                airborne = true;
            } else if airborne {
                landed = true;
                assert_eq!(state.player.sprite_offset_y, 14);
            }
            let next = (state.callback, state.player.char_state, state.variant);
            if next != previous {
                transitions.push(serde_json::json!([
                    state.frame,
                    state.callback,
                    state.animation,
                    state.variant
                ]));
            }
            previous = next;
        }
    }
    let final_state = game.snapshot();
    assert!(airborne && landed);
    assert_eq!(final_state.frame, expected["frames"].as_u64().unwrap());
    assert_eq!(
        i64::from(final_state.player.q_world_x),
        expected["final_q_world_x"].as_i64().unwrap()
    );
    assert_eq!(
        i64::from(final_state.player.q_world_y),
        expected["final_q_world_y"].as_i64().unwrap()
    );
    assert_eq!(final_state.player.q_speed_ground, 0);
    assert_ne!(final_state.player.move_state & FACING_LEFT, 0);
    assert_eq!(
        serde_json::to_value(transitions).unwrap(),
        expected["transitions"]
    );
    assert_eq!(
        gba::sha256(&game.render().unwrap()),
        expected["rgba_sha256"]
    );
}
