//! Compare `stage_objects::itembox` with stg42D.rel's own item box, HUD
//! queue and explosion code run under the main.dol harness
//! (tests/so_itembox_ppc.py), case by case and frame by frame: the events
//! the REL code calls out to (with the answers the oracle's stubs gave) and
//! the state it leaves.
use sa2_asset_tools::stage_objects::{
    host::{Event, ObjectHost},
    itembox::{
        self, explosion, queue, BoxTask, Draw, Global, HitTestHook, ItemBoxHost, ItemEvent, Who,
        World, STG42D,
    },
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn oracle(object: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_itembox_ppc.py"))
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

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}
fn who(w: Who) -> u8 {
    match w {
        Who::Task => 0,
        Who::Parent => 1,
    }
}
fn hook_class(h: HitTestHook) -> u8 {
    match h {
        HitTestHook::Empty => 0,
        HitTestHook::Explosions => 1,
        HitTestHook::Other => 2,
    }
}

/// Records calls in the oracle's JSON shape; a query answers with the
/// value the oracle's stub gave at the same position.
struct Rec<'a> {
    expected: &'a [Value],
    gl: &'a Value,
    events: Vec<Value>,
    hook: HitTestHook,
    center: [u32; 3],
}

impl<'a> Rec<'a> {
    fn new(expected: &'a [Value], gl: &'a Value, hook: HitTestHook) -> Self {
        Self {
            expected,
            gl,
            events: Vec::new(),
            hook,
            center: [0; 3],
        }
    }
    fn answer(&mut self, mut e: Vec<Value>) -> i64 {
        let ret = self
            .expected
            .get(self.events.len())
            .filter(|x| x[0] == e[0])
            .and_then(|x| x.as_array().unwrap().last().cloned())
            .unwrap_or(json!(0));
        e.push(ret.clone());
        self.events.push(Value::Array(e));
        ret.as_i64().unwrap()
    }
    fn push(&mut self, e: Value) {
        self.events.push(e);
    }
    fn draws(&mut self, d: Vec<Draw>) {
        for c in d {
            let e = match c {
                Draw::SetTexture(o) => json!(["tex", o]),
                Draw::Push => json!(["push"]),
                Draw::Pop => json!(["pop"]),
                Draw::Translate(v) => json!(["translate", bits(v)]),
                Draw::RotateY(a) => json!(["rot_y", a]),
                Draw::RotateZ(a) => json!(["rot_z", a]),
                Draw::Scale(v) => json!(["scale", bits(v)]),
                Draw::DisableFog => json!(["fog_dis"]),
                Draw::EnableFog => json!(["fog_en"]),
                Draw::SetFog => json!(["set_fog"]),
                Draw::GjDrawModel(o) => json!(["gj", o]),
                Draw::CnkDrawModel(o) => json!(["cnk", o]),
                Draw::Icon {
                    kind,
                    offset,
                    angle,
                    size,
                } => json!(["icon", kind, bits(offset), angle, size.to_bits()]),
                Draw::Fn800156FC(a) => json!(["f156fc", a.map(f32::to_bits)]),
                Draw::Fn8002B304 => json!(["b304"]),
                Draw::Fn8002B35C => json!(["b35c"]),
                Draw::Fn8002B348 => json!(["b348"]),
                Draw::Fn8002B2F8 => json!(["b2f8"]),
                Draw::OffControl3D(n) => json!(["off3d", n]),
                Draw::OnControl3D(n) => json!(["on3d", n]),
                Draw::Fn801218C8(a, b) => json!(["f218c8", a, b]),
                Draw::Explosion { radius, ratio } => {
                    json!(["exp", radius.to_bits(), ratio.to_bits()])
                }
                Draw::HudIcon { kind, pos, size } => {
                    json!(["hud", kind, bits(pos), size.to_bits()])
                }
                Draw::Fn800148FC => json!(["f148fc"]),
                Draw::Fn80014CD8 => json!(["f14cd8"]),
                Draw::Fn8001475C => json!(["f1475c"]),
            };
            self.push(e);
        }
    }
}

