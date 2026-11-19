//! Opt-in local-source parity. No original save or hardware transcript is
//! available: the second test explicitly uses a synthetic record header with
//! the source-backed default opaque body, never authenticated progression.

use sonic_advance_2_assets::{VerifiedRom, gba, native_save, save_layout, transfer};

fn source_rom() -> Vec<u8> {
    std::fs::read(std::env::var_os("A2NE_ROM").expect("Set A2NE_ROM to local A2NE raw ROM"))
        .expect("Read local A2NE ROM")
}

#[test]
#[ignore = "Requires the user's local verified ROM via A2NE_ROM"]
fn templates_name_rows_and_crc_table_match_actual_source_bytes() {
    let rom_bytes = source_rom();
    let source = VerifiedRom::parse(&rom_bytes).unwrap();
    let garden = source.garden_bytes();
    native_save::verify_reference_crc_table(garden).unwrap();
    let tables = save_layout::SaveTemplates::parse(garden).unwrap();
    let template = &garden[0x2071c..0x20e9c];
    assert_eq!(tables.default_chao_bytes(), template);
    assert_eq!(
        gba::sha256(template),
        "ed21c8ce8c09c5bfde4eb07b7b2d3d8042e501ff7b70f01f293d5c4df35ebfe9"
    );
    let mut glyph_rows = Vec::new();
    for mode in 0..2 {
        for row in 0..10 {
            glyph_rows.extend_from_slice(tables.name_bytes(mode, row).unwrap());
        }
    }
    assert_eq!(glyph_rows, garden[0x202b6..0x20342]);
    assert_eq!(
        gba::sha256(&glyph_rows),
        "3c3e22f3f953b3ab0579bc713390ac5387ba6b139e33b24fab63904cb4c7d2f5"
    );
    assert!(tables.name_bytes(2, 0).is_err());
    assert!(tables.name_bytes(0, 10).is_err());
    assert!(tables.name_bytes(usize::MAX, usize::MAX).is_err());
    let reports = tables.reports();
    assert_eq!(reports[0].sha256, gba::sha256(&glyph_rows));
    assert_eq!(reports[1].sha256, gba::sha256(template));
    let mut altered = garden.to_vec();
    altered[0x2071c] ^= 1;
    assert!(save_layout::SaveTemplates::parse(&altered).is_err());
}

#[test]
#[ignore = "Requires the user's local verified ROM via A2NE_ROM"]
fn source_default_bytes_survive_synthetic_save_and_transfer_inspection() {
    let rom_bytes = source_rom();
    let source = VerifiedRom::parse(&rom_bytes).unwrap();
    let tables = save_layout::SaveTemplates::parse(source.garden_bytes()).unwrap();
    let mut raw = [0_u8; native_save::SAVE_RECORD_LEN];
    raw[2..4].copy_from_slice(&native_save::SAVE_MARKER.to_le_bytes());
    raw[4..8].copy_from_slice(&1_u32.to_le_bytes());
    raw[0xd8..].copy_from_slice(tables.default_chao_bytes());
    let crc = native_save::checksum(&raw[2..]);
    raw[..2].copy_from_slice(&crc.to_le_bytes());
    let before = raw;
    let record = native_save::SaveRecord::parse(&raw).unwrap();
    let report = save_layout::inspect_record(&record);
    assert_eq!(
        report.regions[4].sha256,
        gba::sha256(tables.default_chao_bytes())
    );
    let command = transfer::TransferCommand::parse(0x0004_0022).unwrap();
    let bytes = &record.raw_bytes()[0x58..];
    let payload = transfer::TransferPayload::parse(command, bytes).unwrap();
    assert_eq!(payload.report().sha256, gba::sha256(bytes));
    let words: Vec<u32> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .map(|(i, pair)| {
            u32::from((i as u16).swap_bytes())
                | (u32::from(pair[0]) << 16)
                | (u32::from(pair[1]) << 24)
        })
        .collect();
    assert_eq!(
        transfer::decode_sequential_words(command, &words).unwrap(),
        bytes
    );
    assert_eq!(payload.raw_bytes(), bytes);
    assert_eq!(raw, before);
}
