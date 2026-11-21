//! `sa2-recomp rels`: recompile every REL. Steam's gd_PC copy is used wherever
//! it differs from the disc (mob's rule: Steam always wins). Steam RELs were
//! linked against another main.dol build, so their main.dol imports are mapped
//! to GSNE8P addresses using the RELs both versions share.

use crate::gen::{self, R_REL24};
use crate::rel::{prs, Rel, Reloc};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::path::{Path, PathBuf};

const PER_FILE: usize = 150;

fn read_rel(p: &Path) -> Option<Rel> {
    let raw = std::fs::read(p).ok()?;
    let plausible = |b: &[u8]| b.len() > 0x40 && (1..=3).contains(&u32::from_be_bytes(b[0x1C..0x20].try_into().unwrap()));
    let bytes = if plausible(&raw) { raw } else { prs(&raw) };
    if !plausible(&bytes) {
        return None;
    }
    Rel::parse(bytes).ok()
}

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    // .prs first: the Chao folders also hold a stale, unpacked ChaoStgLobby.rel.
    ["prs", "rel", "REL"].iter().map(|e| dir.join(format!("{name}.{e}"))).find(|p| p.exists())
}

fn main_relocs(r: &Rel) -> Vec<&Reloc> {
    r.relocs.iter().filter(|x| x.module == 0).collect()
}

/// What a main.dol relocation site looks like apart from the address: its
/// kind, section, and the instruction with the relocated field cleared.
fn site_key(r: &Rel, x: &Reloc) -> (u8, u8, u32) {
    let word = r
        .sections
        .get(x.section as usize)
        .filter(|s| s.exec && s.offset != 0)
        .map_or(0, |s| r.word(s, x.offset));
    let mask = match x.kind {
        R_REL24 => 0xFC00_0003,
        4..=6 => 0xFFFF_0000,
        _ => 0,
    };
    (x.kind, x.section, word & mask)
}

