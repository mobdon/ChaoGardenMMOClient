use sonic_advance_assets::{
    binary,
    flash::{self, Validity, FLASH_BYTES, SECTOR_BYTES},
};
fn image(main: &str, garden: &[Option<i64>]) -> Vec<u8> {
    let mut flash = vec![0xff; FLASH_BYTES];
    for (sector, mode) in main.bytes().enumerate() {
        if mode == b'-' {
            continue;
        }
        let at = sector * SECTOR_BYTES;
        let record = &mut flash[at..at + 0x430];
        for (i, byte) in record.iter_mut().enumerate() {
            *byte = (i * 3 + sector) as u8;
        }
        record[..4].copy_from_slice(b"PIRO");
        let sum = record[..0x42c]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v))
            .fold(0u32, u32::wrapping_add);
        record[0x42c..].copy_from_slice(&(sum ^ u32::from(mode == b'x')).to_le_bytes());
    }
    for (index, version) in garden.iter().enumerate() {
        let Some(version) = version else {
            continue;
        };
        let sector = index + 10;
        let at = sector * SECTOR_BYTES;
        let record = &mut flash[at..at + 0x858];
        for (i, byte) in record.iter_mut().enumerate() {
            *byte = (i * 5 + sector) as u8;
        }
        record[2..4].copy_from_slice(&0x0817u16.to_le_bytes());
        record[4..8].copy_from_slice(&(*version as u32).to_le_bytes());
        let crc = flash::tiny_checksum(&record[2..]) ^ u16::from(*version < 0);
        record[..2].copy_from_slice(&crc.to_le_bytes());
    }
    flash
}
#[test]
fn matches_six_executed_asoe_thumb_selector_cases() {
    type Case = (
        &'static str,
        &'static [Option<i64>],
        Option<usize>,
        Option<usize>,
        &'static str,
    );
    let cases: [Case; 6] = [
        (
            "----------",
            &[],
            None,
            None,
            "71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063",
        ),
        (
            "-vxv------",
            &[Some(0), Some(9), Some(9), Some(-99), Some(4), Some(2)],
            Some(3),
            Some(11),
            "2e1ee50461b82dc8a70552a6e7d82577de3172988c93ebd6b445e2417cf00126",
        ),
        (
            "v-vvvvvvvv",
            &[Some(0xffffffff), Some(0)],
            Some(0),
            Some(10),
            "c62d4c01aa1574fe9c8cceccf8cc95a054ba8cfd5be863d6264ddba750452106",
        ),
        (
            "-vvvvvvvvv",
            &[None, None, None, None, None, Some(0)],
            Some(9),
            Some(15),
            "869a73ed6d37ded8850532f4c848062d9ad7cc9481c2bd9b91b305579f91af4f",
        ),
        (
            "xv-xvvvvvv",
            &[Some(-1)],
            Some(1),
            None,
            "62dc1353ab78f62c4a2b7d202383837748f70a93ef64f77082060cb91abef771",
        ),
        (
            "vvvvvvvvvx",
            &[Some(4), Some(8), Some(9), Some(10), Some(-20), Some(19)],
            Some(8),
            Some(15),
            "d16a8371eb56f29aad2ffbb1059c73e007a3e5fdca922b6a1642955263b8debb",
        ),
    ];
    for (main, garden, game_sector, tiny_sector, hash) in cases {
        let bytes = image(main, garden);
        assert_eq!(binary::sha256(&bytes), hash);
        let report = flash::inspect(&bytes).unwrap();
        assert_eq!(
            (report.selected_game_sector, report.selected_tiny_sector),
            (game_sector, tiny_sector)
        );
    }
}
#[test]
fn respects_checksum_boundaries_revision_marker_and_slot_rotation() {
    assert_eq!(flash::tiny_checksum(b"123456789"), 0x906e);
    let mut bytes = image(
        "-vvvvvvvvv",
        &[None, None, None, None, None, Some(0xffffffff)],
    );
    bytes[0x9430] ^= 1;
    bytes[0xf858] ^= 1;
    let report = flash::inspect(&bytes).unwrap();
    assert_eq!(
        (
            report.next_game_sector,
            report.game_erase_ahead_sector,
            report.next_tiny_sector,
            report.next_tiny_generation
        ),
        (0, 1, 10, 0)
    );
    bytes[0x942b] ^= 1;
    bytes[0xf857] ^= 1;
    let report = flash::inspect(&bytes).unwrap();
    assert_eq!(report.game_slots[9].validity, Validity::BadChecksum);
    assert_eq!(report.tiny_slots[5].validity, Validity::BadChecksum);
    bytes[0xf002..0xf004].copy_from_slice(&0x1965u16.to_le_bytes());
    assert_eq!(
        flash::inspect(&bytes).unwrap().tiny_slots[5].validity,
        Validity::WrongMarker
    );
}
#[test]
fn rejects_truncated_or_extended_images() {
    assert!(flash::inspect(&vec![0; FLASH_BYTES - 1]).is_err());
    assert!(flash::inspect(&vec![0; FLASH_BYTES + 1]).is_err());
}
