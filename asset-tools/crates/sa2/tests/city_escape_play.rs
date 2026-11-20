//! City Escape with the stand-in Sonic controller on the installed Steam data.
//! The autopilot only produces stick and button input; the session moves the
//! body. Run with `--ignored` and SA2_GAME_PATH and SA2_PC_CONFIG set.
use sa2_asset_tools::{
    fan_stage::Campaign,
    speed_autopilot::SpeedAutopilot,
    speed_controller::Action,
    speed_session::{Event, Outcome, Sources, SpeedSession, MODULE_FILE},
};
use std::{fs, path::PathBuf};

fn with_session(f: impl FnOnce(SpeedSession)) {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").expect("SA2_PC_CONFIG"));
    let gd = game.join("resource/gd_PC");
    let read = |n: &str| fs::read_to_string(cfg.join(n)).unwrap();
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let module = fs::read(gd.join(MODULE_FILE)).unwrap();
    let campaign = Campaign::new(
        &exe,
        &read("ADVERTISE.ini"),
        &read("sa2lvl.ini"),
        &read("ObjectLists.ini"),
    )
    .unwrap();
    let load = |n: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let p = gd.join(n);
        Ok(p.is_file().then(|| fs::read(p).unwrap()))
    };
    let stage = campaign.mount("Hero Story Sequence", 1, load).unwrap();
    let s = SpeedSession::city_escape(
        &stage,
        Sources {
            exe: &exe,
            stage_module: &module,
        },
    )
    .unwrap();
    f(s)
}

fn run(s: &mut SpeedSession, frames: u32) -> Vec<(u32, Event)> {
    let mut pilot = SpeedAutopilot::new(s);
    let mut out = vec![];
    while s.frame < frames && s.outcome == Outcome::Playing && !pilot.gave_up {
        let c = pilot.controls(s);
        for e in s.step(c) {
            out.push((s.frame, e));
        }
    }
    out
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn installed_inventory_and_sonic_parameters() {
    with_session(|s| {
        assert_eq!((s.level, s.character), (13, 0));
        assert_eq!(s.start, [0.0, 300.0, 340.0]);
        // Sonic's PhysicsArray row 0.
        let p = &s.physics;
        assert_eq!((p.jump2_timer, p.jmp_y_spd, p.run_accel), (60, 1.66, 0.05));
        assert_eq!(
            (p.max_x_spd, p.dash_speed, p.weight, p.rad),
            (3.0, 5.09, 0.08, 4.0)
        );
        assert_eq!(s.rails.len(), 12);
        assert_eq!(s.checkpoints.len(), 3);
        assert_eq!(s.panels.len(), 82);
        assert_eq!(s.goals.len(), 1);
        let g = s.goals[0].position;
        assert!((g[0] - 3955.0).abs() < 0.1 && g[1] == -22139.0 && (g[2] - 16129.81).abs() < 0.1);
        assert!(s.death_zone_count() > 0);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps; takes minutes"]
fn opening_run_reaches_the_first_rails() {
    with_session(|mut s| {
        let events = run(&mut s, 60 * 120);
        let rails = events
            .iter()
            .filter(|(_, e)| {
                matches!(
                    e,
                    Event::Controller {
                        action: Action::RailOn { .. }
                    }
                )
            })
            .count();
        let panels = events
            .iter()
            .filter(|(_, e)| matches!(e, Event::DashPanel { .. }))
            .count();
        assert!(rails >= 1, "no rail ride");
        assert!(panels >= 1, "no dash panel");
        // Past the board-section jump and the first grind.
        assert!(s.player.position[2] > 9900.0, "{:?}", s.player.position);
        assert_eq!(s.deaths, 0);
    });
}
