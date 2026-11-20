//! Compare the land collision port with main.dol over a run of frames on the
//! relocated GameCube stg11D.rel (Iron Gate) plus synthetic land objects:
//! fn_8002334C (`land_select::select_frame`), fn_800D0904
//! (`LandNodes::update`), and per frame four collision queries through
//! fn_800D1020 (`Selection::reselect` then `LandNodes::query`) and
//! fn_800CEE08 (`LandNodes::collide`) for the objects with candidates.
use sa2_asset_tools::{
    land_contact::ContactStats,
    land_nodes::{Candidate, Entry, LandNodes, MeshSet, NjModel, NjObject, Query},
    land_select::{player_positions, select_frame, DynEntry, LandCol, LandTableHead},
};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn u(x: &Value) -> u64 {
    x.as_u64().unwrap()
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
    let unit: Vec<u64> = v["unit"].as_array().unwrap().iter().map(u).collect();
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
                    type_mat: u(&s["type_mat"]) as u16,
                    count: u(&s["n"]) as u16,
                    meshes: s["meshes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| u(x) as u16)
                        .collect(),
                })
                .collect(),
            attrs: m["attrs"]
                .as_array()
                .map(|a| a.iter().map(|x| u(x) as u32).collect()),
            center: v3(&m["center"]),
            radius: f(&m["radius"]),
        });
    }
    let obj_map = v["objects"].as_object().unwrap();
    let obj_at: HashMap<u64, usize> = obj_map
        .keys()
        .enumerate()
        .map(|(i, k)| (k.parse().unwrap(), i))
        .collect();
    let link = |x: &Value| match u(x) {
        0 => None,
        a => Some(obj_at[&a]),
    };
    let mut objects: Vec<NjObject> = obj_map
        .values()
        .map(|o| NjObject {
            evalflags: u(&o["evalflags"]) as u32,
            model: model_at[&u(&o["model"])],
            pos: v3(&o["pos"]),
            ang: [0, 1, 2].map(|j| o["ang"][j].as_i64().unwrap() as i32),
            scl: v3(&o["scl"]),
            child: link(&o["child"]),
            sibling: link(&o["sibling"]),
        })
        .collect();

    let cols_v = v["cols"].as_array().unwrap();
    let cols: Vec<LandCol> = cols_v
        .iter()
        .map(|c| LandCol {
            center: v3(c),
            radius: f(&c[3]),
            flags: u(&c[5]) as u32,
        })
        .collect();
    // Object indices for the COL entries (only collidable ones were dumped).
    let col_objs: Vec<usize> = cols_v
        .iter()
        .map(|c| obj_at.get(&u(&c[4])).copied().unwrap_or(usize::MAX))
        .collect();
    let head = LandTableHead {
        count: v["head"][0].as_i64().unwrap() as i16,
        start: v["head"][1].as_i64().unwrap() as i16,
    };
    let default = v3(&v["default"]);

    // Task ids are the oracle's task addresses.
    let entry = |e: &Value| Entry {
        flags: u(&e[0]) as u32,
        obj: obj_at[&u(&e[1])],
        task: match u(&e[2]) {
            0 => None,
            t => Some(t as usize),
        },
    };

    let mut state = LandNodes::new();
    let (mut tris_seen, mut cands_seen, mut hits_seen) = (0, 0, 0);
    let mut stats = ContactStats::default();
    for (i, fr) in v["frames"].as_array().unwrap().iter().enumerate() {
        for d in fr["moved"].as_array().unwrap() {
            objects[obj_at[&u(&d[0])]].pos = v3(&d[1]);
        }
        let register: Vec<DynEntry> = fr["register"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| DynEntry {
                flags: u(&r[0]) as u32,
                obj: obj_at[&u(&r[1])],
                task: u(&r[2]) as usize,
                active: r[3].as_bool().unwrap(),
            })
            .collect();
        let pos: Vec<[f32; 3]> = fr["positions"].as_array().unwrap().iter().map(v3).collect();
        let positions = player_positions(pos.first().copied(), pos.get(1).copied(), default);
        let mut sel = select_frame(
            &register,
            &objects,
            &models,
            Some(head),
            &cols,
            &col_objs,
            &positions,
            f(&fr["range"]),
        );
        let want_list: Vec<Entry> = fr["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(entry)
            .collect();
        assert_eq!(sel.list, want_list, "frame {i} list");
        let want_dyn: Vec<(u32, usize, usize)> = fr["dynamic"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (u(&e[0]) as u32, obj_at[&u(&e[1])], u(&e[2]) as usize))
            .collect();
        let got_dyn: Vec<(u32, usize, usize)> = sel
            .dynamic
            .iter()
            .map(|e| (e.flags, e.obj, e.task))
            .collect();
        assert_eq!(got_dyn, want_dyn, "frame {i} dynamic subset");
        let want_static: Vec<usize> = fr["statics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| u(x) as usize)
            .collect();
        assert_eq!(sel.statics, want_static, "frame {i} static subset");

        if let Some(r) = link(&fr["refresh"]) {
            state.rebuild = Some(r);
        }
        state.update(&sel.list, &objects, &models);
        assert_eq!(state.rebuild, link(&fr["refresh_after"]), "frame {i}");
        let want = fr["nodes"].as_array().unwrap();
        assert_eq!(state.nodes.len(), want.len(), "frame {i}");
        for (n, w) in state.nodes.iter().zip(want) {
            assert_eq!(n.obj, obj_at[&u(&w["obj"])], "frame {i}");
            assert_eq!(n.flags as u64, u(&w["flags"]));
            assert_eq!(n.max_dz.to_bits() as u64, u(&w["dz"]), "frame {i}");
            assert_eq!(u(&w["w2c"]), 0);
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
                let want: Vec<u32> = x.as_array().unwrap().iter().map(|y| u(y) as u32).collect();
                assert_eq!(got, want, "frame {i}");
            }
            tris_seen += wt.len();
        }

        for (k, q) in fr["queries"].as_array().unwrap().iter().enumerate() {
            let query = Query {
                radius: f(&q["radius"]),
                center: v3(&q["center"]),
                motion: v3(&q["motion"]),
            };
            sel.reselect(
                &objects,
                &models,
                &cols,
                &col_objs,
                query.center,
                query.radius,
            );
            let want_list: Vec<Entry> = q["list"].as_array().unwrap().iter().map(entry).collect();
            assert_eq!(sel.list, want_list, "frame {i} query {k} list");
            let count = state.query(&query, &sel.list);
            assert_eq!(count as u64, u(&q["count"]), "frame {i} query {k}");
            let want_c: Vec<Candidate> = q["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| Candidate {
                    node: u(&c[0]) as usize,
                    tri: u(&c[1]) as usize,
                })
                .collect();
            assert_eq!(state.candidates[..count], want_c[..], "frame {i} query {k}");
            for (n, w) in state.nodes.iter().zip(q["nodes"].as_array().unwrap()) {
                let first = w[0].as_u64().map(|x| x as usize);
                assert_eq!(
                    (n.cand_first, n.cand_count as u64),
                    (first, u(&w[1])),
                    "frame {i} query {k}"
                );
            }
            cands_seen += count;
            for c in q["contacts"].as_array().unwrap() {
                let obj = obj_at[&u(&c["obj"])];
                let got = state.collide(&query, obj, u(&c["mode"]) != 0, &mut stats);
                let s = c["stats"].as_array().unwrap();
                assert_eq!(
                    [stats.tested, stats.swept_to_end, stats.hits].map(u64::from),
                    [u(&s[0]), u(&s[1]), u(&s[2])],
                    "frame {i} query {k} stats"
                );
                let Some(got) = got else {
                    assert_eq!(u(&c["ret"]), 0, "frame {i} query {k} {c}");
                    continue;
                };
                assert_eq!(u(&c["ret"]), 1, "frame {i} query {k}");
                let want = c["hits"].as_array().unwrap();
                assert_eq!(got.hits.len(), want.len(), "frame {i} query {k}");
                assert_eq!(got.hits.len() as u64, u(&c["count"]));
                assert_eq!(got.best as u64, u(&c["best"]), "frame {i} query {k}");
                for (h, w) in got.hits.iter().zip(want) {
                    let t = &state.nodes[h.candidate.node].tris[h.candidate.tri];
                    let mut words: Vec<u32> = bits(t.center)
                        .into_iter()
                        .chain([t.radius_sq.to_bits()])
                        .chain(t.v.iter().flat_map(|p| bits(*p)))
                        .chain([t.attr, h.angles.0 as u32, h.angles.1 as u32])
                        .collect();
                    for v in [h.normal, h.facing, h.foot, h.head, h.along, h.across] {
                        words.extend(bits(v));
                    }
                    words.extend([h.start_height.to_bits(), h.end_height.to_bits()]);
                    let want: Vec<u32> =
                        w.as_array().unwrap().iter().map(|x| u(x) as u32).collect();
                    assert_eq!(words, want, "frame {i} query {k} hit");
                    hits_seen += 1;
                }
                let b = &got.hits[got.best];
                let mut out: Vec<u32> = vec![b.angles.0 as u32, b.angles.1 as u32];
                for v in [b.normal, b.facing, got.foot, got.head, b.along, b.across] {
                    out.extend(bits(v));
                }
                let want: Vec<u32> = c["out"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| u(x) as u32)
                    .collect();
                assert_eq!(out, want, "frame {i} query {k} outputs");
            }
        }
    }
    assert!(
        tris_seen > 10_000,
        "too few triangles exercised: {tris_seen}"
    );
    assert!(
        cands_seen > 300,
        "too few candidates exercised: {cands_seen}"
    );
    assert!(hits_seen > 300, "too few contacts exercised: {hits_seen}");
}
