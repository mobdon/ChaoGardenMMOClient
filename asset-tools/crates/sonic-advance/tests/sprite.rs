use sonic_advance_assets::{
    binary,
    sprite::{AnimationSource, OamObject, Program, SpriteBank, SpriteEvent, SpritePlayer},
    Error, Result,
};

fn words(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

struct Synthetic {
    programs: Vec<Vec<u8>>,
    tiles: [u8; 64],
    palette: [u8; 64],
}

impl Synthetic {
    fn new(program: &[i32]) -> Self {
        Self {
            programs: vec![words(program)],
            tiles: [0x21; 64],
            palette: [0x7f; 64],
        }
    }
}

impl AnimationSource for Synthetic {
    fn program(&self, animation: u16, variant: u8) -> Result<Program<'_>> {
        if variant != 0 {
            return Err(Error("missing synthetic variant".into()));
        }
        Ok(Program {
            bytes: self
                .programs
                .get(usize::from(animation))
                .ok_or_else(|| Error("missing program".into()))?,
            entry_words: 0,
        })
    }
    fn tiles(&self, index: u32, count: usize, eight_bit: bool) -> Result<&[u8]> {
        let stride = if eight_bit { 64 } else { 32 };
        let offset = usize::try_from(index)
            .map_err(|_| Error("index overflow".into()))?
            .checked_mul(stride)
            .ok_or_else(|| Error("index overflow".into()))?;
        binary::range(
            &self.tiles,
            offset,
            count
                .checked_mul(stride)
                .ok_or_else(|| Error("count overflow".into()))?,
        )
    }
    fn palette(&self, index: u32, colors: usize) -> Result<&[u8]> {
        binary::range(
            &self.palette,
            index as usize * 32,
            colors
                .checked_mul(2)
                .ok_or_else(|| Error("count overflow".into()))?,
        )
    }
}

#[test]
fn native_delay_fractional_speed_loops_and_end_are_frame_exact() {
    let source = Synthetic::new(&[6, 0, 6, 1, -3, 4]);
    let mut player = SpritePlayer::new(0, 0);
    for call in 0..600 {
        player.tick(&source).unwrap();
        assert_eq!(player.current_frame, Some(((call / 6) % 2) as u32));
    }
    player = SpritePlayer::new(0, 0);
    player.speed_q4_4 = 8;
    for call in 0..240 {
        player.tick(&source).unwrap();
        assert_eq!(player.current_frame, Some(((call / 12) % 2) as u32));
    }
    let source = Synthetic::new(&[1, -1, -4]);
    let mut player = SpritePlayer::new(0, 0);
    assert_eq!(
        player.tick(&source).unwrap().events,
        [SpriteEvent::Frame(None)]
    );
    assert!(player.tick(&source).unwrap().ended);
    assert!(player.tick(&source).unwrap().events.is_empty());
}

#[test]
fn side_effects_preserve_native_operands_and_hitbox_flips() {
    let hitbox = u32::from_le_bytes([(-3_i8) as u8, (-4_i8) as u8, 5, 6]) as i32;
    let source = Synthetic::new(&[
        -1,
        0,
        2,
        -2,
        0,
        16,
        -5,
        117,
        -6,
        18,
        hitbox,
        -7,
        0xfffe0003_u32 as i32,
        -8,
        42,
        99,
        -11,
        2,
        -12,
        7,
        1,
        0,
        -4,
    ]);
    let mut player = SpritePlayer::new(0, 0);
    player.flip_x = true;
    player.flip_y = true;
    let result = player.tick(&source).unwrap();
    assert!(result.events.contains(&SpriteEvent::Sound(117)));
    assert!(result
        .events
        .contains(&SpriteEvent::Noop8 { operands: [42, 99] }));
    assert_eq!((player.translation_x, player.translation_y), (3, -2));
    let hitbox = player.hitboxes[2].unwrap();
    assert_eq!(
        (
            hitbox.index,
            hitbox.left,
            hitbox.top,
            hitbox.right,
            hitbox.bottom
        ),
        (18, -5, -6, 3, 4)
    );
    assert_eq!((player.priority, player.oam_order), (2, 7));
    assert!(player.tick(&source).unwrap().ended);
}

#[test]
fn changes_enter_new_program_and_next_native_tick_reinitializes_selection() {
    let source = Synthetic {
        programs: vec![words(&[-9, 1]), words(&[3, 7, -4])],
        tiles: [0; 64],
        palette: [0; 64],
    };
    let mut player = SpritePlayer::new(0, 0);
    let changed = player.tick(&source).unwrap();
    assert_eq!((player.animation, player.current_frame), (1, Some(7)));
    assert!(changed.events.contains(&SpriteEvent::AnimationChanged {
        animation: 1,
        variant: 0
    }));
    // Native -9 invalidates prevVariant and executes the selected program now;
    // the ordinary next-call initialization starts its first frame once again.
    assert!(player
        .tick(&source)
        .unwrap()
        .events
        .contains(&SpriteEvent::Frame(Some(7))));
    player.animation = 0;
    assert!(player
        .tick(&source)
        .unwrap()
        .events
        .contains(&SpriteEvent::AnimationChanged {
            animation: 1,
            variant: 0
        }));
}

