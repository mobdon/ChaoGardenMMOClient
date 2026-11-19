use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_animation::Hitbox,
    native_enemies::{EnemyContext, EnemyEvent, StageEnemies},
    native_lifecycle::NativeLifecycle,
    native_player::{Character, PlayerState},
    native_stage::NativeStage,
};

#[derive(Deserialize)]
struct Reference {
    level_ids: Vec<u8>,
    count: usize,
    positions_sha256: String,
}
fn bytes() -> Vec<u8> {
    std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap()
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn all_stage_enemies_match_independent_authoring_records() {
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let references: Vec<Reference> =
        serde_json::from_str(include_str!("../catalog/native-enemy-layouts.json")).unwrap();
    let mut slots = Vec::new();
    for record in references {
        for id in record.level_ids {
            let stage = StageEnemies::load(&rom, id).unwrap();
            assert_eq!(stage.placements().len(), record.count);
            let mut packed = Vec::new();
            for e in stage.placements() {
                packed.extend(e.x.to_le_bytes());
                packed.extend(e.y.to_le_bytes());
                packed.push(e.kind);
                packed.extend(e.data);
            }
            assert_eq!(gba::sha256(&packed), record.positions_sha256, "stage {id}");
            slots.push(id);
        }
    }
    slots.sort_unstable();
    assert_eq!(slots, (0..31).collect::<Vec<_>>());
    for id in 31..=34 {
        assert!(StageEnemies::load(&rom, id).is_err());
    }
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn first_mon_uses_native_animation_terrain_and_contact() {
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let mut enemies = StageEnemies::load(&rom, 0).unwrap();
    let mut life = NativeLifecycle::new([0, 0], false);
    let first = *enemies
        .placements()
        .iter()
        .filter(|e| e.kind == 0)
        .min_by_key(|e| e.x)
        .unwrap();
    assert_eq!((first.x, first.y), (1872, 848));
    let (cx, cy) = (first.x - 100, first.y - 80);
    enemies.admit(&stage, cx, cy).unwrap();
    let (x, y, _) = enemies.sprites().next().unwrap();
    assert_eq!(x, 1872);
    let mut p = PlayerState::new_at(Character::Sonic, x * 256, (y - 20) * 256);
    let hit = Hitbox {
        index: 0,
        rectangle: [-6, -14, 6, 14],
    };
    let off = Hitbox {
        index: -1,
        rectangle: [0; 4],
    };
    let (mut rings, mut score, mut lives) = (13, 49900, 3);
    let mut c = EnemyContext {
        player: &mut p,
        body: [off; 2],
        lifecycle: &mut life,
        rings: &mut rings,
        score: &mut score,
        lives: &mut lives,
        timer_invincibility: 0,
    };
    let mut variants = std::collections::BTreeSet::new();
    let mut min = y;
    for _ in 0..100 {
        let ev = enemies.tick(&mut c, cx, cy).unwrap();
        assert!(
            ev.iter()
                .any(|e| matches!(e, EnemyEvent::HomingTarget { .. }))
        );
        let (_, height, s) = enemies.sprites().next().unwrap();
        assert_eq!(s.animation, 478);
        variants.insert(s.variant);
        min = min.min(height);
    }
    assert!(variants.is_superset(&[1, 2, 3].into_iter().collect()));
    assert_eq!(min, y - 72);
    let (_, height, s) = enemies.sprites().next().unwrap();
    let b = s.hitboxes()[0].rectangle;
    c.player.q_world_y = (height + (i32::from(b[1]) + i32::from(b[3])) / 2) * 256;
    c.body = [hit, off];
    let ev = enemies.tick(&mut c, cx, cy).unwrap();
    assert!(
        ev.iter()
            .any(|e| matches!(e, EnemyEvent::ScatterRings { count: 13, .. }))
    );
    assert_eq!(*c.rings, 0);
    assert_eq!(c.lifecycle.timer_invulnerability, 120);
    assert!(ev.contains(&EnemyEvent::Transition(9)));
    let (_, height, s) = enemies.sprites().next().unwrap();
    let b = s.hitboxes()[0].rectangle;
    c.player.q_world_y = (height + (i32::from(b[1]) + i32::from(b[3])) / 2) * 256;
    c.player.q_speed_air_y = 250;
    c.body = [hit, hit];
    let ev = enemies.tick(&mut c, cx, cy).unwrap();
    assert!(ev.contains(&EnemyEvent::ExtraLifeMusic));
    assert!(ev.contains(&EnemyEvent::Sound(138)));
    assert_eq!(c.player.q_speed_air_y, -250);
    assert_eq!(*c.score, 50000);
    assert_eq!(*c.lives, 4);
    assert_eq!(enemies.sprites().count(), 0);
    enemies.admit(&stage, cx + 1024, cy).unwrap();
    enemies.admit(&stage, cx, cy).unwrap();
    assert_eq!(enemies.sprites().count(), 0);
}
#[test]
#[ignore = "requires verified A2NE_ROM"]
fn mon_callbacks_match_200_original_c_frames() {
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let mut enemies = StageEnemies::load(&rom, 0).unwrap();
    let (cx, cy) = (1772, 768);
    enemies.admit(&stage, cx, cy).unwrap();
    let (x, y, _) = enemies.sprites().next().unwrap();
    let mut p = PlayerState::new_at(Character::Sonic, x * 256, (y - 20) * 256);
    let off = Hitbox {
        index: -1,
        rectangle: [0; 4],
    };
    let mut life = NativeLifecycle::new([0, 0], false);
    let (mut rings, mut score, mut lives) = (0, 0, 3);
    let mut c = EnemyContext {
        player: &mut p,
        body: [off; 2],
        lifecycle: &mut life,
        rings: &mut rings,
        score: &mut score,
        lives: &mut lives,
        timer_invincibility: 0,
    };
    let mut bytes = vec![];
    for i in 0..200 {
        c.player.q_world_x = (x - if i < 90 { 0 } else { 121 }) * 256;
        let ev = enemies.tick(&mut c, cx, cy).unwrap();
        let (_, yy, s) = enemies.sprites().next().unwrap();
        let homing = ev
            .iter()
            .find_map(|e| {
                if let EnemyEvent::HomingTarget { y_q8, .. } = *e {
                    Some(y_q8 - y * 256)
                } else {
                    None
                }
            })
            .unwrap();
        for field in [
            yy - y,
            i32::from(s.variant),
            i32::from(s.render().unwrap().frame_index.unwrap()),
            homing,
        ] {
            bytes.extend(field.to_le_bytes());
        }
    }
    assert_eq!(
        gba::sha256(&bytes),
        "d0a68f2e347b6530066251740b10d9328c293ad0816145dd1e2a6abd8bc7aaf4"
    );
}
#[test]
#[ignore = "requires verified A2NE_ROM"]
fn buzzer_matches_600_compiled_frames_and_original_easy_gate() {
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let stage = NativeStage::leaf_forest(&rom).unwrap();
    let mut enemies = StageEnemies::load(&rom, 0).unwrap();
    let e = *enemies.placements().iter().find(|e| e.kind == 2).unwrap();
    let (cx, cy) = (e.x - 120, e.y - 80);
    let mut easy = StageEnemies::load(&rom, 0).unwrap();
    easy.set_easy(true);
    easy.admit(&stage, cx, cy).unwrap();
    assert!(easy.sprites().all(|(_, _, s)| s.animation != 482));
    enemies.admit(&stage, cx, cy).unwrap();
    let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
    let off = Hitbox {
        index: -1,
        rectangle: [0; 4],
    };
    let mut life = NativeLifecycle::new([0, 0], false);
    let (mut rings, mut score, mut lives) = (0, 0, 3);
    let mut c = EnemyContext {
        player: &mut p,
        body: [off; 2],
        lifecycle: &mut life,
        rings: &mut rings,
        score: &mut score,
        lives: &mut lives,
        timer_invincibility: 0,
    };
    let mut trace = vec![];
    for i in 0..600 {
        c.player.q_world_x = (e.x + if i < 140 { -35 } else { 35 }) * 256;
        c.player.q_world_y = (e.y + if i < 140 { 45 } else { -90 }) * 256;
        enemies.tick(&mut c, cx, cy).unwrap();
        let (x, y, s) = enemies
            .sprites()
            .find(|(_, _, s)| s.animation == 482)
            .unwrap();
        for v in [
            x - e.x,
            y - e.y,
            i32::from(s.variant),
            i32::from(s.render().unwrap().frame_index.unwrap()),
        ] {
            trace.extend(v.to_le_bytes());
        }
    }
    assert_eq!(
        gba::sha256(&trace),
        "14701a9ae406c4ab1eafbce68c70ff54d76067b6e65a04e2b561e5d96cd89bcc"
    );
}
