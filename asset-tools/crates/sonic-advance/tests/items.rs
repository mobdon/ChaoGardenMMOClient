use sonic_advance_assets::{
    binary,
    entities::StageEntities,
    items::{ItemBoxes, ItemEvent, ItemGrant},
    player::Player,
    runtime::exercise_input,
    sprite::{SpriteBank, SpritePlayer},
    stage::Stage,
};

#[test]
#[ignore = "requires private ASOE revision 0; digest captured with mGBA"]
fn side_contact_and_jumping_past_the_neo_green_hill_ring_monitor_match_retail() {
    assert_eq!(
        replay_monitor(false, 2707),
        "e2e708fe36899b16c9ef83245cd471a2c08926fc7fc7482f897d594fe5bd87b3"
    );
}

#[test]
#[ignore = "requires local ASOE; retail trace with an independently observed monitor break"]
fn attacking_monitor_matches_225_frames_including_the_exact_ring_grant() {
    assert_eq!(
        replay_monitor(true, 2774),
        "a27ceb619c41538766caad209e15c2e69943363be0ddfaa82e0fc0c3d74031b1"
    );
}

fn replay_monitor(break_box: bool, end: u32) -> String {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let mut boxes = ItemBoxes::new(&rom, &StageEntities::load(&rom, 0).unwrap(), &bank).unwrap();
    let mut p = Player::sonic_first_stage(&rom).unwrap();
    p.x = 643827;
    p.y = 89367;
    p.vx = 1152;
    p.ground_speed = 1152;
    p.grounded = true;
    let mut actor = SpritePlayer::new(5, 0);
    actor.tick(&bank).unwrap();
    let mut bytes = Vec::new();
    let mut rings = 17i32;
    for frame in 2550..=end {
        for event in boxes.tick(&mut p, &actor, &stage, &bank).unwrap() {
            if let ItemEvent::Grant(ItemGrant::Rings(n)) = event {
                rings += i32::from(n);
            }
        }
        let mut input = exercise_input(frame - 1800);
        if break_box && frame >= 2640 {
            input = Default::default();
        }
        p.step(input, &stage).unwrap();
        let anim = p.animation();
        actor.animation = anim.animation;
        actor.variant = anim.variant as u8;
        actor.speed_q4_4 = anim.speed_q4_4;
        actor.tick(&bank).unwrap();
        for v in [
            p.x,
            p.y,
            p.vx,
            p.vy,
            p.ground_speed,
            p.half_height,
            i32::from(p.angle),
            p.collision_layer as i32,
        ] {
            bytes.extend(v.to_le_bytes());
        }
        if break_box {
            bytes.extend(rings.to_le_bytes());
        }
    }
    binary::sha256(&bytes)
}

#[test]
#[ignore = "requires private ASOE sprite data"]
fn spinning_attack_breaks_a_box_bounces_and_grants_only_on_frame_sixty() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let mut entities = StageEntities::load(&rom, 0).unwrap();
    entities
        .item_boxes
        .records
        .retain(|m| m.x == 2536 && m.y == 360);
    let mut boxes = ItemBoxes::new(&rom, &entities, &bank).unwrap();
    assert_eq!(
        boxes
            .visible_sprites()
            .map(|(_, _, s)| (s.animation, s.variant))
            .collect::<Vec<_>>(),
        vec![(706, 9), (705, 0)]
    );
    let mut p = Player::sonic_first_stage(&rom).unwrap();
    p.x = 2536 * 256;
    p.y = 350 * 256;
    p.vy = 512;
    p.half_height = 9;
    let mut actor = SpritePlayer::new(8, 0);
    actor.tick(&bank).unwrap();
    assert!(actor.hitboxes[1].is_some());
    assert_eq!(
        boxes.tick(&mut p, &actor, &stage, &bank).unwrap(),
        vec![ItemEvent::Break]
    );
    assert_eq!(p.vy, -512);
    for frame in 1..=91 {
        let events = boxes.tick(&mut p, &actor, &stage, &bank).unwrap();
        if frame == 60 {
            assert_eq!(events, vec![ItemEvent::Grant(ItemGrant::Rings(5))]);
        } else {
            assert!(events.is_empty());
        }
        if frame == 1 || frame == 60 || frame == 90 {
            let (_, y, _) = boxes
                .visible_sprites()
                .find(|(_, _, s)| s.animation == 706)
                .unwrap();
            assert_eq!(y, 360 - frame.min(60));
        }
    }
    assert!(boxes.visible_sprites().next().is_none());
}

#[test]
#[ignore = "requires native dry-land floor"]
fn speed_shoes_double_acceleration_and_cap_but_keep_normal_coasting_friction() {
    use sonic_advance_assets::player::Input;
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let mut p = Player::sonic_first_stage(&rom).unwrap();
    p.y = 113920;
    p.grounded = true;
    p.speed_up = true;
    p.step(
        Input {
            right: true,
            ..Default::default()
        },
        &stage,
    )
    .unwrap();
    assert_eq!(p.ground_speed, 16);
    p.step(Input::default(), &stage).unwrap();
    assert_eq!(p.ground_speed, 8);
    p.ground_speed = 2296;
    p.step(
        Input {
            right: true,
            ..Default::default()
        },
        &stage,
    )
    .unwrap();
    assert_eq!(p.ground_speed, 2304);
    p.ground_speed = 300;
    p.step(
        Input {
            left: true,
            ..Default::default()
        },
        &stage,
    )
    .unwrap();
    assert_eq!(p.ground_speed, 108);
    p.grounded = false;
    p.y = 300 * 256;
    p.vy = 0;
    p.vx = 0;
    p.step(
        Input {
            right: true,
            ..Default::default()
        },
        &stage,
    )
    .unwrap();
    assert_eq!(p.vx, 32);
}
