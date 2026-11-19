use sonic_advance_assets::{
    binary,
    entities::{EntityClass, EntityLayout, StageEntities},
    player::{Player, SineTable},
    springs::{SpringContext, StageSprings},
    sprite::{AnimationSource, Program, SpriteBank},
    Error, Result,
};

fn springs(kind: u8, param: u8, custom: u8) -> StageSprings {
    let mut raw = Vec::new();
    for word in [24u32 << 8, 1, 1, 12] {
        raw.extend(word.to_le_bytes());
    }
    raw.extend([10, 10, kind, param, custom, 0, 0, 255]);
    let l = EntityLayout::parse(&raw, EntityClass::Interactable).unwrap();
    StageSprings::new(&StageEntities {
        interactables: l.clone(),
        item_boxes: l.clone(),
        enemies: l,
    })
}
fn context() -> SpringContext {
    SpringContext {
        camera: (0, 0),
        min_y: 0,
        max_y: 1024,
        ia_override: false,
    }
}
fn player_at(x: i32, y: i32, vx: i32, vy: i32, grounded: bool) -> Player {
    let mut sine = [0; 256];
    sine[64] = 256;
    sine[192] = -256;
    let mut p = Player::at_pixels(x, y, SineTable::from_samples(sine).unwrap()).unwrap();
    p.vx = vx;
    p.vy = vy;
    p.ground_speed = vx;
    p.grounded = grounded;
    p
}
struct Stimulus(Vec<u8>);
impl Stimulus {
    fn hitboxes(kind: u8) -> Self {
        let rect = if kind == 9 {
            [-10i8, -6, 10, 8]
        } else {
            [-10, -8, 10, 6]
        };
        let packed = |r: [i8; 4]| i32::from_le_bytes(r.map(|b| b as u8));
        Self(
            [
                -6,
                0,
                packed(rect),
                -6,
                1,
                packed([-22, -10, 22, 10]),
                1,
                0,
                -4,
            ]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect(),
        )
    }
}
impl AnimationSource for Stimulus {
    fn program(&self, _: u16, _: u8) -> Result<Program<'_>> {
        Ok(Program {
            bytes: &self.0,
            entry_words: 0,
        })
    }
    fn tiles(&self, _: u32, _: usize, _: bool) -> Result<&[u8]> {
        Err(Error("unexpected tile request".into()))
    }
    fn palette(&self, _: u32, _: usize) -> Result<&[u8]> {
        Ok(&[0; 32])
    }
}

#[test]
fn retail_thumb_oracle_supplies_all_eight_launch_vectors() {
    // Independently executed ASOE Thumb routines. Synthetic OBJ hitboxes are
    // identical in world orientation for all eight cases (see evidence).
    let cases = [
        (8, 80, 65, 0, 256, false, 20480, 14848, 0, -2688, 0, 1),
        (9, 80, 90, 0, -256, false, 20480, 25600, 0, 2688, 0, 1),
        (10, 60, 80, 256, 0, true, 15104, 20480, -2688, 0, -2688, 3),
        (11, 100, 80, -256, 0, true, 26112, 20480, 2688, 0, 2688, 3),
        (12, 55, 80, 0, 0, false, 14080, 20480, -2688, -2688, 0, 5),
        (13, 105, 80, 0, 0, false, 26880, 20480, 2688, -2688, 0, 5),
        (14, 55, 80, 0, 0, false, 14080, 20480, -2688, -2688, 0, 7),
        (15, 105, 80, 0, 0, false, 26880, 20480, 2688, -2688, 0, 7),
    ];
    for (kind, x, y, vx, vy, grounded, ex, ey, evx, evy, egs, variant) in cases {
        let mut world = springs(kind, 2, 100);
        let mut p = player_at(x, y, vx, vy, grounded);
        let events = world
            .tick(&Stimulus::hitboxes(kind), &mut p, context())
            .unwrap();
        assert_eq!(events.len(), 1, "kind {kind}");
        assert_eq!(events[0].sound, 170);
        assert_eq!(
            (p.x, p.y, p.vx, p.vy, p.ground_speed),
            (ex, ey, evx, evy, egs),
            "kind {kind}"
        );
        let s = world.visible_sprites().next().unwrap().2;
        assert_eq!((s.animation, s.variant), (455, variant));
    }
}

