//! Compare `walker_ground::ground_pass` with main.dol's fn_8003EAFC on Iron
//! Gate's land (tests/walker_ground_ppc.py).
use sa2_asset_tools::{
    land_contact::ContactStats,
    land_nodes::{LandNodes, MeshSet, NjModel, NjObject},
    land_select::{player_positions, select_frame, DynEntry, LandCol, LandTableHead},
    ninja_matrix::MatrixStack,
    walker_collide::{MotionWork, PhysicsRow},
    walker_ground::{
        collision_core, floor_pass, ground_pass, CclHit, CclView, CollisionQuery, FloorExtras,
        GroundEnv, LandContext, MoveVector, PlayerTask, PlayerWork, RideRecord,
    },
};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn u(x: &Value) -> u64 {
    x.as_u64().unwrap()
}

fn i(x: &Value) -> i32 {
    x.as_i64().unwrap() as i32
}

fn v3(x: &Value) -> [f32; 3] {
    [f(&x[0]), f(&x[1]), f(&x[2])]
}

fn i3(x: &Value) -> [i32; 3] {
    [i(&x[0]), i(&x[1]), i(&x[2])]
}

fn bits(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STG11_REL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn ground_pass_matches_main_dol_on_iron_gate() {
    run("ground");
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STG11_REL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn floor_pass_matches_main_dol_on_iron_gate() {
    run("floor");
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STG11_REL and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn collision_core_matches_main_dol_on_iron_gate() {
    run("core");
}

fn run(pass: &str) {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_ground_ppc.py"))
        .env("WALKER_PASS", pass)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();

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
            ang: i3(&o["ang"]),
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
    let col_objs: Vec<usize> = cols_v
        .iter()
        .map(|c| obj_at.get(&u(&c[4])).copied().unwrap_or(usize::MAX))
        .collect();
    let head = LandTableHead {
        count: v["head"][0].as_i64().unwrap() as i16,
        start: v["head"][1].as_i64().unwrap() as i16,
    };
    let rows: Vec<PhysicsRow> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| PhysicsRow {
            run_accel: f(&r[0]),
            f58: f(&r[1]),
            f74: f(&r[2]),
            weight: f(&r[3]),
        })
        .collect();
    let mut records: HashMap<usize, Vec<RideRecord>> = HashMap::new();
    for t in v["tasks"].as_array().unwrap() {
        if let Some(rs) = t[1].as_array() {
            records.insert(
                u(&t[0]) as usize,
                rs.iter()
                    .map(|r| RideRecord {
                        has_callback: u(&r[0]) != 0,
                        dang: i3(&r[1]),
                        dpos: v3(&r[2]),
                    })
                    .collect(),
            );
        }
    }
    let task_list: Vec<usize> = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| u(&t[0]) as usize)
        .collect();

    let mut nodes = LandNodes::new();
    let mut stats = ContactStats::default();
    let mut hits = vec![];
    let mut stack = MatrixStack::new();
    let mut landed_count = 0;
    for (n, c) in v["cases"].as_array().unwrap().iter().enumerate() {
        for d in c["moved"].as_array().unwrap() {
            objects[obj_at[&u(&d[0])]].pos = v3(&d[1]);
        }
        let twp: HashMap<usize, u16> = task_list
            .iter()
            .zip(c["twp_flags"].as_array().unwrap())
            .map(|(t, fl)| (*t, u(fl) as u16))
            .collect();
        let register: Vec<DynEntry> = c["register"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| DynEntry {
                flags: u(&r[0]) as u32,
                obj: obj_at[&u(&r[1])],
                task: u(&r[2]) as usize,
                active: twp[&(u(&r[2]) as usize)] & 0x100 != 0,
            })
            .collect();
        let positions = player_positions(Some(v3(&c["player"])), None, [0.0; 3]);
        let mut sel = select_frame(
            &register,
            &objects,
            &models,
            Some(head),
            &cols,
            &col_objs,
            &positions,
            400.0,
        );
        nodes.update(&sel.list, &objects, &models);

        let t = &c["task"];
        let mut task = PlayerTask {
            mode: u(&t["mode"]) as u8,
            flag: u(&t["flag"]) as u16 as i16,
            angles: i3(&t["angles"]),
            pos: v3(&t["pos"]),
        };
        let mut vel = MoveVector {
            v: v3(&c["vel"]["v"]),
            angles: i3(&c["vel"]["angles"]),
        };
        let p = &c["pw"];
        let opt = |x: &Value| match u(x) {
            0 => None,
            a => Some(a as usize),
        };
        let mut w = PlayerWork {
            pnum: i(&p["pnum"]) as i8,
            character: i(&p["character"]) as i8,
            accel_row: i(&p["accel_row"]) as i8,
            h12: u(&p["h12"]) as u16 as i16,
            h14: u(&p["h14"]) as u16 as i16,
            status: u(&p["status"]) as u16 as i16,
            dotp: f(&p["dotp"]),
            f40: f(&p["f40"]),
            v4c: v3(&p["v4c"]),
            spd: v3(&p["spd"]),
            wall: v3(&p["wall"]),
            floor_normal: v3(&p["floor_normal"]),
            attr: u(&p["attr"]) as u32,
            last_attr: u(&p["last_attr"]) as u32,
            ride: opt(&p["ride"]),
            landed_on: link(&p["landed_on"]),
            w_c0: u(&p["w_c0"]) as u32 as i32,
            height: f(&p["height"]),
            jog: f(&p["jog"]),
            run: f(&p["run"]),
            f_ec: f(&p["f_ec"]),
            f_f0: f(&p["f_f0"]),
            f_f4: f(&p["f_f4"]),
            run_accel: f(&p["run_accel"]),
            f118: f(&p["f118"]),
            f134: f(&p["f134"]),
            weight: f(&p["weight"]),
            flags_1a8: u(&p["flags_1a8"]) as u32,
            height_1b0: f(&p["height_1b0"]),
        };
        let mut q = CollisionQuery {
            radius: 1.0,
            center: [0.0; 3],
            motion: [0.0; 3],
            up: [0.0, 1.0, 0.0],
            count: 0,
            best: 0,
            ang_x: 0,
            ang_z: 0,
            normal: [0.0; 3],
            facing: [0.0; 3],
            foot: [0.0; 3],
            head: [0.0; 3],
            along: [0.0; 3],
            across: [0.0; 3],
        };
        let ride_record =
            |task: usize, pnum: i8| records.get(&task).map(|r| r[(pnum != 0) as usize]);
        let has_collision = |task: usize| twp[&task] & 0x100 != 0;
        let mut env = GroundEnv {
            global: [0.0, -1.0, 0.0],
            stage: i(&c["stage"]),
            rows: &rows,
            stuck: u(&c["stuck"]) != 0,
            mw: MotionWork {
                spd: v3(&c["mw"]["spd"]),
                ang_aim: i(&c["mw"]["aim"]),
            },
            ride_record: &ride_record,
            task_has_collision: &has_collision,
        };
        let (on, level) = (u(&c["analog"][0]) != 0, f(&c["analog"][1]));
        let mut analog = |_: &PlayerTask, _: &PlayerWork| (on, level);
        let mut ride_cb = |p: &mut [f32; 3]| {
            p[0] += 0.75;
            p[1] += -0.5;
            p[2] += 1.25;
        };
        let mut land = LandContext {
            selection: &mut sel,
            nodes: &mut nodes,
            objects: &objects,
            models: &models,
            cols: &cols,
            col_objs: &col_objs,
            stats: &mut stats,
            hits: std::mem::take(&mut hits),
        };
        let ret = if pass == "ground" {
            ground_pass(
                &mut stack,
                &mut land,
                &mut task,
                &mut vel,
                &mut w,
                &mut q,
                &mut env,
                &mut analog,
                &mut ride_cb,
                f(&c["f26"]),
            )
        } else if pass == "core" {
            let k = &c["ccl"];
            let ccl = CclView {
                h2: i(&k["h2"]) as i16,
                flags: u(&k["flags"]) as u16,
                push: v3(&k["push"]),
                self_b3: u(&k["self_b3"]) as u8,
                self_radius: f(&k["self_radius"]),
                hit: k["hit"].as_array().map(|h| CclHit {
                    b0: u(&h[0]) as u8,
                    b1: u(&h[1]) as u8 as i8,
                    b3: u(&h[2]) as u8,
                    radius: f(&h[3]),
                    shape: u(&h[4]) as u16,
                }),
                pushed_info_w4: u(&k["w4"]) as u32,
                interaction: u(&k["interaction"]) as u32,
                carrier_ang_y: k["carrier"].as_i64().map(|a| a as i32),
            };
            let mut x = FloorExtras {
                probe: u(&c["probe"]) != 0,
                analog: &mut analog,
            };
            collision_core(
                &mut stack,
                &mut land,
                &mut task,
                &mut vel,
                &mut w,
                &mut q,
                &mut env,
                &ccl,
                &mut x,
                &mut ride_cb,
                f(&c["f26"]),
            )
        } else {
            let mut x = FloorExtras {
                probe: u(&c["probe"]) != 0,
                analog: &mut analog,
            };
            floor_pass(
                &mut stack,
                &mut land,
                &mut task,
                &mut vel,
                &mut w,
                &mut q,
                &mut env,
                &mut x,
                &mut ride_cb,
                f(&c["f26"]),
            )
        };
        hits = std::mem::take(&mut land.hits);
        let a = &c["after"];
        let ctx = format!("case {n}");
        assert_eq!(ret as u64, u(&a["ret"]), "{ctx} ret");
        assert_eq!(task.mode as u64, u(&a["task"][0]), "{ctx}");
        assert_eq!(
            task.flag as i64,
            a["task"][1].as_i64().unwrap(),
            "{ctx} flag"
        );
        assert_eq!(task.angles, i3(&a["task"][2]), "{ctx} angles");
        assert_eq!(bits(task.pos), bits(v3(&a["task"][3])), "{ctx} pos");
        assert_eq!(bits(vel.v), bits(v3(&a["vel"][0])), "{ctx} vel");
        assert_eq!(vel.angles, i3(&a["vel"][1]), "{ctx} vel angles");
        let pa = &a["pw"];
        assert_eq!(w.h12 as i64, pa["h12"].as_i64().unwrap(), "{ctx}");
        assert_eq!(
            w.status as i64,
            pa["status"].as_i64().unwrap(),
            "{ctx} status"
        );
        assert_eq!(w.f40.to_bits() as u64, u(&pa["f40"]), "{ctx} f40");
        assert_eq!(bits(w.spd), bits(v3(&pa["spd"])), "{ctx} spd");
        assert_eq!(bits(w.wall), bits(v3(&pa["wall"])), "{ctx} wall");
        assert_eq!(
            bits(w.floor_normal),
            bits(v3(&pa["floor_normal"])),
            "{ctx} floor"
        );
        assert_eq!(w.attr as u64, u(&pa["attr"]), "{ctx} attr");
        assert_eq!(w.ride, opt(&pa["ride"]), "{ctx} ride");
        assert_eq!(w.landed_on, link(&pa["landed_on"]), "{ctx} landed");
        assert_eq!(
            [w.run_accel, w.f118, w.weight]
                .map(f32::to_bits)
                .map(u64::from),
            [u(&pa["run_accel"]), u(&pa["f118"]), u(&pa["weight"])],
            "{ctx} physics"
        );
        let qa = &a["q"];
        assert_eq!(q.radius.to_bits() as u64, u(&qa["radius"]), "{ctx} radius");
        assert_eq!(bits(q.center), bits(v3(&qa["center"])), "{ctx} center");
        assert_eq!(bits(q.motion), bits(v3(&qa["motion"])), "{ctx} motion");
        assert_eq!(bits(q.up), bits(v3(&qa["up"])), "{ctx} up");
        assert_eq!(
            (q.count as u64, q.best as u64),
            (u(&qa["count"]), u(&qa["best"])),
            "{ctx} count"
        );
        assert_eq!(
            [q.ang_x, q.ang_z],
            [i(&qa["ang"][0]), i(&qa["ang"][1])],
            "{ctx} q angles"
        );
        let rest: Vec<u32> = [q.normal, q.facing, q.foot, q.head, q.along, q.across]
            .iter()
            .flat_map(|x| bits(*x))
            .collect();
        let want: Vec<u32> = qa["rest"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| u(x) as u32)
            .collect();
        assert_eq!(rest, want, "{ctx} q outputs");
        assert_eq!(bits(env.mw.spd), bits(v3(&a["mw"][0])), "{ctx} mw");
        assert_eq!(env.mw.ang_aim, i(&a["mw"][1]), "{ctx} aim");
        assert_eq!(env.stuck as u64, u(&a["stuck"]), "{ctx} stuck");
        assert_eq!(
            [stats.tested, stats.swept_to_end, stats.hits].map(u64::from),
            [u(&a["stats"][0]), u(&a["stats"][1]), u(&a["stats"][2])],
            "{ctx} stats"
        );
        landed_count += w.landed_on.is_some() as usize;
    }
    assert!(landed_count > 20, "too few landings: {landed_count}");
}
