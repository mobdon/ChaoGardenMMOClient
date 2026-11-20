//! Compare `stage_objects::containers` with stg42D.rel's CONTIRON and
//! CONTWOOD run under the main.dol harness (tests/so_containers_ppc.py),
//! case by case and frame by frame: the calls the REL code makes and the
//! task state it leaves.
use sa2_asset_tools::stage_objects::{
    containers::{
        self, ContainerEvent, ContainerHost, ContainerTask, Kind, STG42D_IRON, STG42D_WOOD,
    },
    host::{Event, ObjectHost, V3},
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn oracle() -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_containers_ppc.py"))
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
fn bits(v: V3) -> Value {
    json!(v.map(f32::to_bits))
}
fn kind(v: &Value) -> Kind {
    match v.as_str().unwrap() {
        "iron" => Kind::Iron,
        _ => Kind::Wood,
    }
}

/// One frame's answers, and the calls recorded in the oracle's JSON shape.
#[derive(Default)]
struct Recorder {
    out: bool,
    stage: i32,
    chunk: bool,
    slot_tag: u32,
    child_ok: bool,
    hit_ok: Vec<bool>,
    hit_id: Vec<u8>,
    pn: i32,
    players: Vec<(Option<i8>, Option<i8>)>,
    eq: u32,
    rands: Vec<i32>,
    shadow: f32,
    flag20: bool,
    events: Vec<Value>,
}

impl ObjectHost for Recorder {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, _r: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, _e: Event) {
        unreachable!()
    }
}

impl ContainerHost for Recorder {
    fn stage_num(&mut self) -> i32 {
        self.events.push(json!(["stage_num"]));
        self.stage
    }
    fn chunk_draw(&mut self, _kind: Kind) -> bool {
        self.chunk
    }
    fn create_landing_child(&mut self, rows: u32) -> bool {
        self.events.push(json!(["child", 0, 0, rows, 1, 0]));
        self.child_ok
    }
    fn hit_count(&mut self) -> u32 {
        self.events.push(json!(["hit_count"]));
        self.hit_ok.len() as u32
    }
    fn hit_task(&mut self, i: u32) -> u32 {
        self.events.push(json!(["hit_task", i]));
        i
    }
    fn breaks(&mut self, kind: Kind, hit: u32) -> bool {
        let name = match kind {
            Kind::Iron => "iron_hit",
            Kind::Wood => "wood_hit",
        };
        self.events.push(json!([name]));
        self.events.push(json!(["hit_check", hit]));
        self.hit_ok[hit as usize]
    }
    fn hit_task_id(&mut self, hit: u32) -> u8 {
        self.hit_id[hit as usize]
    }
    fn is_this_task_player(&mut self, hit: u32) -> i32 {
        self.events.push(json!(["is_player", hit]));
        self.pn
    }
    fn player_mode(&mut self, n: i32) -> Option<i8> {
        self.players[n as usize].0
    }
    fn player_kind(&mut self, n: i32) -> Option<i8> {
        self.players[n as usize].1
    }
    fn equipment(&mut self) -> u32 {
        self.eq
    }
    fn rand(&mut self) -> i32 {
        let v = self.rands.remove(0);
        self.events.push(json!(["rand", v]));
        v
    }
    fn shadow_pos(&mut self, at: V3, ang: [i32; 3]) -> f32 {
        self.events.push(json!([
            "shadow",
            at[0].to_bits(),
            at[1].to_bits(),
            at[2].to_bits(),
            ang
        ]));
        self.shadow
    }
    fn broken_flag(&mut self) -> bool {
        self.events.push(json!(["check_flag"]));
        self.flag20
    }
    fn container_event(&mut self, e: ContainerEvent) {
        let v = match e {
            ContainerEvent::InitShare { rows, count, list } => {
                json!(["init_share", rows, count, list])
            }
            ContainerEvent::StackAdd => json!(["stack_add"]),
            ContainerEvent::StackRemove => json!(["stack_remove"]),
            ContainerEvent::FreeChild => json!(["free_child"]),
            ContainerEvent::Dust { pos, ang, sizes: s } => json!([
                "dust",
                bits(pos),
                s[0].to_bits(),
                s[1].to_bits(),
                ang,
                s[2].to_bits(),
                s[3].to_bits(),
                s[4].to_bits()
            ]),
            ContainerEvent::Sound { id, pos } => {
                json!(["sound", id, true, 1, 0x7f, 0x50, bits(pos)])
            }
            ContainerEvent::Debris { at, sizes: s } => json!([
                "debris",
                bits(at),
                s[0].to_bits(),
                s[1].to_bits(),
                s[2].to_bits()
            ]),
            ContainerEvent::Score(n) => json!(["score", n]),
            ContainerEvent::StopPlayer(n) => json!(["set_velocity", n as u32, 0, 0, 0]),
            ContainerEvent::CollisionEntry => json!(["entry"]),
            ContainerEvent::Collapse(p) => json!(["collapse", bits(p)]),
            ContainerEvent::Piece {
                chunk,
                at,
                spread,
                ang_y,
                model,
                slot,
                list,
                speed,
                floor,
            } => {
                let (w, r8, r9) = if chunk {
                    (0x782c, self.slot_tag + slot.unwrap(), list)
                } else {
                    assert!(slot.is_none());
                    (0x7804, list, 0)
                };
                json!([
                    "piece",
                    w,
                    bits(at),
                    1,
                    bits(spread),
                    ang_y as u32,
                    model,
                    r8,
                    r9,
                    speed.to_bits(),
                    floor.to_bits()
                ])
            }
            ContainerEvent::Item { at, kind } => {
                json!([
                    "item",
                    at[0].to_bits(),
                    at[1].to_bits(),
                    at[2].to_bits(),
                    kind
                ])
            }
            ContainerEvent::MarkBroken => json!(["set_flag"]),
            ContainerEvent::DeadOut => json!(["dead_out"]),
            ContainerEvent::FreeTask => json!(["free_task"]),
        };
        self.events.push(v);
    }
}

