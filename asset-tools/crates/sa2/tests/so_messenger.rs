//! Compare `stage_objects::messenger` (MSGER / Omochao and the ribbon-trail
//! library) with stg42D.rel's own code run under the main.dol harness
//! (tests/so_messenger_ppc.py), case by case and frame by frame: the calls
//! the REL code makes and the state it leaves.
use sa2_asset_tools::ninja_matrix::{MatrixStack, Mtx};
use sa2_asset_tools::prison_visuals::MslRand;
use sa2_asset_tools::stage_objects::{
    dyn_land::{Movable, MovableHost, ShadowBlock, WORDS},
    host::{Event as HostEvent, ObjectHost},
    messenger::{
        self,
        omochao::PlaceHost,
        shadow::{self, Gx, Shadow, ShadowHost},
        trail::{self, Trail, TrailDesc, TrailDraw, TrailHost},
        Event, MessengerHost, MessengerTask, NewTask, SpawnArgs, Spawned, Twp, Work, STG42D,
    },
};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf, process::Command};

fn oracle(object: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_messenger_ppc.py"))
        .arg(object)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    Some(serde_json::from_slice(&o.stdout).unwrap())
}

fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}
fn deque<T>(v: &Value, g: impl Fn(&Value) -> T) -> VecDeque<T> {
    v.as_array().unwrap().iter().map(g).collect()
}

/// Records calls in the oracle's JSON shape.
#[derive(Default)]
struct Recorder {
    events: Vec<Value>,
    children: Vec<Value>,
    next: u32,
    out: bool,
    calloc_ok: VecDeque<bool>,
    tasks_ok: VecDeque<bool>,
    windows: VecDeque<u32>,
    bones: VecDeque<Option<[f32; 3]>>,
    chara: VecDeque<i32>,
    player0: Option<[f32; 3]>,
    body: u32,
    state: i32,
    frame: i32,
    reset: i8,
    count: i32,
    entries: Vec<u32>,
    language: i8,
    player_angles: Vec<[i32; 3]>,
    screen: i32,
    s5: u32,
}

impl Recorder {
    fn plans(&mut self, p: &Value) {
        self.calloc_ok = deque(&p["calloc_ok"], |b| b.as_bool().unwrap());
        self.tasks_ok = deque(&p["tasks_ok"], |b| b.as_bool().unwrap());
        self.windows = deque(&p["windows"], u);
        self.bones = deque(&p["bones"], |b| (!b.is_null()).then(|| v3(b)));
        self.chara = deque(&p["chara"], i);
        self.player0 = (!p["player0"].is_null()).then(|| v3(&p["player0"]));
    }
    fn new_id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
    fn task(&mut self, ev: Value) -> Option<NewTask> {
        let mut ev = ev.as_array().unwrap().clone();
        if !self.tasks_ok.pop_front().unwrap_or(true) {
            ev.push(json!(0));
            self.events.push(json!(ev));
            return None;
        }
        let id = self.new_id();
        ev.push(json!(id));
        self.events.push(json!(ev));
        Some(NewTask { id, has_mwp: true })
    }
}

impl ObjectHost for Recorder {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, _: HostEvent) {
        unreachable!()
    }
}

impl TrailHost for Recorder {
    fn sy_calloc(&mut self, n: u32, size: u32) -> Option<u32> {
        if !self.calloc_ok.pop_front().unwrap_or(true) {
            self.events.push(json!(["calloc", n, size, 0]));
            return None;
        }
        let id = self.new_id();
        self.events.push(json!(["calloc", n, size, id]));
        Some(id)
    }
    fn sy_free(&mut self, id: u32) {
        self.events.push(json!(["free", id]));
    }
    fn game_state(&mut self) -> i32 {
        self.state
    }
    fn frame(&mut self) -> i32 {
        self.frame
    }
    fn trail_reset(&mut self) -> i8 {
        self.reset
    }
    fn trail_draw(&mut self, d: TrailDraw) {
        match d {
            TrailDraw::Plain(v) => {
                let v: Vec<Value> = v
                    .iter()
                    .map(|(p, c)| json!([p[0].to_bits(), p[1].to_bits(), p[2].to_bits(), c]))
                    .collect();
                self.events.push(json!(["draw", v, 1]));
            }
            TrailDraw::Textured { texture, vertices } => {
                self.events.push(json!(["settexnum", texture]));
                let v: Vec<Value> = vertices
                    .iter()
                    .map(|(p, s, t, c)| {
                        json!([
                            p[0].to_bits(),
                            p[1].to_bits(),
                            p[2].to_bits(),
                            s.to_bits(),
                            t.to_bits(),
                            c
                        ])
                    })
                    .collect();
                self.events.push(json!(["draw_tex", v, 1]));
            }
        }
    }
}

