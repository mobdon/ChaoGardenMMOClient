//! Limited independent retail observations, not a full runtime parity claim.
//! Numeric pickup states came from actual ASOE execution in the fingerprinted
//! mGBA core. The private trace, ROM, ring blob and emulator remain local.
use sonic_advance_assets::{
    binary,
    entities::{PracticeRings, RingMap},
};

#[test]
#[ignore = "requires exact SONIC_ADVANCE_ASOE_ROM; no native fixture is distributed"]
fn actual_retail_pickup_centers_select_the_same_two_source_records() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("set source");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let mut ledger = PracticeRings::new(RingMap::load_asoe(&rom, 0).unwrap()).unwrap();
    // Cold boot, 20 settled frames, right 120, right+jump 30, idle 100.
    // Retail absolute frame 1997 / route tick 198: native record 1267 changed
    // to 0xfe and the native ring counter changed from zero to one.
    assert_eq!(
        ledger.collect_at(115977 / 256, 100094 / 256, 6, 9).unwrap(),
        1
    );
    assert!(ledger.is_collected(14));
    assert_eq!((ledger.rings()[14].x, ledger.rings()[14].y), (464, 400));
    // Retail frame 2021 / tick 222: record 1269 changed to 0xfe, count two.
    assert_eq!(
        ledger
            .collect_at(125913 / 256, 101636 / 256, 6, 14)
            .unwrap(),
        1
    );
    assert!(ledger.is_collected(15));
    assert_eq!((ledger.rings()[15].x, ledger.rings()[15].y), (504, 400));
    assert_eq!(ledger.collected_count(), 2);
    assert_eq!(
        ledger
            .collect_at(125913 / 256, 101636 / 256, 6, 14)
            .unwrap(),
        0
    );
}
