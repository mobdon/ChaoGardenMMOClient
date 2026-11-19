use sonic_advance_assets::{
    binary,
    player::{ContactMode, Input, Player, SineTable},
    stage::Stage,
};

#[test]
fn quadrant_boundaries_keep_native_signed_bias() {
    use ContactMode::*;
    for (angle, mode) in [
        (0, Floor),
        (32, Floor),
        (33, Left),
        (96, Left),
        (97, Ceiling),
        (160, Ceiling),
        (161, Right),
        (224, Right),
        (225, Floor),
    ] {
        assert_eq!(ContactMode::for_angle(angle), mode, "{angle}");
    }
}

#[test]
#[ignore = "uses local ASOE; independent C sampler over a direct Python ROM reader"]
fn directional_distances_match_reference_c_over_8192_probes() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let mut state = 0x53414f45u32;
    let mut draw = |n| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state % n) as i32
    };
    let mut digest = 0xcbf29ce484222325u64;
    use ContactMode::*;
    for i in 0..8192 {
        let x = draw(stage.width_px() as u32 + 64) - 32;
        let y = draw(stage.height_px() as u32 + 64) - 32;
        let hit = stage
            .terrain_sensor(
                x,
                y,
                [Floor, Left, Ceiling, Right][i % 4],
                (i >> 2) & 1,
                i & 8 != 0,
            )
            .unwrap();
        for byte in [hit.distance as u8, hit.angle.unwrap_or(0x55)] {
            digest = (digest ^ u64::from(byte)).wrapping_mul(1099511628211);
        }
    }
    assert_eq!(digest, 0xac26d6b1194ba651);
    assert!(stage
        .terrain_sensor(i32::MIN, i32::MAX, Left, usize::MAX, false)
        .is_err());
    assert!(stage
        .terrain_sensor(i32::MIN, i32::MAX, Ceiling, 1, false)
        .is_ok());
    let mut player = Player::at_pixels(104, 444, SineTable::from_asoe(&rom).unwrap()).unwrap();
    player.collision_layer = 2;
    let before = player.clone();
    assert!(player.step(Input::default(), &stage).is_err());
    assert_eq!(player, before);
    for index in 0..18 {
        let player = Player::sonic_stage(&rom, index).unwrap();
        let at = 0x4a6dec + index * 4;
        assert_eq!(
            (player.x >> 8, player.y >> 8),
            (
                i32::from(i16::from_le_bytes([rom[at], rom[at + 1]])),
                i32::from(i16::from_le_bytes([rom[at + 2], rom[at + 3]]))
            )
        );
    }
    assert!(Player::sonic_stage(&rom, usize::MAX).is_err());
}

#[test]
#[ignore = "requires exact private ASOE image; oracle from retail execution in mGBA"]
fn settled_neo_green_hill_600_frame_trace_matches_retail() {
    use sonic_advance_assets::{
        entities::StageEntities, player::UprightSpikes, runtime::exercise_input, sprite::SpriteBank,
    };
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let entities = StageEntities::load(&rom, 0).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let spikes = UprightSpikes::from_bank(&bank).unwrap();
    let mut player = Player::sonic_first_stage(&rom).unwrap();
    player.y = 113920;
    player.grounded = true;
    let mut states = Vec::new();
    for frame in 0..600 {
        player.collision_layer =
            entities.collision_layer_at(player.x >> 8, player.y >> 8, player.collision_layer as u8)
                as usize;
        for spike in entities
            .interactables
            .records
            .iter()
            .filter(|o| o.kind == 1)
        {
            spikes
                .contact(&mut player, &stage, spike.x, spike.y)
                .unwrap();
        }
        player.step(exercise_input(frame), &stage).unwrap();
        for value in [
            player.x,
            player.y,
            player.vx,
            player.vy,
            player.ground_speed,
            player.half_height,
            i32::from(player.angle),
            player.collision_layer as i32,
        ] {
            states.extend(value.to_le_bytes());
        }
    }
    assert_eq!(
        binary::sha256(&states),
        "28e0bd5eea99bda2b150079cc582519dd8655fa9339ce0fa611ebd793e62e83c"
    );
}

#[test]
#[ignore = "requires native spike hitbox and actual stage"]
fn upright_spike_top_applies_one_vulnerable_hit_and_blocks_air_control() {
    use sonic_advance_assets::{
        player::{SpikeContact, UprightSpikes},
        sprite::SpriteBank,
    };
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let spikes = UprightSpikes::from_bank(&bank).unwrap();
    let mut p = Player::sonic_first_stage(&rom).unwrap();
    p.x = 1528 * 256;
    p.y = (456 - 16) * 256;
    p.vx = 256;
    p.vy = 256;
    assert_eq!(
        spikes.contact(&mut p, &stage, 1528, 456).unwrap(),
        SpikeContact::Damage
    );
    assert_eq!(
        (
            p.vx,
            p.vy,
            p.invulnerability_frames,
            p.animation().animation
        ),
        (-384, -768, 120, 19)
    );
    p.step(
        Input {
            right: true,
            ..Input::default()
        },
        &stage,
    )
    .unwrap();
    assert_eq!((p.vx, p.invulnerability_frames), (-384, 120));
    p.x = 1528 * 256;
    p.y = (456 - 16) * 256;
    p.vy = 256;
    assert_ne!(
        spikes.contact(&mut p, &stage, 1528, 456).unwrap(),
        SpikeContact::Damage
    );
}

#[test]
fn spring_animation_advances_on_native_speed_threshold_and_end_command() {
    use sonic_advance_assets::{player::*, Result};
    struct Empty;
    impl SurfaceQuery for Empty {
        fn bounds(&self) -> (i32, i32) {
            (2048, 2048)
        }
        fn ground_below(&self, _: i32, _: i32, _: i32) -> Result<Option<GroundContact>> {
            Ok(None)
        }
        fn solid_at(&self, _: i32, _: i32) -> Result<bool> {
            Ok(false)
        }
    }
    let mut samples = [0; 256];
    samples[64] = 256;
    samples[192] = -256;
    let mut p = Player::at_pixels(200, 200, SineTable::from_samples(samples).unwrap()).unwrap();
    p.apply_spring_state(21);
    assert!(p.take_animation_restart());
    assert!(!p.take_animation_restart());
    p.vy = -385 - GRAVITY;
    p.step(Input::default(), &Empty).unwrap();
    assert_eq!((p.animation().animation, p.animation().variant), (10, 0));
    p.step(Input::default(), &Empty).unwrap();
    assert_eq!((p.animation().animation, p.animation().variant), (10, 1));
    p.animation_finished();
    assert_eq!((p.animation().animation, p.animation().variant), (10, 2));
    p.apply_spring_state(17);
    assert_eq!((p.animation().animation, p.animation().variant), (11, 0));
    p.animation_finished();
    assert_eq!((p.animation().animation, p.animation().variant), (10, 2));
}