#[test]
fn signed_parameter_and_custom_speed_override_and_strict_diagonal_edge() {
    for (first, second, speed, state) in [
        (0, 0, 1920, 17),
        (1, 0, 2304, 21),
        (2, 0, 2688, 17),
        (3, 0, 3072, 21),
        (4, 255, 4080, 17),
        (255, 1, 3072, 21),
    ] {
        let mut world = springs(8, first, second);
        let mut p = player_at(80, 65, 0, 256, false);
        let mut c = context();
        c.ia_override = true;
        assert!(world
            .tick(&Stimulus::hitboxes(8), &mut p, c)
            .unwrap()
            .is_empty());
        let event = world
            .tick(&Stimulus::hitboxes(8), &mut p, context())
            .unwrap()[0];
        assert_eq!((p.vy, event.character_state), (-speed, Some(state)));
    }
    let mut world = springs(12, 2, 0);
    let mut p = player_at(60, 80, 0, 0, false);
    assert!(world
        .tick(&Stimulus::hitboxes(12), &mut p, context())
        .unwrap()
        .is_empty());
    let mut world = springs(12, 2, 0);
    let mut p = player_at(59, 80, 0, 0, false);
    assert_eq!(
        world
            .tick(&Stimulus::hitboxes(12), &mut p, context())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn malformed_source_or_collision_bounds_return_errors() {
    let mut p = player_at(80, 65, 0, 256, false);
    let mut world = springs(8, 2, 0);
    assert!(world
        .tick(&Stimulus(vec![0; 3]), &mut p, context())
        .is_err());
    let mut c = context();
    c.max_y = c.min_y;
    assert!(world.tick(&Stimulus::hitboxes(8), &mut p, c).is_err());
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn retail_spring_shapes_colors_and_compressed_animation_return_to_idle() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    for (kind, param) in [
        (8, 0),
        (8, 2),
        (9, 2),
        (10, 2),
        (11, 2),
        (12, 2),
        (13, 2),
        (14, 2),
        (15, 2),
    ] {
        let mut world = springs(kind, param, 0);
        let mut p = player_at(0, 0, 0, 0, false);
        world.tick(&bank, &mut p, context()).unwrap();
        let s = world.visible_sprites().next().unwrap().2;
        let image = bank.render(s).unwrap().unwrap();
        let hash = binary::sha256(
            &image
                .image
                .pixels
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
        );
        let (expected_hash, x, y, vx, vy, expected_x, expected_y) = match (kind, param) {
            (8, 0) => (
                "148eceb0645b90ac245d5509fd4e9d568246e434b2b6144a8ef3146615761107",
                80,
                49,
                0,
                256,
                20480,
                12544,
            ),
            (8, 2) => (
                "ca9b16bea042e8040838291740851f694216c6f856612ef50b960913b7a7510b",
                80,
                49,
                0,
                256,
                20480,
                12544,
            ),
            (9, _) => (
                "49045eaa4ca474a4c3c24132f090635ae042c5ce330cb31fb1fe59ee724c482a",
                80,
                111,
                0,
                -256,
                20480,
                28416,
            ),
            (10, _) => (
                "09220c8f6f5201c66ba523bfb965b48a2513c967e1e6c0c4e3a15d9ba8056bb2",
                54,
                65,
                256,
                0,
                13568,
                16640,
            ),
            (11, _) => (
                "498caf2910ead9aa86411f207cbe00d4c3d0fa40a57c35567359ae0e80780ba3",
                106,
                65,
                -256,
                0,
                27648,
                16640,
            ),
            (12, _) => (
                "1c5eced5aab9598fb217253d7a6b34136f5cde8a541f45127a583baf1a51d57b",
                55,
                60,
                0,
                0,
                14080,
                15360,
            ),
            (13, _) => (
                "cbd47c9221a640111c9e05c1899f5700189d12c2f72cdc06a8fb329dfae7271a",
                105,
                60,
                0,
                0,
                26880,
                15360,
            ),
            (14, _) => (
                "07601cda3319de4e61f34a57bdb8c844fa528da116b1b13775f109102c9a027c",
                55,
                62,
                0,
                0,
                14080,
                15872,
            ),
            (15, _) => (
                "c5d0023addf50146bddd3827eae4f8d7cbf027c4e119a415cb76788c26a8df44",
                105,
                62,
                0,
                0,
                26880,
                15872,
            ),
            _ => unreachable!(),
        };
        assert_eq!(hash, expected_hash);
        let mut p = player_at(x, y, vx, vy, matches!(kind, 10 | 11));
        assert_eq!(
            world.tick(&bank, &mut p, context()).unwrap().len(),
            1,
            "kind {kind}"
        );
        assert_eq!((p.x, p.y), (expected_x, expected_y));
        let speed = if param == 0 { 1920 } else { 2688 };
        let launch_x = match kind {
            10 | 12 | 14 => -speed,
            11 | 13 | 15 => speed,
            _ => 0,
        };
        let launch_y = match kind {
            9 => speed,
            10 | 11 => 0,
            _ => -speed,
        };
        assert_eq!((p.vx, p.vy), (launch_x, launch_y));
        p.x = 0;
        p.y = 0;
        for _ in 0..120 {
            world.tick(&bank, &mut p, context()).unwrap();
        }
        assert_eq!(
            world.visible_sprites().next().unwrap().2.variant,
            (kind - 8) & !1
        );
    }
}