impl MovableHost for Recorder {
    fn sy_calloc_801197fc(&mut self, count: u32, size: u32) -> bool {
        match self.sy_calloc(count, size) {
            Some(id) => {
                self.body = id;
                true
            }
            None => false,
        }
    }
    fn sy_free_80119828(&mut self) {
        let id = self.body;
        self.sy_free(id);
    }
    fn get_shadow_pos_xyz_80067210(&mut self, _: &mut ShadowBlock) {
        unreachable!()
    }
    fn shadow_water_80066e1c(&mut self, _: &mut ShadowBlock) -> bool {
        unreachable!()
    }
    fn byte_801cc19f(&self) -> i8 {
        unreachable!()
    }
}

impl PlaceHost for Recorder {
    fn fn_80038008(&mut self, n: i32, node: i32) -> Option<[f32; 3]> {
        let r = self.bones.pop_front().flatten();
        self.events
            .push(json!(["bone", n, node, u32::from(r.is_some())]));
        r
    }
    fn player_position(&mut self, n: i32) -> Option<[f32; 3]> {
        if n == 0 {
            self.player0
        } else {
            None
        }
    }
    fn player_character(&mut self, n: i32) -> i32 {
        let v = self.chara.pop_front().unwrap_or(0);
        self.events.push(json!(["chara", n, v]));
        v
    }
}

impl MessengerHost for Recorder {
    fn ccl_init(&mut self, task: u32, rows: u32, count: u8, list: u8) {
        self.events.push(json!(["ccl", task, rows, count, list]));
    }
    fn create_child_task(&mut self, flags: u8, exec: u32, parent: u32) -> Option<NewTask> {
        self.task(json!(["child", flags, exec, parent]))
    }
    fn create_elemental_task(
        &mut self,
        flags: u8,
        level: u8,
        exec: u32,
        name: u32,
    ) -> Option<NewTask> {
        self.task(json!(["elem", flags, level, exec, name]))
    }
    fn message_open(&mut self, kind: i32, text: u32, time: u32, language: i8, owner: u32) -> u32 {
        // The default text is a section-5 offset; the oracle sees its address.
        let text = if text == STG42D.default_text && !self.entries.contains(&text) {
            self.s5 + text
        } else {
            text
        };
        self.events.push(json!([
            "msg_open",
            kind as u32,
            text,
            time,
            language,
            owner,
            0
        ]));
        self.windows.pop_front().unwrap_or(0)
    }
    fn language(&mut self) -> i8 {
        self.language
    }
    fn message_count(&mut self) -> i32 {
        self.count
    }
    fn message_entry(&mut self, i: i32) -> u32 {
        self.entries[i as usize]
    }
    fn player_angles(&mut self, n: i32) -> [i32; 3] {
        self.player_angles[n as usize]
    }
    fn screen(&mut self) -> i32 {
        self.screen
    }
    fn msg_event(&mut self, e: Event) {
        let v = match e {
            Event::ChildInit { task, pos } => {
                self.children.push(json!([task, bits(pos), 6]));
                return;
            }
            Event::DestroyTask(t) => json!(["destroy", t]),
            Event::CollisionEntry { task } => json!(["entry", task]),
            Event::Sound(id) => json!(["se", id, 0, 0, 0]),
            Event::Fog(on) => json!(["fog", u8::from(on)]),
            Event::SetFog => json!(["setfog"]),
            Event::Blend(a, b) => json!(["blend", a, b]),
            Event::SetTexture(t) => json!(["settexture", t]),
            Event::DrawModel { model, matrix } => {
                let m: Vec<u32> = matrix.iter().flatten().map(|x| x.to_bits()).collect();
                json!(["model", model, m])
            }
        };
        self.events.push(v);
    }
}

