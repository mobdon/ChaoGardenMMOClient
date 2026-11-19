use sonic_advance_assets::binary::*;

/// Own exactly one newly created temporary file and clean it up even if a test
/// assertion panics. Filesystem fixtures never contain ROM, art, or save data.
struct PrivateInput(std::path::PathBuf);

impl PrivateInput {
    fn new(contents: &[u8]) -> Self {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for _ in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "sonic-advance-read-test-{}-{epoch}-{}.tmp",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    let owned = Self(path);
                    file.write_all(contents).unwrap();
                    file.sync_all().unwrap();
                    return owned;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("could not create private input: {error}"),
            }
        }
        panic!("could not allocate a unique private input filename");
    }
}

impl Drop for PrivateInput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn bounded_file_reads_accept_exact_limits_and_reject_extra_bytes() {
    let input = PrivateInput::new(&[3, 7, 11, 15]);
    assert_eq!(read_bounded(&input.0, 4).unwrap(), [3, 7, 11, 15]);
    assert!(read_bounded(&input.0, 3).is_err());
    assert!(read_bounded(&input.0, 0).is_err());
    assert!(read_bounded(&input.0, usize::MAX).is_err());
    // A path beneath a regular file cannot exist, without depending on a
    // guessed temporary filename that another process could independently use.
    assert!(read_bounded(&input.0.join("nonexistent"), 4).is_err());

    let empty = PrivateInput::new(&[]);
    assert_eq!(read_bounded(&empty.0, 0).unwrap(), Vec::<u8>::new());
}

#[test]
fn lz77_overlap_literals_and_exact_consumed_parity() {
    // One literal, then a distance-one overlapping match; padding is not consumed.
    let encoded = [0x10, 7, 0, 0, 0x40, b'A', 0x30, 0, 0xee];
    let decoded = decompress(&encoded, 7).unwrap();
    assert_eq!(decoded.bytes, b"AAAAAAA");
    assert_eq!(decoded.consumed, 8);
    for end in 0..decoded.consumed {
        assert!(decompress(&encoded[..end], 7).is_err());
    }
    assert!(decompress(&encoded, 6).is_err());
}

#[test]
fn rle_literal_and_repeat_parity() {
    let encoded = [0x30, 6, 0, 0, 1, 4, 9, 0x81, 3, 0xff];
    let decoded = decompress(&encoded, 6).unwrap();
    assert_eq!(decoded.bytes, [4, 9, 3, 3, 3, 3]);
    assert_eq!(decoded.consumed, 9);
    for end in 0..9 {
        assert!(decompress(&encoded[..end], 6).is_err());
    }
}

#[test]
fn malformed_compression_is_rejected() {
    for input in [
        vec![],
        vec![0x20, 1, 0, 0],
        vec![0x10, 0, 0, 0],
        vec![0x10, 3, 0, 0, 0x80, 0, 0],
        vec![0x10, 2, 0, 0, 0x40, 1, 0, 0],
        vec![0x30, 1, 0, 0, 0x80, 0],
    ] {
        assert!(decompress(&input, 100).is_err(), "{input:?}");
    }
    // More than one flag byte and maximum 4096 distance.
    let mut encoded = vec![0x10, 0x03, 0x10, 0];
    for _ in 0..512 {
        encoded.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }
    encoded.extend_from_slice(&[0x80, 0x0f, 0xff]);
    let decoded = decompress(&encoded, 4099).unwrap();
    assert_eq!(&decoded.bytes[4096..], &[1, 2, 3]);
}

#[test]
fn checked_ranges_headers_and_rom_pointers() {
    let nullable = [0, 0, 0, 0, 4, 0, 0, 8];
    assert_eq!(
        nullable_pointer_table(&nullable, 0, 2, 2).unwrap(),
        [None, Some(4)]
    );
    assert!(nullable_pointer_table(&nullable, 0, 2, 1).is_err());
    assert!(nullable_pointer_table(&nullable, 0, usize::MAX, usize::MAX).is_err());
    assert!(nullable_pointer_table(&[0xff; 8], 0, 1, 1).is_err());
    assert!(range(&[0], usize::MAX, 2).is_err());
    assert!(u16_at(&[0], 0).is_err());
    assert!(u32_at(&[0; 3], 0).is_err());
    assert!(header(&[0; 191]).is_err());
    let mut h = [0_u8; 192];
    h[0xa0..0xac].copy_from_slice(b"SONICADVANCE");
    h[0xac..0xb0].copy_from_slice(b"ASOE");
    h[0xb2] = 0x96;
    h[0xbd] = h[0xa0..0xbd]
        .iter()
        .fold(0_u8, |v, b| v.wrapping_sub(*b))
        .wrapping_sub(0x19);
    assert!(header(&h).unwrap().checksum_valid);
    h[0xa0] ^= 1;
    assert!(!header(&h).unwrap().checksum_valid);
    assert!(verify_asoe(&h).is_err());
    assert_eq!(rom_offset(0x08000004, 8, 4).unwrap(), 4);
    for p in [
        0, 0x02000000, 0x08000008, 0x0a000000, 0xffffffff, 0x08000001,
    ] {
        assert!(rom_offset(p, 8, 4).is_err());
    }
    assert!(rom_offset(0x08000000, 8, 0).is_err());
    assert!(pointer_table(&[], 0, usize::MAX, usize::MAX).is_err());
    let table = [4, 0, 0, 8, 0, 0, 0, 8];
    assert_eq!(pointer_table(&table, 0, 2, 2).unwrap(), [4, 0]);
    assert!(pointer_table(&table, 0, 2, 1).is_err());
}

#[test]
fn deterministic_malformed_corpus_never_panics_or_exceeds_budget() {
    let mut state = 0x12345678_u32;
    for n in 0..4096 {
        let mut bytes = vec![0; n % 128];
        for byte in &mut bytes {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *byte = (state >> 24) as u8;
        }
        if bytes.len() >= 4 {
            bytes[0] = if n % 2 == 0 { 0x10 } else { 0x30 };
            bytes[1] = (n % 65) as u8;
            bytes[2] = 0;
            bytes[3] = 0;
        }
        if let Ok(result) = decompress(&bytes, 64) {
            assert!(result.bytes.len() <= 64);
            assert!(result.consumed <= bytes.len());
        }
    }
}
