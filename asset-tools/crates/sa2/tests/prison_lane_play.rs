//! Prison Lane's native stage rules on the installed Steam data. The
//! session runs only rules read from the game files; these tests supply the
//! player sample the engine would report and check the native responses.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::{MoverProgram, PRISON_LANE as LAYOUT},
    play_session::{Event, MoverState, Outcome, PlayerSample, Session, Sources},
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
        // 15 of the 17 RING_LINEAR lines are exact (flat, Y rotation only).
        assert_eq!(s.rings_placed.len(), 56);
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [-4439.9966, 967.0, -1133.4259]);
        assert_eq!(s.unimplemented.get("PANEL (solid only)"), Some(&17));
        assert_eq!(s.unimplemented.get("FENCES row (solid only)"), Some(&20));
        assert_eq!(s.unimplemented.get("FENCEL row (solid only)"), Some(&1));
        // CONTIRON: a 20-unit cube row, unchanged by quarter turns.
        assert_eq!(s.unimplemented.get("CONTIRON row (solid only)"), Some(&11));
        assert_eq!(s.unimplemented.get("CCUBE (solid only)"), Some(&9));
        assert_eq!(
            s.unimplemented
                .get("CCUBE (rotated: rotation rule not read)"),
            Some(&8)
        );
        assert_eq!(
            s.unimplemented
                .get("RING_LINEAR (arched or tilted: trig/axis calls not read)"),
            Some(&2)
        );
        assert!(s.death_zone_count() > 0);
        assert_eq!((s.level, s.character, s.physics.slot), (9, 6, 6));
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn standing_on_the_one_way_elevator_carries_it_to_its_target() {
    with_session(|mut s| {
        let i = s
            .movers
            .iter()
            .position(|m| m.index == 432)
            .expect("ELV 432");
        let MoverState::Elevator(e) = &s.movers[i].state else {
            panic!("not an elevator");
        };
        let target = e.target;
        let stand = PlayerSample {
            center: s.movers[i].position(),
            standing_on: Some(i),
        };
        let mut started = None;
        let mut bridge = None;
        for _ in 0..2_000 {
            for e in s.step(Some(stand)) {
                match e {
                    Event::ElevatorStarted { mover } if mover == i => started = Some(s.frame),
                    Event::BridgeStarted { .. } if started.is_some() => bridge = Some(s.frame),
                    _ => {}
                }
            }
        }
        // The outer door starts shut and opens at 0.2 per frame: 182 frames
        // before the occupancy test can start the car.
        assert_eq!(started, Some(183));
        let MoverState::Elevator(e) = &s.movers[i].state else {
            unreachable!()
        };
        assert_eq!((e.position, e.mode, e.at_target), (target, 2, true));
        // The bridge beside the departure point extended toward the resting
        // car on the first frame; with its 120-frame cooldown not yet met
        // when the car left, it stays out.
        assert!(bridge.is_none());
        let MoverState::Bridge(b) = &s.movers[2].state else {
            panic!("mover 2 is not a bridge");
        };
        assert!(b.is_extended && !b.moving);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn goal_contact_and_ring_pickup_follow_the_native_rules() {
    with_session(|mut s| {
        let ring = s.rings_placed[0].position;
        let events = s.step(Some(PlayerSample {
            center: ring,
            standing_on: None,
        }));
        // Strictly within 14 of the centre; two lines overlap here, 10 apart.
        let taken = events
            .iter()
            .filter(|e| matches!(e, Event::Ring { .. }))
            .count() as u32;
        assert!(taken >= 1, "{events:?}");
        assert_eq!((s.rings, s.score), (taken, taken * 10));
        let goal = s.goals[0].position;
        let events = s.step(Some(PlayerSample {
            center: goal,
            standing_on: None,
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::GoalContact { sound: 0x1013, .. })));
        assert_eq!(s.slot_words[0], 2);
        assert_eq!(s.outcome, Outcome::Cleared);
    });
}