impl ObjectHost for Rec<'_> {
    fn range_out(&mut self) -> bool {
        self.answer(vec![json!("range_out")]) != 0
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: Event) {
        match e {
            Event::CollisionEntry => self.push(json!(["entry"])),
            other => panic!("{other:?}"),
        }
    }
}

impl ItemBoxHost for Rec<'_> {
    fn check_broken(&mut self, w: Who) -> bool {
        self.answer(vec![json!("broken"), json!(who(w))]) != 0
    }
    fn check_flag_0x20(&mut self, w: Who) -> bool {
        self.answer(vec![json!("flag20"), json!(who(w))]) != 0
    }
    fn ccl_is_hit_player(&mut self) -> u32 {
        self.answer(vec![json!("hit_player")]) as u32
    }
    fn fn_80006574(&mut self) -> u32 {
        self.answer(vec![json!("hit_6574")]) as u32
    }
    fn is_this_task_player(&mut self, task: u32) -> i32 {
        self.answer(vec![json!("which_player"), json!(task)]) as i32
    }
    fn player_flags(&mut self, player: i32) -> i16 {
        i(&self.gl["pflags"][player as usize]) as i16
    }
    fn global(&mut self, g: Global) -> i32 {
        let k = match g {
            Global::Game(itembox::game::QUEUE_ON) => "queue_on",
            Global::Game(itembox::game::SPLIT) => "split",
            Global::Game(itembox::game::FLAG_20) => "flag20",
            Global::Game(itembox::game::FREEZE) => "freeze",
            Global::Game(o) => panic!("game byte {o:#x}"),
            Global::GameTurn => "turn",
            Global::StageFog => "fog",
            Global::Screen => "screen",
            Global::Screens => "screens",
        };
        i(&self.gl[k]) as i32
    }
    fn create_elemental_task(&mut self, level: u8, prio: u8, exec: u32, name: u32) -> bool {
        self.answer(vec![
            json!("create_task"),
            json!(level),
            json!(prio),
            json!(exec),
            json!(name),
        ]) != 0
    }
    fn sy_calloc(&mut self, count: u32, size: u32) -> bool {
        self.answer(vec![json!("calloc"), json!(count), json!(size)]) != 0
    }
    fn hit_test_hook(&mut self) -> HitTestHook {
        self.hook
    }
    fn item_event(&mut self, e: ItemEvent) {
        use ItemEvent as E;
        let v = match e {
            E::CollisionInitShare { rows, count, list } => json!(["init_share", rows, count, list]),
            E::CollisionInit { rows, count, list } => json!(["ccl_init", rows, count, list]),
            E::CollisionCenter(c) => {
                self.center = c.map(f32::to_bits);
                return;
            }
            E::SetHitTestHook(h) => {
                self.hook = h;
                return;
            }
            E::CollisionCalcRange => json!(["calc_range"]),
            E::Fn800067F8 => json!(["f67f8"]),
            E::Fn80018D28 => json!(["f18d28"]),
            E::Fn80018B88 => json!(["f18b88"]),
            E::SetFlag0x20(w) => json!(["set_flag20", who(w)]),
            E::SetBroken(w) => json!(["set_broken", who(w)]),
            E::DeadOut(w) => json!(["dead_out", who(w)]),
            E::FreeTask(w) => json!(["free_task", who(w)]),
            E::DestroyTask => json!(["destroy_task"]),
            E::AddScore(n) => json!(["score", n]),
            E::AddNumRing { player, count } => json!(["rings", player, count]),
            E::AddMechHp { player, hp } => json!(["mech_hp", player, hp.to_bits()]),
            E::Fn800641B8 { player, count } => json!(["f641b8", player, count]),
            E::Fn800373B8(p) => json!(["f373b8", p]),
            E::Fn800374E0(p) => json!(["f374e0", p]),
            E::Fn800375FC(p) => json!(["f375fc", p]),
            E::Fn80037718(p) => json!(["f37718", p]),
            E::Fn800168C8(p) => json!(["f168c8", p]),
            E::BreakEffect { pos, vector, size } => {
                json!(["break", bits(pos), bits(vector), size.to_bits()])
            }
            E::SeCall(id) => json!(["se", id, 0, 0, 0]),
            E::Fn8002FB2C { player, a, b, c } => json!(["f2fb2c", player, a, b, c]),
            E::SyFree => json!(["free"]),
        };
        self.push(v);
    }
}

