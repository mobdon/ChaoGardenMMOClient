use sonic_advance_assets::{binary, graphics, sprite::*};

fn dimensions(flip: u8, oam: u8, count: u16, width: u16, height: u16, x: i16, y: i16) -> [u8; 12] {
    let mut record = [0; 12];
    record[0] = flip;
    record[1] = oam;
    for (at, value) in [
        (2, count),
        (4, width),
        (6, height),
        (8, x as u16),
        (10, y as u16),
    ] {
        record[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    record
}

fn oam(a0: u16, a1: u16, a2: u16) -> [u8; 6] {
    let mut bytes = [0; 6];
    for (at, value) in [(0, a0), (2, a1), (4, a2)] {
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn palette() -> Vec<[u8; 4]> {
    (0_u8..32).map(|v| [v, v.wrapping_mul(3), 0, 255]).collect()
}

#[test]
fn native_layout_uses_six_byte_objects_and_twelve_byte_dimensions() {
    let d = dimensions(0, 1, 1, 16, 8, -3, 5);
    let mut data = vec![0xff; 6];
    data.extend_from_slice(&oam(0x4000, 0, 0x1000));
    let frame = NativeFrame::parse(&d, &data, 1).unwrap();
    assert_eq!(frame.dimensions().offset_x, -3);
    assert_eq!(frame.dimensions().offset_y, 5);
    let object = frame.objects().next().unwrap();
    assert_eq!(
        (object.width, object.height, object.palette_bank),
        (16, 8, 1)
    );
    assert_eq!(object.raw, [0x4000, 0, 0x1000]);
    assert!(NativeFrame::parse(&d, &data[..11], 1).is_err());
    assert!(NativeFrame::parse(&d, &data, 0).is_err());
    assert!(FrameDimensions::parse(&d[..11]).is_err());
    assert!(FrameDimensions::parse(&[0; 12]).is_err());
}

#[test]
fn native_object_tiles_banks_flips_and_overlap_have_exact_pixels() {
    let mut tiles = [0; 64];
    tiles[0] = 0x21; // left two pixels in first tile
    tiles[3] = 0x40; // rightmost pixel in first row
    tiles[28] = 0x03; // leftmost pixel in last row
    tiles[32] = 0x65; // tile one, source x8
    tiles[33] = 0x07; // visible behind a transparent pixel of object zero
    let colors = palette();
    let d = dimensions(0, 0, 1, 16, 8, 3, 5);
    let data = oam(0x4000, 0, 0x1000);
    let native = NativeFrame::parse(&d, &data, 1).unwrap();
    let frame = render_frame(&native, &tiles, &colors, 128).unwrap();
    assert_eq!((frame.offset_x, frame.offset_y), (3, 5));
    assert_eq!(frame.image.pixels[0], colors[17]);
    assert_eq!(frame.image.pixels[1], colors[18]);
    assert_eq!(frame.image.pixels[7], colors[20]);
    assert_eq!(frame.image.pixels[8], colors[21]);
    assert_eq!(frame.image.pixels[7 * 16], colors[19]);
    assert_eq!(frame.image.pixels[2], [0; 4]);
    let flipped_data = oam(0x4000, 0x3000, 0x1000);
    let flipped = render_frame(
        &NativeFrame::parse(&d, &flipped_data, 1).unwrap(),
        &tiles,
        &colors,
        128,
    )
    .unwrap();
    assert_eq!(flipped.image.pixels[7 * 16 + 15], colors[17]);
    assert_eq!(flipped.image.pixels[15], colors[19]);
    let internal = dimensions(3, 0, 1, 16, 8, 3, 5);
    let reflected = render_frame(
        &NativeFrame::parse(&internal, &data, 1).unwrap(),
        &tiles,
        &colors,
        128,
    )
    .unwrap();
    assert_eq!(reflected.image, flipped.image);
    let mut overlap = oam(0, 0, 0).to_vec();
    overlap.extend_from_slice(&oam(0, 0, 1));
    let overlapping = dimensions(0, 0, 2, 8, 8, 0, 0);
    let image = render_frame(
        &NativeFrame::parse(&overlapping, &overlap, 2).unwrap(),
        &tiles,
        &colors,
        64,
    )
    .unwrap();
    // Lower-index object wins; transparent pixels retain the object behind it.
    assert_eq!(image.image.pixels[0], colors[1]);
    assert_eq!(image.image.pixels[1], colors[2]);
    assert_eq!(image.image.pixels[2], colors[7]);
}

#[test]
fn unsupported_oam_modes_and_bad_references_are_errors() {
    let d = dimensions(0, 0, 1, 8, 8, 0, 0);
    for flag in [0x0100, 0x0200, 0x0400, 0x0800, 0x1000, 0x2000, 0xc000] {
        assert!(
            NativeFrame::parse(&d, &oam(flag, 0, 0), 1).is_err(),
            "{flag:#x}"
        );
    }
    assert!(NativeFrame::parse(&d, &oam(0, 1, 0), 1).is_err());
    assert!(NativeFrame::parse(&dimensions(4, 0, 1, 8, 8, 0, 0), &oam(0, 0, 0), 1).is_err());
    let data = oam(0, 0, 0);
    let frame = NativeFrame::parse(&d, &data, 1).unwrap();
    assert!(render_frame(&frame, &[0; 31], &palette(), 64).is_err());
    assert!(render_frame(&frame, &[0; 32], &palette(), 63).is_err());
    assert!(render_frame(&frame, &[0; 32], &palette()[..15], 64).is_err());
    let missing = oam(0, 0, 1);
    let frame = NativeFrame::parse(&d, &missing, 1).unwrap();
    assert!(render_frame(&frame, &[0; 32], &palette(), 64).is_err());
}

#[test]
fn identity_affine_is_explicit_and_does_not_reinterpret_matrix_bits_as_flips() {
    let d = dimensions(0, 0, 1, 8, 8, 0, 0);
    let affine = oam(0x100, 0x3000, 0);
    let ordinary = oam(0, 0, 0);
    assert!(NativeFrame::parse(&d, &affine, 1).is_err());
    let frame = NativeFrame::parse_identity_affine(&d, &affine, 1).unwrap();
    assert!(frame.objects().next().unwrap().affine);
    let mut tiles = [0; 32];
    tiles[0] = 0x21;
    tiles[31] = 0x30;
    let colors = palette();
    let image = render_frame(&frame, &tiles, &colors, 64).unwrap();
    let expected = render_frame(
        &NativeFrame::parse(&d, &ordinary, 1).unwrap(),
        &tiles,
        &colors,
        64,
    )
    .unwrap();
    assert_eq!(image, expected);
    for flags in [0x200, 0x300] {
        assert!(NativeFrame::parse_identity_affine(&d, &oam(flags, 0, 0), 1).is_err());
    }
    assert!(
        NativeFrame::parse_identity_affine(&dimensions(1, 0, 1, 8, 8, 0, 0), &affine, 1).is_err()
    );
}

#[test]
fn viewport_anchor_reflection_clipping_and_bad_lengths_are_checked() {
    let frame = RenderedFrame {
        image: graphics::RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![[1, 2, 3, 255], [4, 5, 6, 255]],
        },
        offset_x: 0,
        offset_y: 0,
    };
    let mut target = vec![0; 3 * 4];
    composite_rgba(&mut target, 3, 1, &frame, 1, 0, false).unwrap();
    assert_eq!(target, [0, 0, 0, 0, 1, 2, 3, 255, 4, 5, 6, 255]);
    target.fill(0);
    composite_rgba(&mut target, 3, 1, &frame, 2, 0, true).unwrap();
    assert_eq!(target, [4, 5, 6, 255, 1, 2, 3, 255, 0, 0, 0, 0]);
    target.fill(0);
    composite_rgba(&mut target, 3, 1, &frame, -1, 0, false).unwrap();
    assert_eq!(&target[..4], &[4, 5, 6, 255]);
    target.fill(0);
    composite_rgba(&mut target, 3, 1, &frame, i32::MAX, i32::MIN, true).unwrap();
    assert_eq!(target, vec![0; 12]);
    assert!(composite_rgba(&mut target, usize::MAX, 2, &frame, 0, 0, false).is_err());
    assert!(composite_rgba(&mut target, 2, 1, &frame, 0, 0, false).is_err());
    let bad = RenderedFrame {
        image: graphics::RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![],
        },
        ..frame.clone()
    };
    assert!(composite_rgba(&mut target, 3, 1, &bad, 0, 0, false).is_err());
    let bad = RenderedFrame {
        image: graphics::RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![[0, 0, 0, 128]],
        },
        ..frame
    };
    assert!(composite_rgba(&mut target, 3, 1, &bad, 0, 0, false).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; native pixels stay local"]
fn authentic_sonic_first_frame_matches_independent_python_oracle() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let clip = load_asoe_animation(&rom, 0, 0, 128, MAX_CLIP_PIXELS).unwrap();
    assert_eq!(clip.loop_start, Some(0));
    assert_eq!(clip.frames.len(), 35);
    let all_rgba: Vec<u8> = clip
        .frames
        .iter()
        .flat_map(|frame| frame.sprite.image.pixels.iter().flatten().copied())
        .collect();
    assert_eq!(all_rgba.len(), 41472 * 4);
    assert_eq!(
        binary::sha256(&all_rgba),
        "1170dfdf3b520f299fea92ff4da35b7bee465017d8e2b007f11fcd5c8b39f223"
    );
    let first = &clip.frames[0];
    assert_eq!((first.duration, first.frame_index), (6, 0));
    assert_eq!(
        (first.sprite.image.width, first.sprite.image.height),
        (32, 40)
    );
    assert_eq!((first.sprite.offset_x, first.sprite.offset_y), (16, 24));
    let bytes: Vec<u8> = first
        .sprite
        .image
        .pixels
        .iter()
        .flatten()
        .copied()
        .collect();
    assert_eq!(
        binary::sha256(&bytes),
        "90318f22d1954f5bd3e6fe97334c93c456a6d7a9aeeb8462712ebe5c876fef2f"
    );
    assert_eq!(
        first
            .sprite
            .image
            .pixels
            .iter()
            .filter(|pixel| pixel[3] != 0)
            .count(),
        508
    );
    assert!(load_asoe_animation(&rom, 0, 1, 128, MAX_CLIP_PIXELS).is_err());
    assert!(load_asoe_animation(&rom, 906, 0, 128, MAX_CLIP_PIXELS).is_err());
    assert!(load_asoe_animation(&rom, 0, 0, 0, MAX_CLIP_PIXELS).is_err());
    assert!(load_asoe_animation(&rom, 0, 0, 128, 1279).is_err());
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; native pixels stay local"]
fn authentic_walk_run_spin_and_ring_clips_use_verified_native_ids_and_durations() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    // Complete scripts were independently assembled from the pinned source
    // and compared byte-for-byte at the supplied ASOE pointers. Sonic state4
    // maps to animation4, state5 (native spin-jump) maps to8; the native ring
    // manager selects707. Run5 also has an explicit native speed override.
    for (id, frames, duration, rgba_hash) in [
        (
            4,
            8,
            3,
            "1cd1cbe3f757bed4e27416b307804520755f31ae59bc0d84ad4e8e184a0225f5",
        ),
        (
            5,
            4,
            2,
            "a4a5a677ba463d4e67be3b299b0e081bdd763d5ef9625019a55de515395e3fce",
        ),
        (
            8,
            4,
            2,
            "507621b81d395981a57dbabf5fab0b59e59ec68c29ff078db192825157a12611",
        ),
        (
            707,
            4,
            8,
            "c3def761cf3f2a9146e2f3eeafc670e618a860eab15a4bbd45c09ac30f7afa5e",
        ),
    ] {
        let clip = if matches!(id, 4 | 5) {
            assert!(load_asoe_animation(&rom, id, 0, 128, MAX_CLIP_PIXELS).is_err());
            load_asoe_animation_identity(&rom, id, 0, 128, MAX_CLIP_PIXELS)
        } else {
            load_asoe_animation(&rom, id, 0, 128, MAX_CLIP_PIXELS)
        }
        .unwrap();
        assert_eq!(clip.frames.len(), frames, "animation{id}");
        assert_eq!(clip.loop_start, Some(0), "animation{id}");
        assert!(clip.frames.iter().all(|frame| frame.duration == duration));
        assert!(clip.frames.iter().all(|frame| frame
            .sprite
            .image
            .pixels
            .iter()
            .any(|pixel| pixel[3] != 0)));
        let bytes: Vec<u8> = clip
            .frames
            .iter()
            .flat_map(|frame| frame.sprite.image.pixels.iter().flatten().copied())
            .collect();
        assert_eq!(binary::sha256(&bytes), rgba_hash, "animation{id}");
    }
}

#[test]
#[ignore = "requires local ASOE; independent tile/OAM Python oracle RGBA digests"]
fn action_and_effect_clip_pixels_match_native_resource_oracle() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    for (id,variant,count,duration,rgba) in [
        (3,0,4,1,"f39a08c0e305059d2970eb3578c0c6c1739466734eb09258c3adf44f777e0f00"),
        (18,0,4,2,"98737ad22516ff83a4fb04e3de117837215c892ccad11be2c2e4505f8b1ef264"),
        (18,1,4,2,"f17070e4f7dffc7837882056f20ed867bf850a6ead0d0be55c3b1a048d0c989c"),
        (708,0,4,8,"84f8813a6334976e33e62f230d2f255bf619a9ff049c441f966cf94300a5b34e"),
        (746,0,8,2,"82e6918a11f4fc61575f296b6e9a9203cd49ef34fde481a91c7da4c4ac36bd1c"),
        (747,0,4,2,"57f0bd77a1070b95de8809dbf803def788f1d48e97c4555efc5bc03fe79c4146"),
    ] {
        let clip=load_asoe_animation(&rom,id,variant,8,32768).unwrap();
        assert_eq!(clip.frames.len(),count);
        assert!(clip.frames.iter().all(|f| f.duration==duration));
        assert_eq!(clip.loop_start.is_none(), (id,variant)==(18,1)||id==708);
        let bytes:Vec<_>=clip.frames.iter().flat_map(|f| f.sprite.image.pixels.iter().flatten().copied()).collect();
        assert_eq!(binary::sha256(&bytes),rgba,"animation {id}/{variant}");
    }
    for (id,var) in [(18,2),(3,1),(708,1),(usize::MAX,usize::MAX)] {
        assert!(load_asoe_animation(&rom,id,var,8,32768).is_err());
    }
}
