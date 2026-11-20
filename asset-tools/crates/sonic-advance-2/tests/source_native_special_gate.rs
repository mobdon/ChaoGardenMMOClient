use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_camera::NativeCamera,
    native_goal::{GoalController, GoalPhase, special_course_wrap},
    native_player::{Character, PlayerState},
};
#[test]
fn seven_ring_goal_approach_and_departure_match_compiled_callbacks() {
    let mut bytes = Vec::new();
    for n in 0..48 {
        let mut p = PlayerState::new_at(Character::Sonic, 54, 21271);
        p.move_state = 0x08000000 | 0x330ee731;
        p.rotation = (n * 7) as u8;
        p.q_speed_ground = (n * 183 - 2200) as i16;
        p.q_speed_air_x = (n * 327 - 1700) as i16;
        p.sprite_offset_x = 5;
        p.sprite_offset_y = 9;
        p.is_boosting = true;
        p.char_state = 0;
        p.held_input = 129;
        p.unk61 = -3;
        p.unk62 = 9;
        p.unk63 = -1;
        p.unk70 = true;
        p.unk71 = 8;
        let mut c = NativeCamera::normal_stage(&p, 50000, 2000).unwrap();
        c.shift_y = 40 - n * 3;
        c.q_max_step_x = 4096;
        let mut goal = GoalController {
            phase: GoalPhase::Init,
            victory_timer: 0,
            goal_x: 0,
        };
        for i in 0..421 {
            if i == 220 + n {
                p.move_state |= 0x04000000;
            }
            let a = goal.step(&mut p, 0, 7, 0, 0, &mut c.shift_y).unwrap();
            assert!(a.special_transition && a.advance_ground);
            p.q_speed_air_x = p.q_speed_ground;
            p.q_world_x += i32::from(p.q_speed_air_x);
            let boost = goal.finish_special_motion(&mut p, &mut c);
            let evt = i32::from(a.stop_water_sound) * 4
                + i32::from(a.begin_results) * 8
                + i32::from(boost) * 65;
            let row = [
                p.q_world_x,
                p.q_world_y,
                i32::from(p.q_speed_ground),
                i32::from(p.q_speed_air_x),
                p.move_state as i32,
                i32::from(p.char_state),
                i32::from(p.is_boosting),
                i32::from(p.held_input),
                i32::from(goal.victory_timer),
                c.shift_y,
                c.q_max_step_x,
                5,
                evt,
                i32::from(goal.phase == GoalPhase::SpecialExit),
            ];
            for w in row {
                bytes.extend(w.to_le_bytes());
            }
        }
    }
    assert_eq!(
        gba::sha256(&bytes),
        "a45594fb1c01e5f90f8a1084c71f41b7a7ee030d0ab008c97515a8a90081e0df"
    );
}
#[test]
#[ignore = "requires verified A2NE_ROM"]
fn seven_ring_course_wrap_matches_c() {
    let rom_bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&rom_bytes).unwrap();
    let mut output = Vec::new();
    let source_thresholds = [
        19488, 18816, 27073, 24193, 24961, 26401, 27552, 21312, 16224, 20064, 29760, 21120, 33505,
        32449,
    ];
    for (level, x) in (0..26).filter(|n| n % 4 < 2).zip(source_thresholds) {
        for i in 0..24 {
            let mut p = PlayerState::new_at(Character::Sonic, x * 256 + i % 3 - 1, 0);
            let mut c = NativeCamera::normal_stage(&p, 50000, 2000).unwrap();
            c.q_max_step_x = if i & 4 != 0 { 0 } else { 4096 };
            c.axes_locked = ((i >> 3) & 1) as u16;
            c.x = 1234;
            c.target_x = 5678;
            special_course_wrap(&rom, level, &mut p, &mut c).unwrap();
            for w in [p.q_world_x, c.x, c.target_x] {
                output.extend(w.to_le_bytes());
            }
        }
    }
    assert_eq!(
        gba::sha256(&output),
        "98c93559cb9f6f190d30429a916be21c16888c947106c815a8271f94ca4fe67a"
    );
}
