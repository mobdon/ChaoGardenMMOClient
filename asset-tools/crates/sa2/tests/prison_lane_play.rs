//! Prison Lane's native stage rules on the installed Steam data. The
//! session runs only rules read from the game files; these tests supply the
//! player sample the engine would report and check the native responses.
use sa2_asset_tools::{
    fan_stage::Campaign,
    iron_gate_movers::{MoverProgram, PRISON_LANE as LAYOUT},
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
        // 17 RING_LINEAR lines (njSin arch, njRotate Z/Y/X).
        assert_eq!(s.rings_placed.len(), 64);
        assert_eq!(s.springs.len(), 1);
        assert_eq!(s.goals.len(), 1);
        assert_eq!(s.goals[0].position, [-4439.9966, 967.0, -1133.4259]);
        assert_eq!(s.unimplemented.get("PANEL (solid only)"), Some(&17));
        // CollisionData boxes placed by the CCL box rule.
        for (name, n) in [
            ("FENCES row (solid only)", 39),
            ("FENCEL row (solid only)", 12),
            ("ROLL row (solid only)", 4),
            ("SPHERE (solid only)", 4),
            ("CCYL (solid only)", 1),
            ("TANK row (solid only)", 1),
            ("CONTIRON row (solid only)", 18),
            ("CONTCHAO row (solid only)", 3),
            ("CONTWOOD row (solid only)", 17),
            ("MECHA row (solid only)", 12),
            ("CCUBE (solid only)", 17),
        ] {
            assert_eq!(s.unimplemented.get(name), Some(&n), "{name}");
        }
        // 8 fence rails, 4 gears, 4 spheres, 1 cylinder, the tank and the
        // 7 sirens' and 34 fluorescent lights' radius-5 spheres, and 24
        // security cameras' radius-8.5 bodies.
        assert_eq!(s.shapes.len(), 83);
        assert_eq!(s.cams.len(), 24);
        assert!(!s.unimplemented.contains_key("CAM"));
        assert_eq!(s.fluos.len(), 34);
        assert_eq!(
            (s.sirens.len(), s.searchlights.len(), s.sound_emitters.len()),
            (7, 12, 0)
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
            position: s.movers[i].position(),
            standing_on: Some(i),
            touching_goal: None,
            hit_cam_lens: None,
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
            position: ring,
            standing_on: None,
            touching_goal: None,
            hit_cam_lens: None,
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
            position: goal,
            standing_on: None,
            // The engine reports the contact; its test is main-program code.
            touching_goal: Some(0),
            hit_cam_lens: None,
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::GoalContact { sound: 0x1013, .. })));
        assert_eq!(s.slot_words[0], 2);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn a_cell_door_opens_once_its_enemy_group_is_destroyed() {
    with_session(|mut s| {
        // 11 CAGED, 14 CAGE, 2 CAGEB, 3 CAGEC.
        assert_eq!(s.doors.len(), 30);
        for (k, n) in [("CAGED", 11), ("CAGE", 14), ("CAGEB", 2), ("CAGEC", 3)] {
            assert_eq!(
                s.doors.iter().filter(|d| d.kind.name == k).count(),
                n,
                "{k}"
            );
        }
        // The cell door on the route near (-3329, 730, 130): group 10.
        let d = s
            .doors
            .iter()
            .position(|d| d.index == 36 && d.kind.name == "CAGED")
            .expect("CAGED 36");
        let group = s.doors[d].group;
        assert_eq!(group, 10);
        let members: Vec<usize> = (0..s.enemies.len())
            .filter(|&i| s.enemies[i].group == group && s.enemies[i].counted)
            .collect();
        assert!(!members.is_empty() && !s.group_clear(group));
        // Closed while the group lives.
        for _ in 0..60 {
            s.step(None);
        }
        assert_eq!(s.doors[d].state, 0);
        for &i in &members {
            assert!(s.destroy_enemy(i));
        }
        assert!(s.group_clear(group));
        let mut opening = None;
        let mut opened = None;
        for _ in 0..400 {
            for e in s.step(None) {
                match e {
                    Event::DoorOpening { door } if door == d => opening = Some(s.frame),
                    Event::DoorOpened { door } if door == d => opened = Some(s.frame),
                    _ => {}
                }
            }
        }
        let (a, b) = (opening.expect("opening"), opened.expect("opened"));
        // Scale.x 0.5 per frame to the 50-unit stop.
        assert_eq!(b - a, 100);
        assert!((s.doors[d].slide - 50.0).abs() < 1.0);
        // The collision box moved with the slide (Rotation.z bit 0 clear:
        // toward -local Z, which the half-turn yaw points along +Z).
        let z = |tris: Vec<[[f32; 3]; 3]>| tris.iter().flatten().map(|v| v[2]).sum::<f32>() / 36.0;
        let mut closed = s.doors[d].clone();
        closed.slide = 0.0;
        let open_z = z(s.door_solid(d));
        let shut = std::mem::replace(&mut s.doors[d], closed);
        let closed_z = z(s.door_solid(d));
        s.doors[d] = shut;
        assert!(
            (open_z - closed_z - 50.0).abs() < 1.0,
            "{open_z} {closed_z}"
        );
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn a_wide_cage_drops_its_bars_and_its_collision_when_open() {
    with_session(|mut s| {
        let d = s
            .doors
            .iter()
            .position(|d| d.kind.name == "CAGEB")
            .expect("CAGEB");
        let group = s.doors[d].group;
        // Four boxes: floor, bars and two posts.
        assert_eq!(s.door_solid(d).len(), 4 * 12);
        let members: Vec<usize> = (0..s.enemies.len())
            .filter(|&i| s.enemies[i].group == group && s.enemies[i].counted)
            .collect();
        for &i in &members {
            s.destroy_enemy(i);
        }
        let mut opened = None;
        for _ in 0..600 {
            if s.step(None)
                .iter()
                .any(|e| matches!(e, Event::DoorOpened { door } if *door == d))
            {
                opened.get_or_insert(s.frame);
            }
        }
        assert!(
            opened.is_some(),
            "group {group} with {} members",
            members.len()
        );
        // Stop 39; once open the exec stops registering the collision.
        assert_eq!(s.doors[d].state, 2);
        assert!(s.door_solid(d).is_empty());
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn wall_gates_drop_when_no_resting_elevator_is_near() {
    with_session(|mut s| {
        assert_eq!(s.walls.len(), 2);
        let elevators: Vec<[f32; 3]> = s
            .movers
            .iter()
            .filter_map(|m| match &m.state {
                MoverState::Elevator(e) => Some(e.position),
                _ => None,
            })
            .collect();
        let near: Vec<bool> = s
            .walls
            .iter()
            .map(|w| {
                elevators.iter().any(|e| {
                    let d = [
                        w.position[0] - e[0],
                        w.position[1] - e[1],
                        w.position[2] - e[2],
                    ];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < 450.0
                })
            })
            .collect();
        assert!(s.walls.iter().all(|w| w.slide == 50.0 && w.state == 0));
        let events = s.step(None);
        for (i, &n) in near.iter().enumerate() {
            let dropping = events
                .iter()
                .any(|e| matches!(e, Event::WallDropping { wall } if *wall == i));
            assert_eq!(dropping, !n, "wall {i}");
        }
        // Five boxes each.
        assert_eq!(s.wall_solid(0).len(), 5 * 12);
    });
}

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn security_camera_tracks_the_player_and_breaks_when_its_lens_is_hit() {
    with_session(|mut s| {
        let c = s.cams[0].position;
        let head = s.cams[0].head;
        // 100 to the side, inside the range test's 300.
        let at = [c[0] + 100.0, c[1], c[2]];
        let sample = |hit| PlayerSample {
            center: at,
            position: at,
            standing_on: None,
            touching_goal: None,
            hit_cam_lens: hit,
        };
        s.step(Some(sample(None)));
        let turned = (s.cams[0].head.wrapping_sub(head) as i16).unsigned_abs();
        assert!(turned > 0 && turned <= 128, "{turned}");
        // The lens task sets the parent's NextAction; the parent explodes
        // on its next exec.
        let events = s.step(Some(sample(Some(0))));
        assert!(!events.iter().any(|e| matches!(e, Event::CamDestroyed { .. })));
        let events = s.step(Some(sample(None)));
        let Some(Event::CamDestroyed { cam: 0, explosion }) = events
            .iter()
            .find(|e| matches!(e, Event::CamDestroyed { .. }))
        else {
            panic!("{events:?}");
        };
        assert_eq!(explosion.position[1], s.cams[0].lens[1] - 10.0);
        // Destroyed cameras no longer turn.
        let head = s.cams[0].head;
        s.step(Some(sample(None)));
        assert_eq!(s.cams[0].head, head);
    });
}
