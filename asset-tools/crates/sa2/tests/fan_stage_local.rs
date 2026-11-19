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
        for &(ordinal, _, level) in expected {
            let stage = campaign.mount(sequence, ordinal, installed).unwrap();
            assert_eq!(stage.dispatch.level, level);
            for (layer_idx, layer) in stage.layers.iter().enumerate() {
                for place in &layer.placements {
                    if level == 13
                        || !stage
                            .object_table
                            .and_then(|t| t.entries.get(place.definition_index?))
                            .is_some_and(|d| d.function_va == 0x6d99a0 && d.name == "SAVEPOINT")
                    {
                        continue;
                    }
                    let request=br#"{"engine_result":1,"stage_byte":1,"hit":[1,0],"players":[{"rotation":[0,0,0],"position_bits":[0,0,0]},{"rotation":[1,2,3],"position_bits":[4,5,6]}]}"#;
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
                    let req = serde_json::json!({"engine_result":1,"stage_byte":1,"frames":[frame,frame]});
                    let seq = stage
                        .replay_object(
                            layer_idx,
                            place.placement_index,
                            &serde_json::to_vec(&req).unwrap(),
                        )
                        .unwrap();
                    assert_eq!(seq["frames"][0]["handoff"], out["handoff"]);
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
                    assert!(stage.level_section.is_none() && stage.terrain.is_none());
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
        (24162, 0, 43, 44, 40)
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
