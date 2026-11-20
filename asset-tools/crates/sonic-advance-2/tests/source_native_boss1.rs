use serde::Deserialize;
use sonic_advance_2_assets::{
    Error, VerifiedRom, gba,
    native_boss1::{
        self as boss, Boss1Context, Boss1Events, Boss1Phase, Boss1Resources, Boss1State,
        Boss1Terrain, HammerState,
    },
    native_goal::ScoreState,
    native_runtime::PseudoRng,
};
use std::cell::RefCell;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    seed: u32,
    cases: usize,
    row_words: usize,
    output_bytes: usize,
    output_sha256: String,
    table_offset: usize,
    table_bytes: usize,
    table_sha256: String,
    palette_offset: usize,
    palette_bytes: usize,
    palette_sha256: String,
}
fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}
struct Terrain {
    seed: u32,
    probes: RefCell<Vec<[i32; 3]>>,
}
impl Terrain {
    fn probe(&self, x: i32, y: i32, kind: i32) -> i32 {
        self.probes.borrow_mut().push([x, y, kind]);
        let value = (x as u32).wrapping_mul(3)
            ^ (y as u32).wrapping_mul(5)
            ^ self.seed
            ^ if kind != 0 { 0x81428391 } else { 0 };
        (value % 25) as i32 - 12
    }
}
impl Boss1Terrain for Terrain {
    fn single_floor(&self, x: i32, y: i32) -> Result<i32, Error> {
        Ok(self.probe(x, y, 0))
    }
    fn hammer_floor(&self, x: i32, y: i32) -> Result<i32, Error> {
        Ok(self.probe(x, y, 1))
    }
}

