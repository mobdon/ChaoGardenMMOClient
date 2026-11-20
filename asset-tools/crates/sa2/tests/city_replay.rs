use sa2_asset_tools::{
    city_replay::{callback_name, replay, replay_bgm},
    object_table::{self, ObjectDefinition},
    set::{self, Placement},
};
use serde_json::json;

fn ring_fixture() -> (Placement, ObjectDefinition) {
    (
        Placement {
            object_id: 0,
            clip_level: 0,
            rotation: [0, 0xffff, 0],
            position: [0.0; 3],
            parameters: [0.0; 3],
        },
        ObjectDefinition {
            load_flags: 2,
            list: 2,
            flags: 16,
            distance: 0.0,
            function_va: 0x6c0f80,
            name: "RING   ".into(),
        },
    )
}

#[test]
fn checks_dispatch_and_ring_observations() {
    let (mut p, mut d) = ring_fixture();
    let empty =
        json!({"range_out":false,"spin_enabled":true,"player_items":[0,null],"ccl_hit_slot":null});
    let hit =
        json!({"ccl_hit_slot":0,"range_out":false,"spin_enabled":true,"player_items":[0,null]});
    let run = |p: &Placement, d: &ObjectDefinition, frames: serde_json::Value| {
        replay(
            p,
            d,
            &serde_json::to_vec(&json!({"frames":frames})).unwrap(),
        )
    };
    let result = run(&p, &d, json!([empty, hit, empty])).unwrap();
    assert_eq!(result["frames"][0]["yaw"], -1);
    assert_eq!(result["frames"][1]["calls"][1], json!(["AddNumRing", 0, 1]));
    assert_eq!(result["frames"][2]["calls"], json!([["DeadOut"]]));
    assert!(run(&p, &d, json!([hit])).is_err());
    let mut shield = empty.clone();
    shield["player_items"][1] = json!(2);
    assert!(run(&p, &d, json!([shield])).is_ok()); // Init never calls the magnetic helper.
    assert!(run(&p, &d, json!([shield, shield])).is_err()); // Active shield requires observed player data.
    p.parameters[0] = -1.0;
    assert!(run(&p, &d, json!([empty])).is_err());
    d.name = "RING".into();
    assert!(callback_name(&p, &d).is_none());
    d.name = "RING   ".into();
    d.function_va += 1;
    assert!(callback_name(&p, &d).is_none());
}

