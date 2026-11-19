use serde::Deserialize;
use sonic_advance_2_assets::{
    gba,
    native_camera::NativeCamera,
    native_player::{Character, PlayerState},
};

#[derive(Deserialize)]
struct Oracle {
    cases: usize,
    seed: u32,
    words_per_case: usize,
    output_bytes: usize,
    output_sha256: String,
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn character(value: u32) -> Character {
    match value {
        0 => Character::Sonic,
        1 => Character::Cream,
        2 => Character::Tails,
        3 => Character::Knuckles,
        _ => Character::Amy,
    }
}

#[test]
fn normal_stage_camera_matches_original_c() {
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-camera-parity.json")).unwrap();
    let mut seed = expected.seed;
    let mut output = Vec::new();
    for row in 0..expected.cases {
        let width = 512 + (next(&mut seed) % 20_000) as i32;
        let height = 256 + (next(&mut seed) % 4000) as i32;
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.q_world_x = (next(&mut seed) % ((width + 512) * 256) as u32) as i32 - 65_536;
        p.q_world_y = (next(&mut seed) % ((height + 512) * 256) as u32) as i32 - 65_536;
        p.q_speed_air_x = ((next(&mut seed) % 20_001) as i32 - 10_000) as i16;
        p.move_state = if next(&mut seed) & 1 != 0 { 2 } else { 0 };
        p.character = character(next(&mut seed) % 5);
        p.unk61 = (next(&mut seed) % 16) as i8;
        p.sprite_offset_y = (next(&mut seed) % 25) as i8;
        let inverted = next(&mut seed) & 1 != 0;
        let mut c = NativeCamera::normal_stage(&p, width, height).unwrap();
        let x = (next(&mut seed) % (width + 256) as u32) as i32 - 128;
        let y = (next(&mut seed) % (height + 256) as u32) as i32 - 128;
        let target_x = (next(&mut seed) % (width + 1024) as u32) as i32 - 512;
        let target_y = (next(&mut seed) % (height + 1024) as u32) as i32 - 512;
        let q_max_x = ((next(&mut seed) % 129) * 32) as i32;
        let max_y = (next(&mut seed) % 33) as i32;
        let shift_x = (next(&mut seed) % 257) as i32 - 128;
        let shift_y = (next(&mut seed) % 97) as i32 - 48;
        let delay = (next(&mut seed) % 5) as i16;
        let deadzone_x = (next(&mut seed) % 33) as i32;
        let deadzone_y = ((next(&mut seed) % 7) * 4) as i32;
        let look_y = (next(&mut seed) % 161) as i32 - 80;
        let locked = (next(&mut seed) & 3) as u16;
        let lead = ((next(&mut seed) % 20_001) as i32 - 10_000) as i16;
        let offset = ((next(&mut seed) % 257) as i32 - 128) as i16;
        let shake_x = ((next(&mut seed) % 17) as i32 - 8) as i16;
        let shake_y = ((next(&mut seed) % 17) as i32 - 8) as i16;
        if row % 17 != 0 {
            c.x = x;
            c.y = y;
            c.target_x = target_x;
            c.target_y = target_y;
            c.q_max_step_x = q_max_x;
            c.max_step_y = max_y;
            c.shift_x = shift_x;
            c.shift_y = shift_y;
            c.target_delay = delay;
            c.deadzone_x = deadzone_x;
            c.deadzone_y = deadzone_y;
            c.look_y = look_y;
            c.axes_locked = locked;
            c.q_velocity_lead_x = lead;
            c.body_offset_y = offset;
            c.shake_x = shake_x;
            c.shake_y = shake_y;
            c.update(&p, inverted).unwrap();
        }
        let words = [
            c.x,
            c.y,
            c.dx,
            c.dy,
            c.target_x,
            c.target_y,
            c.q_max_step_x,
            c.max_step_y,
            c.shift_x,
            c.shift_y,
            c.min_x,
            c.max_x,
            c.min_y,
            c.max_y,
            i32::from(c.target_delay),
            c.deadzone_x,
            c.deadzone_y,
            c.look_y,
            i32::from(c.axes_locked),
            i32::from(c.q_velocity_lead_x),
            i32::from(c.body_offset_y),
            i32::from(c.shake_x),
            i32::from(c.shake_y),
        ];
        assert_eq!(words.len(), expected.words_per_case);
        for word in words {
            output.extend_from_slice(&word.to_le_bytes());
        }
    }
    assert_eq!(output.len(), expected.output_bytes);
    assert_eq!(gba::sha256(&output), expected.output_sha256);
}
