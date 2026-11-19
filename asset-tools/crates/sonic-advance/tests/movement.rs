use sonic_advance_assets::movement::{Input, Movement, BUTTON_JUMP, ONE};

const RIGHT: Input = Input {
    left: false,
    right: true,
    jump: false,
};
const JUMP: Input = Input {
    left: false,
    right: false,
    jump: true,
};

fn on_ground() -> Movement {
    let mut movement = Movement::new(10, 20);
    movement.land_at(20);
    movement
}

#[test]
fn ground_acceleration_and_cap_match_integer_reference_vector() {
    let mut movement = on_ground();
    for _ in 0..120 {
        movement.advance(RIGHT);
    }
    assert_eq!(movement.player.vx, 960);
    assert_eq!(movement.player.x, 10 * ONE + 58080);
    for _ in 0..24 {
        movement.advance(RIGHT);
    }
    assert_eq!(movement.player.vx, 1152);
    assert_eq!(movement.player.x, 10 * ONE + 83520);
    movement.advance(RIGHT);
    assert_eq!(movement.player.vx, 1152);
    assert_eq!(movement.player.x, 10 * ONE + 84672);
}

#[test]
fn brake_reversal_and_coasting_preserve_reference_zero_crossing() {
    let mut movement = on_ground();
    movement.player.ground_speed = -1;
    movement.advance(RIGHT);
    assert_eq!(movement.player.ground_speed, 96);
    movement.advance(Input::default());
    assert_eq!(movement.player.ground_speed, 88);
    for _ in 0..11 {
        movement.advance(Input::default());
    }
    assert_eq!(movement.player.ground_speed, 0);
    movement.player.ground_speed = 1200;
    movement.advance(RIGHT);
    assert_eq!(movement.player.ground_speed, 1200);
}

#[test]
fn jump_transition_then_position_then_gravity_match_reference_order() {
    let mut movement = on_ground();
    movement.advance(JUMP);
    assert_eq!((movement.player.y, movement.player.vy), (20 * ONE, -1248));
    assert!(!movement.player.grounded);
    assert_eq!(movement.frame_input(), BUTTON_JUMP);
    movement.advance(JUMP);
    assert_eq!(
        (movement.player.y, movement.player.vy),
        (20 * ONE - 1248, -1206)
    );
    assert_eq!(movement.frame_input(), 0);
    for _ in 1..30 {
        movement.advance(JUMP);
    }
    assert_eq!(
        (movement.player.y, movement.player.vy),
        (20 * ONE - 19170, 12)
    );
}

#[test]
fn release_cuts_jump_before_integration_and_held_button_does_not_autojump() {
    let mut movement = on_ground();
    movement.advance(JUMP);
    movement.advance(Input::default());
    assert_eq!(
        (movement.player.y, movement.player.vy),
        (20 * ONE - 768, -726)
    );
    movement.advance(JUMP);
    movement.land_at(35);
    movement.advance(JUMP);
    assert!(movement.player.grounded);
    assert_eq!(movement.player.y, 35 * ONE);
    movement.advance(Input::default());
    movement.advance(JUMP);
    assert!(!movement.player.grounded);
}

#[test]
fn ground_and_air_opposite_keys_have_distinct_native_rules() {
    let both = Input {
        left: true,
        right: true,
        jump: false,
    };
    let mut movement = on_ground();
    movement.player.ground_speed = 64;
    movement.advance(both);
    assert_eq!(movement.player.vx, 64);
    movement.player.grounded = false;
    movement.advance(both);
    assert_eq!(movement.player.vx, 48);
    assert!(movement.player.facing_left);
}

#[test]
fn blocked_jump_consumes_edge_instead_of_delaying_it_until_clearance() {
    let mut movement = on_ground();
    movement.advance_with_jump_clearance(JUMP, 3);
    assert!(movement.player.grounded);
    movement.advance_with_jump_clearance(JUMP, 4);
    assert!(movement.player.grounded);
    movement.advance(Input::default());
    movement.advance_with_jump_clearance(JUMP, 4);
    assert!(!movement.player.grounded);
}

#[test]
fn air_drag_uses_signed_shift_and_sa1_has_no_sa2_fall_cap() {
    let mut movement = Movement::new(0, 0);
    movement.player.vx = -31;
    movement.player.vy = -1;
    movement.advance(Input::default());
    assert_eq!(movement.player.vx, -30);
    assert_eq!(movement.player.y, -1);
    movement.player.vy = 4000;
    movement.advance(Input::default());
    assert_eq!(movement.player.vy, 4042);
}

