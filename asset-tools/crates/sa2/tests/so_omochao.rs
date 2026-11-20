//! Omochao's exec (0x156CC) and main display (0x17468), as
//! `stage_objects::messenger::omochao` ports them, against stg42D.rel run
//! under the main.dol harness (tests/so_omochao_ppc.py). Needs the local
//! game files (SA2_MAIN_DOL, SA2_GC_REL_DIR) and the harness's Python
//! packages, so the tests are ignored by default:
//!
//!     cargo test --release --test so_omochao -- --ignored
use sa2_asset_tools::land_probe::{Hit, Shadow};
use sa2_asset_tools::ninja_matrix::{MatrixStack, Mtx};
use sa2_asset_tools::prison_visuals::MslRand;
use sa2_asset_tools::stage_objects::{
    dyn_land::{self, Movable, MovableHost, ShadowBlock, WORDS},
    effects::{EffectHost, Particle},
    host::{Event as HostEvent, ObjectHost},
    messenger::omochao::{
        self, Child, Event, Force, Omochao, OmochaoHost, PlaceHost, PlayerTwp, Twp, Work, STG42D,
    },
    messenger::trail::{Trail, TrailDraw, TrailHost},
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::{path::PathBuf, process::Command};

fn oracle(group: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_omochao_ppc.py"))
        .arg(group)
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
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}
fn mbits(m: &Mtx) -> Value {
    json!(m.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>())
}
fn queue(v: &Value) -> VecDeque<Value> {
    v.as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default()
}

/// The texts the stage data names are section-5 offsets; the oracle
/// records their addresses.
fn text(t: u32, s5: u32) -> u32 {
    if t == STG42D.hint_default || t == STG42D.message_default {
        t + s5
    } else {
        t
    }
}

#[derive(Default)]
struct Rec {
    s5: u32,
    events: Vec<Value>,
    q: HashMap<String, VecDeque<Value>>,
    script: VecDeque<Value>,
    out: bool,
    g: Value,
    players: Vec<Option<(PlayerTwp, u32)>>,
    requests: [i32; 2],
    forces: Vec<Value>,
    model: HashMap<u32, u32>,
    particle: Option<Particle>,
}

impl Rec {
    fn take(&mut self, k: &str) -> Option<Value> {
        self.q.get_mut(k).and_then(|q| q.pop_front())
    }
    fn take_u(&mut self, k: &str) -> u32 {
        self.take(k).map_or(0, |v| u(&v))
    }
    fn gi(&self, k: &str) -> i32 {
        i(&self.g[k])
    }
    /// Replays one scripted GetShadowPosXYZ / fn_80066E1C result.
    fn shadow(&mut self, block: &mut ShadowBlock, water: bool) -> bool {
        let e = self.script.pop_front().expect("a scripted probe");
        let mut s = Shadow::default();
        for (r, ray) in e[3].as_array().unwrap().iter().enumerate() {
            s.coords[r] = f(&ray[1]);
            if u(&ray[0]) != 0 {
                s.hits[r] = Some(Hit {
                    flags: u(&ray[2]),
                    angle_x: i(&ray[3]),
                    angle_z: i(&ray[4]),
                    y: f(&ray[1]),
                    normal: [f(&ray[5]), f(&ray[6]), f(&ray[7])],
                });
            }
        }
        dyn_land::write_shadow(block, &s);
        let ret = water && u(&e[4]) != 0;
        self.events.push(json!([
            "shadow",
            u32::from(water),
            bits(block.point),
            e[3],
            u32::from(ret)
        ]));
        ret
    }
}

impl ObjectHost for Rec {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out", self.out]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: HostEvent) {
        match e {
            HostEvent::CollisionEntry => self.events.push(json!(["entry"])),
            e => panic!("{e:?}"),
        }
    }
}

impl MovableHost for Rec {
    fn sy_calloc_801197fc(&mut self, _: u32, _: u32) -> bool {
        unreachable!()
    }
    fn sy_free_80119828(&mut self) {
        unreachable!()
    }
    fn get_shadow_pos_xyz_80067210(&mut self, block: &mut ShadowBlock) {
        self.shadow(block, false);
    }
    fn shadow_water_80066e1c(&mut self, block: &mut ShadowBlock) -> bool {
        self.shadow(block, true)
    }
    fn byte_801cc19f(&self) -> i8 {
        self.gi("pause") as i8
    }
}

