//! Opt-in native parity. No native input is embedded in the repository.
use sonic_pinball_party::{
    audio, binary,
    catalog::{self, Catalog},
    garden,
    graphics::{self, ColorDepth},
    sha256, tables,
};
use std::{fs::File, io::Read};

fn supplied_rom() -> Vec<u8> {
    let path = std::env::var_os("SPP_ROM").expect("Set SPP_ROM to the supplied local A3VE .gba");
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap()
        .take((catalog::ROM_LEN + 1) as u64)
        .read_to_end(&mut bytes)
        .unwrap();
    catalog::validate_rom(&bytes).unwrap();
    bytes
}

#[test]
fn published_catalog_contains_only_consistent_metadata() {
    let catalog: Catalog = serde_json::from_str(include_str!("../catalog.json")).unwrap();
    assert_eq!(catalog.schema_version, 1);
    assert_eq!(catalog.source.sha256, catalog::ROM_SHA256);
    assert!(catalog.assets.len() <= 16384);
    let mut ids = std::collections::BTreeSet::new();
    for asset in &catalog.assets {
        assert!(ids.insert(&asset.id), "duplicate ID");
        assert!(asset
            .id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-'));
        assert_eq!(asset.offset_hex, format!("0x{:06X}", asset.offset));
        let limit = match asset.space.as_str() {
            "rom" => catalog::ROM_LEN,
            "garden" => catalog::GARDEN_LEN,
            _ => panic!("unknown space"),
        };
        assert!(asset
            .offset
            .checked_add(asset.stored_length)
            .is_some_and(|end| end <= limit));
        for hash in [&asset.stored_sha256, &asset.decoded_sha256] {
            assert_eq!(hash.len(), 64);
            assert!(hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }
        assert!(asset
            .pointer_references
            .iter()
            .all(|r| r % 4 == 0 && r + 4 <= limit));
    }
}

#[test]
#[ignore = "Requires local SPP_ROM; exact A3VE source and every catalog span"]
fn native_catalog_and_pointers_match_every_published_fingerprint() {
    let rom = supplied_rom();
    let pinned: Catalog = serde_json::from_str(include_str!("../catalog.json")).unwrap();
    assert_eq!(catalog::build_catalog(&rom).unwrap(), pinned);
    let source = catalog::VerifiedRom::new(&rom).unwrap();
    garden::verify_payload_layout(source.garden()).unwrap();
    for asset in &pinned.assets {
        let decoded = source.extract(asset).unwrap();
        assert_eq!(sha256(&decoded), asset.decoded_sha256);
        let (bytes, base) = if asset.space == "rom" {
            (&rom[..], 0x08000000)
        } else {
            (source.garden(), 0x02000000)
        };
        for &reference in &asset.pointer_references {
            assert_eq!(
                binary::u32_le(bytes, reference).unwrap(),
                base + asset.offset as u32
            );
        }
    }
    let mut mutated = rom.clone();
    *mutated.last_mut().unwrap() ^= 1;
    assert!(catalog::VerifiedRom::new(&mutated).is_err());
    assert!(catalog::extract_garden(&rom[..rom.len() - 1]).is_err());
}

#[test]
#[ignore = "Requires local SPP_ROM; independent Python graphics fingerprints"]
fn native_graphics_match_independent_oracle() {
    let rom = supplied_rom();
    let source = catalog::VerifiedRom::new(&rom).unwrap();
    let payload = source.garden();
    for (offset, expected) in [
        (
            0x1614c,
            "afc566f4504cdece9766601e3e09e889bd4af69448898664f755257cedbfaa02",
        ),
        (
            0x17214,
            "7382a8e958703e42a5b7230a65cc85ca723bfaddce3b11a6b721fb102fdb10e1",
        ),
    ] {
        let decoded = binary::decode(&payload[offset..], binary::MAX_DECODED).unwrap();
        let tiles = graphics::decode_tiles(&decoded.bytes, ColorDepth::Bpp4, 8192).unwrap();
        let indices: Vec<_> = tiles.iter().flatten().copied().collect();
        assert_eq!(sha256(&indices), expected);
    }
    for (offset, len, expected) in [
        (
            0x1606c,
            0xe0,
            "6e5d530df7bcc6cd7c7d57f227ce45f5d29c547e13b87d8d9a2be73e346031a1",
        ),
        (
            0x17074,
            0x1a0,
            "ad17b72e0b5bc2deef06bdf771b8f9a2fd81feb7e928263f0ccb02278836ada8",
        ),
        (
            0x22b68,
            0x200,
            "31d3c92a33c308633c4121b7ad99e37d3bc29ca0c59153cfebf7388e0bf501a7",
        ),
        (
            0x23e50,
            0x200,
            "4efbda07d7315d516d6f4506eb0faa4c753b1d42804fb7f3e1435ec3f587cd1e",
        ),
    ] {
        let rgba = graphics::decode_palette_le(&payload[offset..offset + len], 256).unwrap();
        assert_eq!(
            sha256(&rgba.iter().flatten().copied().collect::<Vec<_>>()),
            expected
        );
    }
    let tiles = graphics::decode_tiles(&rom[0x31c5fc..0x3295fc], ColorDepth::Bpp4, 8192).unwrap();
    let palette = graphics::decode_palette_le(&rom[0x3295fc..0x3297fc], 256).unwrap();
    let map = binary::decode(&rom[0x3345fc..], 4096).unwrap();
    assert_eq!(map.consumed, 3549);
    let image = graphics::render_text_bg(
        &tiles,
        &map.bytes,
        &palette,
        ColorDepth::Bpp4,
        (32, 64),
        256 * 512,
    )
    .unwrap();
    assert_eq!((image.width, image.height), (256, 512));
    assert_eq!(
        sha256(&image.pixels),
        "947f0e412a386426ab6e363c3c3c225e9e18534c201bcd1a62d60bbd1515c09c"
    );
}

#[test]
#[ignore = "Requires local SPP_ROM; native audio and table metadata"]
fn native_tables_and_audio_are_bounded_and_counted() {
    let rom = supplied_rom();
    let songs = audio::parse_songs(&rom).unwrap();
    assert_eq!(songs.len(), 200);
    assert_eq!(songs.iter().filter(|s| s.track_count == 0).count(), 67);
    assert_eq!(songs.iter().map(|s| s.tracks.len()).sum::<usize>(), 569);
    assert_eq!(songs.iter().map(|s| s.track_count).max(), Some(16));
    let audio_rows: Vec<_> = songs
        .iter()
        .map(|song| {
            serde_json::json!([
                song.id,
                song.header_offset,
                song.track_count,
                song.voicegroup_offset,
                song.tracks.iter().map(|t| t.rom_offset).collect::<Vec<_>>()
            ])
        })
        .collect();
    assert_eq!(
        sha256(&serde_json::to_vec(&audio_rows).unwrap()),
        "d8a1feb96aa487cb7aff4dbaf27c3a841e94c3ba5ca1ade1428492c3f4b510c6"
    );
    let tables = tables::parse_tables(&rom).unwrap();
    assert_eq!(tables.descriptors.len(), 23);
    assert_eq!(
        tables
            .descriptors
            .iter()
            .map(|t| t.primary_tiles.len())
            .sum::<usize>(),
        47
    );
    assert_eq!(tables.supplemental_lz.len(), 7);
    assert_eq!(tables.supplemental_palettes.len(), 14);
    let rows: Vec<_> = tables
        .descriptors
        .iter()
        .map(|t| {
            serde_json::json!([
                t.id,
                t.columns.iter().map(|p| p.rom_offset).collect::<Vec<_>>(),
                t.primary_tiles
                    .iter()
                    .map(|p| p.rom_offset)
                    .collect::<Vec<_>>(),
                t.primary_palettes
                    .iter()
                    .map(|p| p.rom_offset)
                    .collect::<Vec<_>>(),
                t.inferred_tile_allocations
                    .iter()
                    .map(|a| a.bytes)
                    .collect::<Vec<_>>()
            ])
        })
        .collect();
    assert_eq!(
        sha256(&serde_json::to_vec(&rows).unwrap()),
        "04d105544d1e58a68b9ebf65539cf56e8f8adeced4b8814c2e1067a1bbfa0917"
    );
    let supplemental = serde_json::json!([
        tables
            .supplemental_lz
            .iter()
            .map(|p| p.rom_offset)
            .collect::<Vec<_>>(),
        tables
            .supplemental_palettes
            .iter()
            .map(|p| p.rom_offset)
            .collect::<Vec<_>>()
    ]);
    assert_eq!(
        sha256(&serde_json::to_vec(&supplemental).unwrap()),
        "c3c1e1a61a5aeb3df96d7e81b3cc2505f30c12f71c9c307cba37e0680c99de42"
    );
}
