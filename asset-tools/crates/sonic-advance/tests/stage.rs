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
    assert!(Stage::load_asoe(&rom, 1).is_err());
}