impl TrailHost for Rec {
    fn sy_calloc(&mut self, _: u32, _: u32) -> Option<u32> {
        unreachable!()
    }
    fn sy_free(&mut self, _: u32) {
        unreachable!()
    }
    fn game_state(&mut self) -> i32 {
        unreachable!()
    }
    fn frame(&mut self) -> i32 {
        self.gi("frame")
    }
    fn trail_reset(&mut self) -> i8 {
        self.gi("pause") as i8
    }
    fn trail_draw(&mut self, _: TrailDraw) {
        unreachable!()
    }
}

impl EffectHost for Rec {
    fn particle_alloc(&mut self, descriptor: u32) -> Option<Particle> {
        let ok = self.take("palloc").is_none_or(|v| v.as_bool().unwrap());
        self.events
            .push(json!(["palloc", descriptor, u32::from(ok)]));
        ok.then(Particle::default)
    }
}

impl PlaceHost for Rec {
    fn player_character(&mut self, n: i32) -> i32 {
        let v = self.take_u("chara") as i32;
        self.events.push(json!(["chara", n, v]));
        v
    }
    fn fn_80038008(&mut self, n: i32, node: i32) -> Option<[f32; 3]> {
        let r = self.take("bone").filter(|v| !v.is_null());
        self.events
            .push(json!(["bone", n, node, u32::from(r.is_some())]));
        r.map(|v| v3(&v))
    }
    fn player_position(&mut self, n: i32) -> Option<[f32; 3]> {
        self.player(n).map(|p| p.pos)
    }
}

