use sonic_advance_assets::tables::*;

#[test]
fn map_headers_preserve_raw_bytes_without_guessing_index_widths() {
    let mut rom = vec![0; 128];
    rom[..8].copy_from_slice(&[2, 0, 3, 0, 32, 0, 4, 5]);
    for (at, word) in [
        (8, 0x08000040_u32),
        (12, 32),
        (16, 0x08000060),
        (24, 0x08000060),
        (28, 0x08000061),
    ] {
        rom[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }
    rom[22..24].copy_from_slice(&16_u16.to_le_bytes());
    rom[32..36].copy_from_slice(&[7, 0, 8, 0]);
    let map = foreground_map(&rom, 0, 56).unwrap();
    assert_eq!((map.tileset.width_tiles, map.tileset.height_tiles), (2, 3));
    assert_eq!(
        (
            map.tileset.animation_tile_bytes,
            map.tileset.animation_frames,
            map.tileset.animation_delay
        ),
        (32, 4, 5)
    );
    assert_eq!(map.tileset.tiles.len(), 32);
    assert_eq!(map.tileset.palette.unwrap().len(), 32);
    assert_eq!(map.tileset.map_offset, 96);
    assert_eq!(map.metatile_map_offset, 97);
    assert_eq!(map.raw_header, &rom[..36]);
    assert!(foreground_map(&rom, 0, 55).is_err());
    assert!(tilemap(&rom, 0, 5).is_err());
    assert!(tilemap(&rom[..127], 0, 6).is_err());
    assert!(foreground_map(&rom[..35], 0, 56).is_err());
    rom[16..20].fill(0);
    let shared = tilemap(&rom, 0, 6).unwrap();
    assert!(shared.palette.is_none());
    assert!(shared.palette_offset.is_none());
    assert_eq!(shared.palette_color_count, 16);
    rom[16..20].copy_from_slice(&0x08000060_u32.to_le_bytes());
    rom[20..22].copy_from_slice(&250_u16.to_le_bytes());
    assert!(tilemap(&rom, 0, 6).is_err());
    rom[20..22].copy_from_slice(&0_u16.to_le_bytes());
    rom[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(tilemap(&rom, 0, 6).is_err());
}

#[test]
fn ui_records_preserve_unknown_words_and_bound_pointers() {
    let mut rom = vec![0; 64];
    let words = [0x08000020_u32, 0x08000030, 2, 0, 2, 1, 0xffffffff];
    for (i, word) in words.iter().enumerate() {
        rom[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    let row = &ui_graphics(&rom, 0, 1, 1).unwrap()[0];
    assert_eq!((row.tile_offset, row.palette_offset), (32, 48));
    assert_eq!(row.unknown, [2, 0, 2, 1, 0xffffffff]);
    assert!(ui_graphics(&rom[..27], 0, 1, 1).is_err());
    assert!(ui_graphics(&rom, 0, 2, 1).is_err());
    assert!(ui_graphics(&rom, 0, usize::MAX, usize::MAX).is_err());
    rom[4..8].copy_from_slice(&0x08000031_u32.to_le_bytes());
    assert!(ui_graphics(&rom, 0, 1, 1).is_err());
    assert!(asoe_ui_graphics(&rom).is_err());
}

#[test]
fn song_records_and_headers_have_bounded_nonrecursive_views() {
    let mut rom = vec![0; 64];
    rom[..8].copy_from_slice(&[0x10, 0, 0, 8, 3, 0, 4, 0]);
    rom[16..32].copy_from_slice(&[2, 1, 7, 128, 0x30, 0, 0, 8, 0x31, 0, 0, 8, 0x33, 0, 0, 8]);
    let rows = song_table(&rom, 0, 2, 2).unwrap();
    assert_eq!(rows[0].header_offset, Some(16));
    assert_eq!(rows[0].player_fields, [3, 4]);
    assert_eq!(rows[1].header_offset, None);
    let header = song_header(&rom, 16, 2).unwrap();
    assert_eq!(
        (header.block_count, header.priority, header.reverb),
        (1, 7, 128)
    );
    assert_eq!(header.voicegroup_offset, Some(48));
    assert_eq!(header.track_offsets, [49, 51]);
    assert!(song_header(&rom, 16, 1).is_err());
    assert!(song_header(&rom[..31], 16, 2).is_err());
    assert!(song_table(&rom[..7], 0, 1, 1).is_err());
    assert!(song_table(&rom, 0, 2, 1).is_err());
    assert!(song_header(&rom, usize::MAX, 2).is_err());
    let dummy = song_header(&[0, 0, 0, 0], 0, 0).unwrap();
    assert_eq!(dummy.voicegroup_offset, None);
    assert!(dummy.track_offsets.is_empty());
    assert!(song_header(&[0, 0, 0], 0, 0).is_err());
    rom[16] = 255;
    assert!(song_header(&rom, 16, usize::MAX).is_err());
    rom[..4].copy_from_slice(&0x02000000_u32.to_le_bytes());
    assert!(song_table(&rom, 0, 1, 1).is_err());
}
