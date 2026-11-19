use sa2_asset_tools::{
    board_motion::{self, BoardMotionBank},
    city_rel::CityRel,
    manifest::sha256,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

fn digest(bank: &BoardMotionBank, offsets: bool) -> String {
    assert!(bank.for_action(0x78).is_none() && bank.for_action(0x91).is_none());
    keys_digest(&bank.slots, offsets, 145, 2, 3300)
}
fn keys_digest(
    slots: &std::collections::BTreeMap<u16, sa2_asset_tools::motion::Motion>,
    offsets: bool,
    first: u16,
    parts: usize,
    count: usize,
) -> String {
    assert_eq!(slots.len(), 24);
    assert_eq!(
        slots
            .values()
            .map(|m| m.offset)
            .collect::<BTreeSet<_>>()
            .len(),
        21
    );
    let mut out = Vec::new();
    let mut keys = 0;
    for id in first..(first + 24) {
        let m = &slots[&id];
        out.extend(id.to_le_bytes());
        if offsets {
            out.extend(m.offset.to_le_bytes());
        }
        out.extend(m.frame_count.to_le_bytes());
        assert_eq!(m.parts.len(), parts);
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
    assert_eq!(keys, count);

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

#[test]
#[ignore = "requires local retail City Escape data"]
fn rider_motion_oracles() {
    let root = PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    for (path, expected) in [
        (
            "stg13D.rel",
            "b45a29efeaa7cb0cfb2cad90b12aae35daf70d25a2aaadd9ea8e8aa9fcb3ac1f",
        ),
        (
            "DLL/Win32/Data_DLL.dll",
            "229a68d2c8bbbd562997d2ba726020bd5d7355c289b9352cca199e27f538c891",
        ),
    ] {
        let slots = board_motion::rider_slots(&fs::read(root.join(path)).unwrap()).unwrap();
        assert_eq!(keys_digest(&slots, true, 121, 62, 38366), expected);
    }
}
#[test]
#[ignore = "requires installed game and independent split model/action INI under SA2_RIDER_REFERENCE"]
fn rider_rig_and_action_table() {
    use sa2_asset_tools::{city_rider, model};
    let refs = PathBuf::from(std::env::var_os("SA2_RIDER_REFERENCE").unwrap());
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let gd = game.join("resource/gd_PC");
    let prs = fs::read(gd.join("sonicmdl.prs")).unwrap();
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let rel = fs::read(gd.join("stg13D.rel")).unwrap();
    let original = city_rider::rig(&prs, 0).unwrap();
    let split =
        model::read_model(&fs::read(refs.join("split/sonicmdl/0.sa2mdl")).unwrap()).unwrap();
    assert_eq!(original.nodes.len(), split.nodes.len());
    let index = |m: &model::Model, id: Option<u32>| {
        id.filter(|p| *p != 0)
            .map(|p| m.nodes.iter().position(|n| n.offset == p).unwrap())
    };
    for (a, b) in original.nodes.iter().zip(&split.nodes) {
        assert_eq!(
            (
                a.flags,
                a.position,
                a.rotation_bams,
                a.scale,
                a.quaternion_scalar
            ),
            (
                b.flags,
                b.position,
                b.rotation_bams,
                b.scale,
                b.quaternion_scalar
            )
        );
        for (p, q) in [
            (a.parent_offset, b.parent_offset),
            (Some(a.child_offset), Some(b.child_offset)),
            (Some(a.sibling_offset), Some(b.sibling_offset)),
        ] {
            assert_eq!(index(&original, p), index(&split, q));
        }
    }
    let rows = fs::read_to_string(refs.join("actions/figure/sonic/Sonic/Animation List.ini"))
        .unwrap()
        .replace("\r\n", "\n");
    for id in 121..=144 {
        let v = city_rider::sample(&rel, &exe, &prs, id, 0.).unwrap();
        let row = rows
            .split(&format!("[{id}]\n"))
            .nth(1)
            .unwrap()
            .split("\n\n")
            .next()
            .unwrap();
        let fields = row
            .lines()
            .map(|l| l.split_once('=').unwrap())
            .collect::<std::collections::BTreeMap<_, _>>();
        for (src, out) in [
            ("Animation", "motionSlot"),
            ("Model", "modelSlot"),
            ("Property", "motionMode"),
            ("NextAnimation", "nextAction"),
            ("TransitionSpeed", "transitionSpeed"),
            ("AnimationSpeed", "animationSpeed"),
        ] {
            assert_eq!(
                fields[src].parse::<f32>().unwrap(),
                v[out].as_f64().unwrap() as f32
            );
        }
    }
}
