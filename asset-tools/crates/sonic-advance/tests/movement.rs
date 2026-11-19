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