#[test]
fn extreme_public_state_is_normalized_to_native_words_without_panics() {
    let mut movement = Movement::new(i32::MAX, i32::MIN);
    movement.player.vx = i32::MAX;
    movement.player.vy = i32::MAX;
    movement.player.ground_speed = i32::MIN;
    movement.player.frame = u64::MAX;
    movement.advance(RIGHT);
    assert_eq!(movement.player.frame, 0);
    assert_eq!(movement.player.vy, 41);
}

#[test]
fn down_enters_roll_with_native_body_shift_and_first_frame_friction() {
    let mut movement = on_ground();
    movement.player.ground_speed = 256;
    movement.player.vx = 256;
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert!(movement.is_rolling());
    assert_eq!(movement.native_animation(), Some((8, 0)));
    assert_eq!(movement.body_half_height(), 9);
    assert_eq!(movement.player.y, 25 * ONE);
    assert_eq!(movement.player.vx, 248);
    assert_eq!(movement.player.x, 10 * ONE + 248);
    movement.advance_with_actions(RIGHT, false, 4).unwrap();
    assert_eq!(movement.player.vx, 244);
    for _ in 0..61 {
        movement.advance_with_actions(RIGHT, false, 4).unwrap();
    }
    assert!(!movement.is_rolling());
    assert_eq!(movement.player.vx, 0);
    assert_eq!(movement.player.y, 20 * ONE);
    assert_eq!(movement.body_half_height(), 14);
}

#[test]
fn rolling_brake_uses_twenty_four_then_half_acceleration_friction() {
    let mut movement = on_ground();
    movement.player.ground_speed = 20;
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert_eq!(movement.player.vx, 12);
    movement
        .advance_with_actions(
            Input {
                left: true,
                ..Input::default()
            },
            false,
            4,
        )
        .unwrap();
    assert_eq!(movement.player.vx, -92);
    assert!(!movement.player.facing_left);
    movement
        .advance_with_actions(
            Input {
                left: true,
                ..Input::default()
            },
            false,
            4,
        )
        .unwrap();
    assert_eq!(movement.player.vx, -88);
    assert!(movement.player.facing_left);
}

#[test]
fn rolling_jump_locks_directional_air_control_and_landing_uncurls_once() {
    let mut movement = on_ground();
    movement.player.ground_speed = 256;
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    let x = movement.player.x;
    movement.advance_with_actions(JUMP, false, 4).unwrap();
    assert_eq!(movement.player.x, x);
    assert_eq!(movement.player.vx, 248);
    assert_eq!(movement.player.vy, -1248);
    movement
        .advance_with_actions(
            Input {
                right: true,
                jump: true,
                left: false,
            },
            false,
            4,
        )
        .unwrap();
    assert_eq!(movement.player.vx, 241);
    movement.land_at(25);
    assert!(!movement.is_rolling());
    assert_eq!(movement.body_half_height(), 14);
    assert_eq!(movement.player.y, 20 * ONE);
    // Continued grounded settlement must not repeat the uncurl displacement.
    movement.land_at(20);
    assert_eq!(movement.player.y, 20 * ONE);
}

#[test]
fn crouch_precedes_spindash_and_missing_source_rejects_atomically() {
    let mut movement = on_ground();
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert!(movement.is_crouched());
    assert_eq!(movement.native_animation(), Some((3, 0)));
    let before = movement.clone();
    assert!(movement.advance_with_actions(JUMP, true, 4).is_err());
    assert_eq!(movement, before);
    assert!(movement.configure_spindash(&[0; 192]).is_err());
    assert_eq!(movement, before);
    let mut simultaneous = on_ground();
    simultaneous.advance_with_actions(JUMP, true, 4).unwrap();
    assert!(!simultaneous.player.grounded);
    assert!(!simultaneous.is_spindashing());
    assert_eq!(simultaneous.body_half_height(), 9);
    assert_eq!(simultaneous.player.y, 25 * ONE);
}

#[test]
fn horizontal_input_suppresses_down_and_tiny_roll_stops_on_following_tick() {
    let mut movement = on_ground();
    movement.advance_with_actions(RIGHT, true, 4).unwrap();
    assert!(!movement.is_crouched());
    assert!(!movement.is_rolling());
    assert_eq!(movement.held_input() & 0x80, 0);
    movement.player.ground_speed = 1;
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert!(movement.is_rolling());
    assert_eq!(movement.player.vx, 0);
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert!(movement.is_crouched());
    assert!(!movement.is_rolling());
    assert_eq!(movement.body_half_height(), 14);
}

