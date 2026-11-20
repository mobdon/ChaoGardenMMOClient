//! Compare `stage_objects::effects` with stg42D.rel's own particle code run
//! under the main.dol harness (tests/so_effects_ppc.py): the allocator
//! calls, the particle memory the routine leaves, its return value and the
//! MSL rand state, case by case and frame by frame.
use sa2_asset_tools::prison_visuals::MslRand;
use sa2_asset_tools::stage_objects::effects::{
    self, Creator, Descriptor, EffectHost, Exec, Particle, Spawn,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn oracle(which: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/so_effects_ppc.py"))
        .arg(which)
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
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn words(v: &Value) -> [u32; 16] {
    std::array::from_fn(|k| u(&v[k]))
}

struct Alloc {
    pre: Particle,
    fail: bool,
    events: Vec<Value>,
}

impl EffectHost for Alloc {
    fn particle_alloc(&mut self, descriptor: u32) -> Option<Particle> {
        self.events.push(json!(["alloc", descriptor]));
        (!self.fail).then_some(self.pre)
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn particle_creators_match_stg42d() {
    let Some(c) = oracle("creators") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10 * 120);
    for case in cases {
        let kind = Creator::ALL
            .into_iter()
            .find(|k| {
                format!("{:X}", k.stg42d()) == case["kind"].as_str().unwrap()
                    || (*k == Creator::MakeParticle && case["kind"] == "MakeParticle")
            })
            .unwrap();
        let pre = Particle::from_words(words(&case["pre"]));
        let mut host = Alloc {
            pre,
            fail: case["fail"].as_bool().unwrap(),
            events: Vec::new(),
        };
        let mut rand = MslRand {
            seed: u(&case["seed"]),
        };
        let spawn = Spawn {
            pos: v3(&case["pos"]),
            vel: v3(&case["vel"]),
            size: f(&case["size"]),
            work: f(&case["work"]),
        };
        let got = effects::create(kind, kind.stg42d_descriptor(), &spawn, &mut rand, &mut host);
        assert_eq!(json!(host.events), case["events"], "{case}");
        assert_eq!(got.is_none(), u(&case["ret"]) == 0, "{case}");
        let after = got.unwrap_or(pre);
        assert_eq!(after.to_words(), words(&case["post"]), "{kind:?} {case}");
        assert_eq!(rand.seed, u(&case["seed_after"]), "{case}");
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn particle_execs_match_stg42d() {
    let Some(c) = oracle("execs") else {
        return;
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10 * 120);
    for case in cases {
        let kind = Exec::ALL
            .into_iter()
            .find(|k| format!("{:X}", k.stg42d()) == case["kind"].as_str().unwrap())
            .unwrap();
        let d = &case["desc"];
        let desc = Descriptor {
            frames: u(&d[0]),
            frame_step: f(&d[1]),
            drag: f(&d[2]),
            gravity: f(&d[3]),
            growth: f(&d[4]),
        };
        let mut p = Particle::from_words(words(&case["p"]));
        let mut rand = MslRand {
            seed: u(&case["seed"]),
        };
        for (n, fr) in case["frames"].as_array().unwrap().iter().enumerate() {
            let ret = effects::exec(kind, &desc, &mut p, &mut rand);
            assert_eq!(p.to_words(), words(&fr["p"]), "{kind:?} frame {n} {case}");
            assert_eq!(ret, u(&fr["ret"]), "{kind:?} frame {n} {case}");
            assert_eq!(rand.seed, u(&fr["seed"]), "{kind:?} frame {n}");
        }
    }
}

/// A REL's own-module relocations: (section, offset, kind, target section,
/// target offset), and its sections' file offsets.
struct Rel {
    data: Vec<u8>,
    sections: Vec<usize>,
    relocs: Vec<(u8, usize, u8, u8, usize)>,
}

fn be(d: &[u8], at: usize) -> usize {
    u32::from_be_bytes(d[at..at + 4].try_into().unwrap()) as usize
}

fn rel(data: Vec<u8>) -> Rel {
    let (own, count, info) = (be(&data, 0), be(&data, 0xc), be(&data, 0x10));
    let sections = (0..count).map(|k| be(&data, info + 8 * k) & !1).collect();
    let (imp, imp_len) = (be(&data, 0x28), be(&data, 0x2c));
    let mut relocs = Vec::new();
    for k in 0..imp_len / 8 {
        if be(&data, imp + 8 * k) != own {
            continue;
        }
        let (mut at, mut section, mut cursor) = (be(&data, imp + 8 * k + 4), 0u8, 0usize);
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
            } else if kind != 201 {
                relocs.push((section, cursor, kind, s, be(r, 4)));
            }
        }
    }
    Rel {
        data,
        sections,
        relocs,
    }
}

/// The section-4 values (and 0xD540's colour word in section 5) each ported
/// routine loads are the same in every stage REL that carries the routine,
/// so the constants written into the port hold for all of them (the
/// relocation-masked hash alone does not show this: 0xC238 and
/// MakeParticle hash the same and load different constants).
#[test]
#[ignore = "requires SA2_GC_REL_DIR (the GameCube stage RELs)"]
fn routine_constants_are_shared_across_stage_rels() {
    use sa2_asset_tools::stage_objects::{effects::ROUTINES, masked_text};
    let dir = PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").expect("SA2_GC_REL_DIR"));
    let load = |name: &str| {
        let d = std::fs::read(dir.join(name)).unwrap();
        (masked_text(&d).unwrap(), rel(d))
    };
    // The values a routine loads, in instruction order: (site, bytes).
    let values = |r: &Rel, start: usize, len: usize| -> Vec<(usize, Vec<u8>)> {
        let mut v: Vec<_> = r
            .relocs
            .iter()
            .filter(|(s, o, k, ts, _)| {
                *s == 1 && (start..start + len).contains(o) && *k == 4 && (*ts == 4 || *ts == 5)
            })
            .filter_map(|&(_, o, _, ts, to)| {
                let at = r.sections[ts as usize] + to;
                let word = &r.data[at..at + 4];
                // Section 5 targets are descriptors (passed by address)
                // except 0xD540's colour, the one word it loads (+0x19C).
                if ts == 5 && !(len == 0x1d4 && o - start == 0x19e) {
                    return None;
                }
                // The 0x43300000 magic doubles are read as 8 bytes.
                let n = if word == [0x43, 0x30, 0, 0] { 8 } else { 4 };
                Some((o - start, r.data[at..at + n].to_vec()))
            })
            .collect();
        v.sort();
        v
    };
    let (ref_text, ref_rel) = load("stg42D.rel");
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
    let others: Vec<_> = names.iter().map(|n| (n.clone(), load(n))).collect();
    // Several routines share one masked code shape and differ only in the
    // constants they load (0xC238 and MakeParticle; other stage-specific
    // creators built from the same template). A REL carries a routine when
    // one of its copies loads the same constants; same-shaped copies with
    // other constants are other routines and are only counted.
    let wants: Vec<(&[u8], _)> = ROUTINES
        .iter()
        .map(|r| {
            let (start, len) = (r.stg42d as usize, r.len as usize);
            (&ref_text[start..start + len], values(&ref_rel, start, len))
        })
        .collect();
    let mut bad = Vec::new();
    for (r, (code, want)) in ROUTINES.iter().zip(&wants) {
        let len = r.len as usize;
        let (mut rels, mut others_shape) = (0, 0);
        for (_, (text, other)) in &others {
            let mut here = false;
            for o in (0..text.len().saturating_sub(len)).step_by(4) {
                if &text[o..o + len] != *code {
                    continue;
                }
                if values(other, o, len) == *want {
                    here = true;
                } else {
                    others_shape += 1;
                }
            }
            rels += usize::from(here);
        }
        eprintln!(
            "{:32} {:2} loads, same constants in {rels} other RELs ({others_shape} same-shaped copies with other constants)",
            r.name,
            want.len()
        );
        if rels == 0 {
            bad.push(format!(
                "{} found with its constants in no other REL",
                r.name
            ));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
