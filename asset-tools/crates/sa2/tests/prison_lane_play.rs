//! Prison Lane (Hero story, Tails' walker) played by the headless autopilot
//! on the installed Steam data. The autopilot only produces stick/jump
//! input; the session moves the body.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::{MoverProgram, PRISON_LANE as LAYOUT},
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
        module: fs::read(gd.join(LAYOUT.module_file)).unwrap(),
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
    let stage = campaign.mount("Hero Story Sequence", 9, load).unwrap();
    let s = Session::prison_lane(
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
    let p = MoverProgram::verify_layout(&LAYOUT, &i.module).unwrap();
    assert_eq!(
        (p.lift_rise_base, p.lift_speed, p.lift_stand_frames),
        (20.0, 0.5, 30)
    );
    assert_eq!(
        (
            p.elevator_speed,
            p.elevator_door_step,
            p.elevator_door_open,
            p.elevator_door_b_initial,
            p.elevator_door_b_open,
            p.elevator_door_b_close_step
        ),
        (0.6, 0.2, -9.5, 0.0, -36.3, 0.32)
    );
    assert_eq!(
        (p.bridge_extend_x, p.bridge_speed, p.bridge_elevator_range),
        (-140.0, 1.5, 450.0)
    );
    // Iron Gate's module is not accepted as Prison Lane's.
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let other = fs::read(game.join("resource/gd_PC/stg11D.rel")).unwrap();
    assert!(MoverProgram::verify_layout(&LAYOUT, &other).is_err());
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
            (15, 2, 2)
        );
        assert_eq!(s.checkpoints.len(), 3);
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [-4439.9966, 967.0, -1133.4259]);
        assert_eq!(s.unimplemented.get("PANEL (solid only)"), Some(&17));
        assert_eq!(s.unimplemented.get("CCUBE (solid only)"), Some(&17));
        assert!(s.death_zone_count() > 0);
        assert_eq!((s.level, s.character, s.physics.slot), (9, 6, 6));
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn spawn_to_goal_clears_the_mission() {
    with_session(|mut s| {
        let events = run(&mut s, 15_000);
        assert_eq!(s.outcome, Outcome::Cleared, "{events:?}");
        let first =
            |want: &dyn Fn(&Event) -> bool| events.iter().find(|(_, e)| want(e)).map(|(f, _)| *f);
        let cp = |index: usize| {
            first(&|e| matches!(e, Event::Checkpoint { index: i, .. } if *i == index))
                .unwrap_or_else(|| panic!("checkpoint {index}: {events:?}"))
        };
        let (first_cp, second_cp, last_cp) = (cp(197), cp(360), cp(0));
        let elevator =
            first(&|e| matches!(e, Event::ElevatorStarted { mover: 17 })).expect("elevator");
        let goal = first(&|e| matches!(e, Event::GoalContact { sound: 0x1013, .. })).expect("goal");
        assert!(
            first_cp < elevator && elevator < second_cp && second_cp < last_cp && last_cp < goal,
            "{events:?}"
        );
        assert_eq!(s.slot_words[0], 2);
        assert_eq!(s.deaths, 0);
    });
}
