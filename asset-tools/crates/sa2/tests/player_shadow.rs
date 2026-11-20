//! Compare `player_shadow::shadow_probe` with main.dol's fn_8006779C on
//! synthetic land objects (tests/player_shadow_ppc.py).
use sa2_asset_tools::{
    land_nodes::{Entry, LandNodes, NjModel, NjObject, Node, Tri},
    ninja_matrix::MatrixStack,
    player_shadow::{shadow_probe, ShadowBlock},
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}

fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f(&v[k]))
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn shadow_probe_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_shadow_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut stack = MatrixStack::new();
    let (mut floors, mut ceilings) = (0, 0);
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let objs = case["objs"].as_array().unwrap();
        let objects: Vec<NjObject> = objs
            .iter()
            .enumerate()
            .map(|(k, o)| NjObject {
                evalflags: u(&o["ev"]),
                model: k,
                pos: v3(&o["pos"]),
                ang: [0, 1, 2].map(|j| o["ang"][j].as_i64().unwrap() as i32),
                scl: v3(&o["scl"]),
                child: None,
                sibling: None,
            })
            .collect();
        let models: Vec<NjModel> = objs
            .iter()
            .map(|o| NjModel {
                points: vec![],
                sets: vec![],
                attrs: None,
                center: v3(&o["center"]),
                radius: f(&o["radius"]),
            })
            .collect();
        let mut nodes = LandNodes::new();
        for n in case["nodes"].as_array().unwrap() {
            nodes.nodes.push(Node {
                obj: n["obj"].as_u64().unwrap() as usize,
                flags: u(&n["flags"]),
                max_dz: 0.0,
                center: [0.0; 3],
                tris: n["tris"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| Tri {
                        center: [0.0; 3],
                        radius_sq: 0.0,
                        v: [0, 1, 2].map(|q| v3(&t[q])),
                        attr: 0,
                    })
                    .collect(),
                cand_first: None,
                cand_count: 0,
            });
        }
        let list: Vec<Entry> = case["ents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| Entry {
                flags: u(&e["flags"]),
                obj: e["obj"].as_u64().unwrap() as usize,
                task: e["task"].as_u64().map(|t| t as usize),
            })
            .collect();
        let tasks = case["tasks"].as_array().unwrap();
        let task_flag = |t: usize| {
            tasks[t]["twp"]
                .as_bool()
                .unwrap()
                .then(|| u(&tasks[t]["flag"]) as i16)
        };
        let task_offset = |t: usize| {
            tasks[t]["blk"]
                .as_bool()
                .unwrap()
                .then(|| f(&tasks[t]["off"]))
        };
        let prev: Vec<u32> = case["prev"].as_array().unwrap().iter().map(u).collect();
        let mut out = ShadowBlock {
            angle_x: prev[0] as i32,
            angle_z: prev[1] as i32,
            ceiling_flags: prev[2],
            floor_flags: prev[3],
            ceiling_y: f32::from_bits(prev[4]),
            floor_y: f32::from_bits(prev[5]),
            last_ceiling_flags: prev[6],
            last_floor_flags: prev[7],
        };
        shadow_probe(
            &mut stack,
            &nodes,
            &objects,
            &models,
            &list,
            &task_flag,
            &task_offset,
            case["stage"].as_i64().unwrap() as i32,
            v3(&case["p"]),
            &mut out,
        )
        .unwrap();
        let got = vec![
            out.angle_x as u32,
            out.angle_z as u32,
            out.ceiling_flags,
            out.floor_flags,
            out.ceiling_y.to_bits(),
            out.floor_y.to_bits(),
            out.last_ceiling_flags,
            out.last_floor_flags,
        ];
        let want: Vec<u32> = case["block"].as_array().unwrap().iter().map(u).collect();
        assert_eq!(got, want, "case {ci}: {case}");
        floors += (out.floor_y > -1e6) as u32;
        ceilings += (out.ceiling_y < 1e6) as u32;
    }
    assert!(floors > 200 && ceilings > 200, "{floors} {ceilings}");
}
