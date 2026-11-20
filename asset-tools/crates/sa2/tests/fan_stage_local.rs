use sa2_asset_tools::fan_stage::Campaign;
use std::{collections::BTreeSet, fs, path::PathBuf};

/// Captured independently from the three story arrays. The ordinal is in
/// flattened event/level/credits dispatches, not the physical source record.
const HERO: &[(usize, i8, u16)] = &[
    (1, 0, 13),
    (2, 0, 60),
    (5, 4, 16),
    (7, 6, 20),
    (9, 6, 9),
    (11, 0, 10),
    (12, 0, 19),
    (13, 0, 3),
    (16, 4, 5),
    (20, 6, 17),
    (22, 4, 7),
    (24, 6, 70),
    (27, 6, 23),
    (28, 0, 28),
    (30, 4, 25),
    (31, 4, 63),
    (33, 0, 64),
    (36, 6, 24),
    (39, 4, 32),
    (40, 4, 33),
    (43, 0, 22),
    (45, 6, 29),
    (47, 0, 30),
    (49, 0, 42),
];
const DARK: &[(usize, i8, u16)] = &[
    (1, 7, 11),
    (3, 1, 61),
    (6, 5, 18),
    (8, 7, 21),
    (11, 1, 14),
    (14, 5, 26),
    (17, 7, 27),
    (20, 7, 12),
    (22, 7, 20),
    (23, 5, 8),
    (24, 5, 62),
    (26, 1, 4),
    (27, 1, 19),
    (35, 5, 70),
    (36, 1, 6),
    (39, 7, 67),
    (42, 5, 44),
    (43, 5, 33),
    (46, 7, 43),
    (48, 7, 29),
    (50, 1, 40),
    (52, 1, 42),
];
const LAST: &[(usize, i8, u16)] = &[(4, 6, 36), (7, 1, 65), (10, 0, 66)];

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn mounted_campaign_routes() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let gd = game.join("resource/gd_PC");
    let read = |name: &str| fs::read_to_string(cfg.join(name)).unwrap();
    let campaign = Campaign::new(
        &fs::read(game.join("sonic2app.exe")).unwrap(),
        &read("ADVERTISE.ini"),
        &read("sa2lvl.ini"),
        &read("ObjectLists.ini"),
    )
    .unwrap();
    let installed = |name: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let f = gd.join(name);
        if f.is_file() {
            Ok(Some(fs::read(f)?))
        } else {
            Ok(None)
        }
    };
    let demo_routes = [
        ("Sonic SA2 The Trial Demo Sequence", 1, 0, 13),
        ("Sonic Demo Sequence", 0, 0, 13),
        ("Mech Tails Demo Sequence", 0, 6, 9),
        ("Knuckles Demo Sequence", 1, 4, 16),
        ("Shadow Demo Sequence", 0, 1, 14),
        ("Mech Eggman Demo Sequence", 0, 7, 21),
        ("Rouge Demo Sequence", 1, 5, 18),
    ];
    assert_eq!(campaign.demos().len(), demo_routes.len());
    for (name, ordinal, character, level) in demo_routes {
        let stage = campaign.mount(name, ordinal, installed).unwrap();
        assert_eq!(
            stage.character_defaults.source_va,
            0x017391e8 + character as u32 * 132
        );
        assert_eq!(
            (stage.dispatch.character, stage.dispatch.level),
            (character, level)
        );
        assert!(stage.layers.iter().any(|l| !l.placements.is_empty()));
    }
    let hero_bosses = [
        (0, 60),
        (6, 20),
        (0, 19),
        (4, 63),
        (0, 64),
        (4, 33),
        (6, 29),
        (0, 42),
    ];
    let dark_bosses = [
        (1, 61),
        (7, 20),
        (5, 62),
        (1, 19),
        (7, 67),
        (5, 33),
        (7, 29),
        (1, 42),
    ];
    let all_bosses = [
        hero_bosses.as_slice(),
        dark_bosses.as_slice(),
        &[(1, 65), (0, 66)],
    ]
    .concat();
    assert_eq!(campaign.boss_rushes().len(), 34);
    for (name, expected) in [
        ("Hero Boss Rush Story Sequence", hero_bosses.as_slice()),
        ("Dark Boss Rush Story Sequence", dark_bosses.as_slice()),
        ("All Boss Rush Story Sequence", all_bosses.as_slice()),
    ] {
        for (ordinal, &(character, level)) in expected.iter().enumerate() {
            let stage = campaign.mount(name, ordinal, installed).unwrap();
            assert_eq!(
                stage.character_defaults.source_va,
                0x017391e8 + character as u32 * 132
            );
            assert_eq!(
                (stage.dispatch.character, stage.dispatch.level),
                (character, level)
            );
        }
        assert!(campaign.dispatch(name, expected.len()).is_none());
    }
    let mut unique = BTreeSet::new();
    let (mut mounts, mut placements, mut unresolved, mut starts, mut grounds) = (0, 0, 0, 0, 0);
    let mut terrain_ids = BTreeSet::new();
    let mut gaps = vec![];
    let mut checkpoint_levels = std::collections::BTreeSet::new();
    let mut checkpoint_count = 0;
    for (sequence, expected) in [
        ("Hero Story Sequence", HERO),
        ("Dark Story Sequence", DARK),
        ("Last Story Sequence", LAST),
    ] {
        let seen: Vec<_> = campaign
            .levels()
            .iter()
            .filter(|d| d.sequence == sequence)
            .map(|d| (d.action_index, d.character, d.level))
            .collect();
        assert_eq!(seen, expected);
        for &(ordinal, character, level) in expected {
            let stage = campaign.mount(sequence, ordinal, installed).unwrap();
            assert_eq!(
                stage.character_defaults.source_va,
                0x017391e8 + character as u32 * 132
            );
            assert_eq!(stage.dispatch.level, level);
            use sa2_asset_tools::mission_ranks::Observation;
            if level == 70 || level == 13 {
                assert_eq!(stage.mission_ranks.len(), 5);
                let row = stage.mission_ranks.iter().find(|r| r.mission == 1).unwrap();
                if level == 70 {
                    let expected = if stage.dispatch.character == 6 {
                        [210, 195, 170, 165]
                    } else {
                        [230, 220, 210, 200]
                    };
                    assert_eq!(
                        row.differences(Observation::Seconds(200)).unwrap(),
                        expected.map(|cut| 200 - cut)
                    );
                    assert!(row.differences(Observation::StoredScore(200)).is_err());
                } else {
                    assert_eq!(
                        row.differences(Observation::StoredScore(140)).unwrap(),
                        [50, 30, 0, -40]
                    );
                }
            } else if level == 60 {
                assert!(stage.mission_ranks.is_empty());
            }
            for (layer_idx, layer) in stage.layers.iter().enumerate() {
                for place in &layer.placements {
                    if !stage
                        .object_table
                        .and_then(|t| t.entries.get(place.definition_index?))
                        .is_some_and(|d| d.function_va == 0x6d99a0 && d.name == "SAVEPOINT")
                    {
                        continue;
                    }
                    let request=br#"{"engine_result":1,"stage_byte":1,"hit":[1,0],"players":[{"rotation":[0,0,0],"position_bits":[0,0,0]},{"rotation":[1,2,3],"position_bits":[4,5,6]}]}"#;
                    let mut frame: serde_json::Value = serde_json::from_slice(request).unwrap();
                    if level == 13 {
                        frame.as_object_mut().unwrap().remove("engine_result");
                        frame.as_object_mut().unwrap().remove("stage_byte");
                        frame["contact_speed"] = serde_json::json!(1.0);
                        frame["bonus_inputs"] = serde_json::json!([{"value":19,"equipment":0},{"value":60,"equipment":0}]);
                        let req = serde_json::json!({"engine_result":1,"stage_byte":1,"frames":[frame,frame]});
                        let seq = stage
                            .replay_object(
                                layer_idx,
                                place.placement_index,
                                &serde_json::to_vec(&req).unwrap(),
                            )
                            .unwrap();
                        assert_eq!(
                            seq["frames"][0]["itemDelivery"]["notifyArgs"],
                            serde_json::json!([1, 1])
                        );
                        assert_eq!(seq["frames"][0]["itemDelivery"]["engineCalls"][1]["r4"], 20);
                        assert!(seq["frames"][1]["itemDelivery"].is_null());
                        let mut gated = req.clone();
                        gated["initial_bonus_counters"] =
                            serde_json::json!({"direct":7,"indirect":-21});
                        gated["frames"][0]["bonus_inputs"][1]["equipment"] =
                            serde_json::json!(0x4000);
                        let run = |r: &serde_json::Value| {
                            stage.replay_object(
                                layer_idx,
                                place.placement_index,
                                &serde_json::to_vec(r).unwrap(),
                            )
                        };
                        let mut exit = req.clone();
                        exit["init_byte_23"] = serde_json::json!(3);
                        exit["set_query_result"] = serde_json::json!(-1);
                        let exited = run(&exit).unwrap();
                        assert_eq!(exited["initExitCall"], 0x80065148u32);
                        assert_eq!(exited["frames"], serde_json::json!([]));
                        for value in [0, 1, -1] {
                            let mut init = req.clone();
                            init["set_query_result"] = serde_json::json!(value);
                            let replay = run(&init).unwrap();
                            let first = &replay["frames"][0];
                            if value == 0 {
                                assert_eq!(first, &seq["frames"][0]);
                            } else {
                                assert_eq!(first["mode"], 3);
                                assert_eq!(first["angles"], serde_json::json!([0, -16384]));
                                assert_eq!(first["slotTimers"], serde_json::json!([0, 0]));
                                assert!(
                                    first["handoff"].is_null() && first["itemDelivery"].is_null()
                                );
                            }
                        }
                        let result = run(&gated).unwrap();
                        assert!(result["frames"][0]["itemDelivery"].is_null());
                        assert_eq!(
                            result["frames"][0]["bonusCounters"],
                            serde_json::json!({"direct":27,"indirect":-1})
                        );
                        gated["frames"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("bonus_inputs");
                        assert!(run(&gated).is_err());
                        continue;
                    }
                    let out = stage
                        .replay_object(layer_idx, place.placement_index, request)
                        .unwrap();
                    assert_eq!(
                        out["handoff"]["checkpoint_byte"],
                        place.placement.rotation[1] & 255
                    );
                    assert_eq!(out["handoff"]["rotation"], serde_json::json!([1, 2, 3]));
                    assert_eq!(out["slotTimers"], serde_json::json!([0, 300]));
                    let mut frame: serde_json::Value = serde_json::from_slice(request).unwrap();
                    let obj = frame.as_object_mut().unwrap();
                    obj.remove("engine_result");
                    obj.remove("stage_byte");
                    obj.insert("contact_speed".into(), serde_json::json!(1.0));
                    frame["bonus_inputs"] =
                        serde_json::json!([{"value":19,"equipment":0},{"value":60,"equipment":0}]);
                    let req = serde_json::json!({"engine_result":1,"stage_byte":1,"initial_bonus_counters":{"direct":32766,"indirect":-40},"frames":[frame,frame]});
                    let seq = stage
                        .replay_object(
                            layer_idx,
                            place.placement_index,
                            &serde_json::to_vec(&req).unwrap(),
                        )
                        .unwrap();
                    for tick in [0, 1] {
                        assert_eq!(
                            seq["frames"][tick]["bonusCounters"],
                            serde_json::json!({"direct":-32750,"indirect":-20})
                        );
                    }
                    assert_eq!(seq["frames"][0]["handoff"], out["handoff"]);
                    assert_eq!(
                        seq["frames"][0]["bonusSelection"],
                        serde_json::json!({"slot":1,"itemId":1,"counterIncrement":20})
                    );
                    assert_eq!(seq["frames"][0]["itemDelivery"]["engineCalls"][1]["r4"], 20);
                    assert!(seq["frames"][1]["itemDelivery"].is_null());
                    assert!(seq["frames"][1]["bonusSelection"].is_null());
                    assert_eq!(seq["frames"][1]["slotTimers"], serde_json::json!([0, 299]));
                    assert_eq!(seq["frames"][1]["steps"], serde_json::json!([-44, 1810]));
                    checkpoint_levels.insert(level);
                    checkpoint_count += 1;
                }
            }
            if stage.entry_pose.is_some() {
                starts += 1;
            }
            if let Some(t) = &stage.terrain {
                assert!(t.entries > 0);
                grounds += 1;
                if terrain_ids.insert(level) {
                    let world = stage.collision_world().unwrap().unwrap();
                    assert_eq!(world.entries.len(), t.entries);
                }
            }
            if !stage.gaps.is_empty() {
                gaps.push((level, stage.gaps.clone()));
            }
            match level {
                70 => {
                    let route = stage.kart_route.unwrap();
                    assert!(stage.level_section.is_none());
                    // One native posed collision object per tasked piece.
                    let t = stage.terrain.as_ref().unwrap();
                    let pieces = route.course_parts.len();
                    assert_eq!((t.entries, t.stage.visual_count), (pieces, 0));
                    assert_eq!(t.geometry_path, format!("CartCourseModels/{}", route.name));
                    assert!(t
                        .stage
                        .entries
                        .iter()
                        .all(|e| e.surface_flags & 0x2000_0001 == 0x2000_0001));
                    let world = stage.collision_world().unwrap().unwrap();
                    let triangles: usize = world.meshes.iter().map(|m| m.triangles.len()).sum();
                    assert_eq!(triangles, if pieces == 321 { 13643 } else { 18439 });
                    let ground = world
                        .raycast([0., 200., -200.], [0., -1., 0.], 1000.)
                        .unwrap()
                        .expect("course start has ground below");
                    assert_eq!(world.meshes[ground.mesh_index].entry_index, 0);
                    assert_eq!(stage.object_table.unwrap().entries.len(), 13);
                    let (n, h) = if sequence == "Hero Story Sequence" {
                        (
                            34,
                            "e93d8da2d30b59a53f3e3750323da91d6e3bea470a99975cf5d4438d91b79c1c",
                        )
                    } else {
                        (
                            29,
                            "08154159fc8b6a97bf113308b85e7f93959d818a938d07e55ee025e79a2e1aea",
                        )
                    };
                    // Digests independently reconstructed from split.exe CART.ini text exports.
                    let (physics_hash, rank_hash) = if sequence == "Hero Story Sequence" {
                        (
                            "80dfb47e1c6817c627d72095a4f0946ef1df771d9d6ca2d4564d9457615bc2d1",
                            "e93fbf731c121e96617daa04f8bd0e5b3ed9c77050c810d841302b0e92942ff1",
                        )
                    } else {
                        (
                            "964c33e34e1bcb895dffb5c91ba621fa6e539658c6102558e2dbed900fd366f7",
                            "b21c02017fc0ee175568a057b7632b0719e6695eb1ade51cd46eb4bbbae5c25b",
                        )
                    };
                    let f = &route.physics;
                    let physics = [
                        f.acceleration,
                        f.brake_power,
                        f.deceleration,
                        f.speed_cap,
                        f.weight,
                        f.unknown1,
                        f.drift_handling,
                        f.drift_speed_threshold,
                        f.unknown2,
                        f.top_speed,
                    ]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>();
                    let mut times = Vec::new();
                    for r in &route.rank_times {
                        times.push(r.level);
                        for time in [r.d, r.c, r.b, r.a] {
                            times.extend(time);
                        }
                    }
                    assert_eq!(sa2_asset_tools::manifest::sha256(&physics), physics_hash);
                    assert_eq!(sa2_asset_tools::manifest::sha256(&times), rank_hash);
                    assert_eq!(route.physics_sha256, physics_hash);
                    assert_eq!(route.rank_times_sha256, rank_hash);
                    assert_eq!(stage.layers.len(), 1);
                    assert_eq!(stage.layers[0].placements.len(), n);
                    assert_eq!(sa2_asset_tools::manifest::sha256(&route.course_parts), h);
                }
                66 => {
                    assert!(
                        stage.level_section.is_some()
                            && stage.layers.is_empty()
                            && stage.object_table.is_none()
                            && stage.terrain.is_none()
                    );
                }
                _ => assert_eq!(stage.layers.len(), 2, "{level}"),
            }
            for (li, l) in stage.layers.iter().enumerate() {
                mounts += 1;
                unique.insert(l.filename);
                placements += l.placements.len();
                for (index, p) in l.placements.iter().enumerate() {
                    let (rp, def) = stage.resolve(li, index).unwrap();
                    assert_eq!(rp.source_offset, 32 + index * 32);
                    if let Some(def) = def {
                        assert_eq!(
                            def.function_va,
                            stage.object_table.unwrap().entries[usize::from(p.placement.object_id)]
                                .function_va
                        );
                    } else {
                        unresolved += 1;
                        assert!(l.unresolved_ids.contains_key(&p.placement.object_id));
                    }
                }
            }
            if level == 13 {
                assert_eq!(
                    stage
                        .layers
                        .iter()
                        .map(|l| l.placements.len())
                        .collect::<Vec<_>>(),
                    [516, 898]
                );
                assert!(stage.entry_pose.as_ref().unwrap().character == "Sonic");
                for (pose, yaw, xyz) in [
                    (&stage.mission2_end_pose, 0xc000, [-1570., -6060., 8860.]),
                    (&stage.mission3_end_pose, 0x4000, [7700., -13145., 5570.]),
                ] {
                    let p = pose.as_ref().unwrap();
                    assert_eq!(
                        (p.character, p.row_va, p.yaw, p.position),
                        ("Sonic", 0x174a2e0, yaw, xyz)
                    );
                }
                let checkpoints: Vec<_> = stage
                    .checkpoint_placements()
                    .map(|(layer, p)| (layer.filename, p.placement_index, p.source_offset))
                    .collect();
                assert_eq!(
                    checkpoints,
                    [45, 218, 343].map(|n| ("set0013_s.bin", n, 32 + 32 * n))
                );
                let angles: Vec<_> = stage
                    .checkpoint_placements()
                    .map(|(_, p)| {
                        let cp = sa2_asset_tools::checkpoint::CheckpointPlacement {
                            rotation: p.placement.rotation.map(i32::from),
                        };
                        assert_eq!(cp.work_byte(2, 1), 0);
                        assert_eq!(cp.work_byte(0, 0), 0);
                        (cp.work_byte(1, -1), cp.child_angles([0x4000, -0x4000, 0]))
                    })
                    .collect();
                assert_eq!(
                    angles,
                    [
                        (3, [[16384, 49152, 0], [16384, 16384, 0]]),
                        (2, [[16384, 32768, 0], [16384, 0, 0]]),
                        (1, [[16384, 16896, 0], [16384, -15872, 0]])
                    ]
                );
                let end = stage.end_pose.as_ref().unwrap();
                assert_eq!(
                    (end.row_va, end.yaw, end.position),
                    (0x1748538, 0x4000, [3955., -22175., 16130.])
                );
                if let Some(world) = stage.collision_world().unwrap() {
                    let hit = world
                        .raycast([0., 1000., 0.], [0., -1., 0.], 2000.)
                        .unwrap()
                        .unwrap();
                    assert!(hit.distance > 0.);
                }
            }
        }
    }
    assert_eq!(checkpoint_count, 76);
    assert_eq!(checkpoint_levels.len(), 18);
    assert_eq!(campaign.levels().len(), 49);
    assert_eq!(unique.len(), 84);
    assert_eq!(mounts, 94);
    assert_eq!(
        (placements, unresolved, starts, grounds, terrain_ids.len()),
        (24162, 0, 43, 46, 41)
    );
    assert!(campaign
        .mount("Hero Story Sequence", 0, |_| panic!(
            "event must not load resources"
        ))
        .is_err());
    assert!(campaign
        .mount("Hero Story Sequence", 1, |_| Ok(Some(vec![0; 32])))
        .is_err());
    let empty = campaign
        .mount("Hero Story Sequence", 1, |_| Ok(None))
        .unwrap();
    assert!(empty.layers.is_empty() && empty.terrain.is_none() && empty.entry_pose.is_some());
    eprintln!("mounts={mounts}, placements={placements}, unresolved={unresolved}, starts={starts}, grounds={grounds}, terrain={terrain_ids:?}, gaps={gaps:?}");
}
#[test]
#[ignore = "requires installed game and PC_SA2 mappings"]
fn task_coverage_partitions_the_verified_campaign_sets() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let read = |f: &str| fs::read_to_string(cfg.join(f)).unwrap();
    let campaign = Campaign::new(
        &fs::read(game.join("sonic2app.exe")).unwrap(),
        &read("ADVERTISE.ini"),
        &read("sa2lvl.ini"),
        &read("ObjectLists.ini"),
    )
    .unwrap();
    let coverage = sa2_asset_tools::task_coverage::read(&campaign, |f| {
        Ok(Some(fs::read(game.join("resource/gd_PC").join(f))?))
    })
    .unwrap();
    assert_eq!(
        (
            coverage.story_dispatches,
            coverage.layer_mounts,
            coverage.distinct_layers
        ),
        (49, 94, 84)
    );
    assert_eq!(
        (
            coverage.placement_visits,
            coverage.distinct_placements,
            coverage.unresolved_placements
        ),
        (24162, 24003, 0)
    );
    assert_eq!(
        coverage
            .by_function_va
            .values()
            .map(|task| task.distinct_placements)
            .sum::<usize>(),
        coverage.distinct_placements
    );
    assert!(coverage.missing_files.is_empty());
    assert_eq!(
        coverage
            .without_layers
            .iter()
            .map(|d| d.level)
            .collect::<Vec<_>>(),
        [66]
    );
    let absent = sa2_asset_tools::task_coverage::read(&campaign, |_| Ok(None)).unwrap();
    assert_eq!(absent.without_layers.len(), 49);
    assert_eq!(absent.missing_files.len(), 84);
    assert!(absent.by_function_va.is_empty());
}
#[test]
#[ignore = "requires installed Green Forest stage and PC mappings"]
fn green_forest_shared_tasks() {
    use serde_json::json;
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let read = |f: &str| fs::read_to_string(cfg.join(f)).unwrap();
    let campaign = Campaign::new(
        &fs::read(game.join("sonic2app.exe")).unwrap(),
        &read("ADVERTISE.ini"),
        &read("sa2lvl.ini"),
        &read("ObjectLists.ini"),
    )
    .unwrap();
    let stage = campaign
        .mount("Hero Story Sequence", 13, |f| {
            Ok(Some(fs::read(game.join("resource/gd_PC").join(f))?))
        })
        .unwrap();
    let spring = stage
        .replay_object(0, 1, br#"{"out_point":[0,7,0],"frames":[{},{}]}"#)
        .unwrap();
    assert_eq!(spring["callback"], "ObjectSpringB");
    let pos = stage.layers[0].placements[7].placement.position;
    let player = json!({"position":pos,"collision_center":pos,"speed":[0,0,0]});
    let initial = json!({"range_out":false,"spin_enabled":true,"player_items":[2,null]});
    let advance = json!({"range_out":false,"spin_enabled":true,"player_items":[2,null],"magnet_players":[player,null],"advance_result":pos});
    let pickup =
        json!({"range_out":false,"spin_enabled":true,"player_items":[0,null],"ccl_hit_slot":0});
    let input = serde_json::to_vec(&json!({"frames":[initial,advance,pickup,initial]})).unwrap();
    let ring = stage.replay_object(0, 7, &input).unwrap();
    assert_eq!(ring["frames"][1]["calls"][0][0], "CalcAdvanceAsPossible");
    assert_eq!(ring["frames"][1]["calls"][1][0], "CCL_Entry");
    assert_eq!(ring["frames"][1]["magneticTimer"], 1);
    assert_eq!(ring["frames"][1]["yaw"], ring["frames"][0]["yaw"]);
    assert_eq!(ring["frames"][2]["calls"][0], json!(["AddScore", 10]));
    let particles = ring["frames"][2]["pickupParticles"].as_array().unwrap();
    assert_eq!(particles.len(), 4);
    assert_eq!(
        particles[0]["position"],
        json!([pos[0], (pos[1] + 7.2) - 1.5, pos[2] + 4.0])
    );
    assert!(ring["frames"][1].get("pickupParticles").is_none());
    assert_eq!(ring["frames"][3]["calls"], json!([["DeadOut"]]));
    let mut missing = advance;
    missing.as_object_mut().unwrap().remove("advance_result");
    assert!(stage
        .replay_object(
            0,
            7,
            &serde_json::to_vec(&json!({"frames":[initial,missing]})).unwrap()
        )
        .is_err());
}
#[test]
#[ignore = "requires installed game and SA Tools PC_SA2 maps"]
fn mounted_story_goal_samples_live_game_bytes() {
    let root = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let gd = root.join("resource/gd_PC");
    let exe = fs::read(root.join("sonic2app.exe")).unwrap();
    let effects = fs::read(gd.join("efftex_common.prs")).unwrap();
    let ini = |f: &str| fs::read_to_string(cfg.join(f)).unwrap();
    let campaign = Campaign::new(
        &exe,
        &ini("ADVERTISE.ini"),
        &ini("sa2lvl.ini"),
        &ini("ObjectLists.ini"),
    )
    .unwrap();
    let mut hunters = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for dispatch in campaign.levels() {
        if covered.contains(&dispatch.level) || hunters.contains(&dispatch.character) {
            continue;
        }
        let stage = campaign
            .mount(dispatch.sequence, dispatch.action_index, |name| {
                let p = gd.join(name);
                Ok(if p.is_file() {
                    Some(fs::read(p)?)
                } else {
                    None
                })
            })
            .unwrap();
        if matches!(dispatch.character, 4 | 5) {
            let Some((layer, row)) = stage.layers.iter().enumerate().find_map(|(layer, l)| {
                l.placements
                    .iter()
                    .find(|p| {
                        p.definition_index
                            .and_then(|i| stage.object_table?.entries.get(i))
                            .is_some_and(|d| d.function_va == 0x6c63c0 && d.name == "GOALRING")
                    })
                    .map(|p| (layer, p.placement_index))
            }) else {
                continue;
            };
            let module = fs::read(gd.join(format!("stg{:02}D.rel", dispatch.level))).unwrap();
            let attempt = |byte, timer, alloc| serde_json::json!({"game_byte_23":byte,"slot_zero_timer":timer,"range_out":true,"allocation_ok":alloc});
            let mut request = attempt(1, 119, true);
            request["init_retries"] = serde_json::json!([
                attempt(0, 120, true),
                attempt(1, 120, false),
                attempt(1, 120, true)
            ]);
            request["frames"] = serde_json::json!([{"hit":{"kind":"player","slot":0},"game_byte_23":1,"game_byte_37":0,"alternate_timer":32769}]);
            let replay = sa2_asset_tools::goal_ring::replay(
                &stage,
                layer,
                row,
                &module,
                &exe,
                &effects,
                &serde_json::to_vec(&request).unwrap(),
            )
            .unwrap();
            let attempts = replay["initAttempts"].as_array().unwrap();
            assert_eq!(
                attempts.iter().map(|v| &v["route"]).collect::<Vec<_>>(),
                vec![
                    &serde_json::json!("Deferred"),
                    &serde_json::json!("Deferred"),
                    &serde_json::json!("NoWork"),
                    &serde_json::json!({"Contact":{"angle_x":2}})
                ]
            );
            assert!(attempts
                .iter()
                .all(|v| v["work"] == serde_json::json!({"mode":1,"angle_x":2})));
            assert_eq!(replay["frames"][0]["latched"], true);
            assert_eq!(
                replay["frames"][0]["alternateMotion"]["soundBeforeContact"]["r6"],
                -2
            );
            assert_eq!(
                replay["frames"][0]["alternateMotion"]["soundBeforeContact"]["code"],
                0x1012
            );
            let sample = |hit, timer, gate| serde_json::json!({"hit":hit,"game_byte_23":1,"game_byte_37":gate,"alternate_timer":timer});
            let none = serde_json::json!({"kind":"none"});
            let player = serde_json::json!({"kind":"player","slot":0});
            for field_present in [true, false] {
                request["alternate_lifecycle"] = serde_json::json!({"task_field_30_present":field_present,"release":{"work56":4096,"nested":8192}});
                request["frames"] = serde_json::json!([
                    sample(&none, 65535, 0),
                    sample(&player, 59, 0),
                    sample(&none, 60, 1),
                    sample(&none, 61, 0)
                ]);
                let life = sa2_asset_tools::goal_ring::replay(
                    &stage,
                    layer,
                    row,
                    &module,
                    &exe,
                    &effects,
                    &serde_json::to_vec(&request).unwrap(),
                )
                .unwrap();
                let frames = life["frames"].as_array().unwrap();
                assert_eq!(frames.len(), 3);
                assert!(frames[0]["cleanupCall"].is_null() && frames[1]["cleanupCall"].is_null());
                assert_eq!(
                    frames[2]["cleanupCall"],
                    if field_present {
                        0x80065148u32
                    } else {
                        0x80071878u32
                    }
                );
                assert!(frames[2]["alternateMotion"]["soundBeforeContact"].is_null());
                assert_eq!(
                    life["observedRelease"]["events"][0],
                    serde_json::json!(["free", 0x801194e0u32, 8192])
                );
            }
            hunters.insert(dispatch.character);
            continue;
        }
        let Some((layer, row)) = stage.layers.iter().enumerate().find_map(|(layer, l)| {
            l.placements
                .iter()
                .find(|p| {
                    p.definition_index
                        .and_then(|i| stage.object_table?.entries.get(i))
                        .is_some_and(|d| d.function_va == 0x6c63c0 && d.name == "GOALRING")
                        && (p.placement.rotation[0] as i16) % 3 == 0
                })
                .map(|p| (layer, p.placement_index))
        }) else {
            continue;
        };
        let module = fs::read(gd.join(format!("stg{:02}D.rel", dispatch.level))).unwrap();
        let request = serde_json::json!({"game_byte_23":0,"range_out":false,"allocation_ok":true,
            "initial_slot_words":[77,88],"lifecycle":{"initial_timer":58,"game_byte_37":0,"task_field_30_present":true},
            "frames":[{"kind":"none"},{"hit":{"kind":"player","slot":1},"game_byte_23":1,"game_byte_37":1,"entry":{"range_out":false,"frame_counter":24}},
            {"hit":{"kind":"none"},"game_byte_23":0,"game_byte_37":1,"entry":{"frame_counter":25}},{"kind":"none"},{"kind":"none"},{"kind":"none"}]});
        let replay = sa2_asset_tools::goal_ring::replay(
            &stage,
            layer,
            row,
            &module,
            &exe,
            &effects,
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        let frames = replay["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 5);
        assert_eq!(
            frames[1]["entry"],
            serde_json::json!({"rangeChecked":true,"workByte3":2})
        );
        assert_eq!(
            frames[2]["entry"],
            serde_json::json!({"rangeChecked":false,"workByte3":0})
        );
        let mut shadow = request.clone();
        shadow["track_shadow"] = true.into();
        shadow["frames"] = serde_json::json!([
            {"hit":{"kind":"none"},"game_byte_23":0,"entry":{"range_out":false,"frame_counter":31}},
            {"hit":{"kind":"none"},"game_byte_23":0,"entry":{"range_out":false,"frame_counter":32,"shadow_return_bits":(-1000000f32).to_bits()}},
            {"hit":{"kind":"none"},"game_byte_23":0,"entry":{"range_out":false,"frame_counter":64,"shadow_return_bits":(-0.0f32).to_bits()}},
            {"hit":{"kind":"none"},"game_byte_23":0,"entry":{"range_out":false,"frame_counter":96}}]);
        let tracked = sa2_asset_tools::goal_ring::replay(
            &stage,
            layer,
            row,
            &module,
            &exe,
            &effects,
            &serde_json::to_vec(&shadow).unwrap(),
        )
        .unwrap();
        let samples = tracked["frames"].as_array().unwrap();
        let center = stage.layers[layer].placements[row].placement.position;
        for index in [1, 2] {
            let q = &samples[index]["shadow"]["query"];
            assert_eq!(
                q["positionBits"],
                serde_json::json!([
                    center[0].to_bits(),
                    (center[1] + 3.).to_bits(),
                    center[2].to_bits()
                ])
            );
            assert_eq!(q["rotation"][1], samples[index - 1]["regularMotion"]["yaw"]);
        }
        assert!(samples[0]["shadow"]["query"].is_null() && samples[3]["shadow"]["query"].is_null());
        assert_eq!(samples[3]["shadow"]["work40Bits"], (-0.0f32).to_bits());
        if dispatch.level == 13 {
            let mut out = request.clone();
            out["frames"][1]["entry"]["range_out"] = true.into();
            let result = sa2_asset_tools::goal_ring::replay(
                &stage,
                layer,
                row,
                &module,
                &exe,
                &effects,
                &serde_json::to_vec(&out).unwrap(),
            )
            .unwrap();
            let seq = result["frames"].as_array().unwrap();
            assert_eq!(seq.len(), 2);
            assert_eq!(seq[1]["rangeOut"], true);
            assert_eq!(seq[1]["latched"], false);
            assert_eq!(seq[1]["calls"], serde_json::json!([]));
        }
        assert!(frames[1]["calls"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!({"Main":{"address":0x8001baa4u32,"slot":1}})));
        assert_eq!(frames[1]["moduleSlotWords"], serde_json::json!([77, 88]));
        assert_eq!(frames[0]["regularMotion"], frames[2]["regularMotion"]);
        assert_eq!(frames[3]["regularMotion"]["timer"], 59);
        assert_eq!(frames[4]["cleanupCall"], 0x80065148u32);
        covered.insert(dispatch.level);
    }
    assert_eq!(hunters, BTreeSet::from([4, 5]));
    assert_eq!(
        covered,
        BTreeSet::from([3, 4, 6, 9, 10, 11, 12, 13, 14, 17, 21, 22, 23, 24, 27, 28, 30, 40, 43])
    );
}
