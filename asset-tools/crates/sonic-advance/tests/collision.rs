use sonic_advance_assets::{binary, collision::*};

fn word(rom: &mut [u8], at: usize, value: u32) {
    rom[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn half(rom: &mut [u8], at: usize, value: u16) {
    rom[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn fixture(profile: u8, flip_bits: u16) -> Vec<u8> {
    // A synthetic one-metatile map. Tile zero is empty, tile one is used only
    // in the first 8x8 cell. No game bytes or native artwork are embedded.
    let mut rom = vec![0; 400];
    for (index, pointer) in [64_u32, 80, 96, 384, 385, 386].into_iter().enumerate() {
        word(&mut rom, index * 4, pointer + 0x08000000);
    }
    half(&mut rom, 24, 1);
    half(&mut rom, 26, 1);
    half(&mut rom, 28, 96);
    half(&mut rom, 30, 96);
    rom[72..80].fill(profile);
    rom[81] = 32;
    half(&mut rom, 96, 1 | flip_bits);
    rom
}

#[test]
fn boundary_nibbles_preserve_directional_profiles_and_raw_metadata() {
    let rom = fixture(0x23, 0);
    let stage = CollisionStage::parse(&rom, 0).unwrap();
    assert_eq!(stage.header_offset(), 0);
    assert_eq!(stage.raw_header(), &rom[..32]);
    assert_eq!(stage.offsets(), [64, 80, 96, 384, 385, 386]);
    assert_eq!((stage.width_metatiles(), stage.height_metatiles()), (1, 1));
    assert_eq!((stage.pixel_width(), stage.pixel_height()), (96, 96));
    assert_eq!(
        (stage.metatile_prefix_count(), stage.tile_prefix_count()),
        (1, 2)
    );
    let s = stage.sample(3, 2, 0).unwrap().unwrap();
    assert_eq!((s.tile_word, s.tile_index, s.metatile_index), (1, 1, 0));
    assert_eq!((s.vertical_boundary, s.horizontal_boundary), (2, 3));
    assert_eq!((s.vertical_angle, s.horizontal_angle), (32, 32));
    assert!(s.solid_vertical && s.solid_horizontal);
    let s = stage.sample(2, 1, 1).unwrap().unwrap();
    assert!(!s.solid_vertical && !s.solid_horizontal);
    assert!(!stage.sample(8, 8, 1).unwrap().unwrap().solid_vertical);
    for (x, y) in [(-1, 0), (0, -1), (96, 0), (0, 96), (i32::MAX, i32::MIN)] {
        assert_eq!(stage.sample(x, y, 0).unwrap(), None);
    }
    for layer in [2, 3, 0x40, 0xff] {
        assert!(stage.sample(0, 0, layer).is_err());
    }
}

#[test]
fn every_nibble_and_flip_matches_reflected_pixel_coordinates() {
    for nibble in 0_u8..16 {
        // Derive an unflipped pixel set directly from the encoded boundary,
        // then reflect coordinates. This does not repeat the reader's signed
        // boundary transformation.
        let raw_solid = |position: usize| match nibble {
            0 => false,
            1..=7 => position >= usize::from(nibble),
            8 => true,
            9..=15 => position < usize::from(16 - nibble),
            _ => unreachable!(),
        };
        for flip in [0, 0x400, 0x800, 0xc00] {
            let rom = fixture(nibble << 4 | nibble, flip);
            let stage = CollisionStage::parse(&rom, 0).unwrap();
            for y in 0..8 {
                for x in 0..8 {
                    let s = stage.sample(x as i32, y as i32, 0).unwrap().unwrap();
                    let source_y = if flip & 0x800 != 0 { 7 - y } else { y };
                    let source_x = if flip & 0x400 != 0 { 7 - x } else { x };
                    assert_eq!(
                        s.solid_vertical,
                        raw_solid(source_y),
                        "{nibble} {flip} ({x},{y})"
                    );
                    assert_eq!(
                        s.solid_horizontal,
                        raw_solid(source_x),
                        "{nibble} {flip} ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn profile_axis_selection_angles_and_flag_suppression_remain_distinct() {
    let mut rom = fixture(0, 0xc00);
    for (column, byte) in rom[72..80].iter_mut().enumerate() {
        *byte = ((column + 1) << 4 | (8 - column)) as u8;
    }
    // Both native flag bits are retained. Suppression uses only bit zero.
    half(&mut rom, 386, 3 << 2);
    let stage = CollisionStage::parse(&rom, 0).unwrap();
    let s = stage.sample(2, 5, 0).unwrap().unwrap();
    assert_eq!((s.vertical_boundary, s.horizontal_boundary), (-2, -2));
    assert_eq!((s.vertical_angle, s.horizontal_angle), (160, 160));
    assert_eq!(s.flags, 3);
    let suppressed = stage.sample(2, 5, 0x80).unwrap().unwrap();
    assert_eq!(
        (suppressed.vertical_boundary, suppressed.horizontal_boundary),
        (0, 0)
    );
    assert!(!suppressed.solid_vertical && !suppressed.solid_horizontal);
    assert_eq!(
        (suppressed.vertical_angle, suppressed.horizontal_angle),
        (224, 96)
    );
    half(&mut rom, 386, 2 << 2);
    assert_eq!(
        CollisionStage::parse(&rom, 0)
            .unwrap()
            .sample(2, 5, 0x80)
            .unwrap()
            .unwrap()
            .vertical_boundary,
        -2
    );
}

#[test]
fn pixel_probes_find_surfaces_in_each_direction_and_respect_budgets() {
    let rom = fixture(0x23, 0);
    let stage = CollisionStage::parse(&rom, 0).unwrap();
    assert_eq!(
        stage.floor(4, 0, 1, 2).unwrap(),
        Some(ProbeHit {
            x: 4,
            y: 2,
            distance: 2,
            angle: 32,
            flags: 0
        })
    );
    assert_eq!(stage.floor(4, 0, 1, 1).unwrap(), None);
    assert_eq!(stage.wall_right(0, 4, 0, 3).unwrap().unwrap().x, 3);
    assert_eq!(stage.floor(4, 2, 0, 0).unwrap().unwrap().distance, 0);
    assert!(stage.floor(0, 0, 0, MAX_PROBE_DISTANCE + 1).is_err());
    assert_eq!(stage.floor(i32::MAX, i32::MAX, 0, 3).unwrap(), None);
    let rom = fixture(0xdc, 0);
    let stage = CollisionStage::parse(&rom, 0).unwrap();
    assert_eq!(stage.ceiling(4, 7, 0, 7).unwrap().unwrap().y, 2);
    assert_eq!(stage.wall_left(7, 4, 0, 7).unwrap().unwrap().x, 3);
}

#[test]
fn malformed_graphs_and_foreign_sources_are_rejected_before_sampling() {
    let rom = fixture(0x88, 0);
    for length in [0, 31, 71, 79, 81, 383, 384, 385, 387] {
        assert!(
            CollisionStage::parse(&rom[..length], 0).is_err(),
            "length {length}"
        );
    }
    assert!(CollisionStage::parse(&rom, usize::MAX).is_err());
    assert!(CollisionStage::load_asoe(&rom, 0).is_err());
    for (at, value) in [(0, 0), (8, 0x08000191), (20, 0x08000183)] {
        let mut changed = rom.clone();
        word(&mut changed, at, value);
        assert!(CollisionStage::parse(&changed, 0).is_err());
    }
    for at in [24, 26, 28, 30] {
        let mut changed = rom.clone();
        half(&mut changed, at, 0);
        assert!(CollisionStage::parse(&changed, 0).is_err());
    }
    let mut changed = rom.clone();
    half(&mut changed, 24, u16::MAX);
    half(&mut changed, 26, u16::MAX);
    assert!(CollisionStage::parse(&changed, 0).is_err());
    let mut changed = rom.clone();
    changed[384] = 255;
    assert!(CollisionStage::parse(&changed, 0).is_err());
    let mut changed = rom.clone();
    half(&mut changed, 96, 1023);
    assert!(CollisionStage::parse(&changed, 0).is_err());
    let mut changed = rom.clone();
    half(&mut changed, 28, 97);
    assert!(CollisionStage::parse(&changed, 0).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; exact source data stays local"]
fn authentic_zone1_geometry_matches_independent_python_probe_metadata() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let stage = CollisionStage::load_asoe(&rom, 0).unwrap();
    assert_eq!(stage.header_offset(), 0x4bf1f8);
    assert_eq!(
        stage.offsets(),
        [0x4af830, 0x4b16e0, 0x4b1ab8, 0x4be1d8, 0x4be948, 0x4bf0b8]
    );
    assert_eq!(
        (stage.width_metatiles(), stage.height_metatiles()),
        (136, 14)
    );
    assert_eq!((stage.pixel_width(), stage.pixel_height()), (13056, 1344));
    assert_eq!(
        (stage.metatile_prefix_count(), stage.tile_prefix_count()),
        (177, 982)
    );
    for (x, y, budget, expected_y, expected_angle) in [
        (104, 400, 500, 460, 0),
        (104, 0, 1000, 460, 0),
        (104, 444, 200, 460, 0),
        (0, 0, 1000, 460, 0),
        (240, 400, 500, 460, 0),
        (512, 400, 500, 416, 8),
        (800, 400, 500, 425, 242),
        (1200, 400, 500, 436, 0),
    ] {
        let hit = stage.floor(x, y, 1, budget).unwrap().unwrap();
        assert_eq!((hit.x, hit.y, hit.angle), (x, expected_y, expected_angle));
        assert_eq!(i32::from(hit.distance), expected_y - y);
    }
    assert_eq!(stage.floor(104, 444, 0, 500).unwrap(), None);
    assert!(CollisionStage::load_asoe(&rom, COLLISION_STAGE_COUNT).is_err());
    for (offset, length, hash) in [
        (
            0x4bf1f8,
            32,
            "e880e0d1791c8b57e319297b8df40a8f1db6686d519f3d3f7ae93a3191002f80",
        ),
        (
            0x4af830,
            7856,
            "2c13a531cca06371f829e5334ca82d73446b36e01c469b7ae448c1ebb89f0551",
        ),
        (
            0x4b16e0,
            982,
            "3c9b6c2097f83af5298a0d30f702eb979aa2b6dabf75ed7d31ff8c6f6793da33",
        ),
        (
            0x4b1ab8,
            50976,
            "ec205d8e21302c4d7f50ff0dd6e77f7dc30f375126bca2ecc3145c22f35968ee",
        ),
        (
            0x4be1d8,
            1904,
            "ff06829ea0b281e7b059739ca627c64d3f3c711c8b235bfafb07a3eaba4d9d88",
        ),
        (
            0x4be948,
            1904,
            "a7785dae502068ba8c2edcf3de6d96156b05e7cbd040a08b1498f3c2bbff6945",
        ),
        (
            0x4bf0b8,
            246,
            "f9d23e5db890b8bd71cae71c1888132b505435db1005566aeda2562cf0f5a2c2",
        ),
    ] {
        assert_eq!(
            binary::sha256(binary::range(&rom, offset, length).unwrap()),
            hash
        );
    }
}
