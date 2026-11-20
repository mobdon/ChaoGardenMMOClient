//! Compare `stage_objects::ring` with stg42D.rel's own ring code run under
//! the main.dol harness (tests/so_ring_ppc.py), frame by frame: each frame
//! loads the oracle's input state, runs the same routine and compares the
//! events and the resulting task, module and matrix state bit for bit.
use sa2_asset_tools::ninja_matrix::{MatrixStack, Mtx};
use sa2_asset_tools::stage_objects::{
    host::{Event, ObjectHost},
    ring::{self, *},
};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf, process::Command};

fn oracle() -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_ring_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    Some(serde_json::from_slice(&o.stdout).unwrap())
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn v3(v: &Value) -> V3 {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn i3(v: &Value) -> [i32; 3] {
    [0, 1, 2].map(|k| v[k].as_i64().unwrap() as i32)
}
fn b3(v: V3) -> Value {
    json!(v.map(f32::to_bits))
}
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn task_in(t: &Value) -> RingTask {
    RingTask {
        handle: 0x8120_0000,
        twp: TaskWork {
            mode: i(&t["mode"]) as i8,
            smode: i(&t["smode"]) as i8,
            id: i(&t["id"]) as u8,
            btimer: i(&t["btimer"]) as u8,
            wtimer: i(&t["wtimer"]) as u16,
            ang: i3(&t["ang"]),
            pos: v3(&t["pos"]),
            scl: v3(&t["scl"]),
            cwp: t["cwp"].as_array().map(|c| Collision {
                flag: u(&c[0]) as u16,
                colli_range: f(&c[1]),
            }),
        },
        awp: [f(&t["awp"][0]), f(&t["awp"][1])],
        mwp: t["mwp"].as_u64().map(|x| x as usize),
        fwp: u(&t["fwp"]),
        fwk: RingFwk {
            spin: f(&t["fwk"][0]),
            rest: f(&t["fwk"][1]),
            start_pause: i(&t["fwk"][2]) as i32,
            flag: i(&t["fwk"][3]) as i32,
        },
        work: u(&t["work"]),
        ocp: t["ocp"].as_bool().unwrap(),
        dest_ring_end: t["dest"].as_bool().unwrap(),
    }
}

fn task_out(t: &RingTask) -> Value {
    let w = &t.twp;
    let fwk = if t.fwp != 0 {
        json!([
            t.fwk.spin.to_bits(),
            t.fwk.rest.to_bits(),
            t.fwk.start_pause,
            t.fwk.flag
        ])
    } else {
        json!([0, 0, 0, 0])
    };
    json!({
        "mode": w.mode, "smode": w.smode, "id": w.id, "btimer": w.btimer, "wtimer": w.wtimer,
        "ang": w.ang, "pos": b3(w.pos), "scl": b3(w.scl),
        "cwp": w.cwp.map(|c| json!([c.flag, c.colli_range.to_bits()])),
        "awp": [t.awp[0].to_bits(), t.awp[1].to_bits()],
        "mwp": t.mwp, "fwp": t.fwp, "fwk": fwk, "work": t.work, "ocp": t.ocp,
        "dest": t.dest_ring_end,
    })
}

fn world_in(w: &Value) -> RingWorld {
    RingWorld {
        colli_timer: i(&w["colli_timer"]) as i32,
        head: u(&w["head"]),
        cleanup: i(&w["cleanup"]) as i32,
        ring_tp: u(&w["ring_tp"]),
        objects: w["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| RingObject {
                addr: u(&o[0]),
                evalflags: u(&o[1]),
                model: o[2].as_bool().unwrap(),
                pos: v3(&o[3]),
                ang: i3(&o[4]),
                sibling: u(&o[5]),
                shadow_ang: i3(&o[6]),
                shadow_posy: f(&o[7]),
            })
            .collect(),
        links: w["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| RingLink {
                flag: u(&l[0]),
                tp: u(&l[1]),
            })
            .collect(),
    }
}

fn world_out(w: &RingWorld) -> Value {
    let mut objs = w.objects.clone();
    objs.sort_by_key(|o| o.addr);
    json!({
        "colli_timer": w.colli_timer, "head": w.head, "cleanup": w.cleanup, "ring_tp": w.ring_tp,
        "objects": objs.iter().map(|o| json!([o.addr, o.evalflags, o.model, b3(o.pos), o.ang,
            o.sibling, o.shadow_ang, o.shadow_posy.to_bits()])).collect::<Vec<_>>(),
        "links": w.links.iter().map(|l| json!([l.flag, l.tp])).collect::<Vec<_>>(),
    })
}

