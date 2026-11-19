use sonic_advance_assets::{
    binary,
    stage::{self, Stage},
};

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn pointer(bytes: &mut [u8], at: usize, target: usize) {
    put32(bytes, at, 0x08000000 + target as u32);
}

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 0x1000];
    for (field, target) in [
        (0, 0x710),
        (4, 0x730),
        (8, 0x400),
        (12, 0x700),
        (16, 0x701),
        (20, 0x734),
    ] {
        pointer(&mut bytes, field, target);
    }
    for (at, value) in [(24, 1), (26, 1), (28, 96), (30, 96)] {
        put16(&mut bytes, at, value);
    }
    for (at, map) in [(0x20, 0x700), (0x44, 0x701)] {
        put16(&mut bytes, at, 12);
        put16(&mut bytes, at + 2, 12);
        pointer(&mut bytes, at + 8, 0x100);
        put32(&mut bytes, at + 12, 96);
        pointer(&mut bytes, at + 16, 0x200);
        put16(&mut bytes, at + 22, 256);
        pointer(&mut bytes, at + 24, 0x400);
        pointer(&mut bytes, at + 28, map);
        put16(&mut bytes, at + 32, 1);
        put16(&mut bytes, at + 34, 1);
    }
    put16(&mut bytes, 0x68, 1);
    put16(&mut bytes, 0x6a, 1);
    pointer(&mut bytes, 0x70, 0x100);
    put32(&mut bytes, 0x74, 96);
    put16(&mut bytes, 0x7e, 256);
    pointer(&mut bytes, 0x80, 0x740);
    bytes[0x120..0x140].fill(0x11);
    bytes[0x140..0x160].fill(0x22);
    put16(&mut bytes, 0x202, 31);
    put16(&mut bytes, 0x204, 31 << 5);
    put16(&mut bytes, 0x400, 1);
    put16(&mut bytes, 0x402, 2 | 0x800);
    for index in 144..288 {
        put16(&mut bytes, 0x400 + index * 2, 2);
    }
    bytes[0x701] = 1;
    // Tile 1 is a half-height floor. Tile 2 is a full solid tile.
    bytes[0x718..0x720].fill(0x44);
    bytes[0x720..0x728].fill(0x88);
    bytes[0x731] = 16;
    put16(&mut bytes, 0x734, 2 << 2);
    put16(&mut bytes, 0x740, 2);
    bytes
}

fn parse(bytes: &[u8]) -> sonic_advance_assets::Result<Stage<'_>> {
    Stage::from_records(bytes, 0, 0, [0x20, 0x44], 0x68)
}

#[test]
fn byte_metatiles_render_native_layers_palettes_and_transparency() {
    let bytes = fixture();
    let stage = parse(&bytes).unwrap();
    assert_eq!((stage.width_px(), stage.height_px()), (96, 96));
    assert_eq!(stage.tile_word(0, 0, 0).unwrap(), Some(1));
    assert_eq!(stage.tile_word(8, 0, 0).unwrap(), Some(2 | 0x800));
    let mut pixels = vec![0; 24 * 8];
    stage.render_view(0, 0, 24, 8, 0, &mut pixels).unwrap();
    assert_eq!(pixels[0], 0xff0000);
    assert_eq!(pixels[8], 0x00ff00);
    assert_eq!(pixels[16], 0x00ff00); // transparent front shows native back layer
    pixels.fill(0x0000ff);
    stage.overlay_front(0, 0, 24, 8, 0, &mut pixels).unwrap();
    assert_eq!(pixels[0], 0xff0000); // priority-1 front covers priority-2 actor
    assert_eq!(pixels[16], 0x0000ff); // transparent front retains actor
    stage
        .render_view(-100, -100, 24, 8, 0, &mut pixels)
        .unwrap();
    assert!(pixels.iter().all(|&p| p == 0x00ff00)); // actual background, no placeholder
    assert_eq!(stage.evidence().source_sha256, binary::sha256(&bytes));
}

#[test]
fn signed_profiles_flags_flip_angles_and_bounded_floor_probes() {
    let mut bytes = fixture();
    put16(&mut bytes, 0x402, 1 | 0x400 | 0x800);
    let stage = parse(&bytes).unwrap();
    let s = stage.collision_at(0, 0, 0).unwrap().unwrap();
    assert_eq!(
        (s.vertical_height, s.horizontal_width, s.flags, s.angle),
        (4, 4, 2, 16)
    );
    assert!(!stage.solid_at(0, 3, 0).unwrap());
    assert!(stage.solid_at(0, 4, 0).unwrap());
    let flipped = stage.collision_at(8, 0, 0).unwrap().unwrap();
    assert_eq!(
        (
            flipped.vertical_height,
            flipped.horizontal_width,
            flipped.angle
        ),
        (-4, -4, 144)
    );
    assert!(stage.solid_at(8, 0, 0).unwrap());
    assert!(!stage.solid_at(8, 4, 0).unwrap());
    assert_eq!(
        stage.ground_below(0, 0, 8, 0).unwrap().unwrap().surface_y,
        4
    );
    assert_eq!(
        stage.ground_below(0, 6, 8, 0).unwrap().unwrap().surface_y,
        4
    );
    assert!(stage.ground_below(8, 0, 8, 0).unwrap().is_none()); // ceiling is not an upright floor
    assert!(stage.ground_below(0, 0, 513, 0).is_err());
    assert!(stage.ground_below(0, 0, -1, 0).is_err());
    assert!(!stage.solid_at(-1, 0, 0).unwrap());
    assert_eq!(stage.tile_word(96, 0, 0).unwrap(), None);
    assert!(stage.collision_at(0, 0, 2).is_err());
    assert!(stage
        .ground_below(i32::MAX, i32::MAX, 512, 0)
        .unwrap()
        .is_none());
}

