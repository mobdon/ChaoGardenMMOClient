use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_entities::StageEntities,
    native_player::{Character, PlayerState},
    native_springs::{SpringLaunch, StageSprings},
};
#[test]
fn matches_pinned_c_at_followup_callback_entry() {
    let mut bytes = vec![];
    for i in 0i32..4096 {
        let mut p = PlayerState::new_at(
            [
                Character::Sonic,
                Character::Cream,
                Character::Tails,
                Character::Knuckles,
                Character::Amy,
            ][(i as usize / 128) % 5],
            0,
            0,
        );
        p.q_speed_air_x = ((i * 379 % 3001) - 1500) as i16;
        p.q_speed_air_y = ((i * 23 % 4001) - 2000) as i16;
        p.move_state = 0x331ee730
            | if i & 64 != 0 { 2 } else { 0 }
            | if i & 128 != 0 { 64 } else { 0 }
            | if i & 256 != 0 { 1 } else { 0 };
        p.sprite_offset_y = if i & 512 != 0 { 9 } else { 14 };
        p.rotation = (i / 8) as u8;
        p.unk61 = 5;
        p.unk62 = 5;
        p.unk63 = 5;
        p.unk71 = 1;
        p.unk70 = true;
        p.is_boosting = i & 1024 != 0;
        SpringLaunch {
            transition: 14 + (i % 8) as u8,
            speed_id: ((i / 8) % 4) as u8,
            sound_id: 0,
        }
        .apply(if i & 32 != 0 { 9 } else { 0 }, &mut p);
        for x in [
            p.q_world_x,
            p.q_world_y,
            p.q_speed_air_x as i32,
            p.q_speed_air_y as i32,
            p.move_state as i32,
            p.char_state as i32,
            p.sprite_offset_x as i32,
            p.sprite_offset_y as i32,
            p.disable_trick_timer as i32,
            p.is_boosting as i32,
            p.unk6e as i32,
            p.unk61 as i32,
            p.unk62 as i32,
            p.unk63 as i32,
            p.unk70 as i32,
            p.unk71 as i32,
        ] {
            bytes.extend(x.to_le_bytes());
        }
    }
    assert_eq!(
        gba::sha256(&bytes),
        "183af4ac1906b6c22cb39e2445ab7513fb70837a05b7c623964472b4a905fb9d"
    );
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn course_springs_use_native_graphics_collision_and_activation() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for id in [0, 1, 4, 5, 8, 9, 12, 13, 16, 17, 20, 21, 24, 25] {
        let all = StageEntities::load(&rom, id).unwrap();
        for e in all
            .placements()
            .iter()
            .filter(|e| (3..=12).contains(&e.kind))
        {
            let family = match id / 4 {
                2 => 1,
                5 => 2,
                _ => 0,
            };
            if !seen.insert((family, e.kind)) {
                continue;
            }
            let (x, y) = (e.x, e.y);
            let (cx, cy) = ((x - 120).max(0), (y - 80).max(0));
            let mut objects = StageEntities::load(&rom, id).unwrap();
            let mut springs = StageSprings::new(&rom, id);
            let mut p = PlayerState::new_at(Character::Sonic, -100 * 256, -100 * 256);
            objects.update_layers(&mut p, cx, cy);
            springs.tick(&mut objects, &mut p, cx, cy).unwrap();
            let get = |springs: &StageSprings| {
                springs
                    .sprites()
                    .find(|(xx, yy, _)| (*xx, *yy) == (x, y))
                    .map(|(_, _, a)| (a.animation, a.variant, a.hitboxes()[0]))
            };
            let (anim, idle, hit) = get(&springs).unwrap();
            assert!(
                match family {
                    1 => matches!(anim, 572 | 573),
                    2 => anim == 604,
                    _ => anim == 536,
                },
                "{family} {anim}"
            );
            assert_ne!(hit.index, -1);
            p.q_world_x =
                (x + (i32::from(hit.rectangle[0]) + i32::from(hit.rectangle[2])) / 2) * 256;
            p.q_world_y = (y + i32::from(hit.rectangle[1]) - i32::from(p.sprite_offset_y)) * 256;
            let launches = springs.tick(&mut objects, &mut p, cx, cy).unwrap();
            let direction = [14, 15, 17, 16, 18, 19, 21, 20, 19, 18][usize::from(e.kind - 3)];
            assert!(
                launches
                    .iter()
                    .any(|l| l.transition == direction && l.speed_id == e.data[0] & 3),
                "{} {} {:?}",
                id,
                e.kind,
                launches
            );
            assert_eq!(get(&springs).unwrap().1, idle + 1);
            p.q_world_y = -100 * 256;
            for _ in 0..90 {
                springs.tick(&mut objects, &mut p, cx, cy).unwrap();
            }
            assert_eq!(get(&springs).unwrap().1, idle);
            objects.update_layers(&mut p, cx + 1024, cy);
            springs.tick(&mut objects, &mut p, cx + 1024, cy).unwrap();
            assert!(get(&springs).is_none());
            objects.update_layers(&mut p, cx, cy);
            springs.tick(&mut objects, &mut p, cx, cy).unwrap();
            assert_eq!(get(&springs).unwrap().1, idle);
        }
    }
    assert!(seen.len() >= 15, "{seen:?}");
}
