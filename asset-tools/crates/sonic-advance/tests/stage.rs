use sonic_advance_assets::stage::*;

#[test]
fn malformed_and_foreign_stage_headers_are_rejected_before_decode() {
    assert!(Stage::load_asoe(&[0; 192], 0).is_err());
    assert!(Layer::parse(&[0; 35], 0).is_err());
    assert!(Layer::parse(&[0; 128], usize::MAX).is_err());
    let mut raw = vec![0; 512];
    raw[0..4].copy_from_slice(&[11, 0, 12, 0]);
    assert!(Layer::parse(&raw, 0).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; native pixels remain private"]
fn source_stage_decodes_and_renders_native_pixels_with_bounded_output() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let stage = Stage::load_asoe(&rom, 0).unwrap();
    assert_eq!(
        (stage.width(), stage.height(), stage.spawn()),
        (13056, 1344, (104, 444))
    );
    let layer = Layer::parse(&rom, 0x4bf1b0).unwrap();
    assert_eq!((layer.grid_width(), layer.grid_height()), (136, 14));
    assert_eq!(layer.pixel(13056, 1344).unwrap(), None);
    assert_eq!(
        stage.camera(i32::MIN, i32::MIN, usize::MAX, usize::MAX),
        (0, 0)
    );
    assert_eq!(
        stage.camera(i32::MAX, i32::MAX, usize::MAX, usize::MAX),
        (0, 0)
    );
    assert_eq!(stage.camera(i32::MAX, i32::MAX, 240, 160), (12816, 1184));
    let mut frame = vec![0; VIEW_WIDTH * VIEW_HEIGHT * 4];
    stage
        .render_into(&mut frame, VIEW_WIDTH, VIEW_HEIGHT, 0, 364)
        .unwrap();
    assert_eq!(
        sonic_advance_assets::binary::sha256(&frame),
        "4961cea784a74d95e3364ddc17175bbd31989dd3e1ca69dfdc922db42a7e2643"
    );
    stage
        .render_into(&mut frame, VIEW_WIDTH, VIEW_HEIGHT, 400, 336)
        .unwrap();
    assert_eq!(
        sonic_advance_assets::binary::sha256(&frame),
        "b093a6582ba219fcf199b3e1954b52f3d2760e6348a1318e307f5cc2a6196333"
    );
    stage
        .render_into(&mut frame, VIEW_WIDTH, VIEW_HEIGHT, 680, 345)
        .unwrap();
    assert_eq!(
        sonic_advance_assets::binary::sha256(&frame),
        "5fc5902cbc8d65887728b2d6cebb5dc87b063d27ccccb7768ec21d4f8c9b5904"
    );
    assert!(frame.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
    assert!(frame
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[0] != pixel[1]));
    assert!(stage
        .render_into(&mut frame, usize::MAX, 160, 0, 0)
        .is_err());
    assert!(stage.render_into(&mut frame, 240, 160, -1, 0).is_err());
    assert!(stage
        .render_into(&mut frame[..100], 240, 160, 0, 0)
        .is_err());
    assert!(Stage::load_asoe(&rom, 2).is_err());
}

fn synthetic_layer() -> Vec<u8> {
    let mut raw = vec![0_u8; 481];
    for (at, value) in [(0, 12), (2, 12), (22, 16), (32, 1), (34, 1)] {
        raw[at..at + 2].copy_from_slice(&u16::to_le_bytes(value));
    }
    for (at, value) in [
        (8, 0x0800_0040_u32),
        (12, 32),
        (16, 0x0800_00a0),
        (24, 0x0800_00c0),
        (28, 0x0800_01e0),
    ] {
        raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    raw[64..96].fill(0x11);
    raw[96..128].fill(0x22);
    raw[162..164].copy_from_slice(&31_u16.to_le_bytes());
    raw
}

#[test]
fn foreground_animation_and_references_are_bounded_without_source_assets() {
    let raw = synthetic_layer();
    let layer = Layer::parse(&raw, 0).unwrap();
    assert_eq!((layer.grid_width(), layer.grid_height()), (1, 1));
    assert_eq!(layer.pixel(0, 0).unwrap(), Some([255, 0, 0, 255]));
    assert_eq!(layer.pixel(-1, 0).unwrap(), None);
    assert_eq!(layer.pixel(96, 0).unwrap(), None);
    assert!(layer.animation().is_none());
    for length in 0..raw.len() {
        assert!(Layer::parse(&raw[..length], 0).is_err(), "length={length}");
    }
    let mut animated = raw.clone();
    animated[4..6].copy_from_slice(&32_u16.to_le_bytes());
    // An inactive animation with nonzero tile size is inconsistent.
    assert!(Layer::parse(&animated, 0).is_err());
    animated[6] = 2;
    assert!(Layer::parse(&animated, 0).is_err()); // zero delay
    animated[7] = 1;
    let layer = Layer::parse(&animated, 0).unwrap();
    assert_eq!(layer.animation().unwrap().prefix_at(1), &[2; 64]);
    for invalid_word in [1_u16, 0x1000] {
        let mut invalid = raw.clone();
        invalid[192..194].copy_from_slice(&invalid_word.to_le_bytes());
        assert!(Layer::parse(&invalid, 0).is_err());
    }
    let mut invalid = raw;
    invalid[32..36].fill(255);
    assert!(Layer::parse(&invalid, 0).is_err());
}

#[test]
fn terrain_upload_uses_base_prefix_then_appended_phases_and_wraps() {
    let mut base = vec![0x11; 64];
    base[32..].fill(0x77); // The rest of the character bank is not uploaded.
    let mut appended = vec![0x22; 96];
    appended[32..64].fill(0x33);
    appended[64..].fill(0x44);
    let animation = TileAnimation::parse(&base, &appended, 32, 4, 10, 256).unwrap();
    assert_eq!(
        (
            animation.tile_bytes(),
            animation.frame_count(),
            animation.delay()
        ),
        (32, 4, 10)
    );
    for (updates, phase) in [
        (0, 0),
        (1, 0),
        (9, 0),
        (10, 1),
        (11, 1),
        (19, 1),
        (20, 2),
        (29, 2),
        (30, 3),
        (39, 3),
        (40, 0),
        (41, 0),
        (400, 0),
        (u64::MAX, 1),
    ] {
        assert_eq!(animation.phase_at(updates), phase, "updates={updates}");
        assert_eq!(animation.prefix_at(updates), &[phase as u8 + 1; 64]);
    }
    assert!(TileAnimation::parse(&base, &appended, 32, 4, 10, 255).is_err());
    assert!(TileAnimation::parse(&base, &appended[..95], 32, 4, 10, 256).is_err());
    assert!(TileAnimation::parse(&base, &[0; 97], 32, 4, 10, 256).is_err());
    for (bytes, frames, delay) in [
        (0, 4, 10),
        (31, 4, 10),
        (96, 4, 10),
        (32, 0, 10),
        (32, 33, 10),
        (32, 4, 0),
    ] {
        assert!(TileAnimation::parse(&base, &appended, bytes, frames, delay, 256).is_err());
    }
    assert!(TileAnimation::parse(&base[..63], &appended, 32, 4, 10, 256).is_err());
    assert!(TileAnimation::parse(&[0; 32], &[], 32, 1, 1, 64).is_ok());
    // The caller cannot bypass the decoder's absolute decoded-pixel ceiling.
    let huge_base = vec![0; MAX_FRAME_PIXELS];
    assert!(TileAnimation::parse(&huge_base, &[], huge_base.len(), 1, 1, usize::MAX).is_err());
}

#[test]
fn visible_scanline_offsets_match_original_c_routine_vectors() {
    // Original StageBgUpdate_Zone1Acts12 compiled with register/queue stubs;
    // low bytes are the exact sampling offsets for the 256-pixel background.
    let cases = [
        (
            0_u64,
            0,
            "b393978842a0fa3d3e1470196f098f473f9678e72463cb65ec4ab5581856c2e4",
        ),
        (
            16_u64,
            400,
            "6724ecf23dcb1aecda1cc31e6f9519c08728e4950d705d7c83f3ed8255caf5fe",
        ),
        (
            32_u64,
            0,
            "ffb57925d24fbd61112481341c2114eb16eeaf547fd72259e5643ab5cc42ee01",
        ),
        (
            64_u64,
            13056,
            "e5d052ec853bee548a62abe49b62b5af514ffe2bb940a1b5c68d772c02e1b790",
        ),
        (
            4095_u64,
            400,
            "bb0165b645fdc1fdc9ac35afd7e9dfc8680d9a644954d121a1617acaaf35e692",
        ),
        (
            4294967295_u64,
            12816,
            "f75f5d7bd4aad4566ae6427f6a5becb1f09ef3b69675935ae8e5339937accdeb",
        ),
    ];
    for (tick, camera_x, expected) in cases {
        let offsets = (0..VIEW_HEIGHT)
            .map(|y| zone_one_background_scroll(tick, y, camera_x).unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(sonic_advance_assets::binary::sha256(&offsets), expected);
    }
    assert!(zone_one_background_scroll(0, 160, 0).is_err());
    assert!(zone_one_background_scroll(0, usize::MAX, 0).is_err());
    assert!(zone_one_background_scroll(0, 0, -1).is_err());
    assert_eq!(
        zone_one_background_scroll(u64::MAX, 159, i32::MAX).unwrap(),
        255
    );
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; source and output pixels remain private"]
fn native_timed_frames_match_original_c_upload_and_scanline_oracle() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let mut frame = vec![0; VIEW_WIDTH * VIEW_HEIGHT * 4];
    let stage_zero = Stage::load_asoe(&rom, 0).unwrap();
    assert_eq!(stage_zero.index(), 0);
    let stage_one = Stage::load_asoe(&rom, 1).unwrap();
    assert_eq!(
        (
            stage_one.index(),
            stage_one.width(),
            stage_one.height(),
            stage_one.spawn()
        ),
        (1, 11520, 1440, (104, 684))
    );
    assert_eq!(
        stage_one.camera(i32::MAX, i32::MAX, 240, 160),
        (11280, 1280)
    );
    for (stage, front, back, grid) in [
        (&stage_zero, 0x4bf1b0, 0x4bf1d4, (136, 14)),
        (&stage_one, 0x4d6468, 0x4d648c, (120, 15)),
    ] {
        let layer = Layer::parse(&rom, front).unwrap();
        assert_eq!((layer.grid_width(), layer.grid_height()), grid);
        let animation = layer.animation().unwrap();
        assert_eq!(
            (
                animation.tile_bytes(),
                animation.frame_count(),
                animation.delay()
            ),
            (448, 4, 10)
        );
        assert!(Layer::parse(&rom, back).unwrap().animation().is_none());
        assert!(stage.render_at(&mut frame, 160, 240, 0, 0, 0).is_err());
        assert!(stage
            .render_at(&mut frame[..100], 240, 160, 0, 0, 0)
            .is_err());
        assert!(stage
            .render_at(&mut frame, 240, 160, stage.width() + 1, 0, 0)
            .is_err());
        assert!(stage.render_at(&mut frame, 240, 160, 0, -1, 0).is_err());
    }
    let cases = [
        (
            0,
            0,
            364,
            0,
            "4961cea784a74d95e3364ddc17175bbd31989dd3e1ca69dfdc922db42a7e2643",
        ),
        (
            0,
            0,
            364,
            16,
            "2a2e4cab849ba6587c5a092a9d9187a7155bd67189c61af306c005eeb7e10596",
        ),
        (
            0,
            400,
            336,
            0,
            "7b2a6a7daee426ecab1e65f0ff88b11447c68d114df0c13e79357ad59ba20a43",
        ),
        (
            0,
            400,
            336,
            10,
            "7b2a6a7daee426ecab1e65f0ff88b11447c68d114df0c13e79357ad59ba20a43",
        ),
        (
            0,
            2800,
            176,
            0,
            "9be26a0872743ad8fa67884c6b956c1822817f93ef771a2b9ace802b36f4668d",
        ),
        (
            0,
            2800,
            176,
            9,
            "9be26a0872743ad8fa67884c6b956c1822817f93ef771a2b9ace802b36f4668d",
        ),
        (
            0,
            2800,
            176,
            10,
            "0b8d17f0da881d8ee4aaa3b82628d600942396666dcdd0aaa7ca318c3fe09b1b",
        ),
        (
            0,
            2800,
            176,
            20,
            "6b1a49d64ccf31a92a76277fa6dfb7be2bb96bb0cc69ae8f6db757a613979d3c",
        ),
        (
            0,
            2800,
            176,
            30,
            "32b2dd0769ded587edfa57fd12d8ef59eadb6c77014153679705c168ee853f7b",
        ),
        (
            0,
            2800,
            176,
            40,
            "8744b6b1441b26ac37e70e0897c2fe9fb8a4e21790c8788439783b26fbcb616d",
        ),
        (
            0,
            400,
            0,
            64,
            "02524d6cfddd43c68c0bd743fba659db257626d64cfb831f4098aa1b5044172c",
        ),
        (
            0,
            12816,
            1184,
            123,
            "90d5d78caa16f01defb50ee164ddacecd91c99f582de7fccebb4f927cb695d52",
        ),
        (
            1,
            0,
            604,
            0,
            "7bbf2b83ba5ad7f04bc0d8d5778a6578edc2a39f591278e216cea9a3dbdf86b9",
        ),
        (
            1,
            0,
            604,
            16,
            "f9e9d4085bad2741a7b17c6d943c1a579d75276e982e186f5a87cf97555e60c7",
        ),
        (
            1,
            400,
            400,
            10,
            "815744891a32abdaa1ab7ac7aed92c911c37e7954e8a185b859c4d237af0f510",
        ),
        (
            1,
            2800,
            176,
            0,
            "cd11c062a7a1472c0bbbb86ce01fc9fc9676e8d3f7f9945340a771b936856d8e",
        ),
        (
            1,
            2800,
            176,
            10,
            "cd11c062a7a1472c0bbbb86ce01fc9fc9676e8d3f7f9945340a771b936856d8e",
        ),
        (
            1,
            2800,
            176,
            20,
            "6146228bf13cd271c1f0a19025845e99d131a2df1c393f54735ea13b6bb7032b",
        ),
        (
            1,
            2800,
            176,
            30,
            "6146228bf13cd271c1f0a19025845e99d131a2df1c393f54735ea13b6bb7032b",
        ),
        (
            1,
            2800,
            176,
            40,
            "7e31e6e5bef9a9d4c01e5f1c09799be39d34c4f4f4c6e94fd8d9b3fcfa8edd1b",
        ),
        (
            1,
            11280,
            1280,
            123,
            "e03b8bcbf8bc009c58a6b78045f5c22de00df1d9b9a3ebb57ef34ed2c321b68b",
        ),
        (
            1,
            4200,
            96,
            0,
            "8f289485adc86858e987178c45c2b68eeab4464ba51cf8e53240d58abe09f439",
        ),
        (
            1,
            4200,
            96,
            9,
            "8f289485adc86858e987178c45c2b68eeab4464ba51cf8e53240d58abe09f439",
        ),
        (
            1,
            4200,
            96,
            10,
            "7b07ba73bf4fdfb7a6a18443c03633952974f749904ce149b5888a8fb763521a",
        ),
        (
            1,
            4200,
            96,
            20,
            "e9b1a2fa31f3443c18c27790a335f383a90295e4c481695de33d8ea00df54c36",
        ),
        (
            1,
            4200,
            96,
            30,
            "96c5e534c753a3423fc6f1d30b564878973354166efeef9000adb07f017191fd",
        ),
        (
            1,
            4200,
            96,
            40,
            "363336bce4f1299f179a81d37bd5317dadbdadf488eb37625825429f1a738bdc",
        ),
    ];
    for (index, x, y, tick, expected) in cases {
        let stage = if index == 0 { &stage_zero } else { &stage_one };
        stage.render_at(&mut frame, 240, 160, x, y, tick).unwrap();
        assert_eq!(
            sonic_advance_assets::binary::sha256(&frame),
            expected,
            "stage={index} camera=({x},{y}) updates={tick}"
        );
    }
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; no native bytes are embedded"]
fn source_animation_spans_match_exact_verified_banks() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom =
        sonic_advance_assets::binary::read_bounded(std::path::Path::new(&path), 8388608).unwrap();
    let cases = [
        (
            0x4a7670,
            448,
            "510e73c253496bd5b4e96251469f2eb94e93a600b6fdd17c40354476bee6f807",
        ),
        (
            0x4af130,
            448,
            "6ef75a5144126566f10db064a834270acd671db62889bd9142b918114d44a4c1",
        ),
        (
            0x4af2f0,
            448,
            "f0d2dc26d8f1904b33796bcd85d3f6dc1a16675e2608645bc97ddb642f5632e6",
        ),
        (
            0x4af4b0,
            448,
            "3cb4c236831ff8efd8680e46a7fc53e664e05998ae92321bb65b967316d8b61d",
        ),
        (
            0x4bf418,
            448,
            "226f3006b1835a5a168f1b7ba31fbc53fdc3b1646a9ec50baaf5c379ffc96da0",
        ),
        (
            0x4c6ed8,
            448,
            "6c6bd1f6e93883c2a4a1599095f81d588194e53c0c3a39d970c0df0c006680e6",
        ),
        (
            0x4c7098,
            448,
            "5ce4ed73719bc1f1ec250bbf277e9e3e1d939b9df503c8d23a6839d9bf201807",
        ),
        (
            0x4c7258,
            448,
            "db56110ed12f730f84ea6cf9e6e9cf97fef9d486a11cfb75380de5c99ff7256b",
        ),
    ];
    for (offset, length, expected) in cases {
        assert_eq!(
            sonic_advance_assets::binary::sha256(
                sonic_advance_assets::binary::range(&rom, offset, length).unwrap()
            ),
            expected
        );
    }
}
