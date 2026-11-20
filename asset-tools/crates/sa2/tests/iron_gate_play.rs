//! Iron Gate played by the headless autopilot on the installed Steam data.
//! The autopilot only produces stick/jump input; the session moves the body.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::MoverProgram,
    play_autopilot::Autopilot,
    play_session::{Event, MoverState, Outcome, Session, Sources},
};
use std::{fs, path::PathBuf};

struct Installed {
    exe: Vec<u8>,
    module: Vec<u8>,
    campaign_inputs: [String; 3],
    gd: PathBuf,
}

fn installed() -> Installed {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").expect("SA2_PC_CONFIG"));
    let gd = game.join("resource/gd_PC");
    let read = |n: &str| fs::read_to_string(cfg.join(n)).unwrap();
    Installed {
        exe: fs::read(game.join("sonic2app.exe")).unwrap(),
        module: fs::read(gd.join("stg11D.rel")).unwrap(),
        campaign_inputs: [
            read("ADVERTISE.ini"),
            read("sa2lvl.ini"),
            read("ObjectLists.ini"),
        ],
        gd,
    }
}

fn with_session(f: impl FnOnce(Session)) {
    let i = installed();
    let [adv, lvl, obj] = &i.campaign_inputs;
    let campaign = Campaign::new(&i.exe, adv, lvl, obj).unwrap();
    let load = |n: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let p = i.gd.join(n);
        Ok(p.is_file().then(|| fs::read(p).unwrap()))
    };
    let stage = campaign.mount("Dark Story Sequence", 1, load).unwrap();
    let s = Session::iron_gate(
        &stage,
        Sources {
            exe: &i.exe,
            stage_module: &i.module,
        },
    )
    .unwrap();
    f(s)
}

fn run(s: &mut Session, frames: u32) -> Vec<(u32, Event)> {
    let mut pilot = Autopilot::new(s);
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
fn installed_mover_rules_and_session_inventory() {
    let i = installed();
    let p = MoverProgram::verify(&i.module).unwrap();
    assert_eq!(
        (
            p.lift_rise_base,
            p.lift_speed,
            p.lift_stand_frames,
            p.lift_return_frames
        ),
        (20.0, 0.5, 30, 90)
    );
    assert_eq!(
        (p.elevator_speed, p.elevator_door_step, p.elevator_door_open),
        (0.3, 0.2, -9.5)
    );
    assert_eq!(
        (p.bridge_extend_x, p.bridge_speed, p.bridge_elevator_range),
        (-140.0, 8.0, 450.0)
    );
    with_session(|s| {
        let count = |k: &str| {
            s.movers
                .iter()
                .filter(|m| {
                    matches!(
                        (&m.state, k),
                        (MoverState::Lift(_), "lift")
                            | (MoverState::Elevator(_), "elevator")
                            | (MoverState::Bridge(_), "bridge")
                    )
                })
                .count()
        };
        assert_eq!(
            (count("lift"), count("elevator"), count("bridge")),
            (5, 3, 2)
        );
        assert_eq!(s.springs.len(), 6);
        assert_eq!(s.dash_panels.len(), 12);
        assert_eq!(s.checkpoints.len(), 3);
        assert_eq!(s.rings_placed.len(), 10);
        // Only the regular goal reaches its contact route for Mission 1.
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [6231.24, -1488.0, -1229.35]);
        assert!(s.death_zone_count() > 0);
        assert_eq!(s.physics.slot, 7);
        assert!((s.jump_height() - 56.52).abs() < 0.01);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn spawn_through_first_lift_elevator_and_bridge() {
    with_session(|mut s| {
        let events = run(&mut s, 4_500);
        let first =
            |want: &dyn Fn(&Event) -> bool| events.iter().find(|(_, e)| want(e)).map(|(f, _)| *f);
        let lift = first(&|e| matches!(e, Event::LiftStarted { mover: 9 })).expect("lift");
        let elevator =
            first(&|e| matches!(e, Event::ElevatorStarted { mover: 6 })).expect("elevator");
        let bridge = first(&|e| matches!(e, Event::BridgeStarted { mover: 2 })).expect("bridge");
        let checkpoint = first(&|e| matches!(e, Event::Checkpoint { index: 305, .. }))
            .expect("checkpoint after bridge");
        assert!(
            lift < elevator && elevator < bridge && bridge < checkpoint,
            "{events:?}"
        );
        assert!(!events
            .iter()
            .any(|(f, e)| matches!(e, Event::Death { .. }) && *f < checkpoint));
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn last_checkpoint_to_goal_clears_the_mission() {
    with_session(|mut s| {
        s.restart_from_checkpoint(1).unwrap();
        assert_eq!(s.checkpoints[1].position, [3690.9966, -903.0, -684.9978]);
        let events = run(&mut s, 6_000);
        assert_eq!(s.outcome, Outcome::Cleared, "{events:?}");
        assert!(events
            .iter()
            .any(|(_, e)| matches!(e, Event::ElevatorStarted { mover: 4 })));
        assert!(events
            .iter()
            .any(|(_, e)| matches!(e, Event::GoalContact { sound: 0x1013, .. })));
        assert_eq!(s.slot_words[0], 2);
        assert_eq!(s.deaths, 0);
    });
}
