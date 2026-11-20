//! Compare `stage_objects::player_contact` with main.dol's own routines run
//! under the harness (tests/so_contact_ppc.py): for each case the port
//! starts from the bytes the routine read, makes the same engine calls (with
//! the recorded answers), returns the same value and leaves the same bytes
//! in every address the routine wrote, and writes nowhere else.
//! Also checks that the bytes of every routine in `DOL_ROUTINES` hash as
//! recorded.
use sa2_asset_tools::stage_objects::player_contact::{self as pc, ContactHost, Ram};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::{path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap())
        .collect()
}

fn bytes_map(v: &Value) -> BTreeMap<u32, u8> {
    let mut out = BTreeMap::new();
    for run in v.as_array().unwrap() {
        let a = u(&run[0]);
        for (k, b) in hex(run[1].as_str().unwrap()).into_iter().enumerate() {
            out.insert(a + k as u32, b);
        }
    }
    out
}

/// Memory with the routine's initial reads; reading anything else is a
/// divergence.
struct Mem {
    init: BTreeMap<u32, u8>,
    cur: HashMap<u32, u8>,
    written: BTreeMap<u32, u8>,
    unknown: Vec<u32>,
}

impl Ram for Mem {
    fn r8(&mut self, a: u32) -> u8 {
        if let Some(&b) = self.cur.get(&a) {
            return b;
        }
        match self.init.get(&a) {
            Some(&b) => b,
            None => {
                self.unknown.push(a);
                0
            }
        }
    }
    fn w8(&mut self, a: u32, v: u8) {
        self.cur.insert(a, v);
        self.written.insert(a, v);
    }
}

/// Replays the recorded engine calls.
struct Host {
    ev: Vec<Value>,
    at: usize,
    bad: Vec<String>,
}

impl Host {
    fn next(&mut self, name: &str, args: &[u32]) -> u32 {
        let Some(e) = self.ev.get(self.at).cloned() else {
            self.bad.push(format!("extra call {name}{args:x?}"));
            return 0;
        };
        self.at += 1;
        let e = e.as_array().unwrap();
        let got: Vec<u32> = e[1..].iter().map(u).collect();
        if e[0].as_str() != Some(name) || got[..args.len()] != *args {
            self.bad
                .push(format!("call {name}{args:x?}, expected {e:?}"));
        }
        got.get(args.len()).copied().unwrap_or(0)
    }
}

impl ContactHost for Host {
    fn se_call(&mut self, id: u32) {
        self.next("se", &[id]);
    }
    fn voice(&mut self, id: u32) {
        self.next("voice", &[id]);
    }
    fn jingle_play(&mut self, name: u32) {
        self.next("jingle", &[name]);
    }
    fn fn_800be308(&mut self, p: i32) -> u32 {
        self.next("be308", &[p as u32])
    }
    fn fn_800be374(&mut self, p: i32) -> u32 {
        self.next("be374", &[p as u32])
    }
    fn fn_800be408(&mut self, p: i32) -> u32 {
        self.next("be408", &[p as u32])
    }
    fn create_elemental_task(&mut self, flags: u32, level: u32, exec: u32, name: u32) -> u32 {
        self.next("cet", &[flags, level, exec, name])
    }
    fn malloc(&mut self, size: u32) -> u32 {
        self.next("malloc", &[size])
    }
    fn destroy_task(&mut self, task: u32) {
        self.next("destroy", &[task]);
    }
    fn fn_80063e34(&mut self, p: i32) -> i32 {
        self.next("rank", &[p as u32]) as i32
    }
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f32::from_bits(u(&v[k])))
}

