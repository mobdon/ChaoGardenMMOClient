use sa2_asset_tools::{
    board_motion::{self, BoardMotionBank},
    city_rel::CityRel,
    manifest::sha256,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

fn digest(bank: &BoardMotionBank, offsets: bool) -> String {
    assert_eq!(bank.slots.len(), 24);
    assert_eq!(
        bank.slots
            .values()
            .map(|m| m.offset)
            .collect::<BTreeSet<_>>()
            .len(),
        21
    );
    assert!(bank.for_action(0x78).is_none() && bank.for_action(0x91).is_none());
    let mut out = Vec::new();
    let mut keys = 0;
    for id in 145u16..=168 {
        let m = bank.for_action(id as i16 - 24).unwrap();
        out.extend(id.to_le_bytes());
        if offsets {
            out.extend(m.offset.to_le_bytes());
        }
        out.extend(m.frame_count.to_le_bytes());
        assert_eq!(m.parts.len(), 2);
        for part in &m.parts {
            assert_eq!(part.channels.len(), 2);
            for (channel, kind) in part.channels.iter().zip(["position", "rotationBams"]) {
                assert_eq!(channel.kind, kind);
                out.extend(channel.key_count.to_le_bytes());
                for k in &channel.keys {
                    keys += 1;
                    out.extend(k.frame.to_le_bytes());
                    for f in &k.floats {
                        out.extend((if offsets && *f == 0.0 { 0.0 } else { *f }).to_le_bytes());
                    }
                    for n in &k.integers {
                        out.extend(n.to_le_bytes());
                    }
                }
            }
        }
    }
    assert_eq!(keys, 3300);

    sha256(&out)
}

#[test]
#[ignore = "requires local retail City Escape data"]
fn independent_motion_oracles() {
    let root = PathBuf::from(std::env::var_os("SA2_SET_DIR").expect("set SA2_SET_DIR to gd_PC"));
    let rel_bytes = fs::read(root.join("stg13D.rel")).unwrap();
    let rel = board_motion::read(&CityRel::read(&rel_bytes).unwrap()).unwrap();
    let pc =
        board_motion::read_pc(&fs::read(root.join("DLL/Win32/Data_DLL.dll")).unwrap()).unwrap();
    assert_eq!(
        digest(&rel, true),
        "a9d92d9a44ac0d5f87f234db38332c22e31988a9ed4a9a2023a7f37f790960d0"
    );
    assert_eq!(
        digest(&pc, true),
        "5ceea77a77d2d1ae0f4ce2a34f0b73518af5dd69ff9a21d89b7a9a30b5a3a2cb"
    );
    let raw = "bd41749f3c03e66b4e2533e68384be38135cb222044d1115f26db32b539f899f";
    assert_eq!(digest(&rel, false), raw);
    assert_eq!(digest(&pc, false), raw);
}
