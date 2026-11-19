use sa2_asset_tools::set::parse_set;

#[test]
fn preserves_packed_id_and_authored_words() {
    let mut bytes = vec![0; 64];
    bytes[3] = 1;
    bytes[32..40].copy_from_slice(&[0xa1, 0x23, 0xff, 0xfe, 0x80, 0, 0, 1]);
    for (i, value) in [-2.5_f32, 0.0, 1.25, -0.0, 10.0, 7.0].iter().enumerate() {
        bytes[40 + 4 * i..44 + 4 * i].copy_from_slice(&value.to_be_bytes());
    }
    let objects = parse_set(&bytes).unwrap();
    let p = &objects[0];
    assert_eq!((p.object_id, p.clip_level), (0x123, 10));
    assert_eq!(p.rotation, [65534, 32768, 1]);
    assert_eq!(p.position, [-2.5, 0.0, 1.25]);
    assert_eq!(
        p.parameters.map(f32::to_bits),
        [-0.0_f32, 10.0, 7.0].map(f32::to_bits)
    );
}

#[test]
fn requires_complete_header_and_exact_count() {
    assert!(parse_set(&[0; 32]).unwrap().is_empty());
    for size in [0, 4, 31, 33, 64] {
        assert!(parse_set(&vec![0; size]).is_err());
    }
    let mut oversized = [0; 32];
    oversized[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(parse_set(&oversized).is_err());
}

#[test]
#[ignore = "requires SA2_SET_DIR pointing at the local PC resource/gd_PC"]
fn installed_city_escape_variants() {
    let dir = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    use sha2::{Digest, Sha256};
    for (name, count, distinct, digest) in [
        (
            "set0013_s.bin",
            516,
            52,
            "2d3567313f5ecde60f38885aeff6885787ae9068995d94183279e70cccfe7a15",
        ),
        (
            "set0013_u.bin",
            898,
            26,
            "1475cc33492311cffe2b71689c5c75336e92e321c795fbd920fdbc6daaf9df48",
        ),
        (
            "set0013_hd_s.bin",
            476,
            50,
            "9d1f995dfabb430e8e2c1c2264a4cdb295d4479aea939420b0644c86efc59f41",
        ),
        (
            "set0013_2p_s.bin",
            286,
            34,
            "2506a1400eb0f30176cbd6d81f1eacacd4fcb8a97158fe4b16a0b01be26287bd",
        ),
        (
            "set0013_2p_u.bin",
            486,
            21,
            "45ca3691bebaf82ea547fc834ea3d0b52a740c294eb65792ef029732af33218f",
        ),
    ] {
        let objects = parse_set(&std::fs::read(dir.join(name)).unwrap()).unwrap();
        let mut fields = Sha256::new();
        for p in &objects {
            for word in [p.object_id, u16::from(p.clip_level)]
                .into_iter()
                .chain(p.rotation)
            {
                fields.update(word.to_le_bytes());
            }
            for scalar in p.position.into_iter().chain(p.parameters) {
                fields.update(scalar.to_le_bytes());
            }
        }
        assert_eq!(format!("{:x}", fields.finalize()), digest, "{name}");
        assert_eq!(objects.len(), count, "{name}");
        let ids: std::collections::BTreeSet<_> = objects.iter().map(|p| p.object_id).collect();
        assert_eq!(ids.len(), distinct, "{name}");
        assert!(ids.last().unwrap() < &102, "{name}");
    }
}

#[test]
fn rejects_nonfinite_data_and_excessive_counts() {
    use sa2_asset_tools::set::MAX_PLACEMENTS;
    for (offset, value) in [(40, f32::NAN), (60, f32::INFINITY)] {
        let mut bytes = [0; 64];
        bytes[3] = 1;
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(parse_set(&bytes).is_err());
    }
    let count = MAX_PLACEMENTS + 1;
    let mut bytes = vec![0; (count + 1) * 32];
    bytes[..4].copy_from_slice(&(count as u32).to_be_bytes());
    assert!(parse_set(&bytes).is_err());
}

#[test]
#[ignore = "requires the complete numbered PC stage set in SA2_SET_DIR"]
fn installed_numbered_sets() {
    let dir = std::env::var_os("SA2_SET_DIR").unwrap();
    let mut file_count = 0;
    let mut placement_count = 0;
    for file in std::fs::read_dir(dir).unwrap() {
        let file = file.unwrap();
        let name = file.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("set")
            && name.ends_with(".bin")
            && name.as_bytes().get(3).is_some_and(u8::is_ascii_digit)
        {
            let bytes = std::fs::read(file.path()).unwrap();
            placement_count += parse_set(&bytes)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .len();
            file_count += 1;
        }
    }
    assert_eq!((file_count, placement_count), (209, 58_592));
}
