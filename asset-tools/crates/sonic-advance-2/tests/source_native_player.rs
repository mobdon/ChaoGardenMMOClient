use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_player::{Callback, Character, PlayerState},
};

#[derive(Deserialize)]
struct Oracle {
    cases: usize,
    seed: u32,
    words_per_case: usize,
    output_bytes: usize,
    output_sha256: String,
    sine_sha256: String,
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn character(id: u32) -> Character {
    match id {
        0 => Character::Sonic,
        1 => Character::Cream,
        2 => Character::Tails,
        3 => Character::Knuckles,
        _ => Character::Amy,
    }
}

#[test]
#[ignore = "A2NE_ROM required; native_player_oracle.py compiles pinned original C for the recorded primitive boundary"]
fn native_player_primitives_match_original_c() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let sine = rom.range("rom", 0x97aa4, 0xa00).unwrap();
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-player-parity.json")).unwrap();
    assert_eq!(gba::sha256(sine), expected.sine_sha256);
    let sin = |rotation: u8| {
        let offset = usize::from(rotation) * 4 * 2;
        i32::from(i16::from_le_bytes(
            sine[offset..offset + 2].try_into().unwrap(),
        )) >> 6
    };
    let cos = |rotation: u8| {
        let offset = (usize::from(rotation) * 4 + 256) * 2;
        i32::from(i16::from_le_bytes(
            sine[offset..offset + 2].try_into().unwrap(),
        )) >> 6
    };
    let mut seed = expected.seed;
    let mut result = Vec::new();
    for row in 0..expected.cases {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.q_world_x = (next(&mut seed) % 2_000_001) as i32 - 1_000_000;
        p.q_world_y = (next(&mut seed) % 2_000_001) as i32 - 1_000_000;
        p.q_speed_air_x = ((next(&mut seed) % 20_001) as i32 - 10_000) as i16;
        p.q_speed_air_y = ((next(&mut seed) % 20_001) as i32 - 10_000) as i16;
        p.q_speed_ground = ((next(&mut seed) % 10_001) as i32 - 5_000) as i16;
        p.move_state = next(&mut seed) & 0x0230215f;
        p.rotation = next(&mut seed) as u8;
        p.held_input = (next(&mut seed) & 1023) as u16;
        p.frame_input = (next(&mut seed) & 1023) as u16;
        p.top_speed = if next(&mut seed) & 1 != 0 { 3072 } else { 1536 };
        p.max_speed = if next(&mut seed) & 1 != 0 { 3840 } else { 2304 };
        p.acceleration = [8, 12, 14, 16, 18][(next(&mut seed) % 5) as usize];
        p.deceleration = 64;
        p.boost_threshold = (next(&mut seed) % 5) as u8;
        p.boost_speed = (next(&mut seed) % 3000) as i16;
        p.is_boosting = next(&mut seed) & 1 != 0;
        p.walk_anim = (next(&mut seed) % 6) as u8;
        p.control_lock = (next(&mut seed) & 1) as i16;
        p.char_state = (next(&mut seed) % 65) as i16;
        p.sprite_offset_y = if next(&mut seed) & 1 != 0 { 9 } else { 14 };
        p.sprite_frame_flags = next(&mut seed);
        p.character = character(next(&mut seed) % 5);
        p.unk61 = 7;
        p.unk62 = -8;
        p.unk63 = 9;
        p.unk70 = false;
        p.unk71 = 10;
        let inverted = next(&mut seed) & 1 != 0;
        let previous_inverted = next(&mut seed) & 1 != 0;
        let rings = (next(&mut seed) % 201) as u16;
        let clearance = (next(&mut seed) % 16) as i32 - 4;
        let camera_dx = ((next(&mut seed) % 9) as i32 - 4) as i16;
        let is_boss = next(&mut seed) & 1 != 0;
        let current = (next(&mut seed) & 1023) as u16;
        let offset = if next(&mut seed) & 1 != 0 { 9 } else { 14 };
        let mut event = false;
        match row % 16 {
            0 => p.handle_input(current),
            1 => event = p.update_frame_physics(rings),
            2 => event = p.ground_input(inverted),
            3 => p.air_input(),
            4 => p.decay_air_x(false),
            5 => p.decay_air_x(true),
            6 => p.apply_gravity(false),
            7 => p.apply_gravity(true),
            8 => p.advance_position(inverted, previous_inverted),
            9 => p.ground_speed_to_air(sin(p.rotation), cos(p.rotation)),
            10 => p.ground_slope_acceleration(sin(p.rotation)),
            11 => p.relax_rotation(),
            12 => event = p.request_jump(1, clearance),
            13 => {
                let angle = p.rotation.wrapping_sub(64);
                p.initialize_jump(sin(angle), cos(angle), is_boss.then_some(camera_dx));
            }
            14 => p.release_jump(1),
            15 => p.change_sprite_offsets(6, offset, inverted),
            _ => unreachable!(),
        }
        let callback = match p.callback {
            Callback::Ground => 0,
            Callback::InitJump => 1,
            Callback::Airborne => 2,
            Callback::Rolling => 3,
            Callback::Skidding => 4,
        };
        let words = [
            p.q_world_x,
            p.q_world_y,
            i32::from(p.q_speed_air_x),
            i32::from(p.q_speed_air_y),
            i32::from(p.q_speed_ground),
            p.move_state as i32,
            i32::from(p.rotation),
            i32::from(p.held_input),
            i32::from(p.frame_input),
            p.top_speed,
            p.max_speed,
            p.acceleration,
            p.deceleration,
            i32::from(p.boost_threshold),
            i32::from(p.boost_speed),
            i32::from(p.is_boosting),
            i32::from(p.walk_anim),
            i32::from(p.control_lock),
            i32::from(p.char_state),
            i32::from(p.sprite_offset_x),
            i32::from(p.sprite_offset_y),
            p.sprite_frame_flags as i32,
            p.character as i32,
            i32::from(p.unk61),
            i32::from(p.unk62),
            i32::from(p.unk63),
            i32::from(p.unk70),
            i32::from(p.unk71),
            callback,
            i32::from(event),
        ];
        assert_eq!(words.len(), expected.words_per_case);
        for word in words {
            result.extend_from_slice(&word.to_le_bytes());
        }
    }
    assert_eq!(result.len(), expected.output_bytes);
    assert_eq!(gba::sha256(&result), expected.output_sha256);
}
