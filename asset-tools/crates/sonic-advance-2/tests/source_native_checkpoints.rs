use sonic_advance_2_assets::{
    VerifiedRom,
    native_checkpoints::StageCheckpoints,
    native_entities::StageEntities,
    native_lifecycle::{NativeLifecycle, checkpoint_position},
    native_player::{Character, PlayerState},
};

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn native_balls_use_course_destinations_and_all_seven_palette_scripts() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let palette_offset =
        u32::from_le_bytes(bytes[0xf40e0..0xf40e4].try_into().unwrap()) as usize - 0x08000000;
    let mut covered = [false; 7];
    for id in (0u8..28).filter(|i| i % 4 < 2) {
        let mut entities = StageEntities::load(&rom, id).unwrap();
        let Some(e) = entities.placements().iter().find(|e| e.kind == 43).copied() else {
            continue;
        };
        let mut chk = StageCheckpoints::new(&rom, id).unwrap();
        let mut p = PlayerState::new_at(Character::Sonic, e.x * 256 - 1, -100000);
        let (cx, cy) = (e.x - 120, e.y - 80);
        let mut life = NativeLifecycle::new([0, 0], false);
        entities.update_layers(&mut p, cx, cy);
        assert!(
            chk.tick(&mut entities, &p, cx, cy, &mut life)
                .unwrap()
                .is_empty()
        );
        let (_, _, a) = chk
            .sprites()
            .find(|(x, y, _)| (*x, *y) == (e.x, e.y))
            .unwrap();
        let image = a.render().unwrap();
        assert_eq!(a.animation, 531);
        assert_eq!(a.variant, 0);
        assert!(image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        let zone = usize::from(id / 4);
        covered[zone] = true;
        let pal = if zone == 2 { 263 } else { 252 };
        for sel in [pal, pal, pal + 1, pal + 1] {
            chk.tick(&mut entities, &p, cx, cy, &mut life).unwrap();
            let (bank, colors) = chk.background_palette().unwrap();
            assert_eq!(bank, [2, 7, 3, 0, 6, 11, 6][zone]);
            for (j, c) in colors.iter().enumerate() {
                let off = palette_offset + sel * 32 + j * 2;
                assert_eq!(*c, u16::from_le_bytes([bytes[off], bytes[off + 1]]));
            }
        }
        p.q_world_x = e.x * 256;
        for flags in [0x80, 0x400000] {
            p.move_state = flags;
            assert!(
                chk.tick(&mut entities, &p, cx, cy, &mut life)
                    .unwrap()
                    .is_empty()
            );
            assert!(!life.checkpoint_reached);
        }
        p.move_state = 0;
        life.checkpoint_time = 789;
        assert_eq!(
            chk.tick(&mut entities, &p, cx, cy, &mut life).unwrap(),
            vec![152]
        );
        let dest = checkpoint_position(&rom, id).unwrap();
        assert_eq!([life.checkpoint.x, life.checkpoint.y], dest);
        assert_eq!(life.checkpoint.time, 789);
        assert_eq!(
            chk.sprites()
                .find(|(x, y, _)| (*x, *y) == (e.x, e.y))
                .unwrap()
                .2
                .variant,
            1
        );
        for _ in 0..200 {
            assert!(
                chk.tick(&mut entities, &p, cx, cy, &mut life)
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(chk.sprites().count(), 0);
        entities.update_layers(&mut p, cx + 1024, cy);
        chk.tick(&mut entities, &p, cx + 1024, cy, &mut life)
            .unwrap();
        let off = palette_offset + pal * 32;
        for (j, &c) in chk.background_palette().unwrap().1.iter().enumerate() {
            assert_eq!(
                c,
                u16::from_le_bytes([bytes[off + 2 * j], bytes[off + 2 * j + 1]])
            );
        }
        entities.update_layers(&mut p, cx, cy);
        assert!(
            chk.tick(&mut entities, &p, cx, cy, &mut life)
                .unwrap()
                .is_empty()
        );
        assert_eq!(chk.sprites().count(), 0);
    }
    assert_eq!(covered, [true; 7]);
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn distant_route_uses_silent_toggle_and_culls_before_x_test() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut entities = StageEntities::load(&rom, 0).unwrap();
    let e = *entities
        .placements()
        .iter()
        .find(|e| e.kind == 45 && e.y < 256)
        .unwrap();
    let (cx, cy) = (e.x - 120, e.y - 80);
    let mut p = PlayerState::new_at(Character::Sonic, e.x * 256 - 1, 655 * 256);
    let mut c = StageCheckpoints::new(&rom, 0).unwrap();
    let mut life = NativeLifecycle::new([1, 2], false);
    entities.update_layers(&mut p, cx, cy);
    c.tick(&mut entities, &p, cx, cy, &mut life).unwrap();
    assert_eq!(c.sprites().count(), 0);
    p.q_world_x = e.x * 256;
    c.tick(&mut entities, &p, cx - 1024, cy, &mut life).unwrap();
    assert!(!life.checkpoint_reached);
    entities.update_layers(&mut p, cx - 1024, cy);
    c.tick(&mut entities, &p, cx - 1024, cy, &mut life).unwrap();
    entities.update_layers(&mut p, cx, cy);
    life.checkpoint_time = 321;
    assert!(
        c.tick(&mut entities, &p, cx, cy, &mut life)
            .unwrap()
            .is_empty()
    );
    assert!(life.checkpoint_reached);
    assert_eq!(life.checkpoint.time, 321);
}
