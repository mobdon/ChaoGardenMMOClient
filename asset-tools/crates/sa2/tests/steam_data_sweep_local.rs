//! Runs every GameCube-format data reader over the owner's Steam install.
//!
//! Owner-local and ignored by default: set `SA2_GD_PC` to
//! `.../Sonic Adventure 2/resource/gd_PC` and run with `--ignored --nocapture`.
//! Executables and DLLs are never opened; game data is read, never copied.
use sa2_asset_tools::{
    archive, audio, character_motions::Bank, csb, demo, event, message_bank, set, stage_fog,
    stage_land, stage_lighting, texture,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

type Check = fn(&Path, &[u8]) -> Result<String, String>;
/// Group label, folder under gd_PC, lower-case file-name filter, reader.
type Group = (&'static str, &'static str, fn(&str) -> bool, Check);

fn files(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file())
                .filter(|p| {
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    !name.ends_with(".exe") && !name.ends_with(".dll") && keep(&name)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn gvm_archive(_: &Path, b: &[u8]) -> Result<String, String> {
    let d = archive::decompress_prs(b, 64 << 20).map_err(e)?;
    if d.get(..4) != Some(b"GVMH") {
        return Ok("skip:not GVM".into());
    }
    let entries = archive::read_gvm(&d).map_err(e)?;
    let mut palette = 0;
    for x in &entries {
        match texture::decode_gvr(x.data, None) {
            Ok(_) => {}
            Err(err) if err.to_string().contains("palette") => palette += 1,
            Err(err) => return Err(format!("{}: {err}", x.name)),
        }
    }
    Ok(format!(
        "{} textures, {palette} need external palette",
        entries.len()
    ))
}

fn pak_archive(_: &Path, b: &[u8]) -> Result<String, String> {
    let entries = archive::read_pak(b).map_err(e)?;
    let mut dds = 0;
    for x in &entries {
        if x.data.get(..4) == Some(b"DDS ") {
            texture::decode_dds(x.data).map_err(|err| format!("{}: {err}", x.name))?;
            dds += 1;
        }
    }
    Ok(format!("{} entries, {dds} DDS decoded", entries.len()))
}

/// Read a CSB bank and decode every ADX segment it stores.
fn csb_bank(b: &[u8]) -> Result<(usize, usize), String> {
    let bank = csb::read(b).map_err(e)?;
    let (mut decoded, mut other) = (0, 0);
    for el in &bank.elements {
        for s in &el.segments {
            if !s.adx || s.silent {
                other += 1;
                continue;
            }
            let adx = &b[s.offset..s.offset + s.length];
            audio::decode_adx(adx, 64 << 20).map_err(|err| format!("{}: {err}", el.name))?;
            decoded += 1;
        }
    }
    if bank.cues.is_empty() {
        return Err("bank has no cues".into());
    }
    Ok((decoded, other))
}

fn csb_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for p in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        if p.is_dir() {
            csb_files(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("csb")) {
            out.push(p);
        }
    }
}

fn checks() -> Vec<Group> {
    vec![
        (
            "SET layouts",
            "",
            |n| n.starts_with("set") && n.ends_with(".bin"),
            |_, b| {
                set::parse_set(b)
                    .map(|p| format!("{} placements", p.len()))
                    .map_err(e)
            },
        ),
        (
            "GVM textures (PRS)",
            "",
            |n| n.ends_with(".prs"),
            gvm_archive,
        ),
        (
            "GVP palettes",
            "",
            |n| n.ends_with(".gvp"),
            |_, b| texture::read_gvp(b).map(|_| "ok".into()).map_err(e),
        ),
        (
            "GVR textures",
            "",
            |n| n.ends_with(".gvr"),
            |_, b| match texture::decode_gvr(b, None) {
                Ok(i) => Ok(format!("{}x{}", i.width, i.height)),
                Err(err) if err.to_string().contains("palette") => Ok("needs palette".into()),
                Err(err) => Err(e(err)),
            },
        ),
        (
            "PC texture packs",
            "PRS",
            |n| n.ends_with(".pak"),
            pak_archive,
        ),
        ("PC UI packs", "SOC", |n| n.ends_with(".pak"), pak_archive),
        (
            "Character motion banks",
            "",
            |n| n.ends_with("mtn.prs"),
            |_, b| {
                Bank::read(b)
                    .map(|k| format!("{} motions", k.rows.len()))
                    .map_err(e)
            },
        ),
        (
            "Events",
            "event",
            |n| {
                n.len() == 9
                    && n.starts_with('e')
                    && n.ends_with(".prs")
                    && n[1..5].bytes().all(|c| c.is_ascii_digit())
            },
            |_, b| {
                event::read_prs(b)
                    .map(|v| format!("{} scenes", v.scenes.len()))
                    .map_err(e)
            },
        ),
        (
            "Messages",
            "Message",
            |n| n.ends_with(".prs"),
            |_, b| message_bank::read_prs(b).map(|_| "ok".into()).map_err(e),
        ),
        (
            "Messages (K)",
            "MessageK",
            |n| n.ends_with(".prs"),
            |_, b| message_bank::read_prs(b).map(|_| "ok".into()).map_err(e),
        ),
        (
            "Stage fog",
            "",
            |n| n.contains("_fog") && n.ends_with(".bin"),
            |_, b| stage_fog::read(b).map(|_| "ok".into()).map_err(e),
        ),
        (
            "Stage lighting",
            "",
            |n| n.ends_with("_light.bin"),
            |_, b| {
                stage_lighting::derive(b)
                    .map(|r| format!("{} rows", r.len()))
                    .map_err(e)
            },
        ),
        (
            "Stage lighting (GC override)",
            "",
            |n| n.ends_with("_light_gc.bin"),
            |_, b| {
                stage_lighting::resolve(None, Some(b))
                    .map(|_| "ok".into())
                    .map_err(e)
            },
        ),
        (
            "Demo inputs (raw)",
            "",
            |n| n.starts_with("demo") && n.ends_with(".bin"),
            |_, b| {
                demo::read_raw(b)
                    .map(|d| format!("{} frames", d.len()))
                    .map_err(e)
            },
        ),
        (
            "ADX music",
            "ADX",
            |n| n.ends_with(".adx"),
            |_, b| {
                audio::read_adx_header(b).map_err(e)?;
                audio::decode_adx(b, 64 << 20)
                    .map(|p| format!("{} samples", p.samples.len()))
                    .map_err(e)
            },
        ),
        (
            "AFS voice archives",
            "",
            |n| n.ends_with(".afs"),
            |_, b| {
                audio::read_afs(b)
                    .map(|v| format!("{} entries", v.len()))
                    .map_err(e)
            },
        ),
    ]
}

/// Files the readers refuse whose GameCube disc copies are byte-identical, so
/// they are reader limits, not Steam gaps. The Steam-edited text banks
/// (French, German, Italian, Spanish) carry the -1 terminator and are read.
const NO_TERMINATOR: &str = "offset table has no -1 terminator; message_bank requires one";
const KNOWN_REFUSALS: &[(&str, &str)] = &[
    (
        "event/e0350.prs",
        "header byte 0x2b is 4; event::read accepts 0 or 1",
    ),
    ("message/text_e.prs", NO_TERMINATOR),
    ("message/text_j.prs", NO_TERMINATOR),
    ("messagek/text_e.prs", NO_TERMINATOR),
    ("messagek/text_g.prs", NO_TERMINATOR),
    ("messagek/text_j.prs", NO_TERMINATOR),
    ("messagek/text_s.prs", NO_TERMINATOR),
];

fn known(path: &Path, root: &Path) -> Option<&'static str> {
    let rel = path
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    KNOWN_REFUSALS
        .iter()
        .find(|(name, _)| rel == *name)
        .map(|(_, why)| *why)
}

#[test]
#[ignore = "owner-local Steam install"]
fn every_gamecube_format_reader_accepts_the_steam_data() {
    let Ok(root) = std::env::var("SA2_GD_PC") else {
        eprintln!("SA2_GD_PC is not set");
        return;
    };
    let root = PathBuf::from(root);
    let mut failures = Vec::new();
    for (group, sub, keep, check) in checks() {
        let (mut ok, mut skipped, mut limits) = (0, 0, 0);
        for path in files(&root.join(sub), keep) {
            let bytes = fs::read(&path).expect("read Steam data file");
            match (check(&path, &bytes), known(&path, &root)) {
                (Ok(s), _) if s.starts_with("skip:") => skipped += 1,
                (Ok(_), None) => ok += 1,
                (Ok(_), Some(_)) => failures.push(format!(
                    "{group}: {} is now accepted; update KNOWN_REFUSALS",
                    path.display()
                )),
                (Err(_), Some(why)) => {
                    limits += 1;
                    println!("  known reader limit {}: {why}", path.display());
                }
                (Err(err), None) => failures.push(format!("{group}: {}: {err}", path.display())),
            }
        }
        println!("{group:32} ok {ok:4}  skipped {skipped:4}  known limits {limits}");
    }
    // Every CSB sound bank (Steam's replacement for the GameCube .mlt/.mpb banks).
    let mut banks = Vec::new();
    csb_files(&root, &mut banks);
    let (mut decoded, mut other) = (0, 0);
    for path in &banks {
        let bytes = fs::read(path).expect("read Steam CSB");
        match csb_bank(&bytes) {
            Ok((d, o)) => {
                decoded += d;
                other += o;
            }
            Err(err) => failures.push(format!("CSB sound banks: {}: {err}", path.display())),
        }
    }
    println!(
        "{:32} ok {:4}  ADX segments decoded {decoded}, silent stubs or non-ADX {other}",
        "CSB sound banks",
        banks.len()
    );
    // Every static landtable the catalog maps in the Steam stage, boss and Chao modules.
    let mut by_file: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for row in stage_land::catalog() {
        by_file.entry(row.filename).or_default().push(row);
    }
    let (mut lands, mut unsupported) = (0, 0);
    for (name, rows) in by_file {
        let Ok(bytes) = fs::read(root.join(name)) else {
            failures.push(format!("Landtables: {name} missing"));
            continue;
        };
        for row in rows {
            if !row.supported {
                unsupported += 1;
                continue;
            }
            match stage_land::read_named(&bytes, row.symbol) {
                Ok(_) => lands += 1,
                Err(err) => failures.push(format!("Landtables: {name} {}: {err}", row.symbol)),
            }
        }
    }
    println!(
        "{:32} ok {lands:4}  unsupported {unsupported:4}",
        "Landtables (REL)"
    );
    for f in &failures {
        println!("FAIL {f}");
    }
    assert!(
        failures.is_empty(),
        "{} Steam files were refused",
        failures.len()
    );
}
