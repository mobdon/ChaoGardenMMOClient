//! Opt-in parity against the user's local ROM. No proprietary fixture in Git.
use sonic_advance_assets::{animation, binary, catalog, graphics, tables};

fn local_rom() -> Vec<u8> {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap()
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; no art or ROM fixture distributed"]
fn raw_graphics_match_independent_javascript_output_hashes() {
    let rom = local_rom();
    binary::verify_asoe(&rom).unwrap();
    let records = catalog::records().unwrap();
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("graphics-parity.json")).unwrap();
    assert_eq!(oracle["sourceSha256"], binary::ROM_SHA256);
    let outputs = oracle["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 202);
    for expected in outputs {
        let record = records
            .iter()
            .find(|r| r.id == expected["id"].as_str().unwrap())
            .unwrap();
        let input = binary::range(&rom, record.offset, record.length).unwrap();
        let output: Vec<u8> = match expected["codec"].as_str().unwrap() {
            "4bpp-indices" => {
                graphics::decode_tile_indices(input, graphics::BitsPerPixel::Four, 8 * 1024 * 1024)
                    .unwrap()
            }
            "bgr555-rgba-opaque" => input
                .chunks(512)
                .flat_map(|bank| {
                    graphics::decode_palette(bank, 256)
                        .unwrap()
                        .into_iter()
                        .flatten()
                })
                .collect(),
            _ => panic!("unexpected oracle codec"),
        };
        assert_eq!(
            output.len() as u64,
            expected["bytes"].as_u64().unwrap(),
            "{}",
            record.id
        );
        assert_eq!(
            binary::sha256(&output),
            expected["sha256"].as_str().unwrap(),
            "{}",
            record.id
        );
    }
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM; verifies actual version-specific table boundaries"]
fn authentic_tables_preserve_fields_and_do_not_import_european_counts() {
    let rom = local_rom();
    binary::verify_asoe(&rom).unwrap();
    let roots = binary::pointer_table(&rom, 0xd868c, 6, 6).unwrap();
    assert_eq!(
        roots,
        [0xfe3a8, 0x10acb4, 0x119a2c, 0x11a854, 0x11e254, 0x465074]
    );
    for at in &roots[..3] {
        assert_eq!(
            binary::nullable_pointer_table(&rom, *at, 906, 906)
                .unwrap()
                .len(),
            906
        );
        let entries = binary::nullable_pointer_table(&rom, *at, 906, 906).unwrap();
        assert_eq!(entries.iter().filter(|p| p.is_none()).count(), 140);
        assert!(binary::nullable_pointer_table(&rom, *at, 907, 907).is_err());
    }
    let ui = tables::asoe_ui_graphics(&rom).unwrap();
    assert_eq!(ui.len(), 65);
    assert_eq!(
        (ui[0].tile_offset, ui[0].palette_offset, ui[0].unknown),
        (0x658078, 0x65a078, [2, 0, 2, 1, 2])
    );
    let maps = binary::pointer_table(&rom, 0x4a6c40, 89, 89).unwrap();
    let mut shared_palettes = 0;
    for (index, at) in maps.iter().enumerate() {
        let map = if index < 54 && index % 3 != 2 {
            tables::foreground_map(&rom, *at, 1024 * 1024)
                .unwrap()
                .tileset
        } else {
            tables::tilemap(&rom, *at, 1024 * 1024).unwrap()
        };
        assert_eq!(map.raw_header, binary::range(&rom, *at, 28).unwrap());
        if map.palette.is_none() {
            shared_palettes += 1;
        }
    }
    assert!(shared_palettes > 0);
    // The following pointer begins a collision table, not map slot 89.
    let following =
        binary::rom_offset(binary::u32_at(&rom, 0x4a6da4).unwrap(), rom.len(), 4).unwrap();
    assert_eq!(following, 0x4bf1f8);
    assert!(tables::tilemap(&rom, following, 1024 * 1024).is_err());
    let songs = tables::song_table(&rom, 0x6d76ac, 308, 308).unwrap();
    assert_eq!(songs.len(), 308);
    for song in &songs {
        tables::song_header(&rom, song.header_offset.unwrap(), 16).unwrap();
    }
    let records = catalog::records().unwrap();
    for record in records.iter().filter(|r| {
        r.id.starts_with("sonic-advance.asoe.r0.audio.song-") && r.id.ends_with(".header")
    }) {
        let song = tables::song_header(&rom, record.offset, 16).unwrap();
        assert_eq!(record.length, 8 + 4 * song.track_offsets.len());
    }
    let first_script = records
        .iter()
        .find(|r| r.id.ends_with("animations.first-script"))
        .unwrap();
    let raw = binary::range(&rom, first_script.offset, first_script.length).unwrap();
    let script = animation::parse_commands(raw, 4096, 65536).unwrap();
    assert_eq!(script.consumed, raw.len());
    let words: Vec<u8> = script
        .commands
        .iter()
        .flat_map(|command| {
            std::iter::once(command.raw_opcode)
                .chain(command.operands.iter().copied())
                .flat_map(u32::to_le_bytes)
        })
        .collect();
    assert_eq!(words, raw);
}

#[test]
#[ignore = "requires SONIC_ADVANCE_ASOE_ROM pointing to the exact local ASOE revision 0"]
fn authentic_asoe_source_and_every_cataloged_span_match() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    assert_eq!(binary::sha1(&rom), binary::ROM_SHA1);
    let report = catalog::verify(&rom).unwrap();
    assert!(!report.records.is_empty());
    assert_eq!(report.header.game_code, "ASOE");
    let garden = catalog::garden(&rom).unwrap();
    assert_eq!(
        binary::sha1(&garden.bytes),
        "e2a1beec8519d36eae6478cbf3732942ac692023"
    );
    // Regenerate every CRC-table entry from the polynomial, independently of
    // the ROM; this is data parity, not proof of the save checksum's span/init.
    for index in 0..256_u16 {
        let mut value = index;
        for _ in 0..8 {
            value = if value & 1 != 0 {
                (value >> 1) ^ 0x8408
            } else {
                value >> 1
            };
        }
        assert_eq!(
            binary::u16_at(&garden.bytes, 0x18374 + usize::from(index) * 2).unwrap(),
            value
        );
    }
    let mut changed = rom.clone();
    changed[0x100] ^= 1;
    assert!(catalog::verify(&changed).is_err());
    changed[0xac..0xb0].copy_from_slice(b"ASOP");
    assert!(catalog::garden(&changed).is_err());
}
