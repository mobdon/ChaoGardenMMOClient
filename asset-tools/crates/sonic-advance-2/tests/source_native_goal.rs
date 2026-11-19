use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_animation::Hitbox,
    native_campaign_save::CampaignProfile,
    native_entities::StageEntities,
    native_goal::{
        self as goal, GoalController, GoalPhase, ResultsContext, ResultsDestination, ResultsSound,
        ScoreState, StageGoals, StageResults,
    },
    native_player::{self as player, Character, PlayerState},
};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    goal_seed: u32,
    bonus_seed: u32,
    sequence_scenarios: usize,
    sequence_frames: usize,
    outputs: BTreeMap<String, Fingerprint>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fingerprint {
    cases: usize,
    row_words: usize,
    bytes: usize,
    sha256: String,
}
fn metadata() -> Oracle {
    serde_json::from_str(include_str!("../catalog/native-goal-parity.json")).unwrap()
}
fn source(name: &str, expected: &Fingerprint) -> Vec<u8> {
    let directory = std::env::var_os("A2NE_GOAL_ORACLE").expect("A2NE_GOAL_ORACLE");
    let bytes =
        std::fs::read(std::path::PathBuf::from(directory).join(format!("{name}.bin"))).unwrap();
    assert_eq!(bytes.len(), expected.bytes);
    assert_eq!(expected.bytes, expected.cases * expected.row_words * 4);
    assert_eq!(gba::sha256(&bytes), expected.sha256);
    bytes
}
fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}
fn words<const N: usize>(bytes: &[u8]) -> [i32; N] {
    assert_eq!(bytes.len(), N * 4);
    std::array::from_fn(|i| i32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

#[test]
#[ignore = "A2NE_GOAL_ORACLE required; exact pinned original C goal toggle with service stubs"]
fn goal_toggle_and_score_match_original_c() {
    let m = metadata();
    let f = &m.outputs["goals"];
    let data = source("goals", f);
    let mut seed = m.goal_seed;
    for (row, bytes) in data.as_chunks::<36>().0.iter().enumerate() {
        let r: [u32; 10] = std::array::from_fn(|_| next(&mut seed));
        let x = (r[0] % 32 * 8 + r[2] % 80 * 256) as i32;
        let y = (r[1] % 32 * 8 + r[3] % 16 * 256) as i32;
        let mut p = PlayerState::new_at(
            Character::Sonic,
            (x + (r[4] % 1025) as i32 - 512) * 256,
            ((r[5] % 10000) as i32 - 1000) * 256,
        );
        p.move_state = if r[6] & 1 != 0 { player::IN_AIR } else { 0 }
            | if r[6] & 2 != 0 { goal::GOAL_REACHED } else { 0 }
            | if r[6] & 4 != 0 { 0x80 } else { 0 };
        p.q_speed_ground = r[7] as i16;
        let mode = (r[8] % 3) as u8;
        let mut score = ScoreState {
            level_score: r[9] as i32,
            lives: (r[9] >> 24) as u8,
            music_request: 0,
        };
        let event = goal::toggle_crossing(x, &p, mode).unwrap();
        if let Some(event) = event {
            score.award(event.bonus, mode, true);
        }
        let sx = -((r[0] % 1025) as i32) + 512;
        let sy = -((r[1] % 1025) as i32) + 512;
        let removed = sx.wrapping_add(128) as u32 > 496 || !(-128..=288).contains(&sy);
        let actual = [
            event.map_or(0, |e| i32::from(e.transition)),
            event.map_or(0, |e| i32::from(e.stage_flags_or)),
            event.map_or(123, |e| e.x),
            score.level_score,
            i32::from(score.lives),
            i32::from(score.music_request),
            i32::from(removed),
            (r[0] % 32) as i32,
            event.map_or(0, |e| e.bonus as i32),
        ];
        assert_eq!(actual, words::<9>(bytes), "goal row {row}, x={x} y={y}");
    }
}

#[test]
#[ignore = "A2NE_GOAL_ORACLE required; exact pinned original C result constructor with service stubs"]
fn results_bonuses_match_original_c() {
    let m = metadata();
    let f = &m.outputs["bonuses"];
    let data = source("bonuses", f);
    let mut seed = m.bonus_seed;
    let edges = [
        0,
        1799,
        1800,
        2999,
        3000,
        3599,
        3600,
        5399,
        5400,
        7199,
        7200,
        10799,
        10800,
        14399,
        14400,
        17999,
        18000,
        21599,
        21600,
        u32::MAX,
    ];
    for (row, bytes) in data.as_chunks::<24>().0.iter().enumerate() {
        let r: [u32; 4] = std::array::from_fn(|_| next(&mut seed));
        let time = edges.get(row).copied().unwrap_or(r[0]);
        let context = ResultsContext {
            level: 0,
            character: 0,
            game_mode: 0,
            move_state: 0,
            special_rings: r[3] as u8,
        };
        let mut profile = CampaignProfile::new_usa();
        profile.score = r[0];
        let result =
            StageResults::begin(context, time, r[1] as u16, r[2] as u16, &mut profile).unwrap();
        assert_eq!(
            [
                result.time_bonus as i32,
                result.ring_bonus as i32,
                result.special_bonus as i32,
                result.count_frames as i32,
                result.count_frames as i32,
                profile.score as i32
            ],
            words::<6>(bytes),
            "bonus row {row}"
        );
    }
}

#[test]
#[ignore = "A2NE_GOAL_ORACLE required; 64 exact-C result sequences with explicit fade/audio readiness"]
fn results_countdown_and_progression_match_original_c() {
    let m = metadata();
    let f = &m.outputs["sequences"];
    let data = source("sequences", f);
    let levels = [0, 1, 2, 4, 5, 6, 8, 9, 10, 16, 18, 24, 25, 26, 28, 29];
    assert_eq!(f.cases, m.sequence_scenarios * m.sequence_frames);
    for scenario in 0..m.sequence_scenarios {
        let mut profile = CampaignProfile::new_usa();
        profile.score = 0xfffffff0u32.wrapping_add(scenario as u32);
        profile.unlocked_characters = 1;
        profile.unlocked_levels = [if scenario & 1 != 0 { 40 } else { 0 }; 5];
        let mut context = ResultsContext {
            level: levels[scenario % 16],
            character: (scenario % 5) as u8,
            game_mode: ((scenario / 16) % 3) as u8,
            move_state: if scenario & 2 != 0 {
                goal::GOAL_REACHED
            } else {
                0
            },
            special_rings: (scenario % 10) as u8,
        };
        let mut score = ScoreState {
            level_score: 49000 + scenario as i32 * 100,
            lives: 250,
            music_request: 0,
        };
        let mut result = StageResults::begin(
            context,
            scenario as u32 * 401,
            (scenario * 13) as u16,
            (scenario * 13) as u16,
            &mut profile,
        )
        .unwrap();
        let mut destination = [0; 4];
        let mut writes = 0;
        for frame in 0..m.sequence_frames {
            let pressed = u16::from(
                (scenario % 4 == 1 && frame == 150)
                    || (scenario % 4 == 2 && frame == 200)
                    || (scenario % 4 == 3 && frame >= 150),
            );
            let output = result
                .tick(
                    &mut context,
                    &mut profile,
                    &mut score,
                    pressed,
                    frame as u32,
                    frame >= 400 + scenario % 50,
                    frame >= 450,
                )
                .unwrap();
            if let Some(event) = output.destination {
                destination = match event {
                    ResultsDestination::NextStage(level) => [1, i32::from(level), 0, 0],
                    ResultsDestination::SpecialStage => [2, 0, 0, 0],
                    ResultsDestination::CourseSelect {
                        level,
                        unlocked,
                        mode,
                    } => [3, i32::from(level), i32::from(unlocked), i32::from(mode)],
                    ResultsDestination::CharacterUnlock(kind) => [4, 0, 0, i32::from(kind)],
                    ResultsDestination::Ending => [5, 0, 0, 0],
                };
            }
            writes += i32::from(output.request_save);
            let sound = match output.sound {
                None => 0,
                Some(ResultsSound::Counter) => 1,
                Some(ResultsSound::CounterDone) => 2,
            };
            let actual = [
                result.time_bonus as i32,
                result.ring_bonus as i32,
                result.special_bonus as i32,
                result.counter as i32,
                result.count_frames as i32,
                i32::from(result.counting_done),
                score.level_score,
                i32::from(score.lives),
                profile.score as i32,
                i32::from(profile.unlocked_levels[0]),
                i32::from(profile.unlocked_levels[1]),
                i32::from(profile.unlocked_levels[2]),
                i32::from(profile.unlocked_levels[3]),
                i32::from(profile.unlocked_levels[4]),
                i32::from(profile.unlocked_characters),
                context.move_state as i32,
                i32::from(context.level),
                i32::from(score.music_request),
                destination[0],
                destination[1],
                destination[2],
                destination[3],
                writes,
                i32::from(result.graphics_hidden) | (sound << 8),
            ];
            let at = (scenario * m.sequence_frames + frame) * 96;
            assert_eq!(
                actual,
                words::<24>(&data[at..at + 96]),
                "scenario {scenario}, frame {frame}"
            );
        }
    }
}

#[test]
#[ignore = "A2NE_ROM and A2NE_GOAL_ORACLE required; independently decoded actual-ROM goal placements"]
fn goal_placements_and_native_tasks_use_actual_rom_records() {
    let m = metadata();
    let f = &m.outputs["placements"];
    let expected = source("placements", f);
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut actual = Vec::new();
    let mut triggers = 0;
    for level in (0..7).flat_map(|zone| [zone * 4, zone * 4 + 1]) {
        let entities = StageEntities::load(&rom, level).unwrap();
        let goals: Vec<_> = entities
            .placements()
            .iter()
            .copied()
            .filter(|e| matches!(e.kind, 42 | 46))
            .collect();
        for e in goals {
            for value in [
                i32::from(level),
                e.x,
                e.y,
                i32::from(e.kind),
                i32::from(e.data[0]),
                i32::from(e.data[1]),
                i32::from(e.data[2]),
                i32::from(e.data[3]),
            ] {
                actual.extend(value.to_le_bytes());
            }
            if e.kind == 46 {
                let mut entities = StageEntities::load(&rom, level).unwrap();
                let mut p = PlayerState::new_at(Character::Sonic, e.x * 256, e.y * 256);
                p.q_speed_ground = 2561;
                let mut goals = StageGoals::new(&rom);
                let mut score = ScoreState::default();
                entities.update_layers(&mut p, e.x - 100, e.y - 80);
                let events = goals
                    .tick(
                        &mut entities,
                        &p,
                        Hitbox {
                            index: -1,
                            rectangle: [0; 4],
                        },
                        e.x - 100,
                        e.y - 80,
                        0,
                        true,
                        &mut score,
                    )
                    .unwrap();
                assert!(
                    events
                        .iter()
                        .any(|event| event.x == e.x && event.bonus == 800),
                    "level{level} goal{},{}",
                    e.x,
                    e.y
                );
                triggers += 1;
            }
        }
    }
    assert_eq!(actual, expected);
    assert!(triggers >= 14);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerOracle {
    seed: u32,
    cases: usize,
    row_words: usize,
    output_bytes: usize,
    output_sha256: String,
}

#[test]
#[ignore = "A2NE_GOAL_PLAYER_ORACLE required; exact native campaign callbacks with ground-motion service stubs"]
fn goal_player_callbacks_match_original_c() {
    let m: PlayerOracle =
        serde_json::from_str(include_str!("../catalog/native-goal-player-parity.json")).unwrap();
    let data = std::fs::read(
        std::env::var_os("A2NE_GOAL_PLAYER_ORACLE").expect("A2NE_GOAL_PLAYER_ORACLE"),
    )
    .unwrap();
    assert_eq!(data.len(), m.output_bytes);
    assert_eq!(data.len(), m.cases * m.row_words * 4);
    assert_eq!(gba::sha256(&data), m.output_sha256);
    let mut seed = m.seed;
    for (row, bytes) in data.as_chunks::<88>().0.iter().enumerate() {
        let r: [u32; 12] = std::array::from_fn(|_| next(&mut seed));
        let mut p = PlayerState::new_at(Character::Sonic, r[0] as i32, r[1] as i32);
        p.q_speed_ground = r[2] as i16;
        p.move_state = r[3] | goal::GOAL_REACHED;
        p.char_state = 9;
        p.rotation = r[4] as u8;
        p.sprite_offset_x = 6;
        p.sprite_offset_y = if r[4] & 256 != 0 { 9 } else { 14 };
        p.held_input = r[5] as u16;
        p.frame_input = (r[5] >> 16) as u16;
        p.sprite_frame_flags = if r[6] & 1 != 0 {
            player::ANIMATION_OVER
        } else {
            0
        };
        p.unk61 = -5;
        p.unk62 = 7;
        p.unk63 = -9;
        p.unk70 = true;
        p.unk71 = 14;
        let animation = if r[7] & 1 != 0 { 31 } else { 0 };
        let variant = ((r[7] >> 1) % 3) as u8;
        let mut controller = GoalController {
            phase: [
                GoalPhase::Init,
                GoalPhase::Slowdown,
                GoalPhase::Brake,
                GoalPhase::Victory,
                GoalPhase::CameraShift,
            ][row % 5],
            victory_timer: (r[8] % 182) as i16,
            goal_x: if r[10] & 1 != 0 {
                0
            } else {
                (r[10] % 20000) as i32
            },
        };
        let mut shift_y = (r[9] % 90) as i32 - 70;
        let level = (r[11] % 7 * 4 + ((r[11] >> 4) & 1)) as u8;
        let output = controller
            .step(&mut p, level, 0, animation, variant, &mut shift_y)
            .unwrap();
        let phase = match controller.phase {
            GoalPhase::Init => 0,
            GoalPhase::WaitingForGround => 1,
            GoalPhase::Slowdown => 2,
            GoalPhase::Brake => 3,
            GoalPhase::Victory => 4,
            GoalPhase::CameraShift => 5,
            GoalPhase::SpecialTransition => 6,
        };
        let flags = u32::from(output.start_long_brake)
            | u32::from(output.stop_long_brake) << 1
            | u32::from(output.stop_water_sound) << 2
            | u32::from(output.begin_results) << 3
            | u32::from(output.check_braking_dust) << 5;
        let mut score = ScoreState {
            level_score: 49900,
            lives: 250,
            music_request: 0,
        };
        score.award(output.bonus, 0, true);
        assert_eq!(
            [
                phase,
                p.q_world_x,
                p.q_world_y,
                i32::from(p.q_speed_ground),
                p.move_state as i32,
                i32::from(p.char_state),
                i32::from(p.sprite_offset_x),
                i32::from(p.sprite_offset_y),
                p.sprite_frame_flags as i32,
                i32::from(p.unk61),
                i32::from(p.unk62),
                i32::from(p.unk63),
                i32::from(p.unk70),
                i32::from(p.unk71),
                i32::from(controller.victory_timer),
                shift_y,
                flags as i32,
                output.bonus as i32,
                if output.advance_ground { 5 } else { 0 },
                score.level_score,
                i32::from(score.lives),
                i32::from(score.music_request)
            ],
            words::<22>(bytes),
            "goal player row {row}"
        );
    }
}
