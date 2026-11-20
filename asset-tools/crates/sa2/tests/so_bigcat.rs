//! Compare `stage_objects::{big_the_cat, light_modmod}` with stg42D.rel's
//! own BIG THE CAT, LIGHT SW and MODMOD code run under the main.dol harness
//! (tests/so_bigcat_ppc.py), case by case and frame by frame: the events the
//! REL code calls out to, the task state it leaves and the light bytes
//! main.dol's fn_80025028 writes.
use sa2_asset_tools::stage_objects::{
    big_the_cat,
    host::{Event, ObjectHost},
    light_modmod::{
        self as lm, Draw, LightHost, LightSwTask, ModModTask, Phase, PlayerColli, PlayerLights,
    },
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn oracle(object: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_bigcat_ppc.py"))
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
fn ang(v: &Value) -> [i32; 3] {
    [0, 1, 2].map(|k| v[k].as_i64().unwrap() as i32)
}
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}

fn player(v: &Value) -> Option<PlayerColli> {
    if v.get("null").is_some() {
        return None;
    }
    Some(PlayerColli {
        center: v3(&v["center"]),
        a: f(&v["a"]),
        b: f(&v["b"]),
    })
}

/// Records events in the oracle's JSON shape.
struct Recorder {
    out: bool,
    calloc_fail: bool,
    players: [Option<PlayerColli>; 2],
    models: Vec<u64>,
    events: Vec<Value>,
}

impl Recorder {
    fn new(out: bool) -> Self {
        Self {
            out,
            calloc_fail: false,
            players: [None, None],
            models: Vec::new(),
            events: Vec::new(),
        }
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
    fn event(&mut self, _: Event) {
        unreachable!()
    }
}

impl LightHost for Recorder {
    fn sy_calloc_work(&mut self) -> bool {
        self.events.push(json!(["calloc"]));
        !self.calloc_fail
    }
    fn player_colli(&mut self, p: usize) -> Option<PlayerColli> {
        self.players[p]
    }
    fn draw(&mut self, d: Draw) {
        self.events.push(match d {
            Draw::PushMatrix => json!(["push", 0]),
            Draw::Translate(v) => json!(["translate", 0, bits(v)]),
            Draw::RotateX(a) => json!(["rot_x", 0, a as u32]),
            Draw::RotateY(a) => json!(["rot_y", 0, a as u32]),
            Draw::Scale(v) => json!(["scale", 0, bits(v)]),
            Draw::Model { row } => json!(["model", self.models[row as usize]]),
            Draw::PopMatrix => json!(["pop", 1]),
        })
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn big_the_cat_matches_stg42d() {
    let Some(c) = oracle("big") else { return };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        for fr in case["frames"].as_array().unwrap() {
            let mut host = Recorder::new(fr["out"].as_bool().unwrap());
            big_the_cat::exec(&mut host);
            assert_eq!(json!(host.events), fr["events"], "{case}");
            // It installs no callbacks.
            assert_eq!(fr["task"], json!([0, 0, 0]));
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn light_sw_player_test_matches_stg42d() {
    let Some(c) = oracle("light_inside") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4000);
    let mut inside = 0;
    for case in cases {
        let mut task = LightSwTask::new(ang(&case["ang"]), v3(&case["pos"]), v3(&case["scl"]));
        task.shape = case["shape"].as_u64().unwrap() as u16;
        let mut host = Recorder::new(false);
        host.players = [player(&case["players"][0]), player(&case["players"][1])];
        let p = case["p"].as_u64().unwrap() as usize;
        let got = lm::player_inside(&task, p, &mut host);
        assert_eq!(u64::from(got), case["inside"].as_u64().unwrap(), "{case}");
        inside += usize::from(got);
    }
    eprintln!("{inside} of {} inside", cases.len());
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn light_sw_matches_stg42d() {
    let Some(c) = oracle("light_sw") else { return };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 400);
    for case in cases {
        let mut task = LightSwTask::new(ang(&case["ang"]), v3(&case["pos"]), v3(&case["scl"]));
        let l: Vec<u8> = case["lights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        let mut lights = PlayerLights {
            current: [l[0], l[1]],
            default: l[3],
        };
        for fr in case["frames"].as_array().unwrap() {
            let mut host = Recorder::new(fr["out"].as_bool().unwrap());
            host.calloc_fail = fr["calloc_fail"].as_bool().unwrap();
            host.players = [player(&fr["players"][0]), player(&fr["players"][1])];
            let setup = task.phase == Phase::Setup;
            lm::light_sw_frame(&mut task, &mut lights, &mut host);
            if setup && task.phase == Phase::Main {
                if let Some(m) = case["force"].as_i64() {
                    task.mode = m as i8;
                }
            }
            assert_eq!(
                fr["forced"].as_bool().unwrap(),
                setup && task.phase == Phase::Main && !case["force"].is_null()
            );
            assert_eq!(json!(host.events), fr["events"], "{case}");
            let callbacks = match task.phase {
                Phase::Setup => json!([0, 0, 0]),
                Phase::Main => json!([0x14030, 0x13d74, 0x13d78]),
            };
            assert_eq!(callbacks, fr["callbacks"], "{case}");
            assert_eq!(fr["work_ptr"].as_bool().unwrap(), task.phase == Phase::Main);
            if task.phase == Phase::Main {
                assert_eq!(json!(task.work), fr["work"], "{case}");
            }
            assert_eq!(i64::from(task.mode), fr["mode"].as_i64().unwrap(), "{case}");
            assert_eq!(
                u64::from(task.light),
                fr["light"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(
                u64::from(task.shape),
                fr["shape"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(json!(task.ang), fr["ang"], "{case}");
            assert_eq!(
                json!([lights.current[0], lights.current[1], l[2], lights.default]),
                fr["lights"],
                "{case}"
            );
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn modmod_matches_stg42d() {
    let Some(c) = oracle("modmod") else { return };
    let models: Vec<u64> = c["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    // stg42D section 5: the model pointers of the table's two rows.
    assert_eq!(models, [0x20e78, 0x20d10]);
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 200);
    for case in cases {
        let mut task = ModModTask::new(ang(&case["ang"]), v3(&case["pos"]), v3(&case["scl"]));
        task.row = case["row"].as_u64().unwrap() as u16;
        for fr in case["frames"].as_array().unwrap() {
            let mut host = Recorder::new(fr["out"].as_bool().unwrap());
            host.models = models.clone();
            lm::modmod_frame(&mut task, &mut host);
            if task.phase == Phase::Main {
                lm::modmod_display(&task, &mut host);
            }
            assert_eq!(json!(host.events), fr["events"], "{case}");
            assert_eq!(u64::from(task.row), fr["row"].as_u64().unwrap(), "{case}");
            let callbacks = match task.phase {
                Phase::Setup => json!([0, 0, 0]),
                Phase::Main => json!([0x2039c, 0x203c4, 0x20398]),
            };
            assert_eq!(callbacks, fr["callbacks"], "{case}");
        }
    }
}