#[test]
fn music_binding_requires_observed_stage_and_correct_lifecycle() {
    let (mut p, mut d) = ring_fixture();
    p.object_id = 81;
    d.name = "ADXCHG".into();
    d.function_va = 0x5e2d70;
    let tick = json!({"op":"tick","range_out":false,"player_in_sphere":true,"game_state":16});
    let bind = json!({"op":"bind","range_out":false,"short_city_2p":false});
    let mut short = bind.clone();
    short["short_city_2p"] = json!(true);
    let run = |frames| {
        replay(
            &p,
            &d,
            &serde_json::to_vec(&json!({"frames":frames})).unwrap(),
        )
    };
    let result =
        run(json!([bind,tick,{"op":"destroy","game_state":7,"predicate_8001cc18":false}])).unwrap();
    assert_eq!(result["frames"][1]["calls"][1]["argument"], "c_escap2.adx");
    assert_eq!(result["frames"][2]["calls"][0]["argument"], "c_escap1.adx");
    assert!(run(json!([short, tick])).is_err());
    assert!(run(json!([bind, bind])).is_err());
    assert!(replay_bgm(
        br#"{"two_player":false,"stage_variant":0,"frames":[{"game_state":16,"player_modes":[0,0]}]}"#
    )
    .is_err());
    assert!(replay_bgm(br#"{"stage_variant":0,"frames":[]}"#).is_err());
    let dual=replay_bgm(br#"{"two_player":true,"stage_variant":0,"frames":[{"game_state":16,"player_modes":[0,0]}]}"#).unwrap();
    assert!(dual["prolog"].as_array().unwrap().is_empty());
    assert_eq!(dual["frames"][0]["counter"], 1);
    assert_eq!(
        replay_bgm(br#"{"two_player":false,"stage_variant":0,"frames":[]}"#).unwrap()["prolog"][1]
            ["argument"],
        "c_escap1.adx"
    );
}

#[test]
#[ignore = "requires SA2_SET_DIR pointing at the installed resource/gd_PC directory"]
fn installed_city_layouts_dispatch_to_all_seven_callbacks() {
    let dir = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    let exe = std::fs::read(dir.join("../../sonic2app.exe")).unwrap();
    let image = object_table::verify_image(&exe).unwrap();
    let table = object_table::read_table(&exe, &image, object_table::CITY_LIST_RVA).unwrap();
    let mut board_indices = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for file in [
        "set0013_s.bin",
        "set0013_u.bin",
        "set0013_hd_s.bin",
        "set0013_2p_s.bin",
        "set0013_2p_u.bin",
    ] {
        let placements = set::parse_set(&std::fs::read(dir.join(file)).unwrap()).unwrap();
        for (index, p) in placements.into_iter().enumerate() {
            let def = &table[p.object_id as usize];
            let Some(name) = callback_name(&p, def) else {
                continue;
            };
            let input = match name {
                "Ring" => {
                    json!({"frames":[{"range_out":false,"spin_enabled":false,"player_items":[null,null]}]})
                }
                "ObjectSpring" | "ObjectSpringB" => {
                    json!({"out_point":[0,0,0],"frames":[{"range_out":false}]})
                }
                "ObjectBoard" => {
                    board_indices.push((file, index, p.rotation[0] & 1));
                    let slot = (p.rotation[0] & 1) as usize;
                    let mut players = [None, None];
                    players[slot] = Some(
                        json!({"position":[1,2,3],"angles":[4,5,6],"speed":[7,8,9],"action":121,"motion_mode":3,"frame":0.25}),
                    );
                    json!({"frames":[{"op":"bind","allocation_ok":true},{"op":"tick","players":players}]})
                }
                "ObjectBoardCol" => {
                    // Exercise both disagreements: two-player engine state on a regular
                    // layer and single-player engine state on a two-player layer.
                    let two_player = !file.contains("_2p_");
                    let mode = if p.rotation[0] & 1 != 0 { 0x4c } else { 0 };
                    let frame = json!({"range_out":false,"players":[{"mode":mode,"position":p.position},null]});
                    let frames = json!([frame, frame, frame]);
                    assert!(replay(
                        &p,
                        def,
                        &serde_json::to_vec(&json!({"frames":frames})).unwrap()
                    )
                    .is_err());
                    json!({"two_player":two_player,"frames":frames})
                }
                "ObjectSavePoint" => {
                    let player = json!({"position_bits":[1,2,3],"rotation":[4,5,6]});
                    let frame = json!({"hit":[1,0],"players":[player,player],"contact_speed":1.0});
                    json!({"engine_result":0,"stage_byte":1,"frames":[frame,frame]})
                }
                "ObjectCeAdxChg" => {
                    json!({"frames":[{"op":"bind","range_out":false,"short_city_2p":false}]})
                }
                _ => unreachable!(),
            };
            let result = replay(&p, def, &serde_json::to_vec(&input).unwrap()).unwrap();
            assert_eq!(result["callback"], name);
            if name == "ObjectBoard" {
                let state = &result["frames"][1]["state"];
                assert_eq!(state["slot"], p.rotation[0] & 1);
                assert_eq!(state["angles"], json!([4, 32763, 6]));
                assert_eq!(state["cached_frame"], 0.25);
            }
            if name == "ObjectBoardCol" && p.parameters[0] + 10.0 > 0.0 {
                let last = result["frames"][2]["calls"].as_array().unwrap();
                assert_eq!(last.len(), usize::from(!file.contains("_2p_")), "{file}");
            }
            if name == "ObjectSavePoint" {
                assert_eq!(
                    result["frames"][0]["handoff"]["checkpoint_byte"],
                    p.rotation[1] & 255
                );
                assert_eq!(result["frames"][0]["handoff"]["slot"], 1);
                assert_eq!(result["frames"][1]["slotTimers"], json!([0, 299]));
                assert!(result["frames"][1]["handoff"].is_null());
            }
            seen.insert(name);
        }
    }
    assert_eq!(seen.len(), 7);
    assert_eq!(
        board_indices,
        [
            ("set0013_s.bin", 103, 0),
            ("set0013_hd_s.bin", 97, 0),
            ("set0013_2p_s.bin", 244, 0),
            ("set0013_2p_s.bin", 245, 1)
        ]
    );
}

#[test]
fn board_requires_engine_mode_and_rearms_only_in_two_player() {
    let (mut p, mut d) = ring_fixture();
    p.object_id = 59;
    d.name = "BOARDCOL".into();
    d.function_va = 0x5ea210;
    let frame = json!({"range_out":false,"players":[{"position":p.position,"mode":0},null]});
    let mut req = json!({"frames":[frame,frame,frame]});
    let run = |req: &serde_json::Value| replay(&p, &d, &serde_json::to_vec(req).unwrap());
    assert!(run(&req).is_err());
    for mode in [false, true] {
        req["two_player"] = json!(mode);
        let output = run(&req).unwrap();
        assert_eq!(
            output["frames"][1]["calls"],
            json!([["SetInputP", 0, 15, 0]])
        );
        assert_eq!(
            output["frames"][2]["calls"].as_array().unwrap().len(),
            usize::from(mode)
        );
    }
}

#[test]
fn physical_board_keeps_rider_observations_separate_from_detached_motion() {
    let (mut p, mut d) = ring_fixture();
    p.object_id = 77;
    p.rotation[0] = 1;
    d.function_va = 0x5ebc50;
    d.name = "BOARD".into();
    let run = |frames| {
        replay(
            &p,
            &d,
            &serde_json::to_vec(&json!({"frames":frames})).unwrap(),
        )
    };
    let bind = json!({"op":"bind","allocation_ok":true});
    let rider = json!({"position":[1,2,3],"speed":[1,0,-4],"angles":[5,-9,7],"action":121,"frame":3.25,"motion_mode":3});
    let wrong_slot = json!({"op":"tick","players":[rider,null]});
    let attach = json!({"op":"tick","players":[null,rider]});
    let mut detached = attach.clone();
    detached["players"][1]["action"] = json!(145);
    let draw = json!({"op":"draw","two_player":true,"players":[null,{"character":8,"action":121,"motion_mode":2,"frame":4.0}],
        "motions":[{"index":145,"pointer":42}]});
    let destroy = json!({"op":"destroy"});
    let out = run(json!([bind, wrong_slot, attach, draw, detached, destroy])).unwrap();
    let f = out["frames"].as_array().unwrap();
    assert_eq!(f[1]["state"]["mode"], 0);
    assert_eq!(f[2]["state"]["angles"], json!([5, 32777, 7]));
    assert_eq!(f[3]["plan"]["selected"]["texture"], "lbl_13_data_107268");
    let blend = f[3]["plan"]["draw"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|op| op.get("draw_blend"))
        .unwrap();
    assert_eq!(blend["cached_frame"], 3.25);
    assert!(f[4].get("state").is_none());
    assert_eq!(f[4]["calls"], json!([["fn_800DE3F8"], ["CheckRangeOut"]]));
    assert_eq!(f[5]["workCleared"], true);
    assert!(run(json!([bind, attach, detached, attach])).is_err());
    assert!(run(json!([bind, destroy, draw])).is_err());
    assert!(run(json!([{"op":"bind","allocation_ok":false},attach])).is_err());
}

#[test]
fn terrain_ring_follows_small_changes_and_scatters_at_eight() {
    let (mut p, d) = ring_fixture();
    p.parameters[0] = -1.;
    let tick = |terrain| json!({"terrain":terrain,"range_out":false,"spin_enabled":false,"player_items":[null,null]});
    let init = tick(json!({"timer":0,"shadow":{"height":11,"yaw_after":-1}}));
    let probe = |h| json!({"reference_position":[0,0,0],"shadow":{"height":h,"yaw_after":2}});
    let mut frames = vec![init, tick(probe(17))];
    frames.extend((0..8).map(|_| tick(json!({}))));
    let mut last = probe(9);
    last["allocated"] = json!(true);
    last["random"] = json!(0.25);
    frames.extend([tick(last), tick(json!({}))]);
    frames[11]["child"] = json!({"op":"launch","timer":0,"gravity":[0,-1,0],"pitch":0,"roll":0,"spin_random":0.5,"paused":false});
    let mut flight = tick(json!({}));
    flight["child"] = json!({"op":"tick","gravity":[0,-1,0],"paused":false,"range_out":false,"probe":{"surface":null}});
    frames.push(flight.clone());
    let mut hit = flight;
    hit["child"].as_object_mut().unwrap().remove("probe");
    hit["child"]["hit"] = json!([1, 0]);
    frames.push(hit.clone());
    hit["child"].as_object_mut().unwrap().remove("hit");
    frames.push(hit);
    let trace = replay(
        &p,
        &d,
        &serde_json::to_vec(&json!({"frames":frames})).unwrap(),
    )
    .unwrap();
    let f = &trace["frames"];
    assert_eq!(f[1]["position"], json!([0., 6., 0.]));
    assert_eq!(
        f[10]["calls"][3],
        json!(["InitTobitiri", [0., -4., 0.], 16384])
    );
    assert_eq!(f[11]["calls"], json!([["DeadOut"]]));
    assert_eq!(
        f[11]["child"]["task"]["flight"]["position"],
        json!([0., 6., 0.])
    );
    assert_eq!(f[12]["child"]["task"]["age"], 1);
    assert_eq!(f[13]["child"]["events"]["credit_slot"], 1);
    assert_eq!(f[14]["child"]["events"]["destroy"], true);
    assert_eq!(
        f[13]["child"]["pickupAndRemovalCalls"],
        json!([{"AddNumRing":[1,1]},{"SECallRing":1},{"AddMechHP":[1,0.1f32]}])
    );
    let teardown = f[14]["child"]["pickupAndRemovalCalls"].as_array().unwrap();
    assert_eq!(teardown.len(), 5);
    let y = f[13]["child"]["task"]["flight"]["position"][1]
        .as_f64()
        .unwrap() as f32;
    assert_eq!(
        teardown[0]["MakeParticle"]["position"][1],
        json!((y + 7.2f32) - 1.5)
    );
    assert_eq!(teardown[4], "DestroyTask");
}

#[test]
fn range_return_skips_ring_initialization_pickup_and_retirement() {
    let (p, d) = ring_fixture();
    let normal = json!({"range_out":false,"spin_enabled":true,"player_items":[0,null]});
    let mut outside = normal.clone();
    outside["range_out"] = json!(true);
    outside["player_items"] = json!([2, null]);
    let mut hit = normal.clone();
    hit["ccl_hit_slot"] = json!(0);
    let frames = json!([outside, normal, outside, hit, outside, normal]);
    let result = replay(
        &p,
        &d,
        &serde_json::to_vec(&json!({"frames":frames})).unwrap(),
    )
    .unwrap();
    let f = &result["frames"];
    for (i, mode) in [(0, "Init"), (2, "Active"), (4, "Collected")] {
        assert_eq!(f[i]["mode"], mode);
        assert_eq!(f[i]["calls"], json!([["CheckRangeOut"]]));
    }
    assert_eq!(f[2]["yaw"], f[1]["yaw"]);
    assert_eq!(f[5]["calls"], json!([["DeadOut"]]));
    outside["ccl_hit_slot"] = json!(0);
    assert!(replay(
        &p,
        &d,
        &serde_json::to_vec(&json!({"frames":[outside]})).unwrap()
    )
    .is_err());
}

#[test]
fn board_route_needs_independent_rider_evidence() {
    use sa2_asset_tools::{city_replay::board_route, city_stage::CityLayout};
    let (mut trigger, mut td) = ring_fixture();
    trigger.object_id = 59;
    trigger.rotation[0] = 1;
    td.name = "BOARDCOL".into();
    td.function_va = 0x5ea210;
    let (mut board, mut bd) = ring_fixture();
    board.object_id = 77;
    bd.name = "BOARD".into();
    bd.function_va = 0x5ebc50;
    let mut table: Vec<_> = (0..78).map(|_| ring_fixture().1).collect();
    table[59] = td;
    table[77] = bd;
    let layout = CityLayout {
        source_layer: "fixture",
        source_sha256: "",
        executable_sha256: "",
        object_list_rva: 0,
        two_player_layer: false,
        object_table: table,
        placements: vec![trigger, board],
        callback_counts: Default::default(),
        unsupported_placements: 0,
    };
    let frame = json!({"players":[{"position":[0,0,0],"mode":0},null],"range_out_slots":[],
        "rider_motion":[{"angles":[0,0,0],"speed":[0,0,0],"action":0,"motion_mode":3,"frame":0},null]});
    let mut req = json!({"physical_board":{"placement":1,"allocation_ok":true},
        "two_player":false,"frames":[frame,frame]});
    let run = |r: &serde_json::Value| board_route(&layout, &serde_json::to_vec(r).unwrap());
    let trace = run(&req).unwrap();
    assert_eq!(
        trace["frames"][1]["calls"][0]["call"],
        json!(["SetInputP", 0, 34, 0])
    );
    assert_eq!(
        trace["physicalBoard"]["trace"]["frames"][2]["state"]["mode"],
        0
    );
    req["frames"][1]["rider_motion"] = json!([null, null]);
    assert!(run(&req)
        .unwrap_err()
        .to_string()
        .contains("match observed player slots"));
    req["frames"][1]
        .as_object_mut()
        .unwrap()
        .remove("rider_motion");
    assert!(run(&req)
        .unwrap_err()
        .to_string()
        .contains("requires rider observations"));
}