/// Runs one case; returns the routine's r3 when it has one.
fn run(name: &str, a: &[Value], ram: &mut Mem, h: &mut Host) -> Option<u32> {
    Some(match name {
        "is_this_task_player" => pc::is_this_task_player(ram, u(&a[0])) as u32,
        "ccl_is_hit_player_ex" => pc::ccl_is_hit_player_ex(ram, u(&a[0])),
        "fn_80006428" => pc::fn_80006428(ram, u(&a[0])),
        "fn_800059c8" => pc::fn_800059c8(ram, u(&a[0])),
        "ccl_is_hit_player" => pc::ccl_is_hit_player(ram, u(&a[0])),
        "fn_80006574" => pc::fn_80006574(ram, u(&a[0])),
        "fn_80005b04" => pc::fn_80005b04(ram, u(&a[0])),
        "fn_800067c0" => pc::fn_800067c0(ram, u(&a[0])),
        "fn_80005904" => pc::fn_80005904(ram, u(&a[0]), i(&a[1])),
        "ccl_clear_search" => {
            pc::ccl_clear_search(ram);
            return None;
        }
        "fn_8003699c" => u32::from(pc::fn_8003699c(ram, u(&a[0]), u(&a[1]))),
        "fn_800368a0" => u32::from(pc::fn_800368a0(ram, u(&a[0]), u(&a[1]))),
        "get_stage_num" => pc::get_stage_num(ram) as u32,
        _ => {
            match name {
                "set_velocity_p" => pc::set_velocity_p(ram, i(&a[0]), v3(&a[1])),
                "set_input_p" => pc::set_input_p(ram, i(&a[0]), u(&a[1]), u(&a[2])),
                "fn_800399bc" => pc::fn_800399bc(ram, i(&a[0]), v3(&a[1])),
                "fn_8003a54c" => {
                    let at = (!a[2].is_null()).then(|| [0, 1, 2].map(|k| u(&a[2][k])));
                    pc::fn_8003a54c(ram, i(&a[0]), u(&a[1]), at, u(&a[3]))
                }
                "fn_80039eb0" => {
                    let ang = [0, 1, 2].map(|k| i(&a[2][k]));
                    pc::fn_80039eb0(ram, i(&a[0]), v3(&a[1]), ang, i(&a[3]))
                }
                "off_control_3d" => pc::off_control_3d(ram, u(&a[0])),
                "on_control_3d" => pc::on_control_3d(ram, u(&a[0])),
                "add_score" => pc::add_score(ram, i(&a[0])),
                "fn_800641b8" => pc::fn_800641b8(ram, h, i(&a[0]), i(&a[1])),
                "add_mech_hp" => pc::add_mech_hp(ram, h, i(&a[0]), f32::from_bits(u(&a[1]))),
                "add_num_ring" => pc::add_num_ring(ram, h, i(&a[0]), i(&a[1])),
                "fn_800373b8" => pc::fn_800373b8(ram, h, i(&a[0])),
                "fn_800374e0" => pc::fn_800374e0(ram, h, i(&a[0])),
                "fn_800375fc" => pc::fn_800375fc(ram, h, i(&a[0])),
                "fn_80037718" => pc::fn_80037718(ram, h, i(&a[0])),
                "fn_8002fb2c" => pc::fn_8002fb2c(ram, h, i(&a[0]), i(&a[1]), i(&a[2]), i(&a[3])),
                other => panic!("unknown routine {other}"),
            }
            return None;
        }
    })
}

#[test]
#[ignore = "needs SA2_MAIN_DOL and a Python with Unicorn"]
fn player_contact_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_contact_ppc.py"))
        .arg(std::env::var("SO_CONTACT_CASES").unwrap_or_else(|_| "300".into()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut fails = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (k, c) in out["cases"].as_array().unwrap().iter().enumerate() {
        let name = c["r"].as_str().unwrap();
        *counts.entry(name.into()).or_default() += 1;
        let mut ram = Mem {
            init: bytes_map(&c["mem"]),
            cur: HashMap::new(),
            written: BTreeMap::new(),
            unknown: Vec::new(),
        };
        let mut h = Host {
            ev: c["ev"].as_array().unwrap().clone(),
            at: 0,
            bad: Vec::new(),
        };
        let ret = run(name, c["args"].as_array().unwrap(), &mut ram, &mut h);
        let mut why = h.bad;
        if h.at != h.ev.len() {
            why.push(format!("missing calls {:?}", &h.ev[h.at..]));
        }
        if let Some(r) = ret {
            if r != u(&c["ret"]) {
                why.push(format!("returned {r:#x}, expected {:#x}", u(&c["ret"])));
            }
        }
        if !ram.unknown.is_empty() {
            why.push(format!(
                "read unrecorded {:x?}",
                &ram.unknown[..ram.unknown.len().min(8)]
            ));
        }
        let want = bytes_map(&c["out"]);
        if want != ram.written {
            let diff: Vec<String> = want
                .keys()
                .chain(ram.written.keys())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .filter(|a| want.get(a) != ram.written.get(a))
                .take(8)
                .map(|a| {
                    format!(
                        "{a:#x}: dol {:?} port {:?}",
                        want.get(a),
                        ram.written.get(a)
                    )
                })
                .collect();
            why.push(format!("writes differ: {diff:?}"));
        }
        if !why.is_empty() {
            fails.push(format!("case {k} {name} {}: {why:?}", c["args"]));
        }
    }
    eprintln!("cases: {counts:?}, skipped: {}", out["skipped"]);
    for f in fails.iter().take(30) {
        eprintln!("{f}");
    }
    assert!(fails.is_empty(), "{} of the cases differ", fails.len());
}

fn dol_bytes(dol: &[u8], addr: u32, len: u32) -> Option<&[u8]> {
    let w = |o: usize| u32::from_be_bytes(dol[o..o + 4].try_into().unwrap());
    (0..18).find_map(|k| {
        let (off, at, size) = (w(4 * k), w(0x48 + 4 * k), w(0x90 + 4 * k));
        (size != 0 && at <= addr && addr + len <= at + size)
            .then(|| &dol[(off + addr - at) as usize..(off + addr - at + len) as usize])
    })
}

#[test]
#[ignore = "needs SA2_MAIN_DOL"]
fn dol_routine_hashes() {
    let dol = std::fs::read(std::env::var("SA2_MAIN_DOL").unwrap()).unwrap();
    let mut bad = 0;
    for r in pc::DOL_ROUTINES {
        let b = dol_bytes(&dol, r.addr, r.len).expect("in a DOL section");
        let sha = format!("{:x}", Sha256::digest(b));
        if sha != r.sha256 {
            println!("{} hash {sha}", r.name);
            bad += 1;
        }
    }
    assert_eq!(bad, 0);
}
