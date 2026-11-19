use sonic_advance_assets::{binary, hud::*};

fn fixture() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut normal = vec![0; DIGIT_BANK_BYTES];
    for glyph in 0..11 {
        normal[glyph * 64..glyph * 64 + 32].fill(0x11);
        normal[glyph * 64 + 32..glyph * 64 + 64].fill(0x22);
    }
    let red = vec![0x22; DIGIT_BANK_BYTES];
    let ring = vec![0x33; RING_ICON_BYTES];
    let mut palette = vec![0; 32];
    for (index, value) in [(1_usize, 31_u16), (2, 31 << 5), (3, 31 << 10)] {
        palette[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    (normal, red, ring, palette)
}

fn pixel(frame: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    &frame[(y * width + x) * 4..(y * width + x + 1) * 4]
}

#[test]
fn decimal_layout_uses_native_positions_and_saturates_at_three_digits() {
    for (rings, digits, count, x) in [
        (0, [0, 0, 0], 1, 9),
        (9, [0, 0, 9], 1, 9),
        (10, [0, 1, 0], 2, 5),
        (99, [0, 9, 9], 2, 5),
        (100, [1, 0, 0], 3, 1),
        (291, [2, 9, 1], 3, 1),
        (999, [9, 9, 9], 3, 1),
        (1000, [9, 9, 9], 3, 1),
        (u32::MAX, [9, 9, 9], 3, 1),
    ] {
        let layout = counter_layout(rings, 0).unwrap();
        assert_eq!(layout.digits, digits);
        assert_eq!(
            (layout.digit_count, layout.origin_x, layout.origin_y),
            (count, x, 5)
        );
        assert!(!layout.warning_red);
    }
    assert!(counter_layout(0, 32).is_err());
}

#[test]
fn zero_warning_uses_eight_normal_sixteen_red_eight_normal_ticks() {
    let mut phase = 0;
    for tick in 0..64 {
        assert_eq!(
            counter_layout(0, phase).unwrap().warning_red,
            (8..24).contains(&(tick % 32))
        );
        assert!(!counter_layout(1, phase).unwrap().warning_red);
        phase = advance_zero_phase(phase, 0, 1).unwrap();
    }
    assert_eq!(phase, 0);
    assert_eq!(advance_zero_phase(7, 1, 100).unwrap(), 7);
    assert_eq!(advance_zero_phase(7, 0, u32::MAX).unwrap(), 6);
    assert!(advance_zero_phase(32, 0, 1).is_err());
}

#[test]
fn native_screen_placement_tile_order_clipping_and_transparency_are_explicit() {
    let (normal, red, ring, palette) = fixture();
    let hud = Hud::decode(&normal, &red, &ring, &palette).unwrap();
    let mut frame = vec![17; 240 * 160 * 4];
    hud.render_into(&mut frame, 240, 160, 0, 0).unwrap();
    assert_eq!(pixel(&frame, 240, 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&frame, 240, 9, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 240, 9, 13), [0, 255, 0, 255]);
    assert_eq!(pixel(&frame, 240, 30, 31), [17; 4]);
    assert_eq!(pixel(&frame, 240, 239, 159), [17; 4]);
    let normal_frame = frame.clone();
    hud.render_into(&mut frame, 240, 160, 0, 8).unwrap();
    assert_eq!(pixel(&frame, 240, 9, 5), [0, 255, 0, 255]);
    assert_ne!(frame, normal_frame);
    hud.render_into(&mut frame, 240, 160, 0, 24).unwrap();
    assert_eq!(frame, normal_frame);

    let mut icon = ring;
    icon[..32].fill(0);
    icon[32..64].fill(0x22);
    icon[64..96].fill(0x33);
    let hud = Hud::decode(&normal, &red, &icon, &palette).unwrap();
    let mut frame = vec![17; 240 * 160 * 4];
    hud.render_into(&mut frame, 240, 160, 42, 0).unwrap();
    // At screen x0/x6/x14, the clipped icon samples source x2/x8/x16.
    assert_eq!(pixel(&frame, 240, 0, 0), [17; 4]);
    assert_eq!(pixel(&frame, 240, 6, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&frame, 240, 14, 0), [0, 0, 255, 255]);
    let mut one_pixel = [17; 4];
    hud.render_into(&mut one_pixel, 1, 1, 999, 0).unwrap();
    assert_eq!(one_pixel, [17; 4]);
}

#[test]
fn source_bank_sizes_and_output_budgets_reject_without_partial_writes() {
    let (normal, red, ring, palette) = fixture();
    for n in [0, DIGIT_BANK_BYTES - 1] {
        assert!(Hud::decode(&normal[..n], &red, &ring, &palette).is_err());
        assert!(Hud::decode(&normal, &red[..n], &ring, &palette).is_err());
    }
    assert!(Hud::decode(&normal, &red, &ring[..ring.len() - 1], &palette).is_err());
    assert!(Hud::decode(&normal, &red, &ring, &palette[..31]).is_err());
    assert!(Hud::decode(&vec![0; DIGIT_BANK_BYTES + 32], &red, &ring, &palette).is_err());
    assert!(Hud::load_asoe(&[0; 192]).is_err());
    let hud = Hud::decode(&normal, &red, &ring, &palette).unwrap();
    let mut output = vec![17; 240 * 160 * 4];
    let before = output.clone();
    assert!(hud.render_into(&mut output, 240, 160, 0, 32).is_err());
    assert!(hud.render_into(&mut output, usize::MAX, 160, 0, 0).is_err());
    assert!(hud
        .render_into(&mut output, MAX_FRAME_PIXELS + 1, 1, 0, 0)
        .is_err());
    assert!(hud.render_into(&mut output, 0, 160, 0, 0).is_err());
    assert!(hud.render_into(&mut output[..100], 240, 160, 0, 0).is_err());
    assert_eq!(output, before);
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; no glyph/icon pixels distributed"]
fn authentic_hud_matches_independent_raw_byte_python_oracle() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let hud = Hud::load_asoe(&rom).unwrap();
    for (count, phase, sha256) in [
        (
            0,
            0,
            "321b4c6c98fd586d74f5f78a0a81c75afd1feb8568524628ed7247e665da74de",
        ),
        (
            0,
            8,
            "04d3e13ff746cabdc25ce6ac74605acf823eab453d44cc136dbdb645488d8ef3",
        ),
        (
            0,
            24,
            "321b4c6c98fd586d74f5f78a0a81c75afd1feb8568524628ed7247e665da74de",
        ),
        (
            7,
            15,
            "e5b291db99512ff4870a8597a0956c152b427e5d490a3299a3290a214e0eb869",
        ),
        (
            42,
            15,
            "f68dc2c5342daab029714955e62b08967867fb645b0b0913fc62e32fe25b8db4",
        ),
        (
            291,
            15,
            "6a95e7eaaf0f6806cd380439e5ec42ad3145058f371fabc79c696f0035c87b57",
        ),
        (
            999,
            15,
            "b6f502940f6fa144ce2e96b6bf8c8fda6c10c23bb217f669149b1449a4d045f9",
        ),
        (
            1000,
            15,
            "b6f502940f6fa144ce2e96b6bf8c8fda6c10c23bb217f669149b1449a4d045f9",
        ),
    ] {
        let mut frame = vec![0; 240 * 160 * 4];
        hud.render_into(&mut frame, 240, 160, count, phase).unwrap();
        assert_eq!(
            binary::sha256(&frame),
            sha256,
            "count {count}, phase {phase}"
        );
    }
    for (offset, length, expected) in [
        (
            NORMAL_DIGITS_OFFSET,
            DIGIT_BANK_BYTES,
            "8f79fe746f1fd6c4142b40383761787d84708ebd450a404a47d737b5e7b27013",
        ),
        (
            RED_DIGITS_OFFSET,
            DIGIT_BANK_BYTES,
            "af60786628d9293cbf003042ab869b16feff01f2b4cd6dd05c1ca40e2afdf69d",
        ),
        (
            RING_ICON_OFFSET,
            RING_ICON_BYTES,
            "e92c5c2d1dbb9adb2c007528f6d33d3759e28a36face04cac0171d1fccc566b2",
        ),
        (
            PALETTE_OFFSET,
            32,
            "45cdc5144f5dc133f8f4f1837814db8bb14991d91faad7cf513b9000ed249ff6",
        ),
    ] {
        assert_eq!(
            binary::sha256(binary::range(&rom, offset, length).unwrap()),
            expected
        );
    }
}
