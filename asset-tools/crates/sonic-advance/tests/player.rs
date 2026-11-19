use sonic_advance_assets::{binary, player::*, stage::Stage, Error, Result};

struct Floor {
    height: i32,
    wall: Option<i32>,
    ceiling: Option<i32>,
}

impl SurfaceQuery for Floor {
    fn bounds(&self) -> (i32, i32) {
        (4096, 1024)
    }
    fn ground_below(&self, _x: i32, y: i32, distance: i32) -> Result<Option<GroundContact>> {
        Ok(
            (y <= self.height && self.height <= y + distance).then_some(GroundContact {
                surface_y: self.height,
                angle: 0,
            }),
        )
    }
    fn solid_at(&self, x: i32, y: i32) -> Result<bool> {
        Ok(y >= self.height
            || self.wall.is_some_and(|wall| x >= wall)
            || self.ceiling.is_some_and(|ceiling| y <= ceiling))
    }
}

fn trig() -> SineTable {
    // Synthetic fixture table, not shipped native resource data. Tests below
    // use flat/cardinal contacts; source parity uses the actual ROM table.
    let mut values = [0; 256];
    values[64] = 256;
    values[192] = -256;
    SineTable::from_samples(values).unwrap()
}

fn grounded() -> (Player, Floor) {
    let floor = Floor {
        height: 200,
        wall: None,
        ceiling: None,
    };
    let mut player = Player::at_pixels(64, 186, trig()).unwrap();
    player.grounded = true;
    (player, floor)
}

#[test]
fn sa1_flat_ground_trace_uses_4_5_top_speed_and_eight_subpixel_acceleration() {
    let (mut player, floor) = grounded();
    for frame in 1..=144 {
        player
            .step(
                Input {
                    right: true,
                    ..Input::default()
                },
                &floor,
            )
            .unwrap();
        assert_eq!(player.ground_speed, frame * 8);
        assert_eq!(player.x, 64 * ONE + 4 * frame * (frame + 1));
        assert!(player.grounded);
        assert_eq!(player.y, 186 * ONE);
    }
    assert_eq!(player.animation().animation, 5);
    player
        .step(
            Input {
                right: true,
                ..Input::default()
            },
            &floor,
        )
        .unwrap();
    assert_eq!(player.ground_speed, 1152);
    for _ in 0..144 {
        player.step(Input::default(), &floor).unwrap();
    }
    assert_eq!(player.ground_speed, 0);
    assert_eq!(player.animation().animation, 0);
}

#[test]
fn native_reversal_crosses_zero_to_braking_quantum_and_overspeed_is_preserved() {
    let (mut player, floor) = grounded();
    player.ground_speed = 40;
    player
        .step(
            Input {
                left: true,
                ..Input::default()
            },
            &floor,
        )
        .unwrap();
    assert_eq!(player.ground_speed, -96);
    player.ground_speed = 2000;
    player
        .step(
            Input {
                right: true,
                ..Input::default()
            },
            &floor,
        )
        .unwrap();
    assert_eq!(player.ground_speed, 2000);
    player
        .step(
            Input {
                left: true,
                right: true,
                jump: false,
            },
            &floor,
        )
        .unwrap();
    assert_eq!(player.ground_speed, 2000);
}

#[test]
fn grounded_slope_acceleration_uses_native_q8_projection_and_starts_only_when_moving() {
    struct Slope;
    impl SurfaceQuery for Slope {
        fn bounds(&self) -> (i32, i32) {
            (4096, 1024)
        }
        fn ground_below(&self, _: i32, _: i32, _: i32) -> Result<Option<GroundContact>> {
            Ok(Some(GroundContact {
                surface_y: 200,
                angle: 16,
            }))
        }
        fn solid_at(&self, _: i32, y: i32) -> Result<bool> {
            Ok(y >= 200)
        }
    }
    let mut values = [0; 256];
    values[16] = 97; // Mathematical Q8 sin(22.5 degrees), actual table checked below.
    values[64] = 256;
    values[80] = 236;
    values[192] = -256;
    let mut player = Player::at_pixels(64, 186, SineTable::from_samples(values).unwrap()).unwrap();
    player.grounded = true;
    player.angle = 16;
    player.step(Input::default(), &Slope).unwrap();
    assert_eq!(player.ground_speed, 0);
    player.ground_speed = 256;
    player
        .step(
            Input {
                right: true,
                ..Input::default()
            },
            &Slope,
        )
        .unwrap();
    // Existing speed + ((97*3)>>5) slope gravity + 8 input acceleration.
    assert_eq!(player.ground_speed, 273);
    assert_eq!(player.vx, (273 * 236) >> 8);
    assert_eq!(player.vy, (273 * 97) >> 8);
}

