use sa2_asset_tools::{set::Placement, spring::SpringKind, stage_spring_host::replay};
use serde_json::json;

fn placement() -> Placement {
    Placement {
        object_id: 1,
        clip_level: 0,
        rotation: [65535, 32768, 32767],
        position: [10.0, 20.0, 30.0],
        parameters: [3.0, 2.0, 0.0],
    }
}

#[test]
fn signed_placement_and_explicit_contact_drive_callback() {
    let p = placement();
    let req = json!({"out_point":[0,7,0],"frames":[{},{"range_out":true},{},{},{"hit":1},{"player":{"position":[10,20,30],"running_speed":2,"item":0}}]});
    let bytes = serde_json::to_vec(&req).unwrap();
    let v = serde_json::to_value(replay(&p, SpringKind::B, &bytes).unwrap()).unwrap();
    assert_eq!(v["signed_angles"], json!([-1, -32768, 32767]));
    let last = &v["frames"][5]["calls"];
    assert!(last.as_array().unwrap().contains(&json!({"VelocityYRotationNoControl":{"slot":1,"velocity_y":7.0,"angles":[-1,-32768,32767],"frames":3}})));
    assert_eq!(
        last.as_array().unwrap().last(),
        Some(&json!({"Sound":4096}))
    );
}

#[test]
fn replay_rejects_unbounded_and_ambiguous_inputs() {
    let p = placement();
    let test = |v| replay(&p, SpringKind::A, &serde_json::to_vec(&v).unwrap()).is_err();
    assert!(test(
        json!({"out_point":[0,1,0],"frames":vec![json!({});2049]})
    ));
    assert!(test(json!({"out_point":[0,1,0],"frames":[]})));
    assert!(test(json!({"out_point":[0,1,0],"frames":[{"hit":9}]})));
    assert!(test(json!({"out_point":[0,1,0],"frames":[{"ring":1}]})));
    assert!(test(
        json!({"out_point":[0,1,0],"frames":[{"player":{"position":[0,0,0],"item":0,"running_speed":0}}]})
    ));
    assert!(replay(&p, SpringKind::A, &vec![b' '; 1_048_577]).is_err());
}