fn opt(v: &Value) -> Option<&Value> {
    (!v.is_null()).then_some(v)
}

fn globals(g: &Value) -> RingGlobals {
    RingGlobals {
        gravity: v3(&g["gravity"]),
        gravity_angles: [
            i(&g["gravity_angles"][0]) as i32,
            i(&g["gravity_angles"][1]) as i32,
        ],
        pause: i(&g["pause"]) as i8,
        hide_rings: i(&g["hide"]) as i8,
        disable_fog: i(&g["fog"]) as i32,
        players: [0, 1].map(|k| {
            opt(&g["players"][k]).map(|p| PlayerWork {
                item: i(&p[0]) as i16,
                spd: v3(&p[1]),
            })
        }),
        player_tasks: [0, 1].map(|k| {
            opt(&g["player_tasks"][k]).map(|p| PlayerTask {
                pos: v3(&p[0]),
                center: v3(&p[1]),
            })
        }),
        origin: v3(&g["origin"]),
    }
}

fn mtx(v: &Value) -> Mtx {
    let mut m = [[0.0; 4]; 3];
    for (k, x) in v.as_array().unwrap().iter().enumerate() {
        m[k / 4][k % 4] = f(x);
    }
    m
}
fn mtx_bits(m: &Mtx) -> Value {
    json!(m.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>())
}

/// Answers the stubs gave, in order, and the events in the oracle's shape.
struct Recorder {
    out: bool,
    calloc: VecDeque<u32>,
    init: VecDeque<bool>,
    hit: VecDeque<bool>,
    which: VecDeque<i32>,
    shadow: VecDeque<(f32, [i32; 3])>,
    xyz: VecDeque<Vec<Option<ShadowHit>>>,
    create: VecDeque<u32>,
    rand: VecDeque<i32>,
    stack: MatrixStack,
    events: Vec<Value>,
}

impl ObjectHost for Recorder {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, _: Event) {
        unreachable!()
    }
}

impl RingHost for Recorder {
    fn calloc(&mut self, size: u32) -> u32 {
        self.events.push(json!(["calloc", size]));
        self.calloc.pop_front().unwrap()
    }
    fn ccl_init_share(&mut self, list: u8) -> bool {
        self.events.push(json!(["init_share", list]));
        self.init.pop_front().unwrap()
    }
    fn ccl_is_hit_player(&mut self) -> bool {
        self.events.push(json!(["is_hit"]));
        self.hit.pop_front().unwrap()
    }
    fn is_this_task_player(&mut self) -> i32 {
        self.events.push(json!(["which"]));
        self.which.pop_front().unwrap()
    }
    fn get_shadow_pos(&mut self, pos: V3, ang: &mut [i32; 3]) -> f32 {
        self.events.push(json!(["shadow", b3(pos)]));
        let (y, a) = self.shadow.pop_front().unwrap();
        *ang = a;
        y
    }
    fn get_shadow_pos_xyz(&mut self, pos: V3, face: usize) -> Option<ShadowHit> {
        self.events.push(json!(["shadow_xyz", b3(pos)]));
        self.xyz.pop_front().unwrap()[face]
    }
    fn create_task(&mut self, kind: TaskKind) -> u32 {
        self.events.push(match kind {
            TaskKind::Tobitiri => json!(["create", 10, 2, 0x19ff0, 0x1ad2c]),
            TaskKind::RingModel => json!(["create", 2, 1, 0x1b3a4, 0x1b134]),
        });
        self.create.pop_front().unwrap()
    }
    fn rand(&mut self) -> i32 {
        self.events.push(json!(["rand"]));
        self.rand.pop_front().unwrap()
    }
    fn matrix(&mut self) -> &mut MatrixStack {
        &mut self.stack
    }
    fn ring_event(&mut self, e: RingEvent) {
        self.events.push(match e {
            RingEvent::Free(a) => json!(["free", a]),
            RingEvent::CollisionEntry(p) => json!(["entry", b3(p)]),
            RingEvent::AddScore(s) => json!(["score", s]),
            RingEvent::AddNumRing(p, n) => json!(["num_ring", p, n]),
            RingEvent::AddMechHp(p, h) => json!(["mech", p, h.to_bits()]),
            RingEvent::SeCallRing(p) => json!(["se_ring", p]),
            RingEvent::MakeParticle(p, v, s) => json!(["particle", b3(p), b3(v), s.to_bits()]),
            RingEvent::DeadOut => json!(["dead_out"]),
            RingEvent::FreeTask => json!(["free_task"]),
            RingEvent::DestroyTask(t) => json!(["destroy", t]),
            RingEvent::SpawnTobitiri { task, pos, ang_y } => {
                json!(["spawn", task, b3(pos), ang_y])
            }
            RingEvent::SetTexture(t) => json!(["texture", t]),
            RingEvent::DisableFog => json!(["fog_off"]),
            RingEvent::EnableFog => json!(["fog_on"]),
            RingEvent::SetFog => json!(["set_fog"]),
            RingEvent::DrawModel(m, x) => json!(["draw", m, mtx_bits(&x)]),
        });
    }
}

