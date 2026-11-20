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
    let mut pickup_pans = Vec::new();
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
            pickup_pans.extend_from_slice(&state.ring_sound_pans);
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
    assert_eq!(
        u64::from(final_state.rings),
        expected["rings"].as_u64().unwrap()
    );
    assert_eq!(pickup_pans, [-64, 64, -64]);
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

#[test]
#[ignore = "requires local verified A2NE_ROM"]
fn fourteen_course_runs_exercise_springs_layer_switches_death_and_checkpoint_retries() {
    use sonic_advance_2_assets::{native_game::HarnessCallback, native_lifecycle::Phase};
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let (mut springs, mut lower, mut retry, mut dying) = (false, false, false, false);
    for level in (0..28).filter(|n| n % 4 < 2) {
        let mut game = NativeHarness::for_level(&rom, level).unwrap();
        let mut prev = game.snapshot();
        for _ in 0..1800 {
            let next = game.tick(16).unwrap();
            assert!(
                next.halted.is_none(),
                "level {level} frame {}: {:?}",
                next.frame,
                next.halted
            );
            springs |= next.callback == HarnessCallback::Spring
                && (next.animation_sound_ids.contains(&153) || (8..=9).contains(&level));
            lower |= prev.player.layer != next.player.layer;
            dying |= next.lifecycle.phase == Phase::Dying;
            if prev.lifecycle.phase == (Phase::Waiting { frames: 0 })
                && next.lifecycle.phase == Phase::Alive
            {
                assert_eq!(next.lives, prev.lives - 1);
                assert_eq!(next.rings, 0);
                assert_eq!(
                    next.player.q_world_x,
                    i32::from(prev.lifecycle.checkpoint.x) * 256
                );
                assert_eq!(
                    next.player.q_world_y,
                    i32::from(prev.lifecycle.checkpoint.y) * 256
                );
                retry = true;
            }
            prev = next;
        }
        assert_eq!(prev.frame, 1800, "level {level}");
    }
    assert!(
        springs && lower && retry && dying,
        "spring={springs}, layer={lower}, retry={retry}, death={dying}"
    );
}

#[test]
#[ignore = "requires local verified A2NE_ROM"]
fn second_jump_edge_emits_native_insta_shield_in_the_player_loop() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut game = NativeHarness::new(&rom).unwrap();
    for input in [0, 1, 1, 0] {
        assert!(game.tick(input).unwrap().halted.is_none());
    }
    let action = game.tick(1).unwrap();
    assert!(action.halted.is_none());
    assert_eq!((action.player.char_state, action.animation), (18, 17));
    assert!(action.animation_sound_ids.contains(&113));
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn down_and_jump_launch_sonic_from_rest_into_a_moving_roll() {
    use sonic_advance_2_assets::native_game::HarnessCallback as C;
    let b = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&b).unwrap();
    let mut game = NativeHarness::new(&rom).unwrap();
    for _ in 0..8 {
        game.tick(0).unwrap();
    }
    assert_eq!(game.tick(128).unwrap().callback, C::InitCrouch);
    assert_eq!(game.tick(128).unwrap().animation, 2);
    let first = game.tick(129).unwrap();
    assert_eq!(first.callback, C::Spindash);
    assert_eq!(first.animation, 3);
    assert_eq!(first.animation_sound_ids, vec![109, 109]);
    game.tick(128).unwrap();
    let rev = game.tick(129).unwrap();
    assert_eq!(rev.animation_sound_ids, vec![109]);
    assert_eq!(rev.variant, 1);
    let release = game.tick(0).unwrap();
    assert_eq!(release.callback, C::InitRoll);
    assert_eq!(release.animation_sound_ids, vec![110]);
    assert_eq!(release.player.q_speed_ground, 2304);
    let roll = game.tick(0).unwrap();
    assert_eq!(roll.callback, C::Rolling);
    assert_eq!(roll.animation, 4);
    assert!(roll.player.q_world_x > release.player.q_world_x);
    assert_eq!(roll.player.sprite_offset_y, 9);
}