#[test]
fn native_jump_frame_keeps_feet_then_integrates_old_velocity_before_gravity() {
    let (mut player, floor) = grounded();
    let jump = Input {
        jump: true,
        ..Input::default()
    };
    let first = player.step(jump, &floor).unwrap();
    assert!(first.jumped);
    assert_eq!(player.y, 191 * ONE);
    assert_eq!(player.vy, -1248);
    assert_eq!(player.half_height, 9);
    assert_eq!(player.animation().animation, 8);
    player.step(jump, &floor).unwrap();
    assert_eq!(player.y, 191 * ONE - 1248);
    assert_eq!(player.vy, -1206);
    player.step(Input::default(), &floor).unwrap();
    assert_eq!(player.vy, -768 + 42);
    assert_eq!(player.y, 191 * ONE - 1248 - 768);
    for _ in 0..180 {
        player.step(Input::default(), &floor).unwrap();
        if player.grounded {
            break;
        }
    }
    assert!(player.grounded);
    assert_eq!(player.half_height, 14);
    assert_eq!(player.y, 186 * ONE);
    assert_eq!(player.vy, 0);
}

#[test]
fn held_jump_does_not_repeat_and_short_jump_has_lower_apex() {
    let (base, floor) = grounded();
    let mut held = base.clone();
    let mut short = base;
    let mut held_min = held.y;
    let mut short_min = short.y;
    let mut jumped = 0;
    for frame in 0..150 {
        jumped += usize::from(
            held.step(
                Input {
                    jump: true,
                    ..Input::default()
                },
                &floor,
            )
            .unwrap()
            .jumped,
        );
        short
            .step(
                Input {
                    jump: frame == 0,
                    ..Input::default()
                },
                &floor,
            )
            .unwrap();
        held_min = held_min.min(held.y);
        short_min = short_min.min(short.y);
    }
    assert_eq!(jumped, 1);
    assert!(held_min < short_min);
}

#[test]
fn air_input_retains_sa1_signed_shift_drag_condition() {
    assert_eq!(air_horizontal(1024, -1248, Input::default()).unwrap(), 992);
    assert_eq!(
        air_horizontal(-1024, -1248, Input::default()).unwrap(),
        -992
    );
    assert_eq!(air_horizontal(-1, -1248, Input::default()).unwrap(), 0);
    assert_eq!(air_horizontal(1024, 0, Input::default()).unwrap(), 1024);
    assert_eq!(
        air_horizontal(
            1024,
            100,
            Input {
                right: true,
                ..Input::default()
            }
        )
        .unwrap(),
        1040
    );
    assert_eq!(
        air_horizontal(
            1150,
            100,
            Input {
                right: true,
                ..Input::default()
            }
        )
        .unwrap(),
        1152
    );
    assert!(air_horizontal(i32::MAX, 0, Input::default()).is_err());
}