#[test]
fn malformed_branches_commands_and_resources_fail_without_partial_commit() {
    let malformed = [
        vec![-3, 0],  // zero-time loop
        vec![-3, 1],  // before program
        vec![-3, -1], // targets operand
        vec![-1, 0, 1025, 1, 0],
        vec![-2, 0, (250 << 16) | 16, 1, 0],
        vec![-10, 0, 0, 0], // undefined native return remains unsupported
        vec![-13, 0],
        vec![128, 0], // Q8.8 positive-duration overflow
        vec![1, -2],
        vec![-11, 4, 1, 0],
        vec![-12, 32, 1, 0],
        vec![-9, 0x1000000], // variant wider than byte
    ];
    for program in malformed {
        let source = Synthetic::new(&program);
        let mut player = SpritePlayer::new(0, 0);
        assert!(player.tick(&source).is_err(), "{program:?}");
        assert_eq!(player.current_frame, None);
        assert_eq!((player.delay_q8(), player.translation_x), (0, 0));
    }
    for length in 0..8 {
        let source = Synthetic {
            programs: vec![vec![0; length]],
            tiles: [0; 64],
            palette: [0; 64],
        };
        assert!(SpritePlayer::new(0, 0).tick(&source).is_err());
    }
}

#[test]
fn normal_oam_shapes_flips_and_prohibited_modes_are_explicit() {
    let object = OamObject::decode(&[32, 0x40, 0, 0x70, 16, 0x60]).unwrap();
    assert_eq!(
        (
            object.x,
            object.y,
            object.width,
            object.height,
            object.tile,
            object.palette_bank
        ),
        (0, 32, 32, 8, 16, 6)
    );
    assert!(object.flip_x && object.flip_y);
    assert!(OamObject::decode(&[0, 1, 0, 0, 0, 0]).is_ok()); // native template affine-on bit is cleared by DisplaySprite
    for mask in [0x0200_u16, 0x0400, 0x0800, 0x1000, 0xc000] {
        let mut raw = [0; 6];
        raw[..2].copy_from_slice(&mask.to_le_bytes());
        assert!(OamObject::decode(&raw).is_err());
    }
    assert!(OamObject::decode(&[0; 5]).is_err());
}

#[test]
#[ignore = "requires exact private SONIC_ADVANCE_ASOE_ROM"]
fn authentic_animation_loops_native_oam_and_independent_pixel_hashes() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    assert_eq!(bank.variant_count(0).unwrap(), 1);
    assert!(bank.variant_count(15).is_err());
    assert!(bank.program(0, 1).is_err());
    for &(animation, variant) in &[(0, 0), (4, 0), (5, 0), (8, 0), (10, 1), (707, 0)] {
        let mut player = SpritePlayer::new(animation, variant);
        let mut frames = std::collections::BTreeSet::new();
        for _ in 0..600 {
            player.tick(&bank).unwrap();
            if let Some(frame) = player.current_frame {
                frames.insert(frame);
            }
            let image = bank
                .render(&player)
                .unwrap_or_else(|error| {
                    panic!(
                        "animation {animation} frame {:?}: {error}",
                        player.current_frame
                    )
                })
                .unwrap();
            assert!(image.image.pixels.iter().any(|pixel| pixel[3] != 0));
        }
        assert!(frames.len() >= 2, "animation {animation}");
    }
    let mut collection = SpritePlayer::new(708, 0);
    let mut sounds = Vec::new();
    for _ in 0..40 {
        sounds.extend(
            collection
                .tick(&bank)
                .unwrap()
                .events
                .into_iter()
                .filter_map(|event| {
                    if let SpriteEvent::Sound(id) = event {
                        Some(id)
                    } else {
                        None
                    }
                }),
        );
    }
    assert_eq!(sounds, [117]);
    assert!(collection.ended);
    let dimensions = bank.dimensions(0, 0).unwrap();
    assert_eq!(
        (
            dimensions.width,
            dimensions.height,
            dimensions.anchor_x,
            dimensions.anchor_y,
            dimensions.subframes
        ),
        (32, 40, 16, 24, 2)
    );
    let mut player = SpritePlayer::new(0, 0);
    player.tick(&bank).unwrap();
    let original = bank.render(&player).unwrap().unwrap();
    player.flip_x = true;
    let mirrored = bank.render(&player).unwrap().unwrap();
    for y in 0..original.image.height {
        for x in 0..original.image.width {
            assert_eq!(
                original.image.pixels[y * original.image.width + x],
                mirrored.image.pixels[y * original.image.width + original.image.width - 1 - x]
            );
        }
    }
    assert_eq!(original.anchor_x, mirrored.anchor_x);
}