impl OmochaoHost for Rec {
    fn language(&mut self) -> i8 {
        self.gi("language") as i8
    }
    fn byte_42(&mut self) -> i8 {
        self.gi("b42") as i8
    }
    fn fn_800e018c(&mut self) -> u32 {
        let v = self.take_u("onscreen");
        self.events.push(json!(["onscreen", v]));
        v
    }
    fn fn_800e00e8(&mut self, kind: i32, t: u32, time: u32, language: i8) -> u32 {
        let w = self.take_u("win");
        self.events
            .push(json!(["e8", kind, text(t, self.s5), time, language, w]));
        w
    }
    fn fn_800e0118(&mut self, kind: i32, t: u32, time: u32) -> u32 {
        let w = self.take_u("win");
        self.events
            .push(json!(["0118", kind, text(t, self.s5), time, w]));
        w
    }
    fn message_count(&mut self) -> i32 {
        i(&self.g["messages"][0])
    }
    fn message_entry(&mut self, k: i32) -> u32 {
        u(&self.g["messages"][1][k as usize])
    }
    fn hint_count(&mut self) -> i32 {
        i(&self.g["hints"][0])
    }
    fn hint_entry(&mut self, k: i32) -> u32 {
        u(&self.g["hints"][1][k as usize])
    }
    fn hint_request(&mut self, n: i32) -> i32 {
        self.requests[n as usize]
    }
    fn set_hint_request(&mut self, n: i32, v: i32) {
        self.requests[n as usize] = v;
    }
    fn stage_number(&mut self) -> i32 {
        let v = self.take_u("stage");
        self.events.push(json!(["stage", v]));
        v as i32
    }
    fn player(&mut self, n: i32) -> Option<PlayerTwp> {
        self.players.get(n as usize).copied().flatten().map(|p| p.0)
    }
    fn player_task(&mut self, n: i32) -> u32 {
        self.players
            .get(n as usize)
            .copied()
            .flatten()
            .map_or(0, |p| p.1)
    }
    fn player_holds(&mut self, n: i32) -> Option<u32> {
        let h = &self.g["holds"][n as usize];
        (!h.is_null()).then(|| u(h))
    }
    fn ccl_is_hit_player(&mut self, task: u32) -> u32 {
        let v = self.take_u("isHit");
        self.events.push(json!(["isHit", task, v]));
        v
    }
    fn fn_800063f4(&mut self, task: u32) -> u32 {
        let v = self.take_u("f63f4");
        self.events.push(json!(["f63f4", task, v]));
        v
    }
    fn fn_80006574(&mut self, task: u32) -> u32 {
        let v = self.take_u("f6574");
        self.events.push(json!(["f6574", task, v]));
        v
    }
    fn force_nodes(&mut self) -> Vec<u32> {
        (0..self.forces.len() as u32).collect()
    }
    fn force_callback(&mut self, node: u32, pos: [f32; 3], fo: &mut Force) {
        let c = self.forces[node as usize].clone();
        self.events.push(json!(["force", node, bits(pos)]));
        fo.flags |= u(&c["flags"]);
        if !c["vector"].is_null() {
            fo.vector = v3(&c["vector"]);
        }
        if !c["direction"].is_null() {
            fo.direction = v3(&c["direction"]);
        }
    }
    fn fog_switch(&mut self) -> i32 {
        self.gi("fog")
    }
    fn alt_body(&mut self) -> i32 {
        self.gi("alt")
    }
    fn alt_body_model(&mut self) -> u32 {
        u(&self.g["alt_model"])
    }
    fn light(&mut self) -> [f32; 8] {
        std::array::from_fn(|k| f(&self.g["light"][k]))
    }
    fn model_word(&mut self, off: u32) -> u32 {
        *self
            .model
            .get(&off)
            .unwrap_or_else(|| panic!("model word {off:#x}"))
    }
    fn set_model_word(&mut self, off: u32, v: u32) {
        assert!(self.model.contains_key(&off), "model word {off:#x}");
        self.model.insert(off, v);
    }
    fn particle_done(&mut self, p: Particle) {
        self.particle = Some(p);
    }
    fn om_event(&mut self, e: Event) {
        let v = match e {
            Event::Sound(id) => json!(["se", id, 0, 0, 0]),
            Event::SoundAt { id, pos } => json!(["se_pos", id, 0, 0, 0, bits(pos)]),
            Event::SoundTimer { id, task, pos } => {
                json!(["se_timer", id, task, 1, 0, 30, bits(pos)])
            }
            Event::Vanish { pos, vel, a, b } => {
                json!(["vanish", bits(pos), bits(vel), a.to_bits(), b.to_bits()])
            }
            Event::FreeTask(t) => json!(["freetask", t]),
            Event::TrailEffect { pos, vel, size, b } => {
                json!([
                    "trail_fx",
                    bits(pos),
                    bits(vel),
                    size.to_bits(),
                    b.to_bits()
                ])
            }
            Event::Fog(on) => json!(["fog", u32::from(on)]),
            Event::SetFog => json!(["setfog"]),
            Event::Light { func, args } => {
                json!([
                    "light",
                    func,
                    args.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                ])
            }
            Event::Disc { matrix } => json!(["disc", 0x801b_6408u32, mbits(&matrix)]),
            Event::Control3D { on, flags } => json!(["ctrl3d", u32::from(on), flags]),
            Event::SetTexture(t) => json!(["settexture", t]),
            Event::Draw {
                func,
                model,
                matrix,
            } => json!(["draw", func, model, mbits(&matrix)]),
            Event::LightReset(k) => json!(["light_reset", k]),
        };
        self.events.push(v);
    }
}

fn trail_from(v: &Value) -> Trail {
    Trail {
        id: u(&v["id"]),
        points_id: u(&v["points_id"]),
        points: vec![[0.0; 3]; i(&v["cap"]) as usize],
        head: i(&v["head"]),
        count: i(&v["count"]),
        last_frame: i(&v["last"]),
    }
}

fn trail_json(t: &Trail) -> Value {
    json!({"id": t.id, "points_id": t.points_id, "cap": t.points.len(), "head": t.head,
           "count": t.count, "last": t.last_frame})
}

const WORK_I: [&str; 19] = [
    "speed",
    "spin",
    "w20",
    "bob",
    "swing",
    "tilt",
    "scale",
    "timer",
    "talk",
    "w4c",
    "follow",
    "player",
    "flash",
    "flash_target",
    "chatter",
    "away",
    "shake",
    "quiet",
    "ridden",
];

