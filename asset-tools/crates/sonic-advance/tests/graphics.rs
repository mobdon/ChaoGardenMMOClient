use sonic_advance_assets::graphics::*;

fn options(depth: BitsPerPixel) -> RenderOptions {
    RenderOptions {
        depth,
        transparent_zero: false,
        max_pixels: 4096,
    }
}

#[test]
fn bgr555_has_exact_channel_order_and_expansion() {
    // Independently selected endpoint and intermediate RGB values.
    let colors = decode_palette(
        &[0, 0, 31, 0, 0xe0, 3, 0, 0x7c, 0x10, 0x42, 0xff, 0xff],
        256,
    )
    .unwrap();
    assert_eq!(
        colors,
        [
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [132, 132, 132, 255],
            [255, 255, 255, 255]
        ]
    );
    assert!(decode_palette(&[], 256).is_err());
    assert!(decode_palette(&[1], 256).is_err());
    assert!(decode_palette(&[0; 4], 1).is_err());
    assert!(decode_palette(&[0; 514], usize::MAX).is_err());
}

#[test]
fn tile_nibbles_rows_and_tile_boundaries_are_preserved() {
    let mut source = [0_u8; 64];
    source[0] = 0x21;
    source[3] = 0x87;
    source[4] = 0xa9;
    source[32] = 0xfe;
    let decoded = decode_tile_indices(&source, BitsPerPixel::Four, 128).unwrap();
    assert_eq!(&decoded[..10], &[1, 2, 0, 0, 0, 0, 7, 8, 9, 10]);
    assert_eq!(&decoded[64..66], &[14, 15]);
    let source8: Vec<u8> = (0..128).collect();
    assert_eq!(
        decode_tile_indices(&source8, BitsPerPixel::Eight, 128).unwrap(),
        source8
    );
    for depth in [BitsPerPixel::Four, BitsPerPixel::Eight] {
        assert!(decode_tile_indices(&[], depth, 128).is_err());
        assert!(decode_tile_indices(&[0; 31], depth, 128).is_err());
        assert!(decode_tile_indices(&source, depth, 63).is_err());
    }
}

#[test]
fn text_words_and_hardware_screen_blocks_decode_in_logical_order() {
    assert_eq!(
        TextEntry::decode(0xfc23),
        TextEntry {
            tile: 35,
            flip_x: true,
            flip_y: true,
            palette_bank: 15
        }
    );
    let mut raw = vec![0_u8; 64 * 64 * 2];
    for (block, value) in [1_u16, 2, 3, 4].into_iter().enumerate() {
        for word in raw[block * 2048..(block + 1) * 2048].as_chunks_mut::<2>().0 {
            word.copy_from_slice(&value.to_le_bytes());
        }
    }
    let map = decode_text_map(&raw, 64, 64, TextMapLayout::ScreenBlocks, 4096).unwrap();
    for (x, y, tile) in [
        (0, 0, 1),
        (31, 31, 1),
        (32, 0, 2),
        (63, 31, 2),
        (0, 32, 3),
        (32, 32, 4),
        (63, 63, 4),
    ] {
        assert_eq!(map.entries[y * 64 + x].tile, tile);
    }
    assert!(decode_text_map(&[0; 2], 0, 1, TextMapLayout::Linear, 1).is_err());
    assert!(decode_text_map(&[0; 2], 1, 1, TextMapLayout::ScreenBlocks, 1).is_err());
    assert!(decode_text_map(&[0; 1], 1, 1, TextMapLayout::Linear, 1).is_err());
    assert!(decode_text_map(&[0; 4], 1, 1, TextMapLayout::Linear, 1).is_err());
    assert!(decode_text_map(&[], usize::MAX, 2, TextMapLayout::Linear, usize::MAX).is_err());
    assert!(decode_text_map(&raw, 64, 64, TextMapLayout::ScreenBlocks, 4095).is_err());
}

#[test]
fn rendered_pixels_match_independent_banked_flipped_expectations() {
    let mut indices = [0_u8; 64];
    indices[0] = 1;
    indices[7] = 2;
    indices[56] = 3;
    indices[63] = 4;
    let palette: Vec<[u8; 4]> = (0_u8..32).map(|v| [v, v.wrapping_mul(2), 0, 255]).collect();
    // Four map entries use no flip, horizontal, vertical, then both.
    let raw = [0_u16, 0x1400, 0x1800, 0x1c00]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let map = decode_text_map(&raw, 4, 1, TextMapLayout::Linear, 4).unwrap();
    let rendered =
        render_text_background(&indices, &palette, &map, options(BitsPerPixel::Four)).unwrap();
    assert_eq!((rendered.width, rendered.height), (32, 8));
    // Expected top-left/top-right/bottom-left/bottom-right colors per tile.
    for (tile, expected) in [
        [1, 2, 3, 4],
        [18, 17, 20, 19],
        [19, 20, 17, 18],
        [20, 19, 18, 17],
    ]
    .into_iter()
    .enumerate()
    {
        for ((x, y), color) in [(0, 0), (7, 0), (0, 7), (7, 7)].into_iter().zip(expected) {
            assert_eq!(rendered.pixels[y * 32 + tile * 8 + x], palette[color]);
        }
    }
    let transparent = render_text_background(
        &indices,
        &palette,
        &map,
        RenderOptions {
            transparent_zero: true,
            ..options(BitsPerPixel::Four)
        },
    )
    .unwrap();
    assert_eq!(transparent.pixels[1][3], 0);
    assert_eq!(transparent.pixels[0][3], 255);
    assert_eq!(transparent.pixels[8 + 1], [16, 32, 0, 0]);
}

#[test]
fn eight_bit_ignores_bank_and_render_rejects_malformed_references() {
    let map = decode_text_map(&[0, 0xf0], 1, 1, TextMapLayout::Linear, 1).unwrap();
    let palette: Vec<[u8; 4]> = (0_u8..=255).map(|v| [v, 0, 0, 255]).collect();
    let indices = [255_u8; 64];
    let image =
        render_text_background(&indices, &palette, &map, options(BitsPerPixel::Eight)).unwrap();
    assert_eq!(image.pixels, vec![[255, 0, 0, 255]; 64]);
    assert!(render_text_background(&indices, &palette, &map, options(BitsPerPixel::Four)).is_err());
    assert!(render_text_background(
        &indices,
        &palette[..255],
        &map,
        options(BitsPerPixel::Eight)
    )
    .is_err());
    assert!(
        render_text_background(&indices[..63], &palette, &map, options(BitsPerPixel::Eight))
            .is_err()
    );
    assert!(render_text_background(&indices, &[], &map, options(BitsPerPixel::Eight)).is_err());
    assert!(render_text_background(
        &indices,
        &palette,
        &map,
        RenderOptions {
            max_pixels: 63,
            ..options(BitsPerPixel::Eight)
        }
    )
    .is_err());
    let mut invalid = map.clone();
    invalid.entries[0].tile = 1;
    assert!(
        render_text_background(&indices, &palette, &invalid, options(BitsPerPixel::Eight)).is_err()
    );
    invalid.entries[0].tile = 0;
    invalid.entries[0].palette_bank = 16;
    assert!(
        render_text_background(&indices, &palette, &invalid, options(BitsPerPixel::Eight)).is_err()
    );
    invalid.entries.clear();
    assert!(
        render_text_background(&indices, &palette, &invalid, options(BitsPerPixel::Eight)).is_err()
    );
    invalid.width_tiles = usize::MAX;
    invalid.height_tiles = 2;
    assert!(
        render_text_background(&indices, &palette, &invalid, options(BitsPerPixel::Eight)).is_err()
    );
}
