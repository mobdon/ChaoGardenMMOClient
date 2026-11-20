use sonic_advance_assets::{
    binary,
    rings::{LocalCollection, RingLayout},
};

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 21];
    bytes[..4].copy_from_slice(&(21_u32 << 8).to_le_bytes());
    bytes[4..8].copy_from_slice(&1_u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&12_u32.to_le_bytes());
    bytes[16..].copy_from_slice(&[32, 0, 2, 8, 255]);
    bytes
}

#[test]
fn spatial_offsets_seams_and_exact_once_removal_are_preserved() {
    let layout = RingLayout::parse(&fixture()).unwrap();
    assert_eq!((layout.rings[0].x, layout.rings[0].y), (256, 0));
    assert_eq!((layout.rings[1].x, layout.rings[1].y), (16, 64));
    let mut run = LocalCollection::default();
    assert_eq!(run.collect(&layout, 16, 56, 14).unwrap(), 1);
    assert_eq!(run.collect(&layout, 16, 56, 14).unwrap(), 0);
    assert_eq!(run.collect(&layout, i32::MAX, i32::MIN, 14).unwrap(), 0);
    assert_eq!(run.count(), 1);
    assert!(!run.contains(&layout.rings[0]));
}

#[test]
fn spin_contact_and_native_region_scans_do_not_expand_pickups() {
    let mut layout = RingLayout::parse(&fixture()).unwrap();
    let mut run = LocalCollection::default();
    assert_eq!(run.collect(&layout, 16, 78, 9).unwrap(), 0);
    assert_eq!(run.collect(&layout, 16, 78, 14).unwrap(), 1);
    layout.rings.push(layout.rings[1].clone());
    assert_eq!(run.collect(&layout, 16, 56, 9).unwrap(), 0);
    assert!(run.collect(&layout, 16, 56, -14).is_err());
    assert_eq!(run.count(), 1);
    let mut run = LocalCollection::default();
    assert_eq!(run.collect(&layout, 0, 56, 14).unwrap(), 0);
}

#[test]
fn rejects_bad_lengths_dimensions_pointers_and_unterminated_lists() {
    let valid = fixture();
    for len in 0..valid.len() {
        assert!(RingLayout::parse(&valid[..len]).is_err());
    }
    let mut changed = valid.clone();
    changed[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(RingLayout::parse(&changed).is_err());
    let mut changed = valid.clone();
    changed[12..16].copy_from_slice(&1_u32.to_le_bytes());
    assert!(RingLayout::parse(&changed).is_err());
    let mut changed = valid.clone();
    changed[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(RingLayout::parse(&changed).is_err());
    let mut changed = valid;
    changed[20] = 0;
    assert!(RingLayout::parse(&changed).is_err());
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn authentic_all_eighteen_ring_layouts_and_first_stage_positions_match() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../docs/native-rings.json")).unwrap();
    assert_eq!(manifest["sourceSha256"], binary::ROM_SHA256);
    let mut total = 0;
    for expected in manifest["records"].as_array().unwrap() {
        let id = expected["stageIndex"].as_u64().unwrap() as usize;
        let graph = RingLayout::load(&rom, id).unwrap();
        assert_eq!(
            graph.compressed_offset.unwrap() as u64,
            expected["offset"].as_u64().unwrap()
        );
        assert_eq!(
            graph.compressed_length.unwrap() as u64,
            expected["compressedLength"].as_u64().unwrap()
        );
        assert_eq!(
            graph.compressed_sha256.unwrap(),
            expected["compressedSha256"]
        );
        assert_eq!(graph.decoded_sha256, expected["decodedSha256"]);
        assert_eq!(
            graph.rings.len() as u64,
            expected["count"].as_u64().unwrap()
        );
        let coordinates: Vec<u8> = graph
            .rings
            .iter()
            .flat_map(|r| r.x.to_le_bytes().into_iter().chain(r.y.to_le_bytes()))
            .collect();
        assert_eq!(binary::sha256(&coordinates), expected["coordinatesSha256"]);
        total += graph.rings.len();
    }
    assert_eq!(total, 3837);
    let layout = RingLayout::load(&rom, 0).unwrap();
    assert_eq!(layout.compressed_offset, Some(0x6c8d8c));
    assert_eq!(layout.compressed_length, Some(969));
    assert_eq!(
        layout.decoded_sha256,
        "fdf280edc42746ec4e505ce5618828f5a6ff3fbb82d6734eca19e9e60445132f"
    );
    assert_eq!(
        (
            layout.width_regions,
            layout.height_regions,
            layout.rings.len()
        ),
        (51, 6, 291)
    );
    assert_eq!((layout.rings[0].x, layout.rings[0].y), (2152, 128));
}