fn task_from(id: u32, s: &Value) -> Omochao {
    let t = &s["twp"];
    let w = &s["work"];
    let words: Vec<u32> = s["land"].as_array().unwrap().iter().map(u).collect();
    let words: [u32; WORDS] = words.try_into().unwrap();
    let mut work = Work {
        id: u(&w["id"]),
        goal: v3(&w["goal"]),
        anchor: v3(&w["anchor"]),
        home: v3(&w["home"]),
        window: u(&w["window"]),
        ..Work::default()
    };
    for (k, tr) in work.trails.iter_mut().enumerate() {
        *tr = Some(trail_from(&w["trails"][k]));
    }
    let wi = |k: &str| i(&w[k]);
    let wf = |k: &str| f(&w[k]);
    work.speed = wf("speed");
    work.spin = wi("spin");
    work.w20 = wi("w20");
    work.bob = wi("bob");
    work.swing = wi("swing");
    work.tilt = wi("tilt");
    work.scale = wf("scale");
    work.timer = wi("timer");
    work.talk = wi("talk");
    work.w4c = wi("w4c");
    work.follow = wi("follow");
    work.player = wi("player");
    work.flash = wf("flash");
    work.flash_target = wf("flash_target");
    work.chatter = wi("chatter");
    work.away = wi("away");
    work.shake = wf("shake");
    work.quiet = wi("quiet");
    work.ridden = wi("ridden");
    let c = &s["child"];
    Omochao {
        id,
        mode: i(&s["mode"]),
        child: (!c.is_null()).then(|| Child {
            id: u(&c["id"]),
            flags: i(&c["flags"]) as i16,
            pos: v3(&c["pos"]),
            attrs: c["attrs"].as_array().unwrap().iter().map(u).collect(),
        }),
        twp: Twp {
            mode: i(&t["mode"]) as i8,
            flag1: u(&t["flag1"]) as u8,
            btimer: u(&t["btimer"]) as u8,
            flags: i(&t["flags"]) as i16,
            wtimer: u(&t["wtimer"]) as u16,
            ang: [i(&t["ang"][0]), i(&t["ang"][1]), i(&t["ang"][2])],
            pos: v3(&t["pos"]),
            scale: v3(&t["scale"]),
        },
        vel: v3(&s["vel"]),
        movable: Some(Movable::from_words(&words)),
        cwp_flag: s["cwp_flag"].as_u64().map(|x| x as u16),
        work,
    }
}

fn task_json(o: &Omochao) -> Value {
    let t = &o.twp;
    let w = &o.work;
    let mut work = json!({"id": w.id, "goal": bits(w.goal), "anchor": bits(w.anchor),
        "home": bits(w.home), "window": w.window,
        "trails": w.trails.iter().map(|t| trail_json(t.as_ref().unwrap())).collect::<Vec<_>>()});
    let vals: [Value; 19] = [
        json!(w.speed.to_bits()),
        json!(w.spin),
        json!(w.w20),
        json!(w.bob),
        json!(w.swing),
        json!(w.tilt),
        json!(w.scale.to_bits()),
        json!(w.timer),
        json!(w.talk),
        json!(w.w4c),
        json!(w.follow),
        json!(w.player),
        json!(w.flash.to_bits()),
        json!(w.flash_target.to_bits()),
        json!(w.chatter),
        json!(w.away),
        json!(w.shake.to_bits()),
        json!(w.quiet),
        json!(w.ridden),
    ];
    for (k, v) in WORK_I.iter().zip(vals) {
        work[*k] = v;
    }
    json!({
        "mode": o.mode, "vel": bits(o.vel),
        "land": o.movable.as_ref().unwrap().to_words().to_vec(),
        "cwp_flag": o.cwp_flag,
        "twp": {"mode": t.mode, "flag1": t.flag1, "btimer": t.btimer, "flags": t.flags,
                "wtimer": t.wtimer, "ang": t.ang, "pos": bits(t.pos), "scale": bits(t.scale)},
        "work": work,
        "child": o.child.as_ref().map_or(Value::Null, |c| json!({"id": c.id, "flags": c.flags,
            "pos": bits(c.pos), "attrs": c.attrs})),
    })
}

