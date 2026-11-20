//! Compare `player_shadow` with main.dol's fn_8006779C and fn_80067600 on
//! synthetic land objects (tests/player_shadow_ppc.py).
use sa2_asset_tools::{
    land_nodes::{Entry, LandNodes, NjModel, NjObject, Node, Tri},
    ninja_matrix::MatrixStack,
    player_shadow::{shadow_probe, shadow_update, ProbeLand, ShadowBlock, Unported},
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

struct Land {
    nodes: LandNodes,
    objects: Vec<NjObject>,
    models: Vec<NjModel>,
    list: Vec<Entry>,
}

fn land(case: &Value) -> Land {
    let objs = case["objs"].as_array().unwrap();
    let objects = objs
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
    let models = objs
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
    let list = case["ents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Entry {
            flags: u(&e["flags"]),
            obj: e["obj"].as_u64().unwrap() as usize,
            task: e["task"].as_u64().map(|t| t as usize),
        })
        .collect();
    Land {
        nodes,
        objects,
        models,
        list,
    }
}

fn block(w: &[u32]) -> ShadowBlock {
    ShadowBlock {
        angle_x: w[0] as i32,
        angle_z: w[1] as i32,
        ceiling_flags: w[2],
        floor_flags: w[3],
        ceiling_y: f32::from_bits(w[4]),
        floor_y: f32::from_bits(w[5]),
        last_ceiling_flags: w[6],
        last_floor_flags: w[7],
    }
}

fn words(b: &ShadowBlock) -> Vec<u32> {
    vec![
        b.angle_x as u32,
        b.angle_z as u32,
        b.ceiling_flags,
        b.floor_flags,
        b.ceiling_y.to_bits(),
        b.floor_y.to_bits(),
        b.last_ceiling_flags,
        b.last_floor_flags,
    ]
}

fn list_u32(v: &Value) -> Vec<u32> {
    v.as_array().unwrap().iter().map(u).collect()
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
        let l = land(case);
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
        let mut out = block(&list_u32(&case["prev"]));
        shadow_probe(
            &mut stack,
            &l.nodes,
            &l.objects,
            &l.models,
            &l.list,
            &task_flag,
            &task_offset,
            case["stage"].as_i64().unwrap() as i32,
            v3(&case["p"]),
            &mut out,
        )
        .unwrap();
        assert_eq!(words(&out), list_u32(&case["block"]), "case {ci}: {case}");
        floors += (out.floor_y > -1e6) as u32;
        ceilings += (out.ceiling_y < 1e6) as u32;
    }
    assert!(floors > 200 && ceilings > 200, "{floors} {ceilings}");
    for (ci, case) in c["upd"].as_array().unwrap().iter().enumerate() {
        let l = land(case);
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
        let pl = ProbeLand {
            nodes: &l.nodes,
            objects: &l.objects,
            models: &l.models,
            task_flag: &task_flag,
            task_offset: &task_offset,
            stage: case["stage"].as_i64().unwrap() as i32,
        };
        let mut here = block(&list_u32(&case["prev"]));
        let water = case["water"].as_bool().unwrap();
        let mut ahead = if water {
            ShadowBlock::default()
        } else {
            block(&list_u32(&case["prev2"]))
        };
        let mut asked = vec![];
        let mut reselect = |p: [f32; 3], r: f32| {
            asked.push(vec![
                p[0].to_bits(),
                p[1].to_bits(),
                p[2].to_bits(),
                r.to_bits(),
            ]);
            l.list.clone()
        };
        let st = &case["stick"];
        let stick =
            (!water && st[0].as_u64().unwrap() != 0).then(|| st[1].as_i64().unwrap() as i32);
        let ang = if water {
            [0; 3]
        } else {
            [0, 1, 2].map(|j| case["ang"][j].as_i64().unwrap() as i32)
        };
        let (f130, f134) = if water {
            (0.0, 0.0)
        } else {
            (f(&case["f130"]), f(&case["f134"]))
        };
        let r = shadow_update(
            &mut stack,
            &mut reselect,
            &pl,
            ang,
            v3(&case["p"]),
            f130,
            f134,
            stick,
            &mut here,
            &mut ahead,
        );
        if water {
            assert_eq!(r, Err(Unported(0x8005_237C)), "case {ci}");
            continue;
        }
        r.unwrap();
        let calls: Vec<Vec<u32>> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(list_u32)
            .collect();
        assert_eq!(asked, calls, "case {ci}");
        assert_eq!(words(&here), list_u32(&case["here"]), "case {ci}: {case}");
        assert_eq!(words(&ahead), list_u32(&case["ahead"]), "case {ci}: {case}");
    }
}
