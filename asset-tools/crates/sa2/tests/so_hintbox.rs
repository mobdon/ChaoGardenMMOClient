//! Compare `stage_objects::{hintbox, schbox}` with stg42D.rel's own HINTBOX
//! and SCHBOX code run under the main.dol harness (tests/so_hintbox_ppc.py),
//! case by case and frame by frame: the engine calls the REL code makes, in
//! order, and the task, work, player and shared state it leaves.
use sa2_asset_tools::stage_objects::{
    hintbox::{self, BoxHost, BoxWork, PlayerTwp, TaskWk, Text},
    host::{ObjectHost, V3},
    schbox::{self, ManTask, Pad, PauseTask, SearchHost, SearchShared},
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

/// Task addresses the oracle's CreateElementalTask stub hands out.
const MAN_TASK: u32 = 0x8120_3000;
const PAUSE_TASK: u32 = 0x8120_3200;

fn oracle(object: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_hintbox_ppc.py"))
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
    v.as_u64().unwrap() as u32
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn v3(v: &Value) -> V3 {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: V3) -> Value {
    json!(v.map(f32::to_bits))
}

fn text_json(t: Text) -> Value {
    match t {
        Text::Hint(n) => json!(["hint", n]),
        Text::DefaultHint => json!("default"),
        Text::Ptr(p) => json!(["ptr", p]),
        Text::NoText => json!("notext"),
    }
}
fn text_parse(v: &Value) -> Option<Text> {
    match v {
        Value::Null => None,
        Value::String(s) if s == "notext" => Some(Text::NoText),
        Value::String(s) if s == "default" => Some(Text::DefaultHint),
        Value::Array(a) if a[0] == "ptr" => Some(Text::Ptr(u(&a[1]))),
        Value::Array(a) => Some(Text::Hint(i(&a[1]) as i32)),
        _ => panic!("{v}"),
    }
}

fn twp_parse(v: &Value) -> TaskWk {
    TaskWk {
        mode: i(&v["mode"]) as i8,
        smode: i(&v["smode"]) as i8,
        player: u(&v["player"]) as u8,
        wtimer: u(&v["wtimer"]) as u16,
        ang: [0, 1, 2].map(|k| i(&v["ang"][k]) as i32),
        pos: v3(&v["pos"]),
        scale: v3(&v["scale"]),
    }
}
fn twp_json(t: &TaskWk) -> Value {
    json!({"mode": t.mode, "smode": t.smode, "player": t.player, "wtimer": t.wtimer,
           "ang": t.ang, "pos": bits(t.pos), "scale": bits(t.scale)})
}
fn work_parse(v: &Value) -> Option<BoxWork> {
    if v.is_null() {
        return None;
    }
    Some(BoxWork {
        alpha: f(&v["alpha"]),
        fade: f(&v["fade"]),
        ang_y: i(&v["ang_y"]) as i32,
        pos: v3(&v["pos"]),
        spin: i(&v["spin"]) as i32,
        phase: i(&v["phase"]) as i32,
        bob: f(&v["bob"]),
        message: u(&v["message"]),
        hint: v.get("hint").map_or(0, |h| i(h) as i32),
    })
}
fn work_json(w: &Option<BoxWork>, hint: bool) -> Value {
    let Some(w) = w else { return Value::Null };
    let mut j = json!({"alpha": w.alpha.to_bits(), "fade": w.fade.to_bits(), "ang_y": w.ang_y,
        "pos": bits(w.pos), "spin": w.spin, "phase": w.phase, "bob": w.bob.to_bits(),
        "message": w.message});
    if hint {
        j["hint"] = json!(w.hint);
    }
    j
}

fn shared_parse(v: &Value) -> SearchShared {
    SearchShared {
        texts: [0, 1, 2].map(|k| text_parse(&v["texts"][k])),
        man: (!v["man"].is_null()).then(|| ManTask {
            task: u(&v["man"]["task"]),
            wtimer: u(&v["man"]["wtimer"]) as u16,
        }),
        pause: (!v["pause"].is_null()).then(|| PauseTask {
            task: u(&v["pause"]["task"]),
            wtimer: u(&v["pause"]["wtimer"]) as u16,
            message: u(&v["pause"]["message"]),
        }),
        last_hint_high: i(&v["high"]) as i32,
        alt_display: i(&v["alt"]) as i32,
    }
}
fn shared_json(s: &SearchShared) -> Value {
    let cb = |c: &[u32]| json!(c);
    json!({
        "texts": s.texts.map(|t| t.map_or(Value::Null, text_json)),
        "man": s.man.map_or(Value::Null, |m| json!({"task": m.task, "wtimer": m.wtimer,
            "cb": cb(&[schbox::MAN_EXEC, schbox::MAN_DESTRUCTOR])})),
        "pause": s.pause.map_or(Value::Null, |p| json!({"task": p.task, "wtimer": p.wtimer,
            "message": p.message,
            "cb": cb(&[schbox::PAUSE_EXEC, schbox::PAUSE_DISPLAY, schbox::PAUSE_DESTRUCTOR])})),
        "high": s.last_hint_high, "alt": s.alt_display,
    })
}

/// One frame's inputs and the calls recorded in the oracle's JSON shape.
#[derive(Default)]
struct Recorder {
    fr: Value,
    players: [Option<PlayerTwp>; 2],
    range: [Option<V3>; 2],
    /// `playerpwp[i] +0xD, +0xE`.
    pwp: [Option<[i8; 2]>; 2],
    colli_range: f32,
    events: Vec<Value>,
}

impl Recorder {
    fn b(&self, k: &str) -> bool {
        self.fr[k].as_bool().unwrap()
    }
    fn load(&mut self, fr: &Value) {
        self.fr = fr.clone();
        self.events.clear();
        self.colli_range = 0.0;
        for k in 0..2 {
            let p = &fr["players"][k];
            if p.is_null() {
                self.players[k] = None;
                self.range[k] = None;
                continue;
            }
            self.players[k] = Some(PlayerTwp {
                ang: [0, 1, 2].map(|j| i(&p["ang"][j]) as i32),
                pos: v3(&p["pos"]),
            });
            self.range[k] = Some(v3(&p["range"]));
            let d = self.pwp[k].map_or(0, |w| w[0]);
            self.pwp[k] = Some([d, i(&p["action"]) as i8]);
        }
    }
    fn ev(&mut self, e: Value) {
        self.events.push(e);
    }
}

impl ObjectHost for Recorder {
    fn range_out(&mut self) -> bool {
        self.ev(json!(["range_out"]));
        self.b("out")
    }
    fn range_out_with_r(&mut self, _r: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, _e: sa2_asset_tools::stage_objects::host::Event) {
        unreachable!()
    }
}

impl BoxHost for Recorder {
    fn frame_counter(&mut self) -> u32 {
        u(&self.fr["frame"])
    }
    fn language(&mut self) -> i8 {
        u(&self.fr["lang"]) as u8 as i8
    }
    fn shadow_pos(&mut self, p: V3) -> f32 {
        self.ev(json!([
            "shadow",
            p[0].to_bits(),
            p[1].to_bits(),
            p[2].to_bits()
        ]));
        f(&self.fr["shadow"])
    }
    fn range_players(&mut self) -> [Option<V3>; 2] {
        self.range
    }
    fn player_twp(&mut self, k: usize) -> Option<PlayerTwp> {
        self.players.get(k).copied().flatten()
    }
    fn player_pwp_present(&mut self, k: usize) -> bool {
        self.pwp.get(k).is_some_and(|p| p.is_some())
    }
    fn player_action(&mut self, k: usize) -> i8 {
        self.pwp[k].unwrap()[1]
    }
    fn set_player_action(&mut self, k: usize, v: i8) {
        self.pwp[k].as_mut().unwrap()[1] = v;
    }
    fn offer_player_action(&mut self, k: usize, v: i8) {
        self.pwp[k].as_mut().unwrap()[0] = v;
    }
    fn player_has_control(&mut self, _k: u32) -> i8 {
        i(&self.fr["ctrl"]) as i8
    }
    fn control_off(&mut self, k: u32) {
        self.ev(json!(["ctrl_off", k]));
    }
    fn control_on(&mut self, k: u32) {
        self.ev(json!(["ctrl_on", k]));
    }
    fn message_active(&mut self) -> u32 {
        u(&self.fr["busy"])
    }
    fn message_open(&mut self, kind: u32, text: Text, time: u32, language: i8) -> u32 {
        self.ev(json!(["msg", kind, text_json(text), time, language]));
        u(&self.fr["msgtask"])
    }
    fn destroy_task(&mut self, task: u32) {
        self.ev(json!(["destroy", task]));
    }
    fn sound(&mut self, id: u32) {
        self.ev(json!(["se", id]));
    }
    fn look_at(&mut self, player: u32) {
        self.ev(json!(["look", player]));
    }
    fn alloc_work(&mut self, size: u32) -> bool {
        self.ev(json!(["calloc", size]));
        self.b("alloc")
    }
    fn free_work(&mut self) {
        self.ev(json!(["free"]));
    }
    fn collision_init(&mut self, row: u32, count: u32, list: u32) {
        self.ev(json!(["init", row, count, list]));
    }
    fn collision_flags(&mut self) -> u16 {
        u(&self.fr["cflag"]) as u16
    }
    fn collision_entry(&mut self) {
        self.ev(json!(["entry"]));
    }
}

impl SearchHost for Recorder {
    fn play_mode(&mut self) -> i8 {
        i(&self.fr["play_mode"]) as i8
    }
    fn pause_pad(&mut self) -> i8 {
        i(&self.fr["pause_pad"]) as i8
    }
    fn player_mask(&mut self) -> i8 {
        i(&self.fr["mask"]) as i8
    }
    fn has_set_entry(&mut self) -> bool {
        self.b("from_set")
    }
    fn dead_out(&mut self) {
        self.ev(json!(["deadout"]));
    }
    fn free_task(&mut self) {
        self.ev(json!(["freetask"]));
    }
    fn collision_init_share(&mut self, row: u32, count: u32, list: u32) -> bool {
        self.ev(json!(["init_share", row, count, list]));
        self.b("cwp")
    }
    fn set_colli_range(&mut self, r: f32) {
        self.colli_range = r;
    }
    fn create_task(
        &mut self,
        flags: u32,
        level: u32,
        exec: u32,
        name: &'static str,
    ) -> Option<u32> {
        let task = if !self.b("create") {
            0
        } else if name == schbox::MAN_NAME {
            MAN_TASK
        } else {
            PAUSE_TASK
        };
        self.ev(json!(["create", flags, level, exec, name, task]));
        (task != 0).then_some(task)
    }
    fn hunt_level(&mut self) -> Option<i8> {
        self.fr["hunt"].as_i64().map(|h| h as i8)
    }
    fn hint_text(&mut self) -> u32 {
        u(&self.fr["text"])
    }
    fn game_state(&mut self) -> i16 {
        u(&self.fr["gstate"]) as i16
    }
    fn pause_hint_lock(&mut self) -> i32 {
        i(&self.fr["lock"]) as i32
    }
    fn pad(&mut self, port: usize) -> Pad {
        let p = &self.fr["pads"][port];
        Pad {
            on: u(&p[0]),
            press: u(&p[1]),
        }
    }
    fn set_player_pos(&mut self, k: u32, p: V3) {
        self.ev(json!([
            "setpos",
            k,
            p[0].to_bits(),
            p[1].to_bits(),
            p[2].to_bits()
        ]));
    }
}

fn pwp_json(r: &Recorder) -> Value {
    json!(r.pwp.map(|p| p.map_or(Value::Null, |p| json!(p))))
}

/// Start each case with player 1's slot (and player 2's when present)
/// holding zero bytes, as the oracle clears them.
fn start(r: &mut Recorder, case: &Value) {
    let fr0 = &case["frames"][0];
    r.pwp = [0, 1].map(|k| (!fr0["players"][k].is_null()).then_some([0, 0]));
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn hintbox_matches_stg42d() {
    let Some(c) = oracle("hintbox") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert!(cases.len() >= 400);
    let mut frames = 0;
    for (n, case) in cases.iter().enumerate() {
        let count = i(&case["count"]) as i32;
        let mut t = twp_parse(&case["twp"]);
        let mut work = work_parse(&case["work"]);
        let mut r = Recorder::default();
        start(&mut r, case);
        for fr in case["frames"].as_array().unwrap() {
            r.load(fr);
            let mut cb = [0u32; 3];
            match fr["kind"].as_str().unwrap() {
                "setup" => {
                    work = hintbox::setup(&mut t, 0x10e78, &mut r);
                    if work.is_some() {
                        cb = [hintbox::EXEC, hintbox::DISPLAY, hintbox::DESTRUCTOR];
                    }
                }
                "exec" => hintbox::exec(&mut t, work.as_mut().unwrap(), count, &mut r),
                _ => hintbox::destroy(&mut work, &mut r),
            }
            let ctx = || format!("case {n} frame {fr}");
            assert_eq!(json!(r.events), fr["events"], "{}", ctx());
            assert_eq!(twp_json(&t), fr["twp"], "{}", ctx());
            assert_eq!(work_json(&work, true), fr["work"], "{}", ctx());
            assert_eq!(pwp_json(&r), fr["pwp"], "{}", ctx());
            if fr["kind"] == "setup" {
                assert_eq!(json!(cb), fr["cb"], "{}", ctx());
            }
            frames += 1;
        }
    }
    eprintln!("{} cases, {frames} frames", cases.len());
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn schbox_matches_stg42d() {
    let Some(c) = oracle("schbox") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert!(cases.len() >= 500);
    let mut frames = 0;
    for (n, case) in cases.iter().enumerate() {
        let mut t = twp_parse(&case["twp"]);
        let mut work = work_parse(&case["work"]);
        let mut sh = shared_parse(&case["shared"]);
        assert_eq!(shared_json(&sh), case["shared"]);
        let mut r = Recorder::default();
        start(&mut r, case);
        for fr in case["frames"].as_array().unwrap() {
            r.load(fr);
            let kind = fr["kind"].as_str().unwrap();
            let mut cb = None;
            match kind {
                "setup" => {
                    work = schbox::setup(&mut t, &mut sh, 0x1e358, &mut r).map(|(w, alt)| {
                        cb = Some(if alt {
                            [schbox::EXEC, schbox::DISPLAY_ALT, schbox::DESTRUCTOR, 0]
                        } else {
                            [
                                schbox::EXEC,
                                schbox::DISPLAY,
                                schbox::DESTRUCTOR,
                                schbox::DISPLAY_LATE,
                            ]
                        });
                        w
                    });
                }
                "exec" => schbox::exec(&mut t, work.as_mut().unwrap(), &mut sh, &mut r),
                "destroy" => schbox::destroy_box(&mut work, &mut r),
                "man_exec" => {
                    let mut m = sh.man.unwrap();
                    schbox::man_exec(&mut m, &mut r);
                    sh.man = Some(m);
                }
                "man_dest" => {
                    let task = sh.man.unwrap().task;
                    schbox::man_destroy(&mut sh, task);
                }
                "pause_exec" => schbox::pause_exec(&sh.pause.unwrap(), &mut r),
                "pause_disp" => {
                    let mut p = sh.pause.unwrap();
                    schbox::pause_display(&sh, &mut p, &mut r);
                    sh.pause = Some(p);
                }
                "pause_dest" => {
                    let mut p = sh.pause.unwrap();
                    schbox::pause_destroy(&mut sh, &mut p, &mut r);
                    assert_eq!(p.message, u(&fr["pause_message"]));
                }
                _ => panic!("{kind}"),
            }
            let ctx = || format!("case {n} frame {fr}");
            assert_eq!(json!(r.events), fr["events"], "{}", ctx());
            assert_eq!(twp_json(&t), fr["twp"], "{}", ctx());
            assert_eq!(work_json(&work, false), fr["work"], "{}", ctx());
            assert_eq!(pwp_json(&r), fr["pwp"], "{}", ctx());
            assert_eq!(shared_json(&sh), fr["shared"], "{}", ctx());
            assert_eq!(r.colli_range.to_bits(), u(&fr["colli_range"]), "{}", ctx());
            if let Some(cb) = cb {
                assert_eq!(json!(cb), fr["cb"], "{}", ctx());
            }
            frames += 1;
        }
    }
    eprintln!("{} cases, {frames} frames", cases.len());
}
