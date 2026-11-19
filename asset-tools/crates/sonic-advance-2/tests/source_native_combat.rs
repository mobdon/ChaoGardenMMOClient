use sonic_advance_2_assets::{
    gba,
    native_combat::*,
    native_player::{Character, PlayerState},
};

#[test]
fn homing_choice_matches_compiled_original_over_angle_distance_and_character_boundaries() {
    let mut bytes = Vec::new();
    for side in 0..3 {
        let mut p = PlayerState::new_at(
            if side == 2 {
                Character::Tails
            } else {
                Character::Sonic
            },
            256017,
            256231,
        );
        p.move_state = u32::from(side == 1);
        for dy in -129..=129 {
            for dx in -129..=129 {
                let mut t = HomingTarget::default();
                t.consider(&p, (1000 + dx) * 256 + 81, (1000 + dy) * 256 + 133);
                bytes.extend(t.distance_squared.to_le_bytes());
                bytes.extend(i32::from(t.angle).to_le_bytes());
            }
        }
    }
    assert_eq!(
        gba::sha256(&bytes),
        "227e9608d576b4ea98ac4fd8602844528b574f4f920fa738c9a6bc6e9f9b2575"
    );
}
#[test]
fn equal_distance_keeps_first_and_closer_replaces() {
    let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
    p.move_state = 0;
    let mut a = HomingTarget::default();
    a.consider(&p, 32 * 256, 8 * 256);
    let first = a;
    a.consider(&p, 8 * 256, 32 * 256);
    assert_eq!(a, first);
    a.consider(&p, 4 * 256, 6 * 256);
    assert_eq!(a.distance_squared, 52);
    assert_ne!(a.angle, first.angle);
    p.character = Character::Cream;
    p.move_state = 0x100;
    p.frame_input = 1;
    assert!(!a.can_launch(&p, 1));
}
#[test]
#[ignore = "requires verified A2NE_ROM for the sine table"]
fn combat_init_and_unobstructed_motion_match_original_callbacks() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = sonic_advance_2_assets::VerifiedRom::parse(&bytes).unwrap();
    let sine = rom.range("rom", 0x97aa4, 2560).unwrap();
    let trig = |i: usize| i32::from(i16::from_le_bytes([sine[i * 2], sine[i * 2 + 1]])) >> 6;
    for motion in [false, true] {
        let mut packed = vec![];
        for i in 0i32..6144 {
            let mut p = PlayerState::new_at(Character::Sonic, 10003, 20123);
            p.move_state =
                0x331ee730 | if i & 4 != 0 { 64 } else { 0 } | if i & 8 != 0 { 1 } else { 0 };
            p.rotation = (i / 8) as u8;
            p.q_speed_air_x = (i * 53 % 2001 - 1000) as i16;
            p.q_speed_air_y = 500;
            p.sprite_offset_x = 6;
            p.sprite_offset_y = if i & 32 != 0 { 9 } else { 14 };
            p.sprite_frame_flags = 0xabcdef12;
            p.is_boosting = true;
            p.unk61 = 5;
            p.unk62 = 6;
            p.unk63 = 7;
            p.unk70 = true;
            p.unk71 = 2;
            p.unk6e = 3;
            let mut frames = 33;
            let mut timer = 47u16;
            p.acceleration = 12;
            p.top_speed = 1536;
            p.held_input = if i & 16 != 0 { 0x10 } else { 0x20 };
            let angle = (i as usize) % 1024;
            match i % 3 {
                0 => {
                    init_homing(&mut p, trig(angle), trig(angle + 256));
                    frames = 60;
                }
                1 => init_recoil(&mut p),
                _ => {
                    init_hurt(&mut p);
                    timer = 120;
                }
            }
            if motion && i % 3 != 1 {
                for _ in 0..65 {
                    if i % 3 == 0 {
                        homing_tick_before_physics(&mut p, &mut frames);
                        p.air_input();
                        p.decay_air_x(false);
                    }
                    p.apply_gravity(false);
                    p.advance_position(false, false);
                    if i % 3 == 0 {
                        p.relax_rotation();
                        assert_eq!(homing_transition_after_physics(&p), None);
                    }
                    tick_hurt_immunity(p.char_state, &mut timer);
                }
            }
            for v in [
                p.q_world_x,
                p.q_world_y,
                i32::from(p.q_speed_air_x),
                i32::from(p.q_speed_air_y),
                p.move_state as i32,
                i32::from(p.char_state),
                i32::from(p.sprite_offset_x),
                i32::from(p.sprite_offset_y),
                i32::from(p.rotation),
                i32::from(p.is_boosting),
                i32::from(p.unk61),
                i32::from(p.unk62),
                i32::from(p.unk63),
                i32::from(p.unk70),
                i32::from(p.unk71),
                i32::from(p.unk6e),
                p.sprite_frame_flags as i32,
                i32::from(frames),
                i32::from(timer),
                0,
            ] {
                packed.extend(v.to_le_bytes());
            }
        }
        assert_eq!(
            gba::sha256(&packed),
            if motion {
                "b1994bf12c67e72da7edfdaaaddc74cf22239b2d3bc7df4a194a8a178e59d8e8"
            } else {
                "c7eb6f93f93cea98061937c07a933fa866056f23379491874b7d937ee5ed6894"
            },
            "motion={motion}"
        );
    }
}
