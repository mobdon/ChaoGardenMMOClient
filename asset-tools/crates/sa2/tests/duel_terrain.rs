use sa2_asset_tools::duel_platform::{self, FrameInput, Generator, GeneratorState, Spawn};
use std::{fs, path::PathBuf};

fn game() -> PathBuf {
    PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap())
}
fn synthetic() -> Generator {
    Generator {
        start_x: 0.0,
        step_x: 300.0,
        lookahead: 2100.0,
        timer_scale: 0.4,
        timer_floor: -3600.0,
        result_x: -300.0,
        result_timer: 0x8000,
        deadline_frames: 3,
        late_timer: 900,
        fall_after: 900,
        stand_timer: 720,
        collision_radius_pad: 15.0,
        collision_flags: 0x2800_0001,
        collision_radius: 173.29166,
    }
}
fn frame(a: Option<f32>, b: Option<f32>) -> FrameInput {
    FrameInput {
        players: [a, b],
        ..Default::default()
    }
}

#[test]
fn generator_follows_players_and_deadline() {
    let g = synthetic();
    let mut s = GeneratorState::default();
    assert!(g.step(&mut s, &frame(Some(0.0), None)).unwrap().is_empty());
    assert_eq!((s.mode, s.cursor_x, s.frames), (1, 0.0, 1));
    let first = g.step(&mut s, &frame(Some(0.0), Some(0.0))).unwrap();
    assert_eq!(first.len(), 7);
    assert_eq!(first[6].x, 1800.0);
    assert_eq!(first[1].timer, (-120i16) as u16);
    // A player far ahead pulls the cursor forward.
    let ahead = g.step(&mut s, &frame(Some(5000.0), None)).unwrap();
    assert_eq!(ahead.first().unwrap().x, 2100.0);
    assert_eq!(ahead.last().unwrap().x, 6900.0);
    assert_eq!(ahead[0].timer, 1160);
    assert_eq!((s.frames, s.flags), (3, 0));
    // The counter passes three after this frame's spawning; later platforms
    // are created already activated with timer 900.
    assert!(g
        .step(&mut s, &frame(Some(5000.0), None))
        .unwrap()
        .is_empty());
    assert_eq!((s.frames, s.flags), (3, 0x1000));
    let late = g.step(&mut s, &frame(Some(5300.0), None)).unwrap();
    assert_eq!(
        late,
        vec![Spawn {
            x: 7200.0,
            timer: 900,
            activated: true
        }]
    );
}

#[test]
fn result_mode_spawns_one_platform_and_pause_holds_counter() {
    let g = synthetic();
    let mut s = GeneratorState::default();
    let paused = FrameInput {
        paused: true,
        ..frame(None, None)
    };
    g.step(&mut s, &paused).unwrap();
    assert_eq!(s.frames, 0);
    let result = FrameInput {
        result_mode: true,
        ..frame(Some(0.0), Some(0.0))
    };
    let r = g.step(&mut s, &result).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].x, r[0].timer, r[0].activated),
        (-300.0, 0x8000, false)
    );
    assert!(g.step(&mut s, &frame(Some(9e3), None)).unwrap().is_empty());
    assert_eq!(s.mode, 2);
}

#[test]
fn nan_player_spawns_nothing() {
    let g = synthetic();
    let mut s = GeneratorState::default();
    g.step(&mut s, &frame(None, None)).unwrap();
    assert!(g
        .step(&mut s, &frame(Some(f32::NAN), None))
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "requires installed stg42D.rel and sonic2app.exe"]
fn installed_template_matches_executable_platform() {
    let gd = game().join("resource/gd_PC");
    let rel = fs::read(gd.join("stg42D.rel")).unwrap();
    let exe = fs::read(game().join("sonic2app.exe")).unwrap();
    let a = duel_platform::read_rel(&rel).unwrap();
    let b = duel_platform::read(&exe).unwrap();
    assert_eq!(
        (a.node.flags, a.node.rotation_bams, a.node.scale),
        (b.node.flags, b.node.rotation_bams, b.node.scale)
    );
    let (a, b) = (a.attachment, b.attachment);
    assert_eq!(a.vertices, b.vertices);
    assert_eq!(a.normals, b.normals);
    assert_eq!(
        (a.bounds_center, a.bounds_radius),
        (b.bounds_center, b.bounds_radius)
    );
    assert_eq!(a.material_words, b.material_words);
    assert_eq!(a.meshsets.len(), b.meshsets.len());
    for (x, y) in a.meshsets.iter().zip(&b.meshsets) {
        assert_eq!(
            (x.material_id, x.polygon_type, x.attributes),
            (y.material_id, y.polygon_type, y.attributes)
        );
        let p = |m: &sa2_asset_tools::stage::BasicMeshset| {
            m.polygons
                .iter()
                .map(|p| (p.reversed, p.indices.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(p(x), p(y));
    }

    let g = duel_platform::generator(&rel).unwrap();
    assert_eq!(
        (g.start_x, g.step_x, g.lookahead, g.timer_floor, g.result_x),
        (0.0, 300.0, 2100.0, -3600.0, -300.0)
    );
    assert_eq!(g.timer_scale.to_bits(), 0x3ecc_cccd);
    assert_eq!(
        (g.deadline_frames, g.fall_after, g.stand_timer),
        (36000, 900, 720)
    );
    assert_eq!(g.collision_flags, 0x2800_0001);
}

#[test]
fn platform_activates_then_falls_after_the_stand_timer() {
    use sa2_asset_tools::duel_platform_motion::{
        Call, MotionConstants, Platform, PlatformFrame, PlatformWork,
    };
    let g = synthetic();
    let c = MotionConstants {
        visibility_pad: 16.0,
        debris: [150.0, 50.0, 7.0, 0.5, 1.0],
        rumble_strength: 2.0,
        fall_velocity: -0.15,
        floor_y: -100.0,
        terminal_velocity: -2.1,
        acceleration: 0.1,
        rand_scale: 1.0 / 32768.0,
        spin_scale: 64.0,
        roll_scale: 16.0,
        roll_offset: 32.0,
    };
    let p = Platform {
        generator: &g,
        constants: &c,
        has_task_field_30: false,
    };
    let mut w = PlatformWork::spawned(&Spawn {
        x: 0.0,
        timer: 0,
        activated: false,
    });
    let mut rand = [16384, 0, 32767].into_iter();
    let idle = PlatformFrame::default();
    let calls = p.step(&mut w, &idle, &mut rand).unwrap();
    assert!(matches!(calls[0], Call::Init { .. }));
    assert_eq!((w.mode, w.timer), (1, 0));
    let stand = PlatformFrame {
        standing: true,
        ..Default::default()
    };
    p.step(&mut w, &stand, &mut rand).unwrap();
    assert_eq!((w.activated, w.timer), (1, 721));
    let mut frames = 1;
    while w.mode == 1 {
        p.step(&mut w, &idle, &mut rand).unwrap();
        frames += 1;
    }
    // 180 more increments reach 901; the next update starts the fall.
    assert_eq!((frames, w.velocity()), (182, -0.15));
    while w.position[1] >= -100.0 {
        p.step(&mut w, &idle, &mut rand).unwrap();
    }
    // Below the floor: accelerate once and draw the spin from three rand values.
    assert_eq!(
        w.motion[1..4],
        [
            (24 - 32) as u32,
            32,
            (16.0f32 * (32767.0 / 32768.0) - 32.0) as i32 as u32
        ]
    );
    assert_eq!(w.motion[8..11], w.motion[1..4]);
}
