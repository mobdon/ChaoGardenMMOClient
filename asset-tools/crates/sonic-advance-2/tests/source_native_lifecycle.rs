use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_camera::NativeCamera,
    native_lifecycle::{self as life, DeathTick, GameOverCause, NativeLifecycle},
    native_player::{Character, PlayerState},
};

#[derive(Deserialize)]
struct Group {
    cases: usize,
    seed: u32,
    words_per_case: usize,
    bytes: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct Table {
    offset: usize,
    length: usize,
    records: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct Report {
    bounds: Group,
    clock: Group,
    death: Group,
    checkpoint_table: Table,
}

fn report() -> Report {
    serde_json::from_str(include_str!("../catalog/native-lifecycle-parity.json")).unwrap()
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn check(mode: usize, expected: Group) {
    let mut seed = expected.seed;
    let mut output = Vec::new();
    for _ in 0..expected.cases {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        let mut camera = NativeCamera::normal_stage(&p, 2048, 1024).unwrap();
        camera.min_x = (next(&mut seed) % 1000) as i32 - 500;
        camera.max_x = camera.min_x + 500 + (next(&mut seed) % 20_000) as i32;
        camera.min_y = (next(&mut seed) % 1000) as i32 - 500;
        camera.max_y = camera.min_y + 300 + (next(&mut seed) % 3000) as i32;
        camera.x = (next(&mut seed) % 5000) as i32 - 500;
        camera.y = (next(&mut seed) % 2000) as i32 - 500;
        p.q_world_x = (next(&mut seed) % 7_000_001) as i32 - 1_000_000;
        p.q_world_y = (next(&mut seed) % 1_500_001) as i32 - 300_000;
        p.q_speed_air_x = next(&mut seed) as i16;
        p.q_speed_air_y = next(&mut seed) as i16;
        p.q_speed_ground = next(&mut seed) as i16;
        p.move_state = next(&mut seed) & 0x8000_00c8;
        p.sprite_frame_flags = next(&mut seed);
        let mut native = NativeLifecycle::new([0, 0], false);
        native.item_effect = next(&mut seed);
        native.stage_flags = (next(&mut seed) & 0x85) as u16;
        native.stage_time = next(&mut seed);
        native.course_time = next(&mut seed) % 36_003;
        native.checkpoint_time = next(&mut seed);
        let mut lives = (next(&mut seed) % 5) as u8;
        let mut rings = (next(&mut seed) % 1000) as u16;
        let mut special_rings = (next(&mut seed) % 8) as u8;
        native.time_limit_disabled = next(&mut seed) & 1 != 0;
        let mut outcome = 0;
        let mut render_ticks = 0;
        match mode {
            0 => {
                let inverted = native.stage_flags & life::GRAVITY_INVERTED != 0;
                life::normal_bounds(&mut p, &camera, inverted).unwrap();
            }
            1 => {
                native.tick_stage_clock(&mut p);
            }
            _ => {
                p.move_state = p.move_state & !0x8000_0000 | life::DEAD;
                p.q_world_y = camera.y * 256 + (next(&mut seed) % 201) as i32 * 256;
                native.finish_player_main(&mut p, &mut camera).unwrap();
                for _ in 0..512 {
                    let inverted = native.stage_flags & life::GRAVITY_INVERTED != 0;
                    match native
                        .death_task(&mut p, &camera, inverted, &mut lives)
                        .unwrap()
                    {
                        DeathTick::Animate => render_ticks += 1,
                        DeathTick::Wait => (),
                        DeathTick::RestartStage => {
                            rings = 0;
                            special_rings = 0;
                            outcome = 1;
                            break;
                        }
                        DeathTick::GameOver { cause, clear_rings } => {
                            if clear_rings {
                                rings = 0;
                                special_rings = 0;
                            }
                            outcome = match cause {
                                GameOverCause::ZeroLives => 2,
                                GameOverCause::TimeUp => 3,
                            };
                            break;
                        }
                    }
                }
                assert_ne!(outcome, 0);
            }
        }
        let words = [
            p.q_world_x,
            p.q_world_y,
            i32::from(p.q_speed_air_x),
            i32::from(p.q_speed_air_y),
            i32::from(p.q_speed_ground),
            p.move_state as i32,
            i32::from(p.char_state),
            p.sprite_frame_flags as i32,
            i32::from(native.timer_invulnerability),
            native.item_effect as i32,
            if mode == 2 {
                i32::from(native.q_scale_x)
            } else {
                0
            },
            if mode == 2 {
                i32::from(native.q_scale_y)
            } else {
                0
            },
            i32::from(camera.axes_locked),
            i32::from(native.stage_flags),
            native.stage_time as i32,
            native.course_time as i32,
            native.checkpoint_time as i32,
            i32::from(lives),
            i32::from(rings),
            i32::from(special_rings),
            outcome,
            render_ticks,
            0,
        ];
        assert_eq!(words.len(), expected.words_per_case);
        output.extend(words.into_iter().flat_map(i32::to_le_bytes));
    }
    assert_eq!(output.len(), expected.bytes);
    assert_eq!(gba::sha256(&output), expected.sha256);
}

#[test]
fn normal_camera_bounds_match_original_c() {
    check(0, report().bounds);
}

#[test]
fn single_player_stage_clock_matches_original_c() {
    check(1, report().clock);
}

#[test]
fn complete_death_wait_and_resolution_sequences_match_original_c() {
    check(2, report().death);
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn checkpoint_table_matches_actual_rom_and_pinned_source() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let table = report().checkpoint_table;
    assert_eq!((table.records, table.length), (34, 272));
    assert_eq!(
        gba::sha256(rom.range("rom", table.offset, table.length).unwrap()),
        table.sha256
    );
    for level in 0..34 {
        if sonic_advance_2_assets::native_stage::is_course_act(level) {
            let pos = life::checkpoint_position(&rom, level).unwrap();
            let source = rom
                .range("rom", table.offset + usize::from(level) * 8, 8)
                .unwrap();
            assert_eq!(
                pos.map(i32::from)
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .collect::<Vec<_>>(),
                source
            );
        } else {
            assert!(life::checkpoint_position(&rom, level).is_err());
        }
    }
}