#[test]
fn tile_upload_cycles_the_prefix_and_preserves_unrelated_character_data() {
    let mut bytes = fixture();
    put16(&mut bytes, 0x24, 64);
    bytes[0x26] = 2;
    bytes[0x27] = 3;
    bytes[0x180..0x1a0].fill(0x22); // second tile in the uploaded prefix
    pointer(&mut bytes, 0x44 + 8, 0x800); // distinct backing for the second layer
    bytes[0x840..0x860].fill(0x11);
    let stage = parse(&bytes).unwrap();
    for (tick, expected) in [
        (0, 0xff0000),
        (2, 0xff0000),
        (3, 0x00ff00),
        (5, 0x00ff00),
        (6, 0xff0000),
    ] {
        let mut pixels = [0; 24];
        stage.render_view(0, 0, 24, 1, tick, &mut pixels).unwrap();
        assert_eq!(
            (pixels[0], pixels[8], pixels[16]),
            (expected, 0x00ff00, 0xff0000)
        );
    }
    for size in [0, 31, 128] {
        put16(&mut bytes, 0x24, size);
        assert!(parse(&bytes).is_err());
    }
    put16(&mut bytes, 0x24, 64);
    bytes[0x26] = 255; // advertised frames run past end of source
    assert!(parse(&bytes).is_err());
}

