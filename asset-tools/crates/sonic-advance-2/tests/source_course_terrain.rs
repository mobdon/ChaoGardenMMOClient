use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_game::NativeHarness,
    native_stage::{NativeStage, TERRAIN_LEVELS, is_course_act},
};

#[derive(Deserialize)]
struct StageReport {
    reference_commit: String,
    reference_bank_count: usize,
    stages: Vec<Stage>,
}
#[derive(Deserialize)]
struct Stage {
    level: u8,
    dimensions: (i32, i32),
    spawn: (i32, i32),
    scenes: Vec<(i32, i32, u64, String)>,
    probes: Vec<(i32, i32, usize, usize, u64, String)>,
}

#[test]
#[ignore = "requires verified local A2NE_ROM"]
fn all_stage_headers_and_independent_foreground_renders_match() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let expected: StageReport =
        serde_json::from_str(include_str!("../catalog/native-course-terrain.json")).unwrap();
    assert_eq!(
        expected.reference_commit,
        sonic_advance_2_assets::REFERENCE_COMMIT
    );
    assert_eq!(expected.reference_bank_count, 150);
    assert_eq!(expected.stages.len(), usize::from(TERRAIN_LEVELS));
    for (id, report) in expected.stages.into_iter().enumerate() {
        assert_eq!(usize::from(report.level), id);
        let stage = NativeStage::for_level(&rom, report.level).unwrap();
        assert_eq!(stage.dimensions(), report.dimensions);
        assert_eq!(stage.spawn, report.spawn);
        for (x, y, w, h, tick, hash) in report.probes {
            assert_eq!(
                gba::sha256(&stage.render_view_at(x, y, w, h, tick).unwrap()),
                hash,
                "level {id} ({x}, {y}) tick {tick}"
            );
        }
        for (x, y, tick, expected) in report.scenes {
            assert_eq!(
                gba::sha256(&stage.render_scene_at([x, y], [240, 160], tick).unwrap()),
                expected,
                "Leaf Forest level {id} parallax ({x},{y}) tick {tick}"
            );
        }
        if is_course_act(report.level) {
            let mut game = NativeHarness::for_level(&rom, report.level).unwrap();
            assert_eq!(game.snapshot().player.q_world_x, report.spawn.0 * 256);
            assert_eq!(game.snapshot().player.q_world_y, report.spawn.1 * 256);
            for _ in 0..12 {
                let state = game.tick(16).unwrap();
                assert!(state.halted.is_none(), "level {id}: {:?}", state.halted);
            }
            assert_eq!(game.render().unwrap().len(), 240 * 160 * 4);
        } else {
            assert!(NativeHarness::for_level(&rom, report.level).is_err());
        }
    }
    assert!(NativeStage::for_level(&rom, TERRAIN_LEVELS).is_err());
}
