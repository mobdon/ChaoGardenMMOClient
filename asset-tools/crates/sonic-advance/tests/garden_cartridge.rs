use sonic_advance_assets::{binary, flash, garden_cartridge::CartridgeGarden};

fn fixture() -> Vec<u8> {
    let mut f = vec![0xff; flash::FLASH_BYTES];
    let r = &mut f[15 * 4096..15 * 4096 + 0x858];
    for (i, b) in r.iter_mut().enumerate() {
        *b = (i * 31 + 7) as u8;
    }
    r[2..4].copy_from_slice(&0x817u16.to_le_bytes());
    for (p, v) in [(4, 17u32), (8, 100), (12, 800), (0x18, 0xfedcba98)] {
        r[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    let crc = flash::tiny_checksum(&r[2..]);
    r[..2].copy_from_slice(&crc.to_le_bytes());
    f
}

#[test]
fn load_reconcile_rotate_and_reload() {
    let source = fixture();
    let game = CartridgeGarden::load(&source, 125).unwrap();
    assert_eq!(game.rings, 825);
    let copy = game.save_copy();
    assert_eq!(
        flash::inspect(&copy).unwrap().selected_tiny_sector,
        Some(10)
    );
    assert_eq!(&copy[..10 * 4096], &source[..10 * 4096]);
    assert_eq!(&copy[11 * 4096..], &source[11 * 4096..]);
    let record = &copy[10 * 4096..10 * 4096 + 0x858];
    assert_eq!(
        binary::sha256(record),
        "4d796f24d29c01ff2fee2d7a36b31f60132398715a0921bf05331a6685200c13"
    );
    let reloaded = CartridgeGarden::load(&copy, 125).unwrap();
    assert_eq!(reloaded.rings, 825);
    assert_eq!(reloaded.rng, 0xfedcba98);
}
#[test]
fn rejects_missing_or_damaged_records() {
    let mut f = fixture();
    f[15 * 4096 + 0x176] ^= 1;
    for image in [&f[..], &f[..65535], &[]] {
        assert!(CartridgeGarden::load(image, 0).is_err());
    }
}
