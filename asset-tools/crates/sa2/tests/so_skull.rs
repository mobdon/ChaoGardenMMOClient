//! Compare `stage_objects::skull` (SKULL, PSKULL) and
//! `stage_objects::udreel_stoplsd` (UDREEL, STOPLSD) with stg42D.rel's own
//! object code run under the main.dol harness (tests/so_skull_ppc.py), case
//! by case and frame by frame: the events the REL code calls out to and the
//! state it leaves.
use sa2_asset_tools::enemy_lib::Rand;
use sa2_asset_tools::land_probe::{Hit, Shadow};
use sa2_asset_tools::stage_objects::{
    dyn_land::{self, Movable, MovableHost, ShadowBlock, WORDS},
    host::{Event, ObjectHost},
    skull::{self, Draw, SkullEvent, SkullHost, SkullPlayer, SkullTask, Twp},
    udreel_stoplsd::{
        self as reel, ReelData, ReelDisplayHost, ReelEvent, ReelHost, ReelModels, ReelPlayer,
        ReelTask, ReelWork, StopLsdEvent, StopLsdHost, StopLsdTask,
    },
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::{path::PathBuf, process::Command};

fn oracle(group: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_skull_ppc.py"))
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
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}

fn twp(v: &Value) -> Twp {
    Twp {
        mode: i(&v["mode"]) as i8,
        id: i(&v["id"]) as i8,
        b2: u(&v["b2"]) as u8,
        b3: u(&v["b3"]) as u8,
        flag: i(&v["flag"]) as i16,
        w6: u(&v["w6"]) as u16,
        ang: [0, 1, 2].map(|k| i(&v["ang"][k]) as i32),
        pos: v3(&v["pos"]),
        scale: v3(&v["scale"]),
    }
}

fn twp_json(t: &Twp) -> Value {
    json!({"mode": t.mode, "id": t.id, "b2": t.b2, "b3": t.b3, "flag": t.flag, "w6": t.w6,
           "ang": t.ang, "pos": bits(t.pos), "scale": bits(t.scale)})
}

/// Draw calls in the oracle's shape; `f20` follows the model writes.
fn draw_json(d: Draw, f20: &mut HashMap<u32, u32>) -> Option<Value> {
    Some(match d {
        Draw::SetTexture(a) => json!(["tex", a]),
        Draw::Push => json!(["push"]),
        Draw::Pop => json!(["pop"]),
        Draw::TranslateEx(v) => json!(["translate_ex", bits(v)]),
        Draw::Translate(v) => json!(["translate", bits(v)]),
        Draw::RotateY(a) => json!(["rot_y", a]),
        Draw::Scale(v) => json!(["scale", bits(v)]),
        Draw::Model { routine, model } => json!(["model", routine, model, f20[&model]]),
        Draw::On3D(x) => json!(["on3d", x]),
        Draw::Off3D(x) => json!(["off3d", x]),
        Draw::Call(r) => json!(["call", r]),
        Draw::Blend(a, b) => json!(["blend", a, b]),
        Draw::Color(c) => json!(["color", c.map(f32::to_bits)]),
        Draw::ModelF20 { model, value } => {
            f20.insert(model, value.to_bits());
            return None;
        }
    })
}

fn collision(e: Event) -> Value {
    match e {
        Event::CollisionInit { rows, count, list } => json!(["init", rows, count, list]),
        Event::CollisionEntry => json!(["entry"]),
        Event::CollisionCalcRange => json!(["calc", null]),
        Event::CollisionSizes(s) => json!(["sizes", bits(s)]),
    }
}

// --- SKULL / PSKULL ---------------------------------------------------------

struct SkullRec {
    out: bool,
    cond: i8,
    /// syCalloc succeeds; the byte at 0x801CC19F; the oracle's land
    /// probe results, in order.
    calloc: bool,
    g: i8,
    script: VecDeque<Value>,
    players: [Option<SkullPlayer>; 2],
    rand: Rand,
    f20: HashMap<u32, u32>,
    events: Vec<Value>,
}

impl ObjectHost for SkullRec {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: Event) {
        self.events.push(collision(e));
    }
}

