use sonic_advance_assets::{binary, catalog, garden};

fn generated_program() -> Vec<u8> {
    let mut bytes = vec![0; garden::CRC_TABLE_OFFSET + garden::CRC_TABLE_LEN];
    for index in 0..256_u16 {
        let mut value = index;
        for _ in 0..8 {
            value = if value & 1 == 0 {
                value >> 1
            } else {
                (value >> 1) ^ 0x8408
            };
        }
        let offset = garden::CRC_TABLE_OFFSET + usize::from(index) * 2;
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
fn reference_crc_matches_standard_check_vector() {
    assert_eq!(garden::crc16_x25_reference(b"123456789"), 0x906e);
    assert_eq!(garden::crc16_x25_reference(b""), 0);
}

#[test]
fn checks_every_table_word_and_preserves_input() {
    let original = generated_program();
    garden::verify_crc_table(&original).unwrap();
    let before = original.clone();
    for index in 0..256 {
        let mut corrupted = original.clone();
        corrupted[garden::CRC_TABLE_OFFSET + index * 2] ^= 1;
        assert!(garden::verify_crc_table(&corrupted).is_err());
    }
    assert_eq!(original, before);
}

#[test]
fn rejects_table_truncation_and_fabricated_program() {
    let bytes = generated_program();
    for length in [0, garden::CRC_TABLE_OFFSET, bytes.len() - 1] {
        assert!(garden::verify_crc_table(&bytes[..length]).is_err());
    }
    assert!(garden::GardenEvidence::parse(&bytes).is_err());
    assert!(garden::GardenEvidence::parse(&vec![0; catalog::GARDEN_LEN]).is_err());
}

#[test]
#[ignore = "requires the local source named by SONIC_ADVANCE_ASOE_ROM; no ROM is distributed"]
fn exact_local_garden_table_and_opaque_bytes_match() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let decoded = catalog::garden(&rom).unwrap();
    let evidence = garden::GardenEvidence::parse(&decoded.bytes).unwrap();
    assert_eq!(evidence.opaque_program(), decoded.bytes);
    assert_eq!(
        binary::sha256(evidence.crc_table()),
        garden::CRC_TABLE_SHA256
    );
    assert_eq!(evidence.crc_table().len(), 512);
}

#[test]
fn launch_rejects_truncated_loaded_save_and_rom() {
    for size in [0, 0x19, 0x428, 0x42b, 0x42f] {
        assert!(garden::prepare_launch(&[], &vec![0; size], 0).is_err());
    }
    assert!(garden::prepare_launch(&[], &[0; 0x430], 0).is_err());
}

#[test]
#[ignore = "requires local SONIC_ADVANCE_ASOE_ROM"]
fn retail_launch_image_matches_native_stores() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    for (score, language, frame, checksum, digest) in [
        (0u32, 0, 0, 0u32, catalog::GARDEN_SHA256),
        (
            0x12345678,
            0xfe,
            0xfffffff0,
            0x31,
            "bf1b4012860efefdfedb8e2cb3a513e5a17d59238c66373103566e02f6cdf603",
        ),
        (
            u32::MAX,
            0xff,
            u32::MAX,
            u32::MAX,
            "83ed720728e49ac9c80f934e6cf783ad39e9c4e74a4bff5172e16e9d2e4ed0fd",
        ),
    ] {
        let mut save = [0xa5; 0x430];
        save[0x19] = language;
        save[0x428..0x42c].copy_from_slice(&score.to_le_bytes());
        save[0x42c..].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            binary::sha256(&garden::prepare_launch(&rom, &save, frame).unwrap()),
            digest
        );
    }
}
