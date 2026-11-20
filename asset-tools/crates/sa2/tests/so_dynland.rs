//! Compare `stage_objects::dyn_land` (the stage module's movable-object
//! helpers, stg42D.rel 0x378C..0x5238) with the REL's own code run under
//! the main.dol harness (tests/so_dynland_ppc.py), case by case and frame by
//! frame: the calls it makes, the task fields and the whole 0x188-byte body.
//! Also checks that every stage REL carrying the helpers loads the same
//! section-4/5 constants.
use sa2_asset_tools::{
    land_probe::{Hit, Shadow},
    stage_objects::{
        dyn_land::{self, Body, Movable, MovableHost, ShadowBlock, WORDS},
        host::{Event, ObjectHost},
    },
};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::{path::PathBuf, process::Command};

fn oracle(group: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_dynland_ppc.py"))
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

pub fn body(v: &Value) -> Movable {
    let w: [u32; WORDS] = std::array::from_fn(|k| u(&v[k]));
    Movable::from_words(&w)
}

/// The oracle's shadow events, replayed in order through
/// `dyn_land::write_shadow`.
pub struct DynRec {
    pub script: VecDeque<Value>,
    pub calloc: bool,
    pub g: i8,
    pub events: Vec<Value>,
}

impl DynRec {
    pub fn new(oracle_events: &Value, calloc: bool, g: i8) -> Self {
        DynRec {
            script: oracle_events
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e[0] == "shadow")
                .cloned()
                .collect(),
            calloc,
            g,
            events: Vec::new(),
        }
    }

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

impl ObjectHost for DynRec {
    fn range_out(&mut self) -> bool {
        unreachable!()
    }
    fn range_out_with_r(&mut self, _: f32) -> bool {
        unreachable!()
    }
    fn event(&mut self, e: Event) {
        assert_eq!(e, Event::CollisionEntry);
        self.events.push(json!(["entry"]));
    }
}

impl MovableHost for DynRec {
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

struct Task {
    flag: i16,
    b2: u8,
    ang: [i32; 3],
    pos: [f32; 3],
    speed: [f32; 3],
}

fn task(v: &Value) -> Task {
    Task {
        flag: i(&v["flag"]) as i16,
        b2: u(&v["b2"]) as u8,
        ang: [0, 1, 2].map(|k| i(&v["ang"][k]) as i32),
        pos: v3(&v["pos"]),
        speed: v3(&v["speed"]),
    }
}

fn task_json(t: &Task) -> Value {
    json!({"flag": t.flag, "b2": t.b2, "ang": t.ang, "pos": bits(t.pos), "speed": bits(t.speed)})
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn update_matches_stg42d() {
    let Some(c) = oracle("update") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1500);
    for case in cases {
        let mut t = task(&case["task"]);
        let mut st = body(&case["body"]);
        let has_cwp = case["has_cwp"].as_bool().unwrap();
        for fr in case["frames"].as_array().unwrap() {
            t.flag = i(&fr["flag"]) as i16;
            let mut h = DynRec::new(&fr["events"], true, i(&fr["g"]) as i8);
            let b = Body {
                flag: t.flag,
                ang: &mut t.ang,
                pos: &mut t.pos,
                speed: &mut t.speed,
                cwp_flag: has_cwp.then(|| u(&fr["cwp_flag"]) as u16),
            };
            dyn_land::update(b, &mut st, &mut h);
            assert_eq!(json!(h.events), fr["events"], "{case}");
            assert_eq!(task_json(&t), fr["task"], "{case}");
            assert_eq!(json!(st.to_words().to_vec()), fr["body"], "{case}");
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn physics_matches_stg42d() {
    let Some(c) = oracle("physics") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3000);
    for case in cases {
        let mut t = task(&case["task"]);
        let mut st = body(&case["body"]);
        let mut h = DynRec::new(&case["events"], true, 0);
        dyn_land::physics(&mut t.pos, &mut t.speed, &mut t.ang, &mut st, &mut h);
        assert_eq!(json!(h.events), case["events"], "{case}");
        assert_eq!(task_json(&t), case["after"], "{case}");
        assert_eq!(json!(st.to_words().to_vec()), case["after_body"], "{case}");
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn setup_and_release_match_stg42d() {
    let Some(c) = oracle("setup") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 300);
    for case in cases {
        let mut t = task(&case["task"]);
        let mut h = DynRec::new(&json!([]), case["calloc"].as_bool().unwrap(), 0);
        let mut land = dyn_land::setup(
            &mut t.flag,
            &mut t.b2,
            case["has_mwp"].as_bool().unwrap(),
            t.pos,
            &mut h,
        );
        assert_eq!(json!(h.events), case["events"], "{case}");
        assert_eq!(task_json(&t), case["after"]["task"], "{case}");
        assert_eq!(
            land.map(|l| json!(l.to_words().to_vec()))
                .unwrap_or(Value::Null),
            case["after"]["body"],
            "{case}"
        );
        h.events.clear();
        dyn_land::release(&mut land, &mut h);
        assert_eq!(json!(h.events), case["release"], "{case}");
        assert!(land.is_none());
        assert_eq!(u(&case["land_after"]), 0);
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn plane_distance_matches_main_dol() {
    let Some(c) = oracle("plane") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3000);
    for case in cases {
        let (d, foot) =
            dyn_land::plane_distance_80012ea4(v3(&case["p"]), v3(&case["q"]), v3(&case["n"]));
        assert_eq!(d.to_bits(), u(&case["d"]), "{case}");
        assert_eq!(bits(foot), case["foot"], "{case}");
    }
}

#[test]
fn body_words_round_trip() {
    let mut st = Movable::new([1.0, 2.0, 3.0]);
    st.shadow.rays[4].hit = true;
    st.shadow.rays[4].normal = [0.5, -0.25, 1.0];
    st.task_flag = -2;
    assert_eq!(Movable::from_words(&st.to_words()), st);
}

// --- constants shared across the stage RELs -----------------------------------

struct Rel {
    data: Vec<u8>,
    sections: Vec<usize>,
    /// (section, offset, kind, target section, target offset)
    relocs: Vec<(u8, usize, u8, u8, usize)>,
}

fn be(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(d[at..at + 4].try_into().unwrap())
}

fn rel(data: Vec<u8>) -> Rel {
    let count = be(&data, 0xc) as usize;
    let info = be(&data, 0x10) as usize;
    let sections = (0..count)
        .map(|k| (be(&data, info + 8 * k) & !1) as usize)
        .collect();
    let (imp, imp_len) = (be(&data, 0x28) as usize, be(&data, 0x2c) as usize);
    let mut relocs = Vec::new();
    for k in 0..imp_len / 8 {
        let module = be(&data, imp + 8 * k);
        let mut at = be(&data, imp + 8 * k + 4) as usize;
        let (mut section, mut cursor) = (0u8, 0usize);
        loop {
            let r = &data[at..at + 8];
            at += 8;
            let (delta, kind, s) = (u16::from_be_bytes([r[0], r[1]]) as usize, r[2], r[3]);
            if kind == 203 {
                break;
            }
            cursor += delta;
            if kind == 202 {
                (section, cursor) = (s, 0);
                continue;
            }
            if module != 0 {
                relocs.push((section, cursor, kind, s, be(r, 4) as usize));
            }
        }
    }
    Rel {
        data,
        sections,
        relocs,
    }
}

#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn helper_constants_are_shared_across_stage_rels() {
    use sa2_asset_tools::stage_objects::branch_masked_text;
    let dir = PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").expect("SA2_GC_REL_DIR"));
    // The whole block 0x378C..0x5238, and the words it loads from its own
    // sections 4 and 5 (ADDR16_LO), in instruction order.
    const START: usize = 0x378c;
    const LEN: usize = 0x5238 - 0x378c;
    let values = |r: &Rel, start: usize| -> Vec<(usize, Vec<u8>)> {
        let mut v: Vec<_> = r
            .relocs
            .iter()
            .filter(|(s, o, k, ts, _)| {
                *s == 1 && (start..start + LEN).contains(o) && *k == 4 && (*ts == 4 || *ts == 5)
            })
            .map(|&(_, o, _, ts, to)| {
                let at = r.sections[ts as usize] + to;
                // The 0x43300000 conversion double and the BAMS double are
                // read as 8 bytes.
                let n = if r.data[at..at + 4] == [0x43, 0x30, 0, 0]
                    || r.data[at..at + 4] == [0x40, 0xc4, 0x5f, 0x30]
                {
                    8
                } else {
                    4
                };
                (o - start, r.data[at..at + n].to_vec())
            })
            .collect();
        v.sort();
        v
    };
    let load = |name: &str| {
        let d = std::fs::read(dir.join(name)).unwrap();
        (branch_masked_text(&d).unwrap(), rel(d))
    };
    let (ref_text, ref_rel) = load("stg42D.rel");
    let code = &ref_text[START..START + LEN];
    let want = values(&ref_rel, START);
    // ROLL_PUSH (sec5 +0xD0F0) is among them.
    let roll: Vec<u8> = dyn_land::ROLL_PUSH
        .iter()
        .flat_map(|x| x.to_be_bytes())
        .collect();
    let s5 = ref_rel.sections[5] + 0xd0f0;
    assert_eq!(&ref_rel.data[s5..s5 + 12], &roll[..]);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.ends_with(".rel")
                && n != "stg42D.rel"
                && (n.starts_with("stg") || n.to_lowercase().starts_with("boss"))
        })
        .collect();
    names.sort();
    let (mut same, mut other) = (Vec::new(), Vec::new());
    for n in &names {
        let (text, r) = load(n);
        for o in (0..text.len().saturating_sub(LEN)).step_by(4) {
            if &text[o..o + LEN] != code {
                continue;
            }
            // ROLL_PUSH is read as 12 bytes from its first word's address.
            let s5_site = r.relocs.iter().find(|(s, off, k, ts, _)| {
                *s == 1 && *off == o + (0x4cb2 - START) && *k == 4 && *ts == 5
            });
            let roll_ok = s5_site.is_some_and(|&(_, _, _, _, to)| {
                let at = r.sections[5] + to;
                r.data[at..at + 12] == roll[..]
            });
            if values(&r, o) == want && roll_ok {
                same.push(format!("{n}@{o:#x}"));
            } else {
                other.push(format!("{n}@{o:#x}"));
            }
        }
    }
    eprintln!("same constants: {} {same:?}", same.len());
    eprintln!("other constants: {other:?}");
    assert!(same.iter().any(|s| s.starts_with("Boss_Last2D")));
    assert!(other.is_empty(), "{other:?}");
}