#[test]
fn bounded_sweep_stops_at_thin_walls_and_ceiling() {
    let (mut player, mut floor) = grounded();
    floor.wall = Some(80);
    player.ground_speed = 12 * ONE;
    let outcome = player
        .step(
            Input {
                right: true,
                ..Input::default()
            },
            &floor,
        )
        .unwrap();
    assert!(outcome.blocked_x);
    assert!(player.x / ONE + HALF_WIDTH < 80);
    assert_eq!(player.vx, 0);
    floor.wall = None;
    floor.ceiling = Some(150);
    player
        .step(
            Input {
                jump: true,
                ..Input::default()
            },
            &floor,
        )
        .unwrap();
    let mut blocked = false;
    for _ in 0..12 {
        blocked |= player
            .step(
                Input {
                    jump: true,
                    ..Input::default()
                },
                &floor,
            )
            .unwrap()
            .blocked_y;
    }
    assert!(blocked);
    assert!(player.y / ONE - player.half_height > 150);
}

#[test]
fn malformed_state_and_query_failures_are_atomic() {
    struct Failing;
    impl SurfaceQuery for Failing {
        fn bounds(&self) -> (i32, i32) {
            (4096, 1024)
        }
        fn ground_below(&self, _: i32, _: i32, _: i32) -> Result<Option<GroundContact>> {
            Err(Error("query failed".into()))
        }
        fn solid_at(&self, _: i32, _: i32) -> Result<bool> {
            Err(Error("query failed".into()))
        }
    }
    let (mut player, floor) = grounded();
    let original = player.clone();
    assert!(player.step(Input::default(), &Failing).is_err());
    assert_eq!(player, original);
    player.vx = i32::MAX;
    let invalid = player.clone();
    assert!(player.step(Input::default(), &floor).is_err());
    assert_eq!(player, invalid);
    assert!(Player::at_pixels(i32::MAX, 0, trig()).is_err());
    assert!(SineTable::from_samples([0; 256]).is_err());
    struct InvalidContact;
    impl SurfaceQuery for InvalidContact {
        fn bounds(&self) -> (i32, i32) {
            (4096, 1024)
        }
        fn ground_below(&self, _: i32, _: i32, _: i32) -> Result<Option<GroundContact>> {
            Ok(Some(GroundContact {
                surface_y: i32::MAX,
                angle: 0,
            }))
        }
        fn solid_at(&self, _: i32, _: i32) -> Result<bool> {
            Ok(false)
        }
    }
    let (mut player, _) = grounded();
    let original = player.clone();
    assert!(player.step(Input::default(), &InvalidContact).is_err());
    assert_eq!(player, original);
}

#[test]
#[ignore = "requires exact private SONIC_ADVANCE_ASOE_ROM"]
fn native_spawn_tables_and_stage_floor_allow_walking_and_jumping() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let mut player = Player::sonic_first_stage(&rom).unwrap();
    assert_eq!((player.x, player.y), (104 * ONE, 444 * ONE));
    let table = SineTable::from_asoe(&rom).unwrap();
    assert_eq!((table.sin(0), table.cos(0), table.sin(64)), (0, 256, 256));
    assert_eq!((table.sin(16), table.cos(16)), (97, 236));
    // Native first-stage center and terrain must meet; no synthetic floor.
    for _ in 0..30 {
        player.step(Input::default(), &stage).unwrap();
        if player.grounded {
            break;
        }
    }
    assert!(
        player.grounded,
        "native spawn never reached its actual floor: {player:?}"
    );
    let start_x = player.x;
    let grounded_y = player.y;
    let mut jumped = false;
    let mut landed = false;
    let mut minimum_y = player.y;
    for frame in 0..150 {
        let outcome = player
            .step(
                Input {
                    right: true,
                    jump: (50..70).contains(&frame),
                    ..Input::default()
                },
                &stage,
            )
            .unwrap();
        jumped |= outcome.jumped;
        landed |= outcome.landed && jumped;
        minimum_y = minimum_y.min(player.y);
        assert!(!outcome.fell_out);
    }
    assert!(
        player.x > start_x + 200 * ONE,
        "native walk did not advance: {player:?}"
    );
    assert!(jumped && landed, "jump/landing not observed: {player:?}");
    assert!(minimum_y < 430 * ONE);
    eprintln!(
        "native source trace: grounded=({},{}), end=({},{}), minimum_y={}, jumped={}, landed={}",
        start_x, grounded_y, player.x, player.y, minimum_y, jumped, landed
    );
}
