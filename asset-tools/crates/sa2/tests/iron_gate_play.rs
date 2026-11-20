//! Iron Gate's native object rules on the installed Steam data, driven by
//! scripted player samples in place of the engine's player code.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::MoverProgram,
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

fn first<F: Fn(&MoverState) -> bool>(s: &Session, f: F) -> usize {
    s.movers.iter().position(|m| f(&m.state)).unwrap()
}

fn standing(m: usize) -> Option<PlayerSample> {
    Some(PlayerSample {
        standing_on: Some(m),
        ..Default::default()
    })
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn installed_rules_and_inventory() {
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
        assert_eq!(s.checkpoints.len(), 3);
        assert_eq!(s.rings_placed.len(), 10);
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [6231.24, -1488.0, -1229.35]);
        assert!(s.death_zone_count() > 0);
        assert_eq!(s.physics_slot, 7);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn standing_samples_drive_lift_elevator_and_bridge() {
    with_session(|mut s| {
        // The lift at the raised corridor's entrance.
        let lift = first(
            &s,
            |m| matches!(m, MoverState::Lift(l) if l.base == [800.0, -270.0, -638.0]),
        );
        let start = (1..=40)
            .find(|_| {
                s.step(standing(lift))
                    .contains(&Event::LiftStarted { mover: lift })
            })
            .unwrap();
        assert_eq!(start, 31);
        while matches!(&s.movers[lift].state, MoverState::Lift(l) if l.moving) {
            s.step(standing(lift));
        }
        assert!(
            matches!(&s.movers[lift].state, MoverState::Lift(l) if l.raised && l.position[1] == -200.0)
        );

        // The diagonal elevator, then the bridge that meets it at the bottom.
        let elv = first(
            &s,
            |m| matches!(m, MoverState::Elevator(e) if e.origin == [1576.0, -270.0, -1027.5]),
        );
        let bridge = first(
            &s,
            |m| matches!(m, MoverState::Bridge(b) if b.retracted == [2127.0, -669.5, -1027.0]),
        );
        assert!(s
            .step(standing(elv))
            .contains(&Event::ElevatorStarted { mover: elv }));
        let started = (0..3000).any(|_| {
            s.step(standing(elv))
                .contains(&Event::BridgeStarted { mover: bridge })
        });
        assert!(started);
        assert!(
            matches!(&s.movers[elv].state, MoverState::Elevator(e) if e.at_target && e.mode == 2)
        );
        for _ in 0..20 {
            s.step(None);
        }
        assert!(matches!(&s.movers[bridge].state, MoverState::Bridge(b) if b.is_extended));
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn ring_pickup_and_goal_contact() {
    with_session(|mut s| {
        let ring = s.rings_placed[0].position;
        let events = s.step(Some(PlayerSample {
            center: [ring[0] + 13.9, ring[1], ring[2]],
            ..Default::default()
        }));
        assert!(matches!(events[..], [Event::Ring { .. }]));
        assert_eq!((s.rings, s.score), (1, 10));
        assert!(s
            .step(Some(PlayerSample {
                center: ring,
                ..Default::default()
            }))
            .is_empty());

        let events = s.step(Some(PlayerSample {
            goal_hit: Some(0),
            ..Default::default()
        }));
        assert!(events.contains(&Event::GoalContact {
            layer: s.goals[0].layer,
            index: s.goals[0].index,
            sound: 0x1013
        }));
        assert_eq!(s.slot_words[0], 2);
        assert_eq!(s.outcome, Outcome::Cleared);
    });
}