fn hook_of(v: &Value) -> HitTestHook {
    match i(v) {
        0 => HitTestHook::Empty,
        1 => HitTestHook::Explosions,
        _ => HitTestHook::Other,
    }
}

fn queues_json(w: &World) -> Value {
    json!(w
        .queues
        .iter()
        .map(|q| q.as_ref().map(|q| {
            let n = usize::try_from(q.count).unwrap_or(0).min(20);
            json!({
                "mode": q.mode, "player": q.player, "count": q.count, "timer": q.timer,
                "entries": q.entries.iter().take(n)
                    .map(|e| json!([e.kind, e.x.to_bits(), e.size.to_bits()]))
                    .collect::<Vec<_>>(),
            })
        }))
        .collect::<Vec<_>>())
}

fn explosions_json(w: &World) -> Value {
    match &w.explosions {
        None => Value::Null,
        Some(m) => json!(m
            .spheres
            .iter()
            .map(|s| json!([
                s.pos[0].to_bits(),
                s.pos[1].to_bits(),
                s.pos[2].to_bits(),
                s.radius.to_bits(),
                s.max.to_bits()
            ]))
            .collect::<Vec<_>>()),
    }
}

fn box_task(t: &Value) -> BoxTask {
    BoxTask {
        mode: i(&t["mode"]) as i8,
        child: i(&t["child"]) as i8,
        air_item: i(&t["air_item"]) as u8,
        flags: i(&t["flags"]) as i16,
        ang: [0, 1, 2].map(|k| i(&t["ang"][k]) as i32),
        pos: v3(&t["pos"]),
        scale_x: f(&t["scale_x"]),
        spin: f(&t["spin"]),
        size: f(&t["size"]),
        item: i(&t["item"]) as i32,
        has_set: i(&t["has_set"]) != 0,
        parent_has_set: i(&t["parent_has_set"]) != 0,
        work_player: i(&t["work_player"]) as i8,
        display: 0,
        dest: 0,
        display_icon: 0,
    }
}