impl ShadowHost for Recorder {
    fn gx(&mut self, call: Gx) {
        let fl = |v: &[f32]| json!(v.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        let v = match call {
            Gx::InitTexObj {
                owner,
                buf,
                width,
                height,
                format,
                wrap_s,
                wrap_t,
                mipmap,
            } => json!(["tex_init", owner, buf, width, height, format, wrap_s, wrap_t, mipmap]),
            Gx::InitTexObjLod {
                owner,
                min_filt,
                mag_filt,
                lods,
                bias_clamp,
                edge_lod,
                max_aniso,
            } => json!([
                "tex_lod",
                owner,
                min_filt,
                mag_filt,
                fl(&lods),
                bias_clamp,
                edge_lod,
                max_aniso
            ]),
            Gx::InvalidateTexAll => json!(["tex_inval"]),
            Gx::Viewport(v) => json!(["viewport", fl(&v)]),
            Gx::Scissor(v) => json!(["scissor", v]),
            Gx::Fog(on) => json!(["fog", u8::from(on)]),
            Gx::SetFog => json!(["setfog"]),
            Gx::Projection { m, kind } => json!(["projection", fl(m.as_flattened()), kind]),
            Gx::TexCopySrc(v) => json!(["copy_src", v]),
            Gx::TexCopyDst {
                width,
                height,
                format,
                mipmap,
            } => json!(["copy_dst", width, height, format, mipmap]),
            Gx::CopyTex { buf, clear } => json!(["copy_tex", buf, clear]),
            Gx::PixModeSync => json!(["pix_sync"]),
            Gx::Queue {
                kind,
                pos,
                size,
                owner,
                buf,
            } => json!([
                "shadow_queue",
                kind,
                bits(pos),
                size.to_bits(),
                owner,
                buf,
                owner
            ]),
        };
        self.events.push(v);
    }
}

fn shadow_from(v: &Value) -> Shadow {
    Shadow {
        matrix: matrix(&v["matrix"]),
        size: u(&v["size"]),
        buf: u(&v["buf"]),
    }
}

fn shadow_json(s: &Shadow) -> Value {
    json!({
        "matrix": s.matrix.as_flattened().iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "size": s.size, "buf": s.buf,
    })
}

fn trail_from(v: &Value) -> Option<Trail> {
    if v.is_null() {
        return None;
    }
    Some(Trail {
        id: u(&v["id"]),
        points_id: u(&v["points_id"]),
        points: v["points"].as_array().unwrap().iter().map(v3).collect(),
        head: i(&v["head"]),
        count: i(&v["count"]),
        last_frame: i(&v["last"]),
    })
}

fn trail_json(t: Option<&Trail>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => json!({
            "id": t.id, "points_id": t.points_id,
            "points": t.points.iter().map(|p| bits(*p)).collect::<Vec<_>>(),
            "head": t.head, "count": t.count, "last": t.last_frame,
        }),
    }
}

fn desc_from(v: &Value) -> TrailDesc {
    TrailDesc {
        flags: u(&v["flags"]),
        width_head: f(&v["head_w"]),
        width_tail: f(&v["tail_w"]),
        color_head: u(&v["head_c"]),
        color_tail: u(&v["tail_c"]),
        texture: u(&v["tex"]),
    }
}

fn matrix(v: &Value) -> Mtx {
    let w: Vec<f32> = v.as_array().unwrap().iter().map(f).collect();
    [0, 1, 2].map(|r| [0, 1, 2, 3].map(|c| w[4 * r + c]))
}

fn clock(host: &mut Recorder, fr: &Value) {
    host.state = i(&fr["state"]);
    host.frame = i(&fr["frame"]);
    host.reset = i(&fr["reset"]) as i8;
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR, the Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn trails_match_stg42d() {
    let Some(c) = oracle("trail") else {
        return;
    };
    // Omochao's descriptor is the REL's (section 5 +0x173BC).
    let d = &c["messenger_desc"];
    let m = trail::MESSENGER_TRAIL;
    assert_eq!(
        [u(&d[0]), u(&d[1]), u(&d[2]), u(&d[3]), u(&d[4]), u(&d[6])],
        [
            m.flags,
            m.width_head.to_bits(),
            m.width_tail.to_bits(),
            m.color_head,
            m.color_tail,
            m.texture
        ]
    );
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 340);
    let mut frames = 0;
    for case in cases {
        match case["kind"].as_str().unwrap() {
            "alloc" => {
                let mut h = Recorder {
                    next: 1,
                    calloc_ok: deque(&case["calloc_ok"], |b| b.as_bool().unwrap()),
                    ..Default::default()
                };
                let t = trail::alloc(i(&case["n"]), &mut h);
                assert_eq!(json!(h.events), case["alloc_events"], "{case}");
                assert_eq!(trail_json(t.as_ref()), case["trail"], "{case}");
                h.events.clear();
                trail::free(t, &mut h);
                assert_eq!(json!(h.events), case["free_events"], "{case}");
            }
            "drop" => {
                let mut t = trail_from(&case["trail"]).unwrap();
                let r = trail::drop_oldest(&mut t);
                assert_eq!(u32::from(r), u(&case["ret"]), "{case}");
                assert_eq!(trail_json(Some(&t)), case["after"], "{case}");
            }
            _ => {
                let mut t = trail_from(&case["trail"]).unwrap();
                let desc = (!case["desc"].is_null()).then(|| desc_from(&case["desc"]));
                for fr in case["frames"].as_array().unwrap() {
                    let mut h = Recorder::default();
                    clock(&mut h, fr);
                    let mut stack = MatrixStack::new();
                    *stack.current_mut() = matrix(&fr["matrix"]);
                    trail::step(
                        desc.as_ref(),
                        Some(&mut t),
                        v3(&fr["point"]),
                        &mut stack,
                        &mut h,
                    );
                    assert_eq!(json!(h.events), fr["events"], "{case}");
                    assert_eq!(trail_json(Some(&t)), fr["after"], "{case}");
                    frames += 1;
                }
            }
        }
    }
    eprintln!("{} cases, {frames} step frames", cases.len());
}

