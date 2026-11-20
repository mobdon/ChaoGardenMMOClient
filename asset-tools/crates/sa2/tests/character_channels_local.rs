use sa2_asset_tools::{archive, character_motions::Bank, motion::read_motion};
use std::{env, fs, path::PathBuf};

#[test]
#[ignore = "requires installed banks and independent splitMTN exports for Amy, Rouge and Super Sonic"]
fn extended_bank_keys_match_split_files() {
    let gd = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap());
    let refs = PathBuf::from(env::var_os("SA2_CHARACTER_SPLITS").unwrap());
    let mut total = 0;
    for (stem, slots) in [
        ("amy", &[229, 231, 233][..]),
        ("chaos0", &[242, 243, 244, 245, 246, 247, 252]),
        ("cwalk", &[193]),
        ("dwalk", &[198]),
        ("rouge", &[206, 208, 210]),
        ("sshadow", &[111, 115]),
        ("ssonic", &[106, 108]),
    ] {
        let prs = fs::read(gd.join(format!("{stem}mtn.prs"))).unwrap();
        let bank = Bank::read(&prs).unwrap();
        let raw = archive::decompress_prs(&prs, 8 << 20).unwrap();
        for &slot in slots {
            let motion = bank
                .motion(slot)
                .unwrap_or_else(|e| panic!("{stem}:{slot}: {e}"));
            total += 1;
            if !matches!(
                (stem, slot),
                ("amy", 229) | ("rouge", 206) | ("ssonic", 106)
            ) {
                continue;
            }
            let split = fs::read(refs.join(format!("{stem}mtn/{slot}.saanim"))).unwrap();
            let other = read_motion(&split).unwrap();
            assert_eq!(
                (motion.frame_count, motion.type_flags, motion.parts.len()),
                (other.frame_count, other.type_flags, other.parts.len())
            );
            let mut nonempty = 0;
            for (a, b) in motion.parts.iter().zip(&other.parts) {
                assert_eq!(a.channels.len(), b.channels.len());
                for (x, y) in a.channels.iter().zip(&b.channels) {
                    assert_eq!(x.kind, y.kind);
                    assert_eq!(x.keys.len(), y.keys.len());
                    for (l, r) in x.keys.iter().zip(&y.keys) {
                        nonempty += 1;
                        assert_eq!(l.frame, r.frame);
                        assert_eq!(l.floats, r.floats);
                        if x.kind.ends_with("Pointer") {
                            let p = l.integers[0] as usize;
                            let q = r.integers[0] as usize;
                            assert_eq!(p == 0, q == 0);
                            if p != 0 {
                                for n in [0, 4, 8] {
                                    assert_eq!(
                                        f32::from_be_bytes(
                                            raw[p + n..p + n + 4].try_into().unwrap()
                                        ),
                                        f32::from_le_bytes(
                                            split[q + n..q + n + 4].try_into().unwrap()
                                        )
                                    );
                                }
                            }
                        } else {
                            assert_eq!(l.integers, r.integers);
                        }
                    }
                }
            }
            assert!(nonempty > 0);
        }
    }
    assert_eq!(total, 19);
}
