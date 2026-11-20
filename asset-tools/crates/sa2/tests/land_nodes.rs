//! Compare `land_nodes::LandNodes::update` with main.dol's fn_800D0904 over
//! a run of frames on the relocated GameCube stg11D.rel (Iron Gate).
use sa2_asset_tools::land_nodes::{Entry, LandNodes, MeshSet, NjModel, NjObject};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn v3(x: &Value) -> [f32; 3] {
    [f(&x[0]), f(&x[1]), f(&x[2])]
}

fn bits(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STG11_REL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol_on_iron_gate() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/land_nodes_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();

    // The game's nj_unit_matrix is the identity the port starts from.
    let unit: Vec<u32> = v["unit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as u32)
        .collect();
    assert_eq!(
        unit,
        [
            0x3f80_0000,
            0,
            0,
            0,
            0,
            0x3f80_0000,
            0,
            0,
            0,
            0,
            0x3f80_0000,
            0
        ]
    );

    let mut model_at = HashMap::new();
    let mut models = vec![];
    for (k, m) in v["models"].as_object().unwrap() {
        model_at.insert(k.parse::<u64>().unwrap(), models.len());
        models.push(NjModel {
            points: m["points"].as_array().unwrap().iter().map(v3).collect(),
            sets: m["sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| MeshSet {
                    type_mat: s["type_mat"].as_u64().unwrap() as u16,
                    count: s["n"].as_u64().unwrap() as u16,
                    meshes: s["meshes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_u64().unwrap() as u16)
                        .collect(),
                })
                .collect(),
            attrs: m["attrs"]
                .as_array()
                .map(|a| a.iter().map(|x| x.as_u64().unwrap() as u32).collect()),
            center: v3(&m["center"]),
        });
    }
    let obj_map = v["objects"].as_object().unwrap();
    let obj_at: HashMap<u64, usize> = obj_map
        .keys()
        .enumerate()
        .map(|(i, k)| (k.parse().unwrap(), i))
        .collect();
    let link = |x: &Value| match x.as_u64().unwrap() {
        0 => None,
        a => Some(obj_at[&a]),
    };
    let objects: Vec<NjObject> = obj_map
        .values()
        .map(|o| NjObject {
            model: model_at[&o["model"].as_u64().unwrap()],
            pos: v3(&o["pos"]),
            ang: [0, 1, 2].map(|j| o["ang"][j].as_i64().unwrap() as i32),
            scl: v3(&o["scl"]),
            child: link(&o["child"]),
            sibling: link(&o["sibling"]),
        })
        .collect();

    let mut state = LandNodes::new();
    let mut tris_seen = 0;
    for (i, fr) in v["frames"].as_array().unwrap().iter().enumerate() {
        if let Some(r) = link(&fr["refresh"]) {
            state.rebuild = Some(r);
        }
        let entries: Vec<Entry> = fr["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| Entry {
                flags: e[0].as_u64().unwrap() as u32,
                obj: obj_at[&e[1].as_u64().unwrap()],
            })
            .collect();
        state.update(&entries, &objects, &models);
        assert_eq!(state.rebuild, link(&fr["refresh_after"]), "frame {i}");
        let want = fr["nodes"].as_array().unwrap();
        assert_eq!(state.nodes.len(), want.len(), "frame {i}");
        for (n, w) in state.nodes.iter().zip(want) {
            assert_eq!(n.obj, obj_at[&w["obj"].as_u64().unwrap()], "frame {i}");
            assert_eq!(n.flags as u64, w["flags"].as_u64().unwrap());
            assert_eq!(
                n.max_dz.to_bits() as u64,
                w["dz"].as_u64().unwrap(),
                "frame {i}"
            );
            assert_eq!(w["w2c"].as_u64().unwrap(), 0);
            assert_eq!(bits(n.center), bits(v3(&w["center"])), "frame {i}");
            let wt = w["tris"].as_array().unwrap();
            assert_eq!(n.tris.len(), wt.len(), "frame {i}");
            for (t, x) in n.tris.iter().zip(wt) {
                let got: Vec<u32> = bits(t.center)
                    .into_iter()
                    .chain([t.radius_sq.to_bits()])
                    .chain(t.v.iter().flat_map(|p| bits(*p)))
                    .chain([t.attr])
                    .collect();
                let want: Vec<u32> = x
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|y| y.as_u64().unwrap() as u32)
                    .collect();
                assert_eq!(got, want, "frame {i}");
            }
            tris_seen += wt.len();
        }
    }
    assert!(
        tris_seen > 10_000,
        "too few triangles exercised: {tris_seen}"
    );
}