fn twp_from(v: &Value) -> Twp {
    Twp {
        mode: u(&v["mode"]) as u8,
        flag1: u(&v["flag1"]) as u8,
        id: u(&v["id"]) as u8,
        flags: i(&v["flags"]) as i16,
        wtimer: u(&v["wtimer"]) as u16,
        ang: [i(&v["ang"][0]), i(&v["ang"][1]), i(&v["ang"][2])],
        pos: v3(&v["pos"]),
        scale: v3(&v["scale"]),
    }
}

fn work_from(v: &Value) -> Option<Work> {
    if v.is_null() {
        return None;
    }
    let mut w = Work::new(u(&v["id"]));
    w.pos = v3(&v["pos"]);
    w.anchor = v3(&v["anchor"]);
    w.bob = i(&v["bob"]);
    w.f30 = f(&v["f30"]);
    w.sub_task = u(&v["sub_task"]);
    w.home = v3(&v["home"]);
    w.w4c = i(&v["w4c"]);
    w.w50 = u(&v["w50"]);
    w.player = i(&v["player"]);
    for (k, t) in w.trails.iter_mut().enumerate() {
        *t = trail_from(&v["trails"][k]);
    }
    w.shadow = shadow_from(&v["shadow"]);
    Some(w)
}

fn task_from(id: u32, has_mwp: bool, s: &Value) -> MessengerTask {
    MessengerTask {
        id,
        mode: i(&s["mode"]),
        has_mwp,
        mwp_30: s["mwp_30"].as_i64().unwrap_or(0) as u32,
        body: s["land"].as_array().map(|a| {
            let w: Vec<u32> = a.iter().map(u).collect();
            let w: [u32; WORDS] = w.try_into().unwrap();
            Movable::from_words(&w)
        }),
        callbacks: None,
        twp: twp_from(&s["twp"]),
        work: work_from(&s["work"]),
    }
}

