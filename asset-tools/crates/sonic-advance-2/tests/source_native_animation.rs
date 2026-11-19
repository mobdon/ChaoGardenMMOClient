//! Actual cartridge and independently decoded original sprite-image parity.
//! No source image or cartridge fixture is included in the repository.
use serde::Deserialize;
use sonic_advance_2_assets::{VerifiedRom, gba, native_animation::AnimationPlayer};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageOracle {
    reference_commit: String,
    frames: Vec<ReferenceImage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceImage {
    frame: u16,
    width: usize,
    height: usize,
    rgba_sha256: String,
}

#[test]
#[ignore = "Requires A2NE_ROM: exact local verified cartridge"]
fn native_sonic_idle_vm_and_oam_composition_match_original_reference_images() {
    let path = std::env::var_os("A2NE_ROM").expect("Set A2NE_ROM");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 16_777_216);
    let bytes = std::fs::read(path).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut player = AnimationPlayer::new(&rom, 0, 0).unwrap();
    // SAT-R/sa2 commit 2dd3f17d, graphics/sa2/obj_tiles/4bpp/anim_0000/.
    // Independently decoded PNGs, restored to their original five-bit palette
    // channels then expanded by bit replication, as in graphics.rs. The PNG
    // reference export floors v*255/31 (e.g. channel22=>180); bit replication
    // maps channel22=>181. Invisible RGB is canonicalized to zero.
    // PNG Git blobs: f000 d2b9ba9880fe18dec34f7c2c63ca9c19ee4d5a80,
    // f001 96b20b449c3ab6f4b0ac0e95448b116a3bd43e63,
    // f004 b5ceb7a23a9b9094cc068367f2d0024b64decd8d.
    let oracle: ImageOracle =
        serde_json::from_str(include_str!("../catalog/native-sprite-parity.json")).unwrap();
    assert_eq!(
        oracle.reference_commit,
        sonic_advance_2_assets::REFERENCE_COMMIT
    );
    let expected: BTreeMap<_, _> = oracle
        .frames
        .into_iter()
        .map(|image| (image.frame, image))
        .collect();
    assert_eq!(expected.len(), 3);
    let mut checked = BTreeSet::new();
    let mut observed = BTreeSet::new();
    let mut frame_zero_ticks = Vec::new();
    for tick in 0..400 {
        let step = player.advance().unwrap();
        assert!(!step.ended);
        assert!(step.sounds.is_empty());
        let frame = player.render().unwrap();
        assert!(
            frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] == 255)
        );
        let id = step.frame_index.unwrap();
        observed.insert(gba::sha256(&frame.rgba));
        if let Some(image) = expected.get(&id) {
            assert_eq!((frame.width, frame.height), (image.width, image.height));
            assert_eq!(
                gba::sha256(&frame.rgba),
                image.rgba_sha256,
                "source frame {id} at tick {tick}"
            );
            checked.insert(id);
        }
        if tick < 8 && id == 0 {
            frame_zero_ticks.push(tick);
        }
        if tick == 0 {
            assert_eq!(
                (frame.origin_x, frame.origin_y, frame.subframes),
                (16, 24, 2)
            );
            assert_eq!(player.hitboxes()[0].rectangle, [-6, -16, 6, 16]);
            let original = frame.rgba;
            player.set_flip(true, false);
            let mirrored = player.render().unwrap();
            for y in 0..frame.height {
                for x in 0..frame.width {
                    let normal = (y * frame.width + x) * 4;
                    let flipped = (y * frame.width + frame.width - 1 - x) * 4;
                    assert_eq!(
                        &original[normal..normal + 4],
                        &mirrored.rgba[flipped..flipped + 4]
                    );
                }
            }
            player.set_flip(false, false);
        }
    }
    assert_eq!(checked.len(), 3);
    assert_eq!(
        observed.len(),
        13,
        "all 13 original idle images must execute"
    );
    assert_eq!(frame_zero_ticks, [0, 1, 2, 3, 4, 5]);
    assert!(AnimationPlayer::new(&rom, 0, 1).is_err());
}

#[test]
#[ignore = "Requires A2NE_ROM: exact local verified cartridge"]
fn five_native_characters_execute_idle_walk_jump_and_brake_oam_frames() {
    let path = std::env::var_os("A2NE_ROM").expect("Set A2NE_ROM");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 16_777_216);
    let bytes = std::fs::read(path).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    // Pinned include/constants/sa2/animations.h: walk9, jump10/11.
    // sharedplayer.c:5889-5921 selects all six original walk variants.
    for (animation, variant, is_walk) in (0..5).flat_map(|character| {
        (0..6)
            .map(move |variant| (character * 91 + 9, variant, true))
            .chain([0, 7, 10, 11].map(|action| (character * 91 + action, 0, false)))
    }) {
        let mut player = AnimationPlayer::new(&rom, animation, variant).unwrap();
        let mut images = BTreeSet::new();
        let mut shows = 0;
        for _ in 0..180 {
            let step = player.advance().unwrap();
            let frame = player.render().unwrap();
            assert!(!frame.rgba.is_empty(), "animation {animation}/{variant}");
            assert!(
                frame
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 255)
            );
            if step.changed_frame {
                shows += 1;
            }
            images.insert(gba::sha256(&frame.rgba));
        }
        assert!(
            shows >= 1,
            "native animation {animation}/{variant} must show a frame"
        );
        if is_walk {
            assert!(shows >= 2, "native walk {animation}/{variant} must advance");
            assert!(
                images.len() >= 2,
                "native walk {animation}/{variant} must change pixels"
            );
        }
        println!(
            "animation {animation}/{variant}: {shows} show commands, {} distinct native images",
            images.len()
        );
    }
}
