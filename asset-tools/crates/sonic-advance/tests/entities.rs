use sonic_advance_assets::{
    binary,
    entities::{PracticeRings, Ring, RingMap},
};

fn map(width: usize, height: usize, indices: &[u32], records: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 12];
    bytes[4..8].copy_from_slice(&(width as u32).to_le_bytes());
    bytes[8..12].copy_from_slice(&(height as u32).to_le_bytes());
    for index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes.extend_from_slice(records);
    let header = (bytes.len() as u32) << 8;
    bytes[..4].copy_from_slice(&header.to_le_bytes());
    bytes
}

#[test]
fn region_offsets_use_byte_four_base_and_local_coordinates_times_eight() {
    let bytes = map(2, 1, &[16, 21], &[1, 2, 4, 5, 0xff, 3, 4, 0xff]);
    let rings = RingMap::decode(&bytes, 3).unwrap();
    assert_eq!(
        rings,
        vec![
            Ring { x: 8, y: 16, id: 0 },
            Ring {
                x: 32,
                y: 40,
                id: 1
            },
            Ring {
                x: 280,
                y: 32,
                id: 2
            }
        ]
    );
}

#[test]
fn aliases_and_suffix_indices_share_stable_native_record_identities() {
    let bytes = map(3, 1, &[20, 20, 22], &[1, 2, 4, 5, 0xff]);
    let rings = RingMap::decode(&bytes, 5).unwrap();
    assert_eq!(
        rings.iter().map(|r| r.id).collect::<Vec<_>>(),
        [0, 1, 0, 1, 1]
    );
    let mut ledger = PracticeRings::new(rings).unwrap();
    assert_eq!(ledger.collect_at(8, 16, 0, 0).unwrap(), 1);
    assert!(ledger.is_collected(0));
    assert_eq!(ledger.collect_at(264, 16, 0, 0).unwrap(), 0);
    assert_eq!(ledger.collected_count(), 1);
}

#[test]
fn rejects_missing_terminators_header_corruption_and_coordinate_tail_indices() {
    let good = map(1, 1, &[12], &[1, 2, 0xff]);
    for size in 0..good.len() {
        assert!(RingMap::decode(&good[..size], 10).is_err());
    }
    let missing = map(1, 1, &[12], &[1, 2]);
    assert!(RingMap::decode(&missing, 10).is_err());
    assert!(RingMap::decode(&map(1, 1, &[1], &[0xff]), 10).is_err());
    assert!(RingMap::decode(&map(2, 1, &[16, 17], &[1, 2, 0xff]), 10).is_err());
    assert!(RingMap::decode(&map(2, 1, &[17, 16], &[1, 2, 3, 0xff, 0xff]), 10).is_err());
    assert!(RingMap::decode(&map(1, 1, &[12], &[0xfe, 2, 0xff]), 10).is_err());
}

#[test]
fn enforces_dimensions_offsets_and_output_budgets() {
    assert!(RingMap::decode(&map(0, 1, &[], &[]), 1).is_err());
    assert!(RingMap::decode(&map(257, 1, &[], &[]), 1).is_err());
    assert!(RingMap::decode(&map(256, 256, &[], &[]), 1).is_err());
    assert!(RingMap::decode(&map(1, 1, &[u32::MAX], &[0xff]), 1).is_err());
    assert!(RingMap::decode(&map(1, 1, &[12], &[1, 2, 0xff]), 0).is_err());
    assert_eq!(RingMap::decode(&map(1, 1, &[0], &[]), 0).unwrap().len(), 0);
    assert!(RingMap::decode(&map(1, 1, &[0], &[]), usize::MAX).is_err());
}

#[test]
fn inclusive_native_rectangle_contact_is_once_only_and_session_local() {
    let mut ledger = PracticeRings::new(vec![Ring {
        x: 100,
        y: 100,
        id: 0,
    }])
    .unwrap();
    assert_eq!(ledger.collect_at(115, 100, 6, 14).unwrap(), 0);
    assert_eq!(ledger.collect_at(114, 100, 6, 14).unwrap(), 1);
    assert_eq!(ledger.collect_at(100, 100, 6, 14).unwrap(), 0);
    assert_eq!(ledger.collected_count(), 1);
    let reset = PracticeRings::new(ledger.rings().to_vec()).unwrap();
    assert_eq!(reset.collected_count(), 0);
    assert!(!reset.is_collected(0));
    assert!(ledger.collect_at(100, 100, -1, 14).is_err());
    assert!(ledger.collect_at(i32::MAX, i32::MIN, 65, 14).is_err());
}

#[test]
fn invalid_ledger_identities_and_coordinates_are_rejected() {
    assert!(PracticeRings::new(vec![Ring {
        x: 0,
        y: 0,
        id: usize::MAX
    }])
    .is_err());
    assert!(
        PracticeRings::new(vec![Ring { x: 0, y: 0, id: 1 }, Ring { x: 0, y: 0, id: 1 }]).is_err()
    );
    assert!(PracticeRings::new(vec![Ring { x: -1, y: 0, id: 0 }]).is_err());
    assert!(PracticeRings::new(vec![Ring {
        x: 0,
        y: i32::MAX,
        id: 0
    }])
    .is_err());
    let mut empty = PracticeRings::new(vec![]).unwrap();
    assert_eq!(empty.collect_at(i32::MIN, i32::MAX, 64, 64).unwrap(), 0);
}

#[test]
#[ignore = "requires local SONIC_ADVANCE_ASOE_ROM; no native layout fixture is distributed"]
fn authentic_ring_positions_match_pinned_native_csv_hash() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let rings = RingMap::load_asoe(&rom, 0).unwrap();
    assert_eq!(rings.len(), 291);
    let positions = rings
        .iter()
        .flat_map(|r| r.x.to_le_bytes().into_iter().chain(r.y.to_le_bytes()))
        .collect::<Vec<_>>();
    // Independently generated from every row of pinned native rings.csv,
    // rather than an expected output produced by this Rust decoder.
    assert_eq!(
        binary::sha256(&positions),
        "3e02b6cfb65a35132218f1ba779eb5974aec4b09b94230894bcd222686e87f5d"
    );
    assert!(rings.iter().enumerate().all(|(i, r)| r.id == i));
    assert!(RingMap::load_asoe(&rom, 1).is_err());
    let mut changed = rom.clone();
    changed[0x100] ^= 1;
    assert!(RingMap::load_asoe(&changed, 0).is_err());
}