fn task_json(t: &MessengerTask) -> Value {
    let cb = t.callbacks.map_or([0; 5], |c| {
        [c.exec, c.disp, c.dest, c.disp_sort, c.disp_shad]
    });
    let tw = &t.twp;
    let work = t.work.as_ref().map_or(Value::Null, |w| {
        json!({
            "id": w.id, "pos": bits(w.pos), "anchor": bits(w.anchor), "bob": w.bob, "f30": w.f30.to_bits(),
            "sub_task": w.sub_task, "home": bits(w.home), "w4c": w.w4c, "w50": w.w50,
            "player": w.player,
            "trails": w.trails.iter().map(|t| trail_json(t.as_ref())).collect::<Vec<_>>(),
            "shadow": shadow_json(&w.shadow),
        })
    });
    json!({
        "callbacks": cb, "mode": t.mode,
        "mwp_30": if t.has_mwp { json!(t.mwp_30) } else { Value::Null },
        "land": t.body.as_ref().map(|b| b.to_words().to_vec()),
        "twp": {"mode": tw.mode, "flag1": tw.flag1, "id": tw.id, "flags": tw.flags,
                "wtimer": tw.wtimer, "ang": tw.ang,
                "pos": bits(tw.pos), "scale": bits(tw.scale)},
        "work": work,
    })
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR, the Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn messenger_matches_stg42d() {
    let Some(c) = oracle("messenger") else {
        return;
    };
    let s5 = u(&c["s5"]);
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 390);
    for case in cases {
        match case["kind"].as_str().unwrap() {
            "create" => {
                let id = u(&case["id"]);
                let mut task = task_from(id, case["has_mwp"].as_bool().unwrap(), &case["before"]);
                let mut h = Recorder {
                    next: id + 1,
                    out: case["out"].as_bool().unwrap(),
                    s5,
                    ..Default::default()
                };
                h.plans(&case["plans"]);
                let mut rand = MslRand {
                    seed: u(&case["plans"]["seed"]),
                };
                messenger::create(&mut task, &STG42D, &mut rand, &mut h);
                assert_eq!(rand.seed, u(&case["seed_after"]), "{case}");
                assert_eq!(json!(h.events), case["events"], "{case}");
                assert_eq!(task_json(&task), case["created"], "{case}");
                assert_eq!(json!(h.children), case["children"], "{case}");
                if let Some(w) = task.work.as_mut() {
                    w.sub_task = u(&case["sub_task"]);
                    h.events.clear();
                    messenger::destroy(&mut task, &mut h);
                    assert_eq!(json!(h.events), case["destroy_events"], "{case}");
                    assert!(task.work.is_none());
                    assert_eq!(u(&case["after"]), 0);
                } else {
                    assert!(case["destroy_events"].is_null());
                }
            }
            "spawn" => {
                let a = &case["args"];
                let args = SpawnArgs {
                    pos: v3(&a["pos"]),
                    yaw: i(&a["yaw"]),
                    w50: u(&a["w50"]),
                    message: i(&a["message"]),
                    mode: i(&a["mode"]),
                    parent: a["parent"].as_i64().map(|p| p as u32),
                };
                let mut h = Recorder {
                    next: u(&case["first_id"]),
                    count: i(&case["count"]),
                    entries: case["entries"].as_array().unwrap().iter().map(u).collect(),
                    language: i(&case["language"]) as i8,
                    s5,
                    ..Default::default()
                };
                h.plans(&case["plans"]);
                let mut rand = MslRand {
                    seed: u(&case["plans"]["seed"]),
                };
                let r = messenger::spawn(&args, &STG42D, &mut rand, &mut h);
                assert_eq!(rand.seed, u(&case["seed_after"]), "{case}");
                assert_eq!(json!(h.events), case["events"], "{case}");
                match r {
                    None => assert_eq!(u(&case["ret"]), 0, "{case}"),
                    Some(Spawned::Messenger(t)) => {
                        assert_eq!(t.id, u(&case["ret"]), "{case}");
                        assert_eq!(task_json(&t), case["task"], "{case}");
                        assert_eq!(json!(h.children), case["children"], "{case}");
                    }
                    Some(Spawned::Message { task, window }) => {
                        assert_eq!(task, u(&case["ret"]), "{case}");
                        assert_eq!(window, u(&case["window"]), "{case}");
                        h.events.clear();
                        messenger::message_exec(task, window, &mut h);
                        assert_eq!(json!(h.events), case["exec_events"], "{case}");
                        h.events.clear();
                        messenger::message_destroy(window, &mut h);
                        assert_eq!(json!(h.events), case["dest_events"], "{case}");
                    }
                }
            }
            _ => {
                let mut h = Recorder::default();
                let pexec = case["parent_exec"].as_i64().map(|p| p as u32);
                messenger::collision_child_exec(u(&case["id"]), pexec, &mut h);
                assert_eq!(json!(h.events), case["events"], "{case}");
            }
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR, the Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn displays_match_stg42d() {
    let Some(c) = oracle("display") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 240);
    for case in cases {
        let mut task = task_from(u(&case["id"]), true, &case["before"]);
        let mut h = Recorder {
            player_angles: case["player_angles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| [i(&a[0]), i(&a[1]), i(&a[2])])
                .collect(),
            screen: i(&case["screen"]),
            ..Default::default()
        };
        clock(&mut h, &case["clock"]);
        let mut stack = MatrixStack::new();
        let m = matrix(&case["matrix"]);
        *stack.current_mut() = m;
        if u(&case["fn"]) == 0x18040 {
            messenger::display_trails(&mut task, &STG42D, &mut stack, &mut h);
        } else {
            messenger::display_shadow(&mut task, &STG42D, &mut stack, &mut h);
        }
        assert_eq!(*stack.current(), m);
        assert_eq!(json!(h.events), case["events"], "{case}");
        // The callbacks are not part of the work the displays touch.
        let mut after = case["after"].clone();
        after["callbacks"] = json!([0, 0, 0, 0, 0]);
        let mut got = task_json(&task);
        got["callbacks"] = json!([0, 0, 0, 0, 0]);
        assert_eq!(got, after, "{case}");
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR, the Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn shadow_helpers_match_stg42d() {
    let Some(c) = oracle("shadow") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1000);
    for case in cases {
        let id = u(&case["id"]);
        let mut h = Recorder {
            next: id + 1,
            ..Default::default()
        };
        let mut stack = MatrixStack::new();
        if case["kind"] == "init" {
            h.calloc_ok = deque(&case["calloc_ok"], |b| b.as_bool().unwrap());
            let mut sh = Shadow::default();
            shadow::init(&mut sh, id, u(&case["size"]), &mut h);
            assert_eq!(json!(h.events), case["events"], "{case}");
            assert_eq!(shadow_json(&sh), case["after"], "{case}");
            h.events.clear();
            shadow::free(&mut sh, &mut h);
            shadow::free(&mut sh, &mut h);
            assert_eq!(json!(h.events), case["free_events"], "{case}");
            assert_eq!(shadow_json(&sh), case["freed"], "{case}");
            continue;
        }
        let mut sh = shadow_from(&case["before"]);
        let m = matrix(&case["matrix"]);
        *stack.current_mut() = m;
        let pos = v3(&case["pos"]);
        match case["kind"].as_str().unwrap() {
            "end" => {
                stack.push();
                shadow::end(&sh, &mut stack, &mut h);
                assert_eq!(stack.depth(), 0);
            }
            "draw" => shadow::draw(
                u(&case["queue_kind"]),
                pos,
                f(&case["size"]),
                id,
                &sh,
                &mut h,
            ),
            kind => {
                let a = |k: usize| case["angles"][k].as_i64().map(|x| x as i32);
                let (hw, sin) = (f(&case["half_width"]), f(&case["sin"]));
                if kind == "capture" {
                    shadow::capture(&mut sh, hw, pos, a(0), a(1), sin, &mut stack, &mut h);
                } else {
                    let ts = f(&case["tex_scale"]);
                    shadow::capture_ex(&mut sh, hw, ts, sin, pos, a(0), a(1), &mut stack, &mut h);
                }
                assert_eq!(stack.depth(), 1);
                assert_eq!(*stack.current(), matrix(&case["current"]), "{case}");
                assert_eq!(json!(h.events), case["capture_events"], "{case}");
                h.events.clear();
                shadow::end(&sh, &mut stack, &mut h);
                assert_eq!(*stack.current(), m);
            }
        }
        assert_eq!(json!(h.events), case["events"], "{case}");
        assert_eq!(shadow_json(&sh), case["after"], "{case}");
    }
}

/// The ported routines are the same code in the other stage RELs apart
/// from module-local branch displacements (`b`/`bl` within section 1, which
/// a REL resolves at link time, so they differ wherever the stage's code
/// before the target differs): with those masked too, each routine appears
/// in at least 17 other GameCube stage RELs (the message routines are in
/// 17, the rest in 61 to 65).
#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn routines_are_shared_with_local_branches_masked() {
    let dir = PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").expect("SA2_GC_REL_DIR"));
    let mask = |t: Vec<u8>| -> Vec<u8> {
        t.chunks(4)
            .flat_map(|w| {
                let x = u32::from_be_bytes(w.try_into().unwrap_or([0; 4]));
                let x = if x >> 26 == 18 { x & 0xfc00_0003 } else { x };
                x.to_be_bytes()
            })
            .collect()
    };
    let text = |p: &std::path::Path| {
        mask(sa2_asset_tools::stage_objects::masked_text(&std::fs::read(p).unwrap()).unwrap())
    };
    let reference = text(&dir.join("stg42D.rel"));
    let others: Vec<Vec<u8>> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
            n.ends_with(".rel")
                && n != "stg42d.rel"
                && (n.starts_with("stg") || n.starts_with("boss"))
        })
        .map(|p| text(&p))
        .collect();
    for r in messenger::ROUTINES {
        let code = &reference[r.stg42d as usize..(r.stg42d + r.len) as usize];
        let n = others
            .iter()
            .filter(|t| {
                (0..t.len().saturating_sub(code.len()))
                    .step_by(4)
                    .any(|o| &t[o..o + code.len()] == code)
            })
            .count();
        eprintln!("{:40} in {n:2} other RELs (local branches masked)", r.name);
        assert!(n >= 17, "{}", r.name);
    }
}
