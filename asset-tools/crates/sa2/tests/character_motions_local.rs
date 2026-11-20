use sa2_asset_tools::{character_motions::Bank, motion::read_motion};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires installed banks and independent splitMTN export of sonicmtn.prs"]
fn installed_banks_and_split_sonic_keys() {
    let gd = PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    let refs = PathBuf::from(std::env::var_os("SA2_MTN_SPLIT").unwrap());
    let mut banks = 0;
    for file in fs::read_dir(&gd).unwrap() {
        let path = file.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("mtn.prs")
        {
            continue;
        }
        let bank = Bank::read(&fs::read(&path).unwrap()).unwrap();
        banks += 1;
        if path.ends_with("amymtn.prs") {
            assert_eq!(bank.rows.iter().filter(|r| r.slot == 212).count(), 2);
            assert!(bank.motion(212).is_err());
        }
        if !path.ends_with("sonicmtn.prs") {
            continue;
        }
        let ini = fs::read_to_string(refs.join("sonicmtn.ini")).unwrap();
        let mut mapped = 0;
        for line in ini.lines() {
            let Some((slot, addr)) = line.split_once("=animation_") else {
                continue;
            };
            let slot: u16 = slot.parse().unwrap();
            let row = bank.rows.iter().find(|r| r.slot == slot).unwrap();
            assert_eq!(row.offset, u32::from_str_radix(addr.trim(), 16).unwrap());
            let motion = bank.motion(slot).unwrap();
            // splitMTN writes one file per distinct motion pointer.
            let first = bank.rows.iter().find(|r| r.offset == row.offset).unwrap();
            let reference =
                read_motion(&fs::read(refs.join(format!("{}.saanim", first.slot))).unwrap())
                    .unwrap();
            assert_eq!(
                (
                    motion.frame_count,
                    motion.part_count,
                    motion.type_flags,
                    motion.interpolation_and_slots
                ),
                (
                    reference.frame_count,
                    reference.part_count,
                    reference.type_flags,
                    reference.interpolation_and_slots
                )
            );
            for (a, b) in motion.parts.iter().zip(&reference.parts) {
                assert_eq!(a.channels.len(), b.channels.len());
                for (x, y) in a.channels.iter().zip(&b.channels) {
                    assert_eq!(x.kind, y.kind);
                    assert_eq!(
                        x.keys.len(),
                        y.keys.len(),
                        "slot {slot} part {} {}",
                        a.index,
                        x.kind
                    );
                    for (l, r) in x.keys.iter().zip(&y.keys) {
                        assert_eq!(l.frame, r.frame);
                        assert_eq!(l.integers, r.integers);
                        assert_eq!(
                            l.floats, r.floats,
                            "slot {slot} part {} frame {}",
                            a.index, l.frame
                        );
                    }
                }
            }
            mapped += 1;
        }
        assert_eq!(mapped, bank.rows.len());
    }
    assert_eq!(banks, 17);
}