fn state(t: &ContainerTask) -> Value {
    json!({
        "smode": t.smode, "btimer": t.btimer, "wtimer": t.wtimer, "pos": bits(t.pos),
        "fall": t.fall.to_bits(), "speed": t.speed.to_bits(), "ground": t.ground.to_bits(),
        "attr": t.attr, "child": t.child.map(bits),
    })
}

fn blank(ang: [i32; 3]) -> ContainerTask {
    ContainerTask {
        smode: 0,
        btimer: 0,
        wtimer: 0,
        ang,
        pos: [0.0; 3],
        fall: 0.0,
        speed: 0.0,
        ground: 0.0,
        attr: 0,
        child: None,
        set_placed: false,
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn containers_match_stg42d() {
    let Some(c) = oracle() else {
        return;
    };
    let slot_tag = c["slot_tag"].as_u64().unwrap() as u32;
    let layout = |k| match k {
        Kind::Iron => STG42D_IRON,
        Kind::Wood => STG42D_WOOD,
    };
    let i32s = |v: &Value| -> [i32; 3] { [0, 1, 2].map(|k| v[k].as_i64().unwrap() as i32) };

    let setups = c["setup"].as_array().unwrap();
    assert_eq!(setups.len(), 120);
    for case in setups {
        let k = kind(&case["kind"]);
        let mut t = blank(i32s(&case["ang"]));
        let mut host = Recorder {
            out: case["out"].as_bool().unwrap(),
            stage: case["stage"].as_i64().unwrap() as i32,
            chunk: case["cnk"].as_u64().unwrap() != 0,
            ..Default::default()
        };
        let cb = containers::setup(k, &layout(k), &mut t, &mut host);
        assert_eq!(json!(host.events), case["events"], "{case}");
        assert_eq!(state(&t), case["state"], "{case}");
        let (exec, disp, dest) = cb.map_or((0, 0, 0), |c| (c.exec, c.disp, c.dest));
        assert_eq!(
            json!([exec, disp, dest]),
            json!([case["exec"], case["disp"], case["dest"]]),
            "{case}"
        );
        let sort = cb
            .and_then(|c| c.sort_disp)
            .map_or(case["sort_before"].clone(), |s| json!(s));
        assert_eq!(sort, case["sort"], "{case}");
    }

    let dests = c["dest"].as_array().unwrap();
    assert_eq!(dests.len(), 20);
    for case in dests {
        let mut t = blank([0; 3]);
        t.ground = 1.0;
        let mut host = Recorder::default();
        containers::dest(&mut t, &mut host);
        assert_eq!(json!(host.events), case["events"]);
        assert_eq!(json!(t.ground.to_bits()), case["ground"]);
    }

    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 800);
    let mut frames = 0;
    for case in cases {
        let k = kind(&case["kind"]);
        let s = &case["start"];
        let mut t = ContainerTask {
            smode: s["smode"].as_i64().unwrap() as i8,
            btimer: s["btimer"].as_u64().unwrap() as u8,
            wtimer: s["wtimer"].as_u64().unwrap() as u16,
            ang: i32s(&s["ang"]),
            pos: v3(&s["pos"]),
            fall: f(&s["fall"]),
            speed: f(&s["speed"]),
            ground: f(&s["ground"]),
            attr: s["attr"].as_u64().unwrap() as u32,
            child: s["child"].as_array().map(|_| v3(&s["child"])),
            set_placed: case["ocp"].as_bool().unwrap(),
        };
        for fr in case["frames"].as_array().unwrap() {
            let players = fr["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p["mode"].as_i64().map(|v| v as i8),
                        p["char"].as_i64().map(|v| v as i8),
                    )
                })
                .collect();
            let mut host = Recorder {
                out: fr["out"].as_bool().unwrap(),
                chunk: case["cnk"].as_u64().unwrap() != 0,
                slot_tag,
                child_ok: fr["child_ok"].as_bool().unwrap(),
                hit_ok: fr["hit_ok"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_bool().unwrap())
                    .collect(),
                hit_id: fr["hit_id"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect(),
                pn: fr["pn"].as_i64().unwrap() as i32,
                players,
                eq: fr["eq"].as_u64().unwrap() as u32,
                rands: fr["rands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap() as i32)
                    .collect(),
                shadow: f(&fr["shadow"]),
                flag20: fr["flag20"].as_bool().unwrap(),
                ..Default::default()
            };
            containers::exec(k, &layout(k), &mut t, &mut host);
            assert_eq!(json!(host.events), fr["events"], "{case}");
            assert_eq!(state(&t), fr["state"], "{case}");
            frames += 1;
        }
    }
    eprintln!("{} cases, {frames} frames", cases.len());
}