fn recorder(fr: &Value) -> Recorder {
    let list = |k: &str| fr[k].as_array().unwrap().clone();
    let mut stack = MatrixStack::new();
    *stack.current_mut() = mtx(&fr["matrix_in"]);
    Recorder {
        out: fr["out"].as_bool().unwrap(),
        calloc: list("calloc").iter().map(u).collect(),
        init: list("init").iter().map(|v| v.as_bool().unwrap()).collect(),
        hit: list("hit").iter().map(|v| v.as_bool().unwrap()).collect(),
        which: list("which").iter().map(|v| i(v) as i32).collect(),
        shadow: list("shadow")
            .iter()
            .map(|v| (f(&v[0]), i3(&v[1])))
            .collect(),
        xyz: list("xyz")
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|h| {
                        (!h.is_null()).then(|| ShadowHit {
                            onpos: f(&h[0]),
                            angx: i(&h[1]) as i32,
                            angz: i(&h[2]) as i32,
                        })
                    })
                    .collect()
            })
            .collect(),
        create: list("create").iter().map(u).collect(),
        rand: list("rand").iter().map(|v| i(v) as i32).collect(),
        stack,
        events: Vec::new(),
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR, the Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn ring_code_matches_stg42d() {
    let Some(c) = oracle() else {
        return;
    };
    let mut frames = 0;
    let mut ops = std::collections::BTreeMap::<String, usize>::new();
    for case in c["cases"].as_array().unwrap() {
        for fr in case["frames"].as_array().unwrap() {
            let mut task = task_in(&fr["task_in"]);
            let mut world = world_in(&fr["world_in"]);
            let g = globals(&fr["globals"]);
            let mut h = recorder(fr);
            let op = fr["op"].as_str().unwrap();
            let arg = u(&fr["arg"]);
            match op {
                "ring" => ring::ring_exec(&mut task, &mut world, &g, &mut h),
                "tobitiri" => ring::tobitiri_exec(&mut task, &mut world, &g, &mut h),
                "ring_end" => ring::ring_end(&mut task, &mut world, &mut h),
                "model_exec" => ring::ring_model_exec(&mut world, arg, &mut h),
                "model_dest" => ring::ring_model_dest(&mut world, arg, &mut h),
                "disp" => ring::ring_model_disp(&world, &g, &mut h),
                "disp2" => ring::ring_model_disp2(&world, &g, &mut h),
                "link" => ring::link(&mut task, arg as usize, &mut world),
                "unlink" => ring::unlink(&mut task, &mut world),
                _ => unreachable!(),
            }
            let ctx = || format!("{op} {}", fr["task_in"]);
            assert_eq!(json!(h.events), fr["events"], "events {}", ctx());
            assert_eq!(task_out(&task), fr["task"], "task {}", ctx());
            assert_eq!(world_out(&world), fr["world"], "world {}", ctx());
            assert_eq!(h.stack.depth() as i64, i(&fr["depth"]), "depth {}", ctx());
            assert_eq!(
                mtx_bits(h.stack.current()),
                fr["matrix"],
                "matrix {}",
                ctx()
            );
            assert!(h.calloc.is_empty() && h.rand.is_empty() && h.create.is_empty());
            frames += 1;
            *ops.entry(op.into()).or_default() += 1;
        }
    }
    for v in c["faces"].as_array().unwrap() {
        assert_eq!(ring::o_ring_5(v3(&v[0])) as i64, i(&v[1]), "{v}");
    }
    eprintln!(
        "{frames} frames matched {ops:?}, {} o_ring_5 cases",
        c["faces"].as_array().unwrap().len()
    );
}