/// Pair the disc and Steam builds' main.dol relocations of one module: the
/// longest common subsequence of their sites (inserted or removed code shifts
/// offsets, so pairing by position fails once the counts differ).
fn align<'a>(d: &'a Rel, s: &'a Rel) -> Vec<(&'a Reloc, &'a Reloc)> {
    let (a, b) = (main_relocs(d), main_relocs(s));
    if a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x.kind == y.kind) {
        return a.into_iter().zip(b).collect();
    }
    // shortcut: plain O(n*m) table, capped; fine for SA2's modules (largest ~6k sites).
    if a.len().saturating_mul(b.len()) > 64_000_000 {
        return Vec::new();
    }
    let (ka, kb): (Vec<_>, Vec<_>) = (a.iter().map(|x| site_key(d, x)).collect(), b.iter().map(|x| site_key(s, x)).collect());
    let w = b.len() + 1;
    let mut t = vec![0u16; (a.len() + 1) * w];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            t[i * w + j] = if ka[i] == kb[j] { t[(i + 1) * w + j + 1] + 1 } else { t[(i + 1) * w + j].max(t[i * w + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        if ka[i] == kb[j] {
            out.push((a[i], b[j]));
            i += 1;
            j += 1;
        } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

pub fn run(disc: &Path, steam: &Path, config: &Path, out: &Path) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(config)
        .map_err(|e| format!("{}: {e}", config.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("symbols.txt").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    // Load both versions and learn the Steam -> GSNE8P main.dol address map.
    let mut pairs = Vec::new();
    let mut votes: HashMap<u32, HashMap<u32, u32>> = HashMap::new();
    for n in &names {
        let disc_path = find(disc, n);
        let d = disc_path.as_deref().and_then(read_rel);
        let s = find(steam, n).and_then(|p| read_rel(&p));
        if let (Some(d), Some(s)) = (&d, &s) {
            for (x, y) in align(d, s) {
                *votes.entry(y.addend).or_default().entry(x.addend).or_default() += 1;
            }
        }
        pairs.push((n.clone(), d, s, disc_path));
    }
    let mut map: HashMap<u32, u32> =
        votes.into_iter().map(|(k, v)| (k, v.into_iter().max_by_key(|(_, c)| *c).unwrap().0)).collect();
    // Addresses no aligned pair covers: the two builds differ by inserted code,
    // so the Steam->disc shift is constant over long stretches. Use it when the
    // nearest mapped addresses on both sides agree on it.
    let known: BTreeMap<u32, u32> = map.iter().map(|(k, v)| (*k, *v)).collect();
    for p in &pairs {
        let Some(s) = &p.2 else { continue };
        for r in main_relocs(s) {
            if map.contains_key(&r.addend) {
                continue;
            }
            let lo = known.range(..r.addend).next_back();
            let hi = known.range(r.addend..).next();
            if let (Some((la, ld)), Some((ha, hd))) = (lo, hi) {
                let shift = ld.wrapping_sub(*la);
                if hd.wrapping_sub(*ha) == shift {
                    map.insert(r.addend, r.addend.wrapping_add(shift));
                }
            }
        }
    }

    std::fs::create_dir_all(out.join("src")).map_err(|e| e.to_string())?;
    let mut lib = String::from(
        "//! Generated by sa2-recomp from the local disc and Steam RELs. Do not commit.\n\
         #![allow(unused_assignments, unreachable_code, unused_variables, unused_mut, unused_braces, non_snake_case, clippy::all)]\n\
         use sa2_recomp::rt::Runtime;\n",
    );
    let mut register = String::from("pub fn register(rt: &mut Runtime) {\n");
    let dol_syms = std::fs::read_to_string(config.join("symbols.txt")).unwrap_or_default();
    let dol_funcs: BTreeSet<u32> =
        crate::dol::parse_symbols(&dol_syms).into_iter().filter(|s| s.function).map(|s| s.addr).collect();
    // One crate per module in one workspace: they compile in parallel, and no
    // single rustc has to hold all ~65k functions.
    let me = Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string().replace('\\', "/");
    let crate_toml = |name: &str, deps: &str| {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\nsa2-recomp = {{ path = \"{me}/rt\", package = \"sa2-recomp-rt\" }}\n{deps}"
        )
    };
    let mut members = Vec::new();
    let mut seen_ids = BTreeMap::new();
    let mut report = String::new();
    let (mut total_fns, mut total_unknown, mut unmapped_total) = (0, 0, 0);

    std::fs::create_dir_all(out.join("hybrid")).map_err(|e| e.to_string())?;
    let mut hybrids = String::new();
    for (name, d, s, disc_path) in &pairs {
        // Disc code, Steam values (the default): the disc module, which matches
        // GSNE8P's main.dol, with Steam's section bytes where they line up.
        // SA2_STEAM_MODULES=a,b|* uses Steam's own copy (imports remapped).
        let packed = disc_path.as_ref().is_some_and(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("prs")));
        let built;
        let (rel, from_steam) = match (d, s) {
            (_, Some(s)) if steam_only(name) || d.is_none() => (s, true),
            (Some(d), Some(s)) if !packed && !plain(name) => {
                let h = crate::hybrid::hybrid(d, s);
                let _ = writeln!(hybrids, "{name}: {} bytes from Steam, {} unmatched", h.patched, h.unmatched.len());
                for u in &h.unmatched {
                    let _ = writeln!(hybrids, "    {u}");
                }
                std::fs::write(out.join("hybrid").join(format!("{name}.rel")), &h.bytes).map_err(|e| e.to_string())?;
                built = Rel::parse(h.bytes).map_err(|e| format!("{name}: {e}"))?;
                (&built, false)
            }
            (Some(d), _) => {
                // A PRS-packed disc module can't be repacked with Steam's values:
                // the disc file is served as it is.
                if let Some(p) = disc_path {
                    let file = p.file_name().unwrap_or_default();
                    std::fs::copy(p, out.join("hybrid").join(file)).map_err(|e| e.to_string())?;
                }
                let why = if packed { "PRS-packed" } else if plain(name) { "Steam values not applied" } else { "no Steam copy" };
                let _ = writeln!(hybrids, "{name}: disc copy as is ({why})");
                (d, false)
            }
            _ => {
                let _ = writeln!(report, "{name}: missing in both folders");
                continue;
            }
        };
        let Some((ti, text)) = rel.text() else { continue };
        if let Some(prev) = seen_ids.insert((rel.id, text.size), name.clone()) {
            let _ = writeln!(report, "{name}: module id {} and code size also used by {prev}: only one is reachable", rel.id);
        }
        let id = rel.id;
        let word = |o: u32| (o.checked_add(4)? <= text.size).then(|| rel.word(text, o));

        // Relocations in the text section, as Rust target expressions.
        let mut relocs = HashMap::new();
        let mut starts: BTreeSet<u32> = BTreeSet::from([0]);
        // The header's prolog, epilog and unresolved handlers (section byte at
        // 0x30..0x33, offsets at 0x34..0x3F) are called by address.
        for k in 0..3 {
            let sec = rel.bytes.get(0x30 + k).copied().unwrap_or(0) as u32;
            let at = 0x34 + 4 * k;
            if sec == ti {
                if let Some(b) = rel.bytes.get(at..at + 4) {
                    starts.insert(u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                }
            }
        }
        let mut extra = BTreeSet::new();
        let mut unmapped = 0;
        let mut bad_calls = 0;
        for r in &rel.relocs {
            let target = if r.module == 0 {
                let a = if from_steam {
                    match map.get(&r.addend) {
                        Some(&a) => a,
                        None => {
                            unmapped += 1;
                            r.addend
                        }
                    }
                } else {
                    r.addend
                };
                // Check: a call into main.dol must land on a GSNE8P function start.
                if r.kind == R_REL24 && !dol_funcs.contains(&a) {
                    bad_calls += 1;
                }
                let a = {
                    a
                };
                format!("{a:#010x}u32")
            } else {
                format!("rt::sec(m, {}, {}).wrapping_add({:#x})", r.module, r.target_section, r.addend)
            };
            if r.module == id && r.target_section as u32 == ti {
                if r.kind == R_REL24 || gen::looks_like_entry(word, r.addend) {
                    starts.insert(r.addend);
                } else {
                    extra.insert(r.addend);
                }
            }
            if r.section as u32 == ti {
                // ADDR16 relocations name the halfword (instruction + 2); key
                // every code relocation by its instruction.
                relocs.insert(r.offset & !3, (r.kind, target));
            }
        }
        // Function boundaries: the decomp's symbols when the code is the disc's,
        // otherwise call targets (a jump into the middle becomes a tail call).
        if !from_steam {
            let syms = std::fs::read_to_string(config.join(name).join("symbols.txt")).unwrap_or_default();
            starts.extend(crate::dol::parse_symbols(&syms).into_iter().filter(|s| s.function && s.section == ".text").map(|s| s.addr));
        }
        let funcs =
            gen::layout(starts.into_iter().map(|a| (a, 0)).collect(), &[(0, text.size)], word, |o| relocs.contains_key(&o));
        let modname = format!("m_{}", name.to_lowercase());
        let mut dispatch = format!("pub fn dispatch(sec: u32, off: u32) -> Option<Func> {{\n    if sec != {ti} {{ return None; }}\n    match off {{\n");
        let mut module_src = String::from("use sa2_recomp::rt::Func;\n");
        std::fs::create_dir_all(out.join(&modname).join("src")).map_err(|e| e.to_string())?;
        let mut unknown = 0;
        for (part, chunk) in funcs.chunks(PER_FILE).enumerate() {
            let mut src = String::from("use sa2_recomp::rt::{self, *};\n\n");
            for &(a, end) in chunk {
                let ctx = gen::Ctx { start: a, end, rel: Some((id, ti)), relocs: relocs.clone() };
                let words: Vec<u32> = (a..end).step_by(4).map(|o| rel.word(text, o)).collect();
                let ex: BTreeSet<u32> = extra.range(a..end).copied().collect();
                let (code, u) = gen::function(name, &ctx, &words, &ex);
                unknown += u;
                src.push_str(&code);
                let _ = writeln!(dispatch, "        {a:#x} => Some(p{part}::{}),", gen::fn_name(&ctx));
            }
            let file = format!("{modname}/src/p{part}.rs");
            std::fs::write(out.join(&file), src).map_err(|e| e.to_string())?;
            let _ = writeln!(module_src, "pub mod p{part};");
        }
        dispatch.push_str("        _ => None,\n    }\n}\n");
        module_src.push_str(&dispatch);
        let module_src = format!(
            "//! {name}, generated by sa2-recomp. Do not commit.\n#![allow(unused_assignments, unreachable_code, unused_variables, unused_mut, unused_braces, non_snake_case, clippy::all)]\n{module_src}"
        );
        std::fs::write(out.join(&modname).join("src/lib.rs"), module_src).map_err(|e| e.to_string())?;
        std::fs::write(out.join(&modname).join("Cargo.toml"), crate_toml(&modname, "")).map_err(|e| e.to_string())?;
        members.push(modname.clone());
        // Keyed by code size too: some modules share an id (titleD/stg00D).
        // One feature per module (named like its crate): a build links only the
        // modules it needs (`--features sa2-recomp-rels/m_mcwarnd`), or `all`.
        let _ = writeln!(register, "    #[cfg(feature = \"{modname}\")]\n    rt.rels.insert(({id}, {:#x}), {modname}::dispatch);", text.size);
        let _ = writeln!(
            report,
            "{name}: id {id} {} functions {} unknown {unknown} calls not at a main.dol function {bad_calls} unmapped main.dol imports {unmapped}",
            if from_steam { "STEAM" } else { "disc" },
            funcs.len()
        );
        total_fns += funcs.len();
        total_unknown += unknown;
        unmapped_total += unmapped;
    }
    register.push_str("}\n");
    lib.push_str(&register);
    std::fs::write(out.join("src/lib.rs"), lib).map_err(|e| e.to_string())?;
    let deps: String = members.iter().map(|m| format!("{m} = {{ path = \"{m}\", optional = true }}\n")).collect();
    let feats: String = members.iter().map(|m| format!("{m} = [\"dep:{m}\"]\n")).collect();
    let list: Vec<String> = members.iter().map(|m| format!("\"{m}\"")).collect();
    let toml = crate_toml("sa2-recomp-rels", &deps)
        + &format!("\n[features]\nall = [{}]\n{feats}", list.join(", "))
        + &format!("\n[workspace]\nmembers = [{}]\n\n[profile.dev]\nopt-level = 1\n", list.join(", "));
    std::fs::write(out.join("Cargo.toml"), toml).map_err(|e| e.to_string())?;
    // 24 of these crates compiling at once made rustc abort (0xc0000409) on a
    // 32 GB machine; 6 at a time builds all 94 in under 3 minutes.
    std::fs::create_dir_all(out.join(".cargo")).map_err(|e| e.to_string())?;
    std::fs::write(out.join(".cargo/config.toml"), "[build]\njobs = 6\n").map_err(|e| e.to_string())?;
    std::fs::write(out.join("REPORT.txt"), &report).map_err(|e| e.to_string())?;
    std::fs::write(out.join("HYBRID.txt"), &hybrids).map_err(|e| e.to_string())?;
    println!(
        "modules {} | functions {total_fns} | unknown instructions {total_unknown} | unmapped Steam imports {unmapped_total} | address map {} entries | details in REPORT.txt",
        pairs.len(),
        map.len()
    );
    Ok(())
}

fn steam_only(name: &str) -> bool {
    std::env::var("SA2_STEAM_MODULES").unwrap_or_default().split(',').any(|s| s.trim() == "*" || s.trim().eq_ignore_ascii_case(name))
}

/// Modules kept as the disc has them (no Steam values): SA2_PLAIN_MODULES,
/// plus eventD by default. shortcut: eventD's patched data crashes the opening
/// event (a wild pointer at frame 882); find the bad patch, then drop it here.
fn plain(name: &str) -> bool {
    name.eq_ignore_ascii_case("eventD")
        || std::env::var("SA2_PLAIN_MODULES").unwrap_or_default().split(',').any(|s| s.trim().eq_ignore_ascii_case(name))
}
