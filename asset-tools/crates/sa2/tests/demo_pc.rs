use sa2_asset_tools::demo::{read_raw, RECORD_BYTES};
use sha2::{Digest, Sha256};

#[test]
fn preserves_frame_fields_and_cursor_on_failed_seek() {
    let mut bytes = [0; RECORD_BYTES * 2];
    let b = &mut bytes[RECORD_BYTES..];
    b[0..4].copy_from_slice(&0x8000_0001u32.to_be_bytes());
    b[12..16].copy_from_slice(&0x4000_0000u32.to_be_bytes());
    b[18..20].copy_from_slice(&(-225i16).to_be_bytes());
    b[24..28].copy_from_slice(&[0x30, 0xfe, 0xee, 0xed]);
    b[32..36].copy_from_slice(&0x7fc0_1234u32.to_be_bytes());
    b[36..40].copy_from_slice(&(-47i32).to_be_bytes());
    let demo = read_raw(&bytes).unwrap();
    let mut cursor = demo.cursor();
    cursor.seek(1).unwrap();
    assert!(cursor.seek(3).is_err());
    let f = cursor.next().unwrap();
    assert_eq!(f.players[0].held, 0x8000_0001);
    assert_eq!(f.players[0].magnitude().to_bits(), 0x7fc0_1234);
    assert_eq!(f.players[1].pressed, 0x4000_0000);
    assert_eq!(f.players[1].transformed_x, -225);
    assert_eq!(f.players[1].joy_angle, -47);
    assert_eq!(f.players[0].stick_pushed, 0x30);
    assert_eq!(f.unknown, [0xee, 0xed]);
    assert_eq!(cursor.next(), None);
    cursor.seek(0).unwrap();
    assert_eq!(cursor.next(), demo.frame(0));
    cursor.seek(demo.len()).unwrap();
    assert_eq!(cursor.len(), 0);
    assert!(demo.frame(usize::MAX).is_none());
    assert!(read_raw(&[]).is_err());
    assert!(read_raw(&[0; 43]).is_err());
}

#[test]
#[ignore = "requires SA2_SET_DIR containing the four installed raw demo BIN files"]
fn all_physical_records_match_compiled_demo_input_declaration() {
    let dir = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    for (file, count, expected) in [
        (
            "demo13_2p.bin",
            3440,
            "7f4c4aebfbca7740c06244b89c307f6dca8a88fbaee876805f5f7bbe9c0280a8",
        ),
        (
            "demo46_2p.bin",
            3719,
            "3cab59553b703f7279611a35461ac5c88022f71db3db3e09a378da14a06d5941",
        ),
        (
            "demo48_2p.bin",
            1600,
            "688463f1f999005ce8c92f463e3575b786b9e3ad4bc846ebeb78cdf15e3d4372",
        ),
        (
            "demo53_2p.bin",
            3401,
            "9f65c0ef13727f6b27f5d0bab316d4c4aacc88dcfb94549b86918fc9c83a2ae0",
        ),
    ] {
        let bytes = std::fs::read(dir.join(file)).unwrap();
        let demo = read_raw(&bytes).unwrap();
        assert_eq!(demo.len(), count);
        let mut fields = Sha256::new();
        for (index, frame) in demo.cursor().enumerate() {
            assert_eq!(demo.frame(index), Some(frame));
            for p in frame.players {
                fields.update(p.held.to_le_bytes());
                fields.update(p.pressed.to_le_bytes());
                fields.update(p.transformed_x.to_le_bytes());
                fields.update(p.transformed_y.to_le_bytes());
                fields.update([p.stick_pushed]);
                fields.update(p.joy_angle.to_le_bytes());
                fields.update(p.magnitude_bits.to_le_bytes());
            }
            fields.update(frame.unknown);
        }
        assert_eq!(format!("{:x}", fields.finalize()), expected, "{file}");
    }
}
