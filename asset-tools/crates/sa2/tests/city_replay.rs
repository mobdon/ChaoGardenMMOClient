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
    assert!(run(&p, &d, json!([shield])).is_err());
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
fn installed_city_layouts_dispatch_to_all_five_callbacks() {
    let dir = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    let exe = std::fs::read(dir.join("../../sonic2app.exe")).unwrap();
    let image = object_table::verify_image(&exe).unwrap();
    let table = object_table::read_table(&exe, &image, object_table::CITY_LIST_RVA).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for file in [
        "set0013_s.bin",
        "set0013_u.bin",
        "set0013_hd_s.bin",
        "set0013_2p_s.bin",
        "set0013_2p_u.bin",
    ] {
        let placements = set::parse_set(&std::fs::read(dir.join(file)).unwrap()).unwrap();
        for p in placements {
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
                "ObjectCeAdxChg" => {
                    json!({"frames":[{"op":"bind","range_out":false,"short_city_2p":false}]})
                }
                _ => unreachable!(),
            };
            let result = replay(&p, def, &serde_json::to_vec(&input).unwrap()).unwrap();
            assert_eq!(result["callback"], name);
            if name == "ObjectBoardCol" && p.parameters[0] + 10.0 > 0.0 {
                let last = result["frames"][2]["calls"].as_array().unwrap();
                assert_eq!(last.len(), usize::from(!file.contains("_2p_")), "{file}");
            }
            seen.insert(name);
        }
    }
    assert_eq!(seen.len(), 5);
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
