use sonic_advance_2_assets::{
    VerifiedRom,
    native_animation::Hitbox,
    native_player::{Character, PlayerState},
    native_rings::stage_ring_palette,
    native_scatter::{ScatterFrame, StageScatter},
    native_stage::NativeStage,
};
#[test]
#[ignore = "requires verified A2NE_ROM"]
fn leaf_scatter_starts_next_task_can_recollect_and_uses_native_art() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let mut s = StageScatter::new(&rom).unwrap();
    let (mut p, mut rng) = (
        PlayerState::new_at(Character::Sonic, 96 * 256, 655 * 256),
        0,
    );
    let body = Hitbox {
        index: -1,
        rectangle: [-120, -120, 120, 120],
    };
    let camera = [0, 570];
    s.tick(&stage, &p, body, camera, Default::default())
        .unwrap();
    assert_eq!(s.emit(96, 655, 50, 1, false, &mut rng), Some(118));
    assert_eq!(s.sprites().count(), 0);
    let full_rng = rng;
    assert_eq!(s.emit(96, 655, 1, 1, false, &mut rng), Some(118));
    assert_eq!(rng, full_rng);
    p.char_state = 20;
    let mut frames = std::collections::BTreeSet::new();
    for t in 0..34 {
        let frame = ScatterFrame {
            stage_time: t,
            timer_invulnerability: 1,
            ..Default::default()
        };
        assert!(s.tick(&stage, &p, body, camera, frame).unwrap().is_empty());
        let (_, _, a) = s.sprites().next().unwrap();
        assert_eq!(a.animation, 726);
        assert_eq!(
            &a.palette_words()[96..112],
            &stage_ring_palette(&rom).unwrap()
        );
        frames.insert(a.render().unwrap().frame_index.unwrap());
    }
    assert_eq!(frames.len(), 4);
    // Original scatter collision tests the rectangle bytes even when the animation's hitbox index is -1.
    let got = s
        .tick(
            &stage,
            &p,
            body,
            camera,
            ScatterFrame {
                stage_time: 34,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!got.is_empty());
    p.move_state |= 128;
    for t in 35..=181 {
        assert!(
            s.tick(
                &stage,
                &p,
                body,
                camera,
                ScatterFrame {
                    stage_time: t,
                    ..Default::default()
                }
            )
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(s.sprites().count(), 0);
    assert_eq!(s.emit(96, 655, 32, 1, false, &mut rng), Some(118));
    assert_ne!(rng, full_rng);
}
