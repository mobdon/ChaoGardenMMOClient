use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_collision::{air_collision, ground_follow, ground_wall, jump_clearance, native_arctan2},
    native_player::{Callback, Character, PlayerState},
    native_stage::NativeStage,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    seed: u32,
    cases: usize,
    row_words: usize,
    unsupported_cases: usize,
    output_bytes: usize,
    output_sha256: String,
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[test]
#[ignore = "A2NE_ROM and A2NE_COLLISION_ORACLE required; native_collision_oracle.py executes pinned original C"]
fn native_collision_matches_original_c() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-collision-parity.json")).unwrap();
    let oracle =
        std::fs::read(std::env::var_os("A2NE_COLLISION_ORACLE").expect("A2NE_COLLISION_ORACLE"))
            .unwrap();
    assert_eq!(oracle.len(), expected.output_bytes);
    assert_eq!(gba::sha256(&oracle), expected.output_sha256);
    assert_eq!(expected.row_words, 18);
    assert_eq!(
        expected.output_bytes,
        expected.cases * expected.row_words * 4
    );
    let mut seed = expected.seed;
    let mut unsupported = 0;
    for (row, raw) in oracle.chunks_exact(expected.row_words * 4).enumerate() {
        let words: Vec<_> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| i32::from_le_bytes(*bytes))
            .collect();
        let x = next(&mut seed);
        let qx = ((x % 19_840) as i32 - 32) * 256 + (x >> 24) as i32;
        let y = next(&mut seed);
        let qy = ((y % 3_136) as i32 - 32) * 256 + (y >> 24) as i32;
        let air_x = ((next(&mut seed) % 12_289) as i32 - 6_144) as i16;
        let air_y = ((next(&mut seed) % 12_289) as i32 - 6_144) as i16;
        let ground = ((next(&mut seed) % 8_193) as i32 - 4_096) as i16;
        let flags = next(&mut seed);
        let mut p = PlayerState::new_at(Character::Sonic, qx, qy);
        p.q_speed_air_x = air_x;
        p.q_speed_air_y = air_y;
        p.q_speed_ground = ground;
        p.rotation = ((flags % 49) as i32 - 24) as u8;
        p.layer = (flags & 1) as u8;
        p.sprite_offset_x = 6;
        p.sprite_offset_y = if flags & 2 != 0 { 9 } else { 14 };
        p.unk28 = 0x40;
        p.unk29 = 0xc0;
        p.unk62 = 3;
        p.unk63 = -4;
        p.defeat_score_index = 7;
        p.move_state = 0;
        p.callback = Callback::Ground;
        if row % 4 == 3 {
            p.rotation = 0;
            p.move_state = 0x116;
            p.callback = Callback::Airborne;
        }
        assert_eq!(
            i32::from(native_arctan2(air_x, air_y)),
            words[2],
            "row {row}: integer angle"
        );
        let original = p;
        let result = match row % 4 {
            0 => jump_clearance(&stage, &mut p),
            1 => ground_wall(&stage, &mut p).map(|_| 0),
            2 => ground_follow(&stage, &mut p).map(|_| 0),
            3 => air_collision(&stage, &mut p).map(|_| 0),
            _ => unreachable!(),
        };
        if words[0] != 0 {
            unsupported += 1;
            assert!(result.is_err(), "row {row}: unsupported native path");
            assert_eq!(p, original, "row {row}: transactional unsupported state");
            continue;
        }
        let numeric_result = result.unwrap_or_else(|err| panic!("row {row}: {err}"));
        let actual = [
            0,
            numeric_result,
            i32::from(native_arctan2(air_x, air_y)),
            p.q_world_x,
            p.q_world_y,
            i32::from(p.q_speed_air_x),
            i32::from(p.q_speed_air_y),
            i32::from(p.q_speed_ground),
            p.move_state as i32,
            i32::from(p.rotation),
            i32::from(p.sprite_offset_x),
            i32::from(p.sprite_offset_y),
            i32::from(p.unk28),
            i32::from(p.unk29),
            i32::from(p.unk62),
            i32::from(p.unk63),
            i32::from(p.defeat_score_index),
            i32::from(p.callback == Callback::Ground),
        ];
        assert_eq!(
            actual.as_slice(),
            words,
            "row {row}, operation {}: initial {original:?}",
            row % 4
        );
    }
    assert_eq!(unsupported, expected.unsupported_cases);
}