fn boxes(object: &str, air: bool) {
    let Some(c) = oracle(object) else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 400);
    for case in cases {
        let mut t = box_task(&case["task"]);
        let mut world = World::default();
        let mut hook = hook_of(&case["hook"]);
        let mut center = [0u32; 3];
        for (n, fr) in case["frames"].as_array().unwrap().iter().enumerate() {
            let expected = fr["events"].as_array().unwrap();
            let mut host = Rec::new(expected, &fr["globals"], hook);
            host.center = center;
            if air {
                itembox::air_exec(&mut t, &mut world, &STG42D, &mut host);
            } else {
                itembox::ground_exec(&mut t, &mut world, &STG42D, &mut host);
            }
            let ctx = format!("{object} frame {n} task {}", case["task"]);
            assert_eq!(json!(host.events), fr["events"], "{ctx}");
            (hook, center) = (host.hook, host.center);
            let state = json!({
                "mode": t.mode, "air_item": t.air_item, "item": t.item,
                "spin": t.spin.to_bits(), "size": t.size.to_bits(),
                "display": t.display, "dest": t.dest, "display_icon": t.display_icon,
                "center": center, "hook": hook_class(hook),
                "queues": queues_json(&world), "explosions": explosions_json(&world),
            });
            assert_eq!(state, fr["state"], "{ctx}");
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn itembox_matches_stg42d() {
    boxes("ground", false);
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn itemboxair_matches_stg42d() {
    boxes("air", true);
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn item_box_queue_matches_stg42d() {
    let Some(c) = oracle("queue") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 150);
    for case in cases {
        let mut world = World::default();
        let hook = hook_of(&case["hook"]);
        for (n, fr) in case["frames"].as_array().unwrap().iter().enumerate() {
            let expected = fr["events"].as_array().unwrap();
            let mut host = Rec::new(expected, &fr["globals"], hook);
            while let Some(e) = expected.get(host.events.len()) {
                let k = e.get(1).map(i).unwrap_or(0);
                match e[0].as_str().unwrap() {
                    "callback" => {
                        host.push(e.clone());
                        queue::item_callback(&mut world, k as i32, i(&e[2]) as i32, &mut host);
                    }
                    "exec" => {
                        host.push(e.clone());
                        queue::exec(world.queues[k as usize].as_mut().unwrap(), &mut host);
                    }
                    "dest" => {
                        host.push(e.clone());
                        queue::dest(&mut world, k as usize, &mut host);
                    }
                    "display" => {
                        host.push(e.clone());
                        let d =
                            queue::display(world.queues[k as usize].as_ref().unwrap(), &mut host);
                        host.draws(d);
                    }
                    _ => break,
                }
            }
            let ctx = format!("queue case frame {n}");
            assert_eq!(json!(host.events), fr["events"], "{ctx}");
            assert_eq!(json!({"queues": queues_json(&world)}), fr["state"], "{ctx}");
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn explosions_match_stg42d() {
    let Some(c) = oracle("explosion") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 150);
    let mut hits = 0;
    for case in cases {
        let mut world = World::default();
        let mut hook = hook_of(&case["hook"]);
        for (n, fr) in case["frames"].as_array().unwrap().iter().enumerate() {
            let expected = fr["events"].as_array().unwrap();
            let mut host = Rec::new(expected, &fr["globals"], hook);
            while let Some(e) = expected.get(host.events.len()) {
                match e[0].as_str().unwrap() {
                    "spawn" => {
                        host.push(e.clone());
                        explosion::spawn(&mut world, v3(&e[1]), &mut host);
                    }
                    "exec" => {
                        host.push(e.clone());
                        explosion::exec(world.explosions.as_mut().unwrap(), &mut host);
                    }
                    "dest" => {
                        host.push(e.clone());
                        explosion::dest(&mut world, &mut host);
                    }
                    "display" => {
                        host.push(e.clone());
                        let d = explosion::display(world.explosions.as_ref().unwrap(), &mut host);
                        host.draws(d);
                    }
                    "hit_test" => {
                        let hit = explosion::hit_test(&world, v3(&e[1]));
                        hits += usize::from(hit);
                        host.push(json!(["hit_test", e[1], u32::from(hit)]));
                    }
                    _ => break,
                }
            }
            let ctx = format!("explosion frame {n}");
            assert_eq!(json!(host.events), fr["events"], "{ctx}");
            hook = host.hook;
            assert_eq!(
                json!({"explosions": explosions_json(&world), "hook": hook_class(hook)}),
                fr["state"],
                "{ctx}"
            );
        }
    }
    assert!(hits > 100, "{hits}");
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn item_box_displays_match_stg42d() {
    let Some(c) = oracle("display") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 600);
    for case in cases {
        let t = box_task(&case["task"]);
        let calls = case["calls"].as_array().unwrap();
        for (k, call) in calls.iter().enumerate() {
            let expected = call.as_array().unwrap();
            let mut host = Rec::new(expected, &case["globals"], HitTestHook::Empty);
            let d = match k {
                0 => itembox::ground_display(&t, &STG42D, &mut host),
                1 => itembox::ground_display_icon(&t, &STG42D, &mut host),
                2 => itembox::ground_display_shell(&t, &STG42D),
                3 => itembox::air_display(&t, &STG42D),
                _ => itembox::air_display_icon(&t, &STG42D),
            };
            host.draws(d);
            assert_eq!(json!(host.events), *call, "display {k} {case}");
        }
    }
}