fn rec_for(c: &Value, s5: u32) -> Rec {
    let players = c["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (!p.is_null()).then(|| {
                (
                    PlayerTwp {
                        ang: [i(&p["ang"][0]), i(&p["ang"][1]), i(&p["ang"][2])],
                        pos: v3(&p["pos"]),
                    },
                    u(&p["task"]),
                )
            })
        })
        .collect();
    let g = c["globals"].clone();
    let mut q = HashMap::new();
    if let Some(m) = c["queues"].as_object() {
        for (k, v) in m {
            q.insert(k.clone(), queue(v));
        }
    }
    let script = c["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e[0] == "shadow")
        .cloned()
        .collect();
    Rec {
        s5,
        q,
        script,
        out: c["out"].as_bool().unwrap_or(false),
        requests: [i(&g["requests"][0]), i(&g["requests"][1])],
        forces: g["forces"].as_array().unwrap().clone(),
        g,
        players,
        ..Rec::default()
    }
}

fn first_diff(a: &[Value], b: &[Value]) -> String {
    for (k, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            return format!("event {k}: port {x}\n oracle {y}");
        }
    }
    format!("lengths {} vs {}", a.len(), b.len())
}

#[test]
#[ignore]
fn omochao_exec_matches_stg42d() {
    let Some(o) = oracle("exec") else {
        return;
    };
    let s5 = u(&o["s5"]);
    let cases = o["cases"].as_array().unwrap();
    for (n, c) in cases.iter().enumerate() {
        let mut host = rec_for(c, s5);
        let mut t = task_from(u(&c["id"]), &c["before"]);
        let mut rand = MslRand {
            seed: u(&c["globals"]["seed"]),
        };
        let mut stack = MatrixStack::new();
        omochao::exec(&mut t, &STG42D, &mut stack, &mut rand, &mut host);
        let ev = c["events"].as_array().unwrap();
        assert!(
            host.events == *ev,
            "case {n}: {}",
            first_diff(&host.events, ev)
        );
        assert_eq!(task_json(&t), c["after"], "case {n}: state");
        assert_eq!(rand.seed, u(&c["seed_after"]), "case {n}: rand");
        assert_eq!(
            json!(host.requests),
            c["requests_after"],
            "case {n}: requests"
        );
        let p = host.particle.map(|p| json!(p.to_words().to_vec()));
        assert_eq!(
            p.unwrap_or(Value::Null),
            c["particle"],
            "case {n}: particle"
        );
        assert_eq!(stack.depth(), 0);
    }
    eprintln!("{} exec cases", cases.len());
}

#[test]
#[ignore]
fn omochao_display_matches_stg42d() {
    let Some(o) = oracle("display") else {
        return;
    };
    let s5 = u(&o["s5"]);
    let cases = o["cases"].as_array().unwrap();
    for (n, c) in cases.iter().enumerate() {
        let mut host = rec_for(c, s5);
        for w in c["model"].as_array().unwrap() {
            host.model.insert(u(&w[0]), u(&w[1]));
        }
        let mut t = task_from(u(&c["id"]), &c["before"]);
        let mut stack = MatrixStack::new();
        let mv: Vec<f32> = c["matrix"].as_array().unwrap().iter().map(f).collect();
        let mat: Mtx = std::array::from_fn(|r| std::array::from_fn(|k| mv[4 * r + k]));
        *stack.current_mut() = mat;
        omochao::display(&mut t, &STG42D, &mut stack, &mut host);
        let ev = c["events"].as_array().unwrap();
        assert!(
            host.events == *ev,
            "case {n}: {}",
            first_diff(&host.events, ev)
        );
        assert_eq!(task_json(&t), c["after"], "case {n}: state");
        for w in c["model_after"].as_array().unwrap() {
            assert_eq!(
                host.model[&u(&w[0])],
                u(&w[1]),
                "case {n}: model word {}",
                w[0]
            );
        }
        assert_eq!(stack.depth(), 0);
        assert_eq!(mbits(stack.current()), c["matrix"]);
    }
    eprintln!("{} display cases", cases.len());
}