#[test]
fn malformed_graphs_and_output_budgets_fail_before_rendering() {
    let bytes = fixture();
    for length in [0, 31, 127, 0x600, 0x735] {
        assert!(parse(&bytes[..length]).is_err());
    }
    let mut changed = bytes.clone();
    put16(&mut changed, 28, 95);
    assert!(parse(&changed).is_err());
    let mut changed = bytes.clone();
    pointer(&mut changed, 0x20 + 28, 0x702);
    assert!(parse(&changed).is_err());
    let mut changed = bytes.clone();
    put16(&mut changed, 0x400, 1023);
    assert!(parse(&changed).is_err());
    let mut changed = bytes.clone();
    pointer(&mut changed, 0, 0xffff);
    assert!(parse(&changed).is_err());
    let mut changed = bytes.clone();
    put16(&mut changed, 24, 1025);
    assert!(parse(&changed).is_err());
    let mut changed = bytes.clone();
    put16(&mut changed, 0x20 + 22, 16);
    assert!(parse(&changed).is_err()); // missing initial banks are not invented black pixels
    let stage = parse(&bytes).unwrap();
    assert!(stage.render_view(0, 0, usize::MAX, 8, 0, &mut []).is_err());
    assert!(stage.render_view(0, 0, 0, 0, 0, &mut []).is_err());
    assert!(stage.render_view(0, 0, 8, 8, 0, &mut [0; 63]).is_err());
    assert!(Stage::load(&bytes, 0).is_err()); // generic fixtures never pass native identity
    assert!(Stage::from_records(&bytes, 18, 0, [0x20, 0x44], 0x68).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; hashes only, no native pixels distributed"]
fn all_eighteen_actual_stage_graphs_and_native_viewport_hashes_match() {
    const ORIGIN_HASHES: [&str; 18] = [
        "fc572a21540b6bab76b80aaeaefe5eb6b06e8a57fb28545d450c9b81c933c4a5",
        "fc572a21540b6bab76b80aaeaefe5eb6b06e8a57fb28545d450c9b81c933c4a5",
        "730c9a4aa4bb366adb4085bb695f4545aa95654887a3e5fca2020d788e9886ce",
        "3e9f894ae5dae0b24872ba164098ae3634221ea769054511d58dca75cff2f6b9",
        "330b06157573d7f329c8df8250dd364f85fe2d75ea360f9cb078605649ac472c",
        "2e3a207674ce59cc4dde2ec38759b708620577ec0a48fa75faf8185e3d058811",
        "60ca5805f72ad62306064d1e03e1e18703d08218a41761adc454d0329aa21764",
        "91964edc784b065d003ce15be94af8af01966550c39bb9dec5ace7d8c9f5014e",
        "ad203045611b0d4ba443b72a40baf810a6d9de75c79ce4a7ae60debc0af5066c",
        "854b36270fdb25e9878495c1cd3d88b1cb66c2ff1afe7881f9049c165903056c",
        "be35ee0ecd30be7a513243e6cf5bd8db755451868750a368dbc70b9100639f9a",
        "0fc4b47479582e3e202a75bcc1c4ef06c57274414f89e2d460c07a4f76afb2b8",
        "a92b5179ab173e93a3b005e8dc860b918bc0b51fc7c6a74aced06d3708198ac0",
        "68928be772ae494a38b787167ffc57c62b2619ed809e1e41d890f8967b1d9de9",
        "0242ed2b06282e11935dfd7929393d3c574e47059105f9c2a0f62fb9722b14d7",
        "3e9f894ae5dae0b24872ba164098ae3634221ea769054511d58dca75cff2f6b9",
        "ef038e146f22047faf1e240a8b493643f63fd938f1ce192c171ff24cad60b58f",
        "0c87f7f77728bc334895d2b6e93dcf784c5683bb82642b315cc2c442570d1b22",
    ];
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    for (index, expected) in ORIGIN_HASHES.iter().enumerate() {
        let stage = Stage::load(&rom, index).unwrap();
        let e = stage.evidence();
        assert_eq!(e.source_sha256, binary::ROM_SHA256);
        assert!(e.spans.len() >= 16);
        for s in &e.spans {
            assert_eq!(
                binary::sha256(binary::range(&rom, s.offset, s.length).unwrap()),
                s.sha256
            );
        }
        let mut pixels = vec![0; 240 * 160];
        stage.render_view(0, 0, 240, 160, 0, &mut pixels).unwrap();
        assert!(pixels.iter().all(|p| p & 0xff000000 == 0));
        let rgb: Vec<u8> = pixels
            .iter()
            .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8])
            .collect();
        assert_eq!(&binary::sha256(&rgb), expected, "stage {index}");
        if index == 0 {
            assert_eq!((stage.width_px(), stage.height_px()), (13056, 1344));
            assert_eq!(
                (e.referenced_metatiles, e.referenced_tile_prefix),
                (177, 982)
            );
            for (x, y, tick, expected) in [
                (
                    0,
                    360,
                    0,
                    "1ef1c1917a3fb8b0ce7097a5b3f90f20a48e99b213f17970931dc850edba5df5",
                ),
                (
                    104,
                    444,
                    0,
                    "fe5933997e0db422d0435af4ed580855e7fec39ec3186216802b30c171e059ce",
                ),
                (
                    1024,
                    512,
                    320,
                    "be0ce1f7141a7724b4b2e909f42beae5cd7c041ae745ba4333ade1832fa2cec4",
                ),
            ] {
                stage
                    .render_view(x, y, 240, 160, tick, &mut pixels)
                    .unwrap();
                let rgb: Vec<u8> = pixels
                    .iter()
                    .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8])
                    .collect();
                assert_eq!(binary::sha256(&rgb), expected, "stage 0 camera {x},{y}");
            }
            let mut samples = Vec::new();
            for layer in 0..2 {
                for y in (0..1344).step_by(17) {
                    for x in (0..2048).step_by(13) {
                        let s = stage.collision_at(x, y, layer).unwrap().unwrap();
                        samples.extend_from_slice(&s.tile_word.to_le_bytes());
                        samples.extend_from_slice(&[
                            s.vertical_height as u8,
                            s.horizontal_width as u8,
                            s.flags,
                            s.angle,
                            s.horizontal_angle,
                            u8::from(s.solid_at_y((y & 7) as u8)),
                        ]);
                    }
                }
            }
            assert_eq!(samples.len() / 8, 25280);
            assert_eq!(
                binary::sha256(&samples),
                "4aeeb78e04ef05a509c5a78052871ca3ac88b0387ef4772720b20765c1f87d1d"
            );
        }
    }
    assert!(Stage::load(&rom, stage::STAGE_COUNT).is_err());
}

#[test]
#[ignore = "requires local ASOE ROM"]
fn act_two_animation_matches_native_upload_and_scroll_reference() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 1).unwrap();
    let mut pixels = vec![0; 240 * 160];
    for (tick, expected) in [
        (
            0,
            "8f289485adc86858e987178c45c2b68eeab4464ba51cf8e53240d58abe09f439",
        ),
        (
            9,
            "8f289485adc86858e987178c45c2b68eeab4464ba51cf8e53240d58abe09f439",
        ),
        (
            10,
            "7b07ba73bf4fdfb7a6a18443c03633952974f749904ce149b5888a8fb763521a",
        ),
        (
            20,
            "e9b1a2fa31f3443c18c27790a335f383a90295e4c481695de33d8ea00df54c36",
        ),
        (
            30,
            "96c5e534c753a3423fc6f1d30b564878973354166efeef9000adb07f017191fd",
        ),
        (
            40,
            "363336bce4f1299f179a81d37bd5317dadbdadf488eb37625825429f1a738bdc",
        ),
    ] {
        stage
            .render_view(4200, 96, 240, 160, tick, &mut pixels)
            .unwrap();
        let rgba: Vec<_> = pixels
            .iter()
            .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8, 255])
            .collect();
        assert_eq!(binary::sha256(&rgba), expected, "tick {tick}");
    }
}