#[test]
#[ignore = "requires exact local SONIC_ADVANCE_ASOE_ROM release lookup"]
fn actual_source_spindash_preserves_retail_overcharge_and_frame_order() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let mut movement = on_ground();
    movement.configure_spindash(&rom).unwrap();
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    movement.advance_with_actions(JUMP, true, 4).unwrap();
    assert!(movement.is_spindashing());
    assert_eq!(movement.spindash_charge(), 0);
    let x = movement.player.x;
    let y = movement.player.y;
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    movement.advance_with_actions(JUMP, true, 4).unwrap();
    assert_eq!(movement.spindash_charge(), 2048);
    assert_eq!(movement.native_animation(), Some((18, 1)));
    movement.notify_animation_finished();
    assert_eq!(movement.native_animation(), Some((18, 0)));
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    assert_eq!(movement.spindash_charge(), 1984);
    movement.advance_with_actions(JUMP, true, 4).unwrap();
    assert_eq!(movement.spindash_charge(), 2434);
    movement
        .advance_with_actions(Input::default(), false, 4)
        .unwrap();
    assert!(!movement.is_spindashing());
    assert!(movement.is_rolling());
    // Charge index nine deliberately reads the adjacent retail table word.
    assert_eq!(movement.player.ground_speed, 2112);
    assert_eq!(movement.player.vx, 0);
    assert_eq!((movement.player.x, movement.player.y), (x, y));
    movement.advance_with_actions(RIGHT, false, 4).unwrap();
    assert_eq!(movement.player.vx, 2108);
    assert_eq!(movement.player.x, x + 2108);
}

#[test]
#[ignore = "requires exact local SONIC_ADVANCE_ASOE_ROM release lookup"]
fn repeated_physical_rev_edges_remain_inside_verified_thirty_three_word_prefix() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let mut movement = on_ground();
    movement.configure_spindash(&rom).unwrap();
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap();
    movement.advance_with_actions(JUMP, true, 4).unwrap();
    for _ in 0..4096 {
        movement
            .advance_with_actions(Input::default(), true, 4)
            .unwrap();
        movement.advance_with_actions(JUMP, true, 4).unwrap();
    }
    assert_eq!(movement.spindash_charge(), 8324);
    movement.player.facing_left = true;
    movement
        .advance_with_actions(Input::default(), false, 4)
        .unwrap();
    assert_eq!(movement.player.ground_speed, 2295);
}

#[test]
#[ignore = "requires local ROM and the independently recorded retail mGBA trace"]
fn spindash_ticks_match_retail_mgba_1800_through_1810() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let mut movement = on_ground();
    movement.configure_spindash(&rom).unwrap();
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap(); // 1800 crouch
    assert_eq!(movement.body_half_height(), 14);
    movement.advance_with_actions(JUMP, true, 4).unwrap(); // 1801 charge
    assert_eq!(movement.player.y, 25 * ONE);
    assert_eq!(movement.body_half_height(), 9);
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap(); // 1802
    movement.advance_with_actions(JUMP, true, 4).unwrap(); // 1803 rev
    assert_eq!(movement.spindash_charge(), 2048);
    movement
        .advance_with_actions(Input::default(), true, 4)
        .unwrap(); // 1804
    assert_eq!(movement.spindash_charge(), 1984);
    movement.advance_with_actions(JUMP, true, 4).unwrap(); // 1805 rev
    assert_eq!(movement.spindash_charge(), 2434);
    for expected in [2358, 2285, 2214] {
        // 1806,1807,1808
        movement
            .advance_with_actions(Input::default(), true, 4)
            .unwrap();
        assert_eq!(movement.spindash_charge(), expected);
    }
    let x = movement.player.x;
    movement
        .advance_with_actions(Input::default(), false, 4)
        .unwrap(); // 1809 release
    assert_eq!(movement.spindash_charge(), 2214);
    assert_eq!(movement.player.ground_speed, 2304);
    assert_eq!(movement.player.vx, 0);
    assert_eq!(movement.player.x, x);
    movement.land_at(25); // Runtime terrain continuation must preserve release speed.
    assert_eq!(movement.player.ground_speed, 2304);
    movement
        .advance_with_actions(Input::default(), false, 4)
        .unwrap(); // 1810 roll
    assert_eq!(movement.player.ground_speed, 2300);
    assert_eq!(movement.player.vx, 2300);
    assert_eq!(movement.player.x, x + 2300);
}
