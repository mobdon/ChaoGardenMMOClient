//! Level 42 checks. Every constant comes from the installed stg42D.rel.
use sa2_asset_tools::duel_platform::{self, FrameInput, Generator, GeneratorState, SpawnWork};
use sa2_asset_tools::duel_platform_motion::{
    self, Call, Platform, PlatformFrame, PlatformStart, PlatformWork,
};
use std::{fs, path::PathBuf};

fn game() -> PathBuf {
    PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap())
}
fn rel() -> Vec<u8> {
    fs::read(game().join("resource/gd_PC/stg42D.rel")).unwrap()
}
fn installed() -> Generator {
    duel_platform::generator(&rel()).unwrap()
}
fn frame(a: Option<f32>, b: Option<f32>) -> FrameInput {
    FrameInput {
        players: [a, b],
        ..Default::default()
    }
}

#[test]
#[ignore = "requires installed stg42D.rel"]
fn generator_follows_players_and_deadline() {
    let g = installed();
    let mut s = GeneratorState::default();
    assert!(g.step(&mut s, &frame(Some(0.0), None)).unwrap().is_empty());
    assert_eq!((s.mode, s.cursor_x, s.frames), (1, 0.0, 1));
    let first = g.step(&mut s, &frame(Some(0.0), Some(0.0))).unwrap();
    assert_eq!(first.len(), 7);
    assert_eq!(first[6].unwrap().x, 1800.0);
    assert_eq!(first[1].unwrap().timer, (-120i16) as u16);
    let ahead = g.step(&mut s, &frame(Some(5000.0), None)).unwrap();
    assert_eq!(
        (ahead[0].unwrap().x, ahead[0].unwrap().timer),
        (2100.0, 1160)
    );
    assert_eq!(ahead.last().unwrap().unwrap().x, 6900.0);
    // A null allocation leaves that platform unwritten; the cursor still advances.
    let skip = FrameInput {
        failed_allocations: vec![0],
        ..frame(Some(5300.0), None)
    };
    assert_eq!(g.step(&mut s, &skip).unwrap(), vec![None]);
    assert_eq!(s.cursor_x, 7500.0);
    while s.flags == 0 {
        assert!(g.step(&mut s, &frame(None, None)).unwrap().is_empty());
    }
    assert_eq!(s.frames, 36000);
    let late = g.step(&mut s, &frame(Some(5600.0), None)).unwrap();
    assert_eq!(
        late,
        vec![Some(SpawnWork {
            x: 7500.0,
            timer: 900,
            activated: true
        })]
    );
}

#[test]
#[ignore = "requires installed stg42D.rel"]
fn result_mode_spawns_one_platform_and_pause_holds_counter() {
    let g = installed();
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
    assert_eq!(
        r,
        vec![Some(SpawnWork {
            x: -300.0,
            timer: 0x8000,
            activated: false
        })]
    );
    assert!(g.step(&mut s, &frame(Some(9e3), None)).unwrap().is_empty());
    assert_eq!(s.mode, 2);
}

#[test]
#[ignore = "requires installed stg42D.rel and sonic2app.exe"]
fn installed_template_matches_executable_platform() {
    let rel = rel();
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
#[ignore = "requires installed stg42D.rel"]
fn platform_activates_then_falls_after_the_stand_timer() {
    let rel = rel();
    let g = installed();
    let c = duel_platform_motion::constants(&rel).unwrap();
    let p = Platform {
        generator: &g,
        constants: &c,
        has_task_field_30: false,
    };
    let mut w = PlatformWork::start(&PlatformStart {
        mode: 0,
        activated: 0,
        flags: 0,
        timer: 0,
        angles: [0; 3],
        position: [0.0; 3],
        motion: [0; 14],
    });
    let mut rand = [16384, 0, 32767].into_iter();
    let idle = PlatformFrame::default();
    // A failed model copy frees the object copy and retries next update.
    let fail = PlatformFrame {
        failed_allocations: vec![1],
        ..Default::default()
    };
    let calls = p.step(&mut w, &fail, &mut rand).unwrap();
    assert_eq!(calls.last(), Some(&Call::FreeObject));
    assert_eq!((w.mode, w.object_y), (0, None));
    let calls = p.step(&mut w, &idle, &mut rand).unwrap();
    assert!(matches!(calls[2], Call::Register { .. }));
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
    assert_eq!((frames, w.velocity()), (182, c.fall_velocity));
    while w.position[1] >= c.floor_y {
        p.step(&mut w, &idle, &mut rand).unwrap();
    }
    assert_eq!(w.motion[1..4], [(24 - 32) as u32, 32, 0xffff_fff0]);
    assert_eq!(w.motion[8..11], w.motion[1..4]);
}