#[test]
#[ignore = "A2NE_ROM and A2NE_BOSS1_ORACLE required; exact native Egg Hammer Tank II C with explicit service stubs"]
fn boss1_constructor_roll_in_attacks_suspension_and_hits_match_original_c() {
    let metadata: Oracle =
        serde_json::from_str(include_str!("../catalog/native-boss1-parity.json")).unwrap();
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let resources = Boss1Resources::load(&rom).unwrap();
    assert_eq!(
        gba::sha256(&bytes[metadata.table_offset..metadata.table_offset + metadata.table_bytes]),
        metadata.table_sha256
    );
    assert_eq!(
        gba::sha256(
            &bytes[metadata.palette_offset..metadata.palette_offset + metadata.palette_bytes]
        ),
        metadata.palette_sha256
    );
    let data =
        std::fs::read(std::env::var_os("A2NE_BOSS1_ORACLE").expect("A2NE_BOSS1_ORACLE")).unwrap();
    assert_eq!(data.len(), metadata.output_bytes);
    assert_eq!(data.len(), metadata.cases * metadata.row_words * 4);
    assert_eq!(gba::sha256(&data), metadata.output_sha256);
    let mut seed = metadata.seed;
    for (row, raw) in data.as_chunks::<308>().0.iter().enumerate() {
        let expected: [i32; 77] =
            std::array::from_fn(|i| i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()));
        let r: [u32; 15] = std::array::from_fn(|_| next(&mut seed));
        let context = Boss1Context {
            final_stage: r[4] & 8 != 0,
            difficulty: u8::from(r[4] & 16 != 0),
            game_mode: (r[4] % 3) as u8,
        };
        let mut state = Boss1State::create(&resources, context).unwrap();
        state.x = (r[0] % 10000) as i32 * 256 + (r[0] >> 24) as i32;
        state.y = (r[1] % 2000) as i32 * 256 + (r[1] >> 24) as i32;
        state.speed_x = r[2] as i16;
        state.speed_y = (r[2] >> 16) as i16;
        state.hammer_angle = (r[3] % 1536) as i32 - 128;
        state.hammer_extension = 1 << (r[4] % 5);
        state.hammer_state = HammerState::try_from((row % 8) as u32).unwrap();
        state.timer = r[5] % 1000;
        if row % 7 == 0 {
            state.timer = 1;
        }
        if row % 11 == 0 {
            state.timer = 0;
        }
        if row % 17 == 3 {
            state.timer = r[5] % 61;
        }
        state.scene_timer = r[6] % 140;
        state.arm_base_sin = r[7];
        state.arm_base_angle = r[8];
        state.front_axle_y = (r[9] % 8192) as i32;
        state.rear_axle_y = (r[10] % 8192) as i32;
        state.health = (r[11] % 10) as u8;
        state.invulnerability = (r[11] >> 8) as u8;
        state.pilot_laugh_timer = (r[11] >> 16) as u8;
        state.pilot_variant = (r[11] >> 24) as u8;
        state.pilot_previous_variant = 7;
        for i in 0..8 {
            state.arm_angles[i] = ((r[12] >> (i * 3)) % 1024) as i32 - 128;
            state.hammer_positions[i] =
                resources.hammer_positions[i] + ((r[13] >> (i * 3)) % 16384) as i32;
            state.hammer_angles[i] = ((r[14] >> (i * 3)) % 1024) as i32 - 128;
        }
        let terrain = Terrain {
            seed: r[7],
            probes: RefCell::new(Vec::new()),
        };
        let mut rng = PseudoRng::seeded(r[8]);
        let mut score = ScoreState {
            level_score: 49900,
            lives: 250,
            music_request: 0,
        };
        let mut output = Boss1Events::default();
        let mut touching = -1;
        match row % 17 {
            operation @ 0..=7 => {
                state.hammer_state = HammerState::try_from(operation as u32).unwrap();
                output = state.hammer_step(&resources, &terrain, &mut rng).unwrap();
            }
            8 => state.update_arm(&resources),
            9 => output = state.handle_hit(&mut score),
            10 => state.update_pilot(),
            11 => state.pilot_laugh(),
            12 => state.falling_step(),
            13 => state.on_terrain_step(&resources, &terrain).unwrap(),
            14 => touching = i32::from(state.is_touching_terrain(&terrain).unwrap()),
            15 => output = state.motion_tick(&resources, &terrain, &mut rng).unwrap(),
            16 => {
                state = Boss1State::create(&resources, context).unwrap();
                rng.state = r[14];
            }
            _ => unreachable!(),
        }
        let probes = terrain.probes.borrow();
        let mut actual = vec![
            state.x,
            state.y,
            i32::from(state.speed_x),
            i32::from(state.speed_y),
            state.hammer_angle,
            state.hammer_extension as i32,
            state.hammer_state as i32,
            state.timer as i32,
            state.scene_timer as i32,
            state.arm_base_sin as i32,
            state.arm_base_angle as i32,
            state.front_axle_y,
            state.rear_axle_y,
            i32::from(state.health),
            i32::from(state.invulnerability),
            i32::from(state.pilot_laugh_timer),
            i32::from(state.pilot_variant),
            i32::from(state.pilot_previous_variant),
            rng.state as i32,
            score.level_score,
            i32::from(score.lives),
            i32::from(output.music_request.unwrap_or(0)),
            i32::from(score.music_request),
            i32::from(output.sound.unwrap_or(0)),
            i32::from(output.screen_shake),
            i32::from(output.render),
            i32::from(state.phase == Boss1Phase::Active),
            if output.unlock_player_input {
                0
            } else {
                0x200000
            },
            touching,
            probes.len() as i32,
        ];
        actual.extend(state.arm_positions);
        actual.extend(state.arm_angles);
        actual.extend(state.hammer_positions);
        actual.extend(state.hammer_angles);
        for i in 0..5 {
            actual.extend(probes.get(i).copied().unwrap_or([0; 3]));
        }
        assert_eq!(
            actual.as_slice(),
            expected,
            "boss1 row{row} operation{}",
            row % 17
        );
    }
}

#[test]
#[ignore = "A2NE_ROM required; native boss motion probe failures must be transactional"]
fn boss1_motion_error_preserves_state_and_rng() {
    struct Failure;
    impl Boss1Terrain for Failure {
        fn single_floor(&self, _x: i32, _y: i32) -> Result<i32, Error> {
            Err(Error("probe unavailable".into()))
        }
        fn hammer_floor(&self, _x: i32, _y: i32) -> Result<i32, Error> {
            Err(Error("probe unavailable".into()))
        }
    }
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let resources = Boss1Resources::load(&rom).unwrap();
    let mut state = Boss1State::create(
        &resources,
        Boss1Context {
            final_stage: false,
            difficulty: 0,
            game_mode: 0,
        },
    )
    .unwrap();
    state.phase = Boss1Phase::Active;
    let before = state.clone();
    let mut rng = PseudoRng::seeded(22);
    assert!(state.motion_tick(&resources, &Failure, &mut rng).is_err());
    assert_eq!(state, before);
    assert_eq!(rng.state, 22);
    assert_eq!(boss::vector_angle(0, 0), -1);
}