#[test]
#[ignore = "requires exact private SONIC_ADVANCE_ASOE_ROM; independent hash-only oracle"]
fn authentic_rgba_frames_match_independent_reference_at_exact_ticks() {
    // Metadata from a separately written JavaScript reader/compositor. No
    // native pixels, palettes, scripts or ROM bytes are distributed here.
    #[allow(clippy::type_complexity)]
    let cases: &[(u16, u8, usize, u32, usize, usize, i16, i16, &str)] = &[
        (
            0,
            0,
            1,
            0,
            32,
            40,
            16,
            24,
            "90318f22d1954f5bd3e6fe97334c93c456a6d7a9aeeb8462712ebe5c876fef2f",
        ),
        (
            0,
            0,
            8,
            1,
            32,
            32,
            16,
            16,
            "c76e0960db7ca4429a1d6193d95da70de059ccabc9436099c2e639f7c9b594b9",
        ),
        (
            0,
            0,
            16,
            2,
            32,
            32,
            16,
            16,
            "2cdcd53a10c4bb3ff74d99b5107f5a0a859b8ba8d45bb1fd4bc5fb7caf62dcec",
        ),
        (
            0,
            0,
            32,
            5,
            32,
            40,
            16,
            24,
            "5ed975ddb697a9ca2f3797b22caac882362e091669ce69f1e6037695b866c2e5",
        ),
        (
            0,
            0,
            64,
            8,
            32,
            32,
            16,
            16,
            "bdc72d5f7eb36f1d697b36eddbc463b1f14f10605f1f4bad26b73a95202bce1f",
        ),
        (
            4,
            0,
            1,
            0,
            64,
            64,
            32,
            48,
            "1b9db99de764fb3c70785bbdb77fde6f84899f47dea196f755a3c1a86aba78f7",
        ),
        (
            4,
            0,
            8,
            2,
            64,
            64,
            32,
            48,
            "5b712afd38be453fd6db311e44645b57adeb94d0d3c953602bc9e28b3fba21ec",
        ),
        (
            4,
            0,
            16,
            5,
            64,
            64,
            32,
            48,
            "05553d4a5c5bded7083a6bb8563f73bbd285cff6d813f266dc8761fb6c453398",
        ),
        (
            4,
            0,
            32,
            2,
            64,
            64,
            32,
            48,
            "5b712afd38be453fd6db311e44645b57adeb94d0d3c953602bc9e28b3fba21ec",
        ),
        (
            4,
            0,
            64,
            5,
            64,
            64,
            32,
            48,
            "05553d4a5c5bded7083a6bb8563f73bbd285cff6d813f266dc8761fb6c453398",
        ),
        (
            5,
            0,
            1,
            0,
            64,
            64,
            32,
            48,
            "10e838f6a5499aec62b96ed906bd9d13d8e593220829562bbdb1700a92a1f7e1",
        ),
        (
            5,
            0,
            8,
            3,
            64,
            64,
            32,
            48,
            "d31cd8f3cbbf857a4ae2f2c9fe19e322a978a3b04f8acdfb1c3fcb101277d471",
        ),
        (
            5,
            0,
            16,
            3,
            64,
            64,
            32,
            48,
            "d31cd8f3cbbf857a4ae2f2c9fe19e322a978a3b04f8acdfb1c3fcb101277d471",
        ),
        (
            5,
            0,
            32,
            3,
            64,
            64,
            32,
            48,
            "d31cd8f3cbbf857a4ae2f2c9fe19e322a978a3b04f8acdfb1c3fcb101277d471",
        ),
        (
            5,
            0,
            64,
            3,
            64,
            64,
            32,
            48,
            "d31cd8f3cbbf857a4ae2f2c9fe19e322a978a3b04f8acdfb1c3fcb101277d471",
        ),
        (
            8,
            0,
            1,
            0,
            32,
            32,
            16,
            16,
            "18a734562a1d3dc3fd5655f5e990eba1d8a00014f0a6cf2daf9cc72b1abd5ee2",
        ),
        (
            8,
            0,
            8,
            3,
            32,
            32,
            16,
            16,
            "121c176b5763ba9509a126610b6682d4ee426eff0dd9ca368ab837574f48ca0b",
        ),
        (
            8,
            0,
            16,
            3,
            32,
            32,
            16,
            16,
            "121c176b5763ba9509a126610b6682d4ee426eff0dd9ca368ab837574f48ca0b",
        ),
        (
            8,
            0,
            32,
            3,
            32,
            32,
            16,
            16,
            "121c176b5763ba9509a126610b6682d4ee426eff0dd9ca368ab837574f48ca0b",
        ),
        (
            8,
            0,
            64,
            3,
            32,
            32,
            16,
            16,
            "121c176b5763ba9509a126610b6682d4ee426eff0dd9ca368ab837574f48ca0b",
        ),
        (
            10,
            1,
            1,
            0,
            32,
            48,
            16,
            28,
            "d99321258f88cafe4f16b55a904f8d0fd25ed420f898858f9cab97d242f491a8",
        ),
        (
            10,
            1,
            8,
            6,
            32,
            40,
            16,
            20,
            "83bd4a6e399d96e2aa94ab3adda0748475e243bc7a64c8eaa78196c618b7f669",
        ),
        (
            10,
            1,
            16,
            8,
            32,
            40,
            16,
            20,
            "6a82a474966f2ac0a49dd6c73ab2ea7aa8b9d1738af4808a03f28f2044d9ed7f",
        ),
        (
            10,
            1,
            32,
            8,
            32,
            40,
            16,
            20,
            "6a82a474966f2ac0a49dd6c73ab2ea7aa8b9d1738af4808a03f28f2044d9ed7f",
        ),
        (
            10,
            1,
            64,
            8,
            32,
            40,
            16,
            20,
            "6a82a474966f2ac0a49dd6c73ab2ea7aa8b9d1738af4808a03f28f2044d9ed7f",
        ),
        (
            707,
            0,
            1,
            0,
            16,
            16,
            8,
            16,
            "d30d59d41535e8bfa411196488c2e207f010ef2216c9d8d5bc279fea45e87d90",
        ),
        (
            707,
            0,
            8,
            0,
            16,
            16,
            8,
            16,
            "d30d59d41535e8bfa411196488c2e207f010ef2216c9d8d5bc279fea45e87d90",
        ),
        (
            707,
            0,
            16,
            1,
            16,
            16,
            8,
            16,
            "6abd8392f7bca288c80744a1ba484de9f3741c96c3bf4c2d83031a3c6aa57b86",
        ),
        (
            707,
            0,
            32,
            3,
            16,
            16,
            8,
            16,
            "e15ca70c5d1cf3e1a348f4a9a49f5da4ed69612ff533c9d5911035f3202dc54e",
        ),
        (
            707,
            0,
            64,
            3,
            16,
            16,
            8,
            16,
            "e15ca70c5d1cf3e1a348f4a9a49f5da4ed69612ff533c9d5911035f3202dc54e",
        ),
        (
            708,
            0,
            1,
            0,
            16,
            16,
            8,
            16,
            "2055c2090382b3fa29bce440a9c0d12ccbf55bd10bcfede360bc4af06f79bd65",
        ),
        (
            708,
            0,
            8,
            0,
            16,
            16,
            8,
            16,
            "2055c2090382b3fa29bce440a9c0d12ccbf55bd10bcfede360bc4af06f79bd65",
        ),
        (
            708,
            0,
            16,
            1,
            16,
            16,
            8,
            16,
            "78bef034186294839c195dbbabdd9b7bd0c1344cd1a62d938cb4ed0d1f550a1b",
        ),
        (
            708,
            0,
            32,
            3,
            16,
            16,
            8,
            16,
            "a5b5bbbadde8415ee05b427441cf4dfd6616ebb85dfdeeee2f6ea3f56bb04df3",
        ),
        (
            708,
            0,
            64,
            3,
            16,
            16,
            8,
            16,
            "a5b5bbbadde8415ee05b427441cf4dfd6616ebb85dfdeeee2f6ea3f56bb04df3",
        ),
    ];
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    assert_eq!(cases.len(), 35);
    for &(animation, variant, ticks, frame, width, height, anchor_x, anchor_y, expected) in cases {
        let mut player = SpritePlayer::new(animation, variant);
        for _ in 0..ticks {
            player.tick(&bank).unwrap();
        }
        let rendered = bank.render(&player).unwrap().unwrap();
        assert_eq!(player.current_frame, Some(frame));
        assert_eq!(
            (
                rendered.image.width,
                rendered.image.height,
                rendered.anchor_x,
                rendered.anchor_y
            ),
            (width, height, anchor_x, anchor_y)
        );
        let rgba: Vec<u8> = rendered.image.pixels.into_iter().flatten().collect();
        assert_eq!(
            binary::sha256(&rgba),
            expected,
            "animation {animation} variant {variant} tick {ticks}"
        );
    }
}