impl SkullRec {
    /// Replays one scripted GetShadowPosXYZ / fn_80066E1C result.
    fn shadow(&mut self, block: &mut ShadowBlock, water: bool) -> bool {
        let e = self.script.pop_front().expect("a scripted probe");
        let mut s = Shadow::default();
        for (r, ray) in e[3].as_array().unwrap().iter().enumerate() {
            s.coords[r] = f(&ray[1]);
            if u(&ray[0]) != 0 {
                s.hits[r] = Some(Hit {
                    flags: u(&ray[2]),
                    angle_x: i(&ray[3]) as i32,
                    angle_z: i(&ray[4]) as i32,
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

impl MovableHost for SkullRec {
    fn sy_calloc_801197fc(&mut self, count: u32, size: u32) -> bool {
        self.events.push(json!(["calloc", count, size]));
        self.calloc
    }
    fn sy_free_80119828(&mut self) {
        self.events.push(json!(["free"]));
    }
    fn get_shadow_pos_xyz_80067210(&mut self, block: &mut ShadowBlock) {
        self.shadow(block, false);
    }
    fn shadow_water_80066e1c(&mut self, block: &mut ShadowBlock) -> bool {
        self.shadow(block, true)
    }
    fn byte_801cc19f(&self) -> i8 {
        self.g
    }
}

impl SkullHost for SkullRec {
    fn condition(&mut self) -> i8 {
        self.events.push(json!(["cond_get"]));
        self.cond
    }
    fn players(&self) -> [Option<SkullPlayer>; 2] {
        self.players
    }
    fn rand(&mut self) -> &mut Rand {
        &mut self.rand
    }
    fn skull(&mut self, e: SkullEvent) {
        let v = match e {
            SkullEvent::SetCondition => json!(["cond_set", 1]),
            SkullEvent::Drop { kind, at } => json!(["drop", kind, bits(at)]),
            SkullEvent::DeadOut => json!(["dead_out"]),
            SkullEvent::FreeTask => json!(["free_task"]),
            SkullEvent::Draw(d) => match draw_json(d, &mut self.f20) {
                Some(v) => v,
                None => return,
            },
        };
        self.events.push(v);
    }
}

fn skull_state(t: &SkullTask) -> Value {
    json!({"twp": twp_json(&t.twp), "cb": [t.exec, t.disp, t.dest, 0], "alpha": t.alpha.to_bits(),
           "attr": t.attr, "speed": bits(t.speed), "mwp30": t.mwp_30,
           "land": t.land.map(|l| json!(l.to_words().to_vec()))})
}

fn skull_players(v: &Value) -> [Option<SkullPlayer>; 2] {
    [0, 1].map(|k| {
        let p = &v[k];
        (!p.is_null()).then(|| SkullPlayer {
            rides: p["rides"].as_bool().unwrap(),
            ang_y: i(&p["ang_y"]) as i32,
            center: (!p["center"].is_null()).then(|| v3(&p["center"])),
        })
    })
}

fn skull_group(group: &str) -> usize {
    let Some(c) = oracle(group) else {
        return 0;
    };
    let cases = c["cases"].as_array().unwrap();
    for case in cases {
        let kind = case["kind"].as_str().unwrap();
        let words: [u32; WORDS] = std::array::from_fn(|k| u(&case["land0"][k]));
        let land0 = Movable::from_words(&words);
        let has_land = case["has_land"].as_bool().unwrap();
        let mut t = SkullTask {
            twp: twp(&case["twp"]),
            has_set: case["has_set"].as_bool().unwrap(),
            alpha: f(&case["alpha"]),
            attr: [0, 1, 2].map(|k| u(&case["attr"][k])),
            speed: v3(&case["speed"]),
            land: (matches!(kind, "pskull_exec" | "destructor") && has_land).then_some(land0),
            ..Default::default()
        };
        let mut rand = Rand(u(&case["seed"]));
        for fr in case["frames"].as_array().unwrap() {
            t.cwp_flag = u(&fr["cwp_flag"]) as u16;
            let mut h = SkullRec {
                out: fr["out"].as_bool().unwrap(),
                cond: i(&fr["cond"]) as i8,
                calloc: has_land,
                g: i(&fr["g"]) as i8,
                script: fr["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e[0] == "shadow")
                    .cloned()
                    .collect(),
                players: skull_players(&case["players"]),
                rand,
                f20: HashMap::new(),
                events: Vec::new(),
            };
            match kind {
                "skull_load" => skull::skull_load(&mut t, skull::SKULL_ROW, &mut h),
                "skull_exec" => skull::skull_exec(&mut t, &mut h),
                "pskull_load" => skull::pskull_load(&mut t, skull::PSKULL_ROWS, &mut h),
                "pskull_exec" => skull::pskull_exec(&mut t, &mut h),
                _ => skull::destructor(&mut t, &mut h),
            }
            rand = h.rand;
            assert_eq!(json!(h.events), fr["events"], "{kind} {case}");
            assert_eq!(skull_state(&t), fr["state"], "{kind} {case}");
        }
    }
    cases.len()
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn skull_matches_stg42d() {
    let n = skull_group("skull");
    assert!(n == 0 || n == 360);
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn pskull_matches_stg42d() {
    let n = skull_group("pskull");
    assert!(n == 0 || n == 1400);
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn skull_display_matches_stg42d() {
    let Some(c) = oracle("skull_display") else {
        return;
    };
    let (texlist, model) = (u(&c["texlist"]), u(&c["model"]));
    let s5 = u(&c["s5"]);
    assert_eq!(texlist, s5 + skull::TEXLIST);
    assert_eq!(model, s5 + skull::MODEL);
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 200);
    for case in cases {
        let t = SkullTask {
            twp: twp(&case["twp"]),
            alpha: f(&case["alpha"]),
            ..Default::default()
        };
        let mut h = SkullRec {
            out: false,
            cond: 0,
            calloc: false,
            g: 0,
            script: VecDeque::new(),
            players: [None; 2],
            rand: Rand(0),
            f20: HashMap::from([(model, u(&c["f20"]))]),
            events: Vec::new(),
        };
        skull::display(&t, texlist, model, &mut h);
        assert_eq!(json!(h.events), case["events"], "{case}");
    }
}

// --- STOPLSD ----------------------------------------------------------------

#[derive(Default)]
struct StopRec {
    out: bool,
    sizes: Option<[f32; 3]>,
    w2c: u32,
    events: Vec<Value>,
}

impl ObjectHost for StopRec {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: Event) {
        match e {
            Event::CollisionSizes(s) => self.sizes = Some(s),
            Event::CollisionCalcRange => {
                let s = self.sizes.take().expect("sizes before CCL_CalcRange");
                self.events.push(json!(["calc", bits(s), self.w2c]))
            }
            e => self.events.push(collision(e)),
        }
    }
}

impl StopLsdHost for StopRec {
    fn stoplsd(&mut self, e: StopLsdEvent) {
        match e {
            StopLsdEvent::InfoWord2c(w) => self.w2c = w,
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn stoplsd_matches_stg42d() {
    let Some(c) = oracle("stoplsd") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 300);
    for case in cases {
        let mut t = StopLsdTask {
            twp: twp(&case["twp"]),
            ..Default::default()
        };
        let mut w2c = u(&case["w2c"]);
        for fr in case["frames"].as_array().unwrap() {
            let mut h = StopRec {
                out: fr["out"].as_bool().unwrap(),
                w2c,
                ..Default::default()
            };
            match u(&fr["fn"]) {
                0xcb8c => reel::stoplsd_load(&mut t, reel::STOPLSD_ROWS, &mut h),
                0xccbc => reel::stoplsd_exec(&mut h),
                // 0xCCB4 and 0xCCB8 are `blr`.
                _ => {}
            }
            w2c = h.w2c;
            assert_eq!(json!(h.events), fr["events"], "{case}");
            assert_eq!(twp_json(&t.twp), fr["twp"], "{case}");
            assert_eq!(json!([t.exec, t.disp, t.dest, 0]), fr["cb"], "{case}");
        }
    }
}

// --- UDREEL -----------------------------------------------------------------

#[derive(Default)]
struct ReelRec {
    out: bool,
    calloc: bool,
    alt: bool,
    g42: i8,
    shadow: f32,
    hit_id: u16,
    slot: i32,
    players: [Option<ReelPlayer>; 2],
    events: Vec<Value>,
}

impl ObjectHost for ReelRec {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: Event) {
        self.events.push(collision(e));
    }
}

impl ReelHost for ReelRec {
    fn calloc(&mut self) -> bool {
        self.events.push(json!(["calloc", 1, 0x4c]));
        self.calloc
    }
    fn alt_display(&self) -> bool {
        self.alt
    }
    fn mode_42(&self) -> i8 {
        self.g42
    }
    fn shadow_height(&mut self, at: [f32; 3]) -> f32 {
        self.events.push(json!(["shadow", bits(at)]));
        self.shadow
    }
    fn other_id(&self) -> u16 {
        self.hit_id
    }
    fn is_this_task_player(&mut self) -> i32 {
        self.events.push(json!(["is_player"]));
        self.slot
    }
    fn players(&mut self) -> &mut [Option<ReelPlayer>; 2] {
        &mut self.players
    }
    fn reel(&mut self, e: ReelEvent) {
        self.events.push(match e {
            ReelEvent::DynLand { flags } => json!(["dyn_land", flags]),
            ReelEvent::DynLandRemove => json!(["dyn_remove"]),
            ReelEvent::Free => json!(["free"]),
            ReelEvent::Sound(at) => json!(["sound", reel::REEL_SOUND, 1, 0, 10, bits(at)]),
            ReelEvent::Velocity { slot, v } => json!(["velocity", slot, bits(v)]),
            ReelEvent::Input { slot, a, b } => json!(["input", slot, a, b]),
            ReelEvent::Vibrate { slot, a, b } => json!(["action", slot, a, b, 0]),
            ReelEvent::PlayerPosition { slot, at } => json!(["set_pos", slot, bits(at)]),
            ReelEvent::Draw(_) => unreachable!(),
        });
    }
}

fn work(v: &Value) -> ReelWork {
    let fl = &v["f"];
    ReelWork {
        obj: std::array::from_fn(|k| u(&v["obj"][k])),
        len: f(&fl[0]),
        speed: f(&fl[1]),
        end: f(&fl[2]),
        rope: f(&fl[3]),
        shadow: f(&fl[4]),
    }
}

fn work_json(w: &ReelWork) -> Value {
    let fl = [w.len, w.speed, w.end, w.rope, w.shadow].map(f32::to_bits);
    json!({"obj": w.obj, "f": fl})
}

fn reel_data(c: &Value) -> ReelData {
    ReelData {
        row: u(&c["row"]),
        template: std::array::from_fn(|k| u(&c["template"][k])),
        model_radius: f(&c["radius"]),
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn udreel_load_matches_stg42d() {
    let Some(c) = oracle("udreel_load") else {
        return;
    };
    let data = reel_data(&c);
    assert_eq!(data.row, reel::UDREEL_ROW);
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 300);
    for case in cases {
        let mut t = ReelTask {
            twp: twp(&case["twp"]),
            ..Default::default()
        };
        for fr in case["frames"].as_array().unwrap() {
            let mut h = ReelRec {
                out: fr["out"].as_bool().unwrap(),
                calloc: fr["calloc"].as_bool().unwrap(),
                alt: case["alt"].as_bool().unwrap(),
                ..Default::default()
            };
            reel::udreel_load(&mut t, &data, &mut h);
            let st = &fr["state"];
            assert_eq!(json!(h.events), fr["events"], "{case}");
            assert_eq!(twp_json(&t.twp), st["twp"], "{case}");
            assert_eq!(json!([t.exec, t.disp, t.dest, t.disp2]), st["cb"], "{case}");
            let w = t.work.as_ref().map(work_json).unwrap_or(Value::Null);
            assert_eq!(w, st["work"], "{case}");
        }
    }
    for d in c["dest"].as_array().unwrap() {
        let mut w = ReelWork::default();
        w.obj[1] = u32::from(d["model"].as_bool().unwrap());
        let mut t = ReelTask {
            work: Some(w),
            ..Default::default()
        };
        let mut h = ReelRec::default();
        reel::udreel_destructor(&mut t, &mut h);
        assert_eq!(json!(h.events), d["events"]);
        assert!(t.work.is_none() && u(&d["work"]) == 0);
    }
}

fn reel_players(v: &Value) -> [Option<ReelPlayer>; 2] {
    [0, 1].map(|k| {
        let p = &v[k];
        Some(ReelPlayer {
            mode: i(&p["mode"]) as i8,
            ang: [0, 1, 2].map(|j| i(&p["ang"][j]) as i32),
            pos: v3(&p["pos"]),
            character: i(&p["character"]) as i8,
            center: (!p["center"].is_null()).then(|| v3(&p["center"])),
        })
    })
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn udreel_matches_stg42d() {
    let Some(c) = oracle("udreel") else {
        return;
    };
    let data = reel_data(&c);
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1500);
    for case in cases {
        let mut t = ReelTask {
            twp: twp(&case["twp"]),
            work: Some(work(&case["work"])),
            counter: i(&case["counter"]) as i32,
            attr: u(&case["attr"]),
            info_c: f(&case["info_c"]),
            ..Default::default()
        };
        let mut players = reel_players(&case["players"]);
        for fr in case["frames"].as_array().unwrap() {
            t.cwp_flag = u(&fr["cwp_flag"]) as u16;
            let mut h = ReelRec {
                out: fr["out"].as_bool().unwrap(),
                g42: i(&fr["g42"]) as i8,
                shadow: f(&fr["shadow"]),
                hit_id: u(&fr["hit_id"]) as u16,
                slot: i(&fr["slot"]) as i32,
                players,
                ..Default::default()
            };
            reel::udreel_exec(&mut t, &data, &mut h);
            players = h.players;
            for e in &mut h.events {
                if e[0] == "calc" {
                    e[1] = json!(t.info_c.to_bits());
                }
            }
            let st = &fr["state"];
            assert_eq!(json!(h.events), fr["events"], "{case}");
            assert_eq!(twp_json(&t.twp), st["twp"], "{case}");
            assert_eq!(work_json(t.work.as_ref().unwrap()), st["work"], "{case}");
            assert_eq!(i64::from(t.counter), i(&st["counter"]), "{case}");
            assert_eq!(t.attr, u(&st["attr"]), "{case}");
            assert_eq!(t.info_c.to_bits(), u(&st["info_c"]), "{case}");
            let ps: Vec<Value> = players
                .iter()
                .map(|p| {
                    let p = p.unwrap();
                    json!({"mode": p.mode, "ang": p.ang, "pos": bits(p.pos)})
                })
                .collect();
            assert_eq!(json!(ps), st["players"], "{case}");
        }
    }
}

struct DrawRec {
    fog: bool,
    frame: u32,
    f20: HashMap<u32, u32>,
    events: Vec<Value>,
}

impl ReelDisplayHost for DrawRec {
    fn fog(&self) -> bool {
        self.fog
    }
    fn frame(&self) -> u32 {
        self.frame
    }
    fn draw(&mut self, d: Draw) {
        if let Some(v) = draw_json(d, &mut self.f20) {
            self.events.push(v);
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn udreel_display_matches_stg42d() {
    let Some(c) = oracle("udreel_display") else {
        return;
    };
    let mv = &c["models"];
    let m = ReelModels {
        texlist: u(&mv["texlist"]),
        base: u(&mv["base"]),
        rope: u(&mv["rope"]),
        hook: u(&mv["hook"]),
        rope_f20: f(&mv["rope_f20"]),
        alt: [u(&mv["alt"][0]), u(&mv["alt"][1])],
        shadow_texlist: u(&mv["shadow_texlist"]),
        shadow_model: u(&mv["shadow_model"]),
    };
    let f20: HashMap<u32, u32> = c["f20"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.parse().unwrap(), u(v)))
        .collect();
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 300);
    for case in cases {
        let t = ReelTask {
            twp: twp(&case["twp"]),
            ..Default::default()
        };
        let zero = [0u32; 14];
        let w = work(&json!({"obj": zero, "f": case["f"]}));
        let mut h = DrawRec {
            fog: case["fog"].as_bool().unwrap(),
            frame: u(&case["frame"]),
            f20: f20.clone(),
            events: Vec::new(),
        };
        match u(&case["fn"]) {
            0x1ff48 => reel::display(&t, &w, &m, &mut h),
            0x20088 => reel::display_shadow(&t, &w, &m, &mut h),
            _ => reel::display_alt(&t, &w, &m, &mut h),
        }
        assert_eq!(json!(h.events), case["events"], "{case}");
    }
}

// --- identity of the routines with unrelocated local calls -------------------

/// `masked_text` with every `b`/`bl` displacement masked as well.
fn branch_masked(rel: &[u8]) -> Vec<u8> {
    let mut t = sa2_asset_tools::stage_objects::masked_text(rel).unwrap();
    for w in t.as_chunks_mut::<4>().0 {
        let ins = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
        if ins >> 26 == 18 {
            w.copy_from_slice(&(ins & 0xfc00_0003).to_be_bytes());
        }
    }
    t
}

#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn local_call_routines_are_shared_across_stage_rels() {
    use sa2_asset_tools::{manifest::sha256, stage_objects::masked_text};
    let dir = PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").expect("SA2_GC_REL_DIR"));
    let rel = std::fs::read(dir.join("stg42D.rel")).unwrap();
    let (plain, masked) = (masked_text(&rel).unwrap(), branch_masked(&rel));
    let mut others = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".rel")
            && name != "stg42D.rel"
            && (name.starts_with("stg") || name.to_lowercase().starts_with("boss"))
        {
            others.push(branch_masked(&std::fs::read(&p).unwrap()));
        }
    }
    for r in skull::BL_MASKED_ROUTINES
        .iter()
        .chain(reel::BL_MASKED_ROUTINES)
    {
        let (a, b) = (r.stg42d as usize, (r.stg42d + r.len) as usize);
        assert_eq!(sha256(&plain[a..b]), r.masked_sha256, "{}", r.name);
        let code = &masked[a..b];
        let n = others
            .iter()
            .filter(|t| t.windows(code.len()).step_by(4).any(|w| w == code))
            .count();
        eprintln!("{:50} in {n:2} other RELs (branch-masked)", r.name);
        assert!(n > 0, "{}", r.name);
    }
}
