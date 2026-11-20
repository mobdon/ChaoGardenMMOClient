//! Iron Gate's native stage rules on the installed Steam data. The session
//! runs only rules read from the game files; these tests supply the player
//! sample the engine would report and check the native responses.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::MoverProgram,
    play_session::{Event, MoverState, PlayerSample, Session, Sources},
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
        // Every spring has an X or Z angle that is not a quarter turn, so
        // its launch vector would need the unread trig table.
        assert_eq!(s.springs.len(), 0);
        assert_eq!(s.checkpoints.len(), 3);
        assert_eq!(s.rings_placed.len(), 10);
        // Only the regular goal reaches its contact route for Mission 1.
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [6231.24, -1488.0, -1229.35]);
        // CONTIRON CollisionData (stg11D.rel sec5+0x656D8): a 20-unit cube.
        assert_eq!(s.unimplemented.get("CONTIRON row (solid only)"), Some(&7));
        assert_eq!(s.unimplemented.get("CONTCHAO row (solid only)"), Some(&3));
        assert_eq!(s.unimplemented.get("CONTWOOD row (solid only)"), Some(&27));
        assert_eq!(s.unimplemented.get("MECHA row (solid only)"), Some(&4));
        assert!(s.death_zone_count() > 0);
        assert_eq!(s.physics.slot, 7);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn standing_on_a_lift_raises_it_by_its_native_rule() {
    with_session(|mut s| {
        let i = 9;
        let MoverState::Lift(l) = &s.movers[i].state else {
            panic!("mover 9 is not a lift");
        };
        let top = l.top;
        let stand = PlayerSample {
            center: s.movers[i].position(),
            standing_on: Some(i),
            touching_goal: None,
        };
        let mut started = None;
        for _ in 0..400 {
            if s.step(Some(stand))
                .iter()
                .any(|e| matches!(e, Event::LiftStarted { mover } if *mover == i))
            {
                started.get_or_insert(s.frame);
            }
        }
        // Rises after 30 standing frames (the 31st), at 0.5 per frame.
        assert_eq!(started, Some(31));
        let MoverState::Lift(l) = &s.movers[i].state else {
            unreachable!()
        };
        assert!(l.raised && l.position == top);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn goal_contact_writes_the_slot_word() {
    with_session(|mut s| {
        let goal = s.goals[0].position;
        let events = s.step(Some(PlayerSample {
            center: goal,
            standing_on: None,
            // The engine reports the contact; its test is main-program code.
            touching_goal: Some(0),
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::GoalContact { sound: 0x1013, .. })));
        assert_eq!(s.slot_words[0], 2);
    });
}
