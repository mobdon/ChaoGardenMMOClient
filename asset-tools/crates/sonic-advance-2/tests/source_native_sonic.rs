use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_player::{Character, PlayerState},
    native_sonic::{
        AirAction, SonicInputBuffer, try_no_target_air_action, uncurl_accepts_air_input,
    },
};

#[derive(Deserialize)]
struct Group {
    cases: usize,
    seed: u32,
    values_per_case: usize,
    bytes: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct Resource {
    offset: usize,
    length: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct Report {
    actions: Group,
    input: Group,
    rom_sonic_patterns: Vec<Resource>,
}

fn report() -> Report {
    serde_json::from_str(include_str!("../catalog/native-sonic-parity.json")).unwrap()
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[test]
fn no_target_airborne_action_initialization_matches_original_c() {
    let expected = report().actions;
    let mut seed = expected.seed;
    let mut output = Vec::new();
    for _ in 0..expected.cases {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.q_world_x = (next(&mut seed) % 2_000_001) as i32 - 1_000_000;
        p.q_world_y = (next(&mut seed) % 2_000_001) as i32 - 1_000_000;
        p.q_speed_air_x = next(&mut seed) as i16;
        p.q_speed_air_y = next(&mut seed) as i16;
        p.move_state = next(&mut seed) & 0x330e_e777;
        p.rotation = next(&mut seed) as u8;
        p.held_input = (next(&mut seed) & 1023) as u16;
        p.frame_input = (next(&mut seed) & 1) as u16;
        p.char_state = (next(&mut seed) % 81) as i16;
        p.sprite_offset_x = 6;
        p.sprite_offset_y = ((next(&mut seed) % 40) as i32 - 8) as i8;
        p.sprite_frame_flags = next(&mut seed);
        p.unk61 = 7;
        p.unk62 = -8;
        p.unk63 = 9;
        p.unk6e = (next(&mut seed) % 3) as u8;
        p.unk70 = next(&mut seed) & 1 != 0;
        p.unk71 = (next(&mut seed) % 3) as u8;
        let inverted = next(&mut seed) & 1 != 0;
        let action = try_no_target_air_action(&mut p, 1, inverted);
        let event = match action {
            AirAction::None => 0,
            AirAction::InstaShield => 1,
            AirAction::ForwardThrust { .. } => 2,
        };
        let stop = matches!(
            action,
            AirAction::ForwardThrust {
                stop_sound_281: true
            }
        );
        let words = [
            p.q_world_x,
            p.q_world_y,
            i32::from(p.q_speed_air_x),
            i32::from(p.q_speed_air_y),
            p.move_state as i32,
            i32::from(p.rotation),
            i32::from(p.char_state),
            i32::from(p.sprite_offset_x),
            i32::from(p.sprite_offset_y),
            p.sprite_frame_flags as i32,
            i32::from(p.unk61),
            i32::from(p.unk62),
            i32::from(p.unk63),
            i32::from(p.unk6e),
            i32::from(p.unk70),
            i32::from(p.unk71),
            event,
            i32::from(action.sound().unwrap_or(0)),
            if stop { 281 } else { 0 },
            i32::from(action == AirAction::InstaShield),
            i32::from(action.returns_early()),
            i32::from(action.returns_early() && uncurl_accepts_air_input(&p)),
        ];
        assert_eq!(words.len(), expected.values_per_case);
        output.extend(words.into_iter().flat_map(i32::to_le_bytes));
    }
    assert_eq!(output.len(), expected.bytes);
    assert_eq!(gba::sha256(&output), expected.sha256);
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn sonic_input_history_matches_original_c_and_actual_rom_patterns() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let expected = report();
    for resource in expected.rom_sonic_patterns {
        assert_eq!(
            gba::sha256(rom.range("rom", resource.offset, resource.length).unwrap()),
            resource.sha256
        );
    }
    let mut buffer = SonicInputBuffer::from_rom(&rom).unwrap();
    let expected = expected.input;
    let mut seed = expected.seed;
    let mut output = Vec::new();
    let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
    p.unk71 = 0;
    let mut selected = 0;
    for row in 0..expected.cases {
        if row % 257 == 0 {
            buffer.reset_counters();
        }
        p.move_state = if next(&mut seed).is_multiple_of(16) {
            0x200000
        } else {
            0
        };
        p.move_state |= if next(&mut seed).is_multiple_of(31) {
            0x80
        } else {
            0
        };
        p.frame_input = (next(&mut seed) & 1023) as u16;
        p.unk70 = next(&mut seed) & 1 != 0;
        if row % 19 == 0 {
            p.unk71 = 0;
        }
        if row % 64 < 4 {
            p.move_state = 0;
            p.unk70 = true;
            p.unk71 = 0;
            p.frame_input = if row % 2 != 0 { 0x10 } else { 0 };
        }
        buffer.handle_frame(&mut p).unwrap();
        output.extend(buffer.state_bytes());
        output.push(p.unk71);
        selected += usize::from(p.unk71 == 1);
    }
    assert!(
        selected > 128,
        "Exercise actual successful combo selections"
    );
    assert_eq!(expected.values_per_case, 71);
    assert_eq!(output.len(), expected.bytes);
    assert_eq!(gba::sha256(&output), expected.sha256);
}
