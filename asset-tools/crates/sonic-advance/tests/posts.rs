use sonic_advance_assets::{
    binary,
    entities::{EntityClass, EntityLayout, StageEntities},
    posts::{PostInput, StagePosts},
    sprite::{Hitbox, SpriteBank},
};

fn input() -> PostInput {
    PostInput {
        x: 80,
        y: 80,
        half_height: 14,
        camera_x: 0,
        camera_y: 0,
        clock: 234,
        ia_override: false,
        body: None,
        grounded: true,
        ground_speed_q8: 576,
    }
}

fn single(kind: u8, data: [u8; 4]) -> StageEntities {
    let mut raw = Vec::new();
    for word in [24u32 << 8, 1, 1, 12] {
        raw.extend(word.to_le_bytes());
    }
    raw.extend([10, 10, kind]);
    raw.extend(data);
    raw.push(255);
    let layout = EntityLayout::parse(&raw, EntityClass::Interactable).unwrap();
    StageEntities {
        interactables: layout.clone(),
        item_boxes: layout.clone(),
        enemies: layout,
    }
}

fn rom() -> Vec<u8> {
    std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap()
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn retail_checkpoint_hitbox_activation_variants_and_camera_reentry() {
    let rom = rom();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let mut posts = StagePosts::new(0, &single(7, [2, 250, 0, 0]));
    let mut i = input();
    i.ia_override = true;
    posts.tick(&bank, i).unwrap();
    let (_, _, s) = posts.visible_sprites().next().unwrap();
    assert_eq!((s.animation, s.variant), (452, 0));
    let hb = s.hitboxes[0].unwrap();
    assert_eq!((hb.left, hb.top, hb.right, hb.bottom), (-7, -44, 8, 5));
    let render = bank.render(s).unwrap().unwrap();
    let rgba: Vec<_> = render.image.pixels.iter().flatten().copied().collect();
    assert_eq!(
        (
            render.image.width,
            render.image.height,
            render.anchor_x,
            render.anchor_y
        ),
        (16, 48, 8, 43)
    );
    assert_eq!(
        binary::sha256(&rgba),
        "c87ad319558df2061a82dccfd90b9fac1f03b2fbd8453dd9f19fb49e7cf44f07"
    );
    i.x = 80 + i32::from(hb.right);
    i.y = 80 + i32::from(hb.top);
    posts.tick(&bank, i).unwrap();
    assert!(posts.checkpoint.is_none());
    i.ia_override = false;
    i.x += 1;
    posts.tick(&bank, i).unwrap();
    assert!(posts.checkpoint.is_none());
    i.x -= 1;
    posts.tick(&bank, i).unwrap();
    let c = posts.checkpoint.unwrap();
    assert_eq!((c.index, c.x, c.y, c.clock), (2, 80, 66, 234));
    assert_eq!(posts.visible_sprites().next().unwrap().2.variant, 1);
    i.clock += 1;
    for _ in 0..180 {
        posts.tick(&bank, i).unwrap();
    }
    assert_eq!(posts.visible_sprites().next().unwrap().2.variant, 2);
    assert_eq!(posts.checkpoint.unwrap(), c);
    i.camera_x = 500;
    posts.tick(&bank, i).unwrap();
    assert_eq!(posts.visible_sprites().count(), 0);
    i.camera_x = 0;
    posts.tick(&bank, i).unwrap();
    assert_eq!(posts.visible_sprites().next().unwrap().2.variant, 2);
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn retail_goal_gates_hidden_posts_and_results_timing() {
    let rom = rom();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let mut posts = StagePosts::new(7, &single(0, [1, 1, 0, 0]));
    let mut i = input();
    i.x = 81;
    i.y = 0;
    posts.tick(&bank, i).unwrap();
    assert!(posts.goal.is_none());
    let s = posts.visible_sprites().next().unwrap().2;
    let render = bank.render(s).unwrap().unwrap();
    let rgba: Vec<_> = render.image.pixels.iter().flatten().copied().collect();
    assert_eq!(
        (
            render.image.width,
            render.image.height,
            render.anchor_x,
            render.anchor_y
        ),
        (48, 40, 24, 38)
    );
    assert_eq!(
        binary::sha256(&rgba),
        "05f38963b5e89ee3f8a0402be78cbd43723393926f8c2b01f056f005178be50e"
    );
    i.x = 80;
    i.y = 1;
    posts.tick(&bank, i).unwrap();
    assert!(posts.goal.is_none());
    i.x += 1;
    posts.tick(&bank, i).unwrap();
    assert_eq!(posts.goal.unwrap().elapsed, 1);
    i.grounded = false;
    for _ in 0..121 {
        posts.tick(&bank, i).unwrap();
    }
    assert!(!posts.goal.unwrap().results_ready);
    i.grounded = true;
    i.ground_speed_q8 = 575;
    posts.tick(&bank, i).unwrap();
    assert!(!posts.goal.unwrap().braking);
    i.ground_speed_q8 = 576;
    posts.tick(&bank, i).unwrap();
    assert!(posts.goal.unwrap().braking);
    posts.tick(&bank, i).unwrap();
    assert!(posts.goal.unwrap().results_ready);
    let mut mp = StagePosts::new(0, &single(0, [1, 0, 0, 0]));
    mp.tick(&bank, i).unwrap();
    assert!(mp.goal.is_none());
    assert_eq!(mp.visible_sprites().count(), 0);
    let mut hidden = StagePosts::new(0, &single(0, [0, 1, 0, 0]));
    hidden.tick(&bank, i).unwrap();
    assert!(hidden.goal.is_some());
    assert_eq!(hidden.visible_sprites().count(), 0);
    let mut egg = StagePosts::new(10, &single(0, [0, 1, 0, 0]));
    egg.tick(&bank, i).unwrap();
    assert!(egg.goal.is_none());
    i.y = 80;
    i.body = Some(Hitbox {
        index: 0,
        left: -1,
        top: 0,
        right: 3,
        bottom: 4,
    });
    egg.tick(&bank, i).unwrap();
    assert!(egg.goal.is_some());
    let mut rocket = StagePosts::new(10, &single(7, [2, 0, 0, 0]));
    i.y = 70;
    rocket.tick(&bank, i).unwrap();
    assert!(rocket.checkpoint.is_none());
    i.y -= 1;
    rocket.tick(&bank, i).unwrap();
    assert_eq!(rocket.checkpoint.unwrap().index, 2);
    assert_eq!(rocket.visible_sprites().count(), 0);
}
