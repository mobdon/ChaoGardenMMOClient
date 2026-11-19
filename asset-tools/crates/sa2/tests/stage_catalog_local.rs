// Preset explicit sections for this independently fingerprinted installation.
const CASES: &[(&str, &str)] = &[
    ("0000", "Extra Stages\\\\Test Stage"),
    ("0003", "Hero Story\\\\Green Forest"),
    ("0004", "Dark Story\\\\White Jungle"),
    ("0005", "Hero Story\\\\Pumpkin Hill"),
    ("0006", "Dark Story\\\\Sky Rail"),
    ("0007", "Hero Story\\\\Aquatic Mine"),
    ("0008", "Dark Story\\\\Security Hall"),
    ("0009", "Hero Story\\\\Prison Lane"),
    ("0010", "Hero Story\\\\Metal Harbor"),
    ("0011", "Dark Story\\\\Iron Gate"),
    ("0012", "Dark Story\\\\Weapons Bed"),
    ("0013", "Hero Story\\\\City Escape"),
    ("0014", "Dark Story\\\\Radical Highway"),
    ("0015", "2P Mech Stages\\\\Weapons Bed"),
    ("0016", "Hero Story\\\\Wild Canyon"),
    ("0017", "Hero Story\\\\Mission Street"),
    ("0018", "Dark Story\\\\Dry Lagoon"),
    ("0019", "Bosses\\\\Sonic vs Shadow 1"),
    ("0020", "Bosses\\\\Tails vs Eggman 1"),
    ("0021", "Dark Story\\\\Sand Ocean"),
    ("0022", "Hero Story\\\\Crazy Gadget"),
    ("0023", "Hero Story\\\\Hidden Base"),
    ("0024", "Hero Story\\\\Eternal Engine"),
    ("0025", "Hero Story\\\\Death Chamber"),
    ("0026", "Dark Story\\\\Egg Quarters"),
    ("0027", "Dark Story\\\\Lost Colony"),
    ("0028", "Hero Story\\\\Pyramid Cave"),
    ("0029", "Bosses\\\\Tails vs Eggman 2"),
    ("0030", "Hero Story\\\\Final Rush"),
    ("0031", "Extra Stages\\\\Green Hill"),
    ("0032", "Hero Story\\\\Meteor Herd"),
    ("0033", "Bosses\\\\Knuckles vs Rouge"),
    ("0034", "Last Story\\\\Cannon's Core (Sonic)"),
    ("0035", "Last Story\\\\Cannon's Core (Eggman)"),
    ("0036", "Last Story\\\\Cannon's Core (Tails)"),
    ("0037", "Last Story\\\\Cannon's Core (Rouge)"),
    ("0038", "Last Story\\\\Cannon's Core (Knuckles)"),
    ("0039", "2P Mech Stages\\\\Mission Street"),
    ("0040", "Dark Story\\\\Final Chase"),
    ("0041", "2P Treasure Hunting Stages\\\\Wild Canyon"),
    ("0042", "Bosses\\\\Sonic vs Shadow 2"),
    ("0043", "Dark Story\\\\Cosmic Wall"),
    ("0044", "Dark Story\\\\Mad Space"),
    ("0045", "2P Mech Stages\\\\Sand Ocean"),
    ("0046", "2P Treasure Hunting Stages\\\\Dry Lagoon"),
    ("0047", "2P Mech Stages\\\\Pyramid Race"),
    ("0048", "2P Mech Stages\\\\Hidden Base"),
    ("0049", "2P Treasure Hunting Stages\\\\Pool Quest"),
    ("0050", "2P Treasure Hunting Stages\\\\Planet Quest"),
    ("0051", "2P Mech Stages\\\\Deck Race"),
    ("0052", "2P Speed Stages\\\\Downtown Race"),
    ("0053", "2P Mech Stages\\\\Cosmic Wall"),
    ("0054", "2P Speed Stages\\\\Grind Race"),
    ("0055", "2P Mech Stages\\\\Lost Colony"),
    ("0056", "2P Mech Stages\\\\Eternal Engine"),
    ("0057", "2P Speed Stages\\\\Metal Harbor"),
    ("0058", "2P Mech Stages\\\\Iron Gate"),
    ("0059", "2P Treasure Hunting Stages\\\\Death Chamber"),
    ("_b_bigbogy", "Bosses\\\\King Boom Boo"),
    ("_b_bigfoot", "Bosses\\\\Big Foot"),
    ("_b_fdog", "Bosses\\\\Flying Dog"),
    ("_b_golem", "Bosses\\\\Egg Golem (Sonic)"),
    ("_b_golem_e", "Bosses\\\\Egg Golem (Eggman)"),
    ("_b_hotshot", "Bosses\\\\Hot Shot"),
    ("_b_last1", "Bosses\\\\Biolizard"),
    ("_chao_dark", "Chao World\\\\Dark Garden"),
    ("_chao_hero", "Chao World\\\\Hero Garden"),
    ("_chao_neut", "Chao World\\\\Neutral Garden"),
];
use sa2_asset_tools::{manifest::sha256, set::parse_set, stage_catalog::Catalog};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
#[ignore = "requires the original PC game and both SA Tools PC_SA2 configuration files"]
fn installed_numbered_boss_and_chao_sets() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let level = fs::read_to_string(cfg.join("sa2lvl.ini")).unwrap();
    let objects = fs::read_to_string(cfg.join("ObjectLists.ini")).unwrap();
    assert_eq!(
        sha256(level.as_bytes()),
        "854a458f0a3b868194727988b7276830dd7a87bc383939d191945754032035c4"
    );
    assert_eq!(
        sha256(objects.as_bytes()),
        "85fb9c71b82690f806ba59fe977eb390c73932c84594484e10f7f62a31cdbabd"
    );
    let catalog = Catalog::new(&exe, &level, &objects).unwrap();
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(game.join("resource/gd_PC")).unwrap() {
        let path = entry.unwrap().path();
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        if name.starts_with("set") && name.ends_with(".bin") && !name.starts_with("setcart") {
            files.insert(name, path);
        }
    }
    let mut source = Sha256::new();
    let mut semantic = Sha256::new();
    let cases: BTreeMap<_, _> = CASES.iter().copied().collect();
    let mut totals = [[0usize; 3]; 3];
    for (name, path) in files {
        let b = fs::read(path).unwrap();
        source.update(format!("{name} {}\n", sha256(&b)));
        semantic.update(format!("{name}\n"));
        let stem = &name[3..name.len() - 4];
        let numbered = stem.starts_with(|c: char| c.is_ascii_digit());
        let cat = if numbered {
            0
        } else if stem.starts_with("_b_") {
            1
        } else {
            2
        };
        let key = if numbered {
            &stem[..4]
        } else {
            stem.strip_suffix("_s")
                .or_else(|| stem.strip_suffix("_u"))
                .unwrap_or(stem)
        };
        let totals = &mut totals[cat];
        totals[0] += 1;
        if key == "0001" {
            assert!(catalog.bind(&b, "0001").is_err());
            for p in parse_set(&b).unwrap() {
                semantic.update(p.object_id.to_le_bytes());
                semantic.update(b"UNKNOWN\0");
                totals[1] += 1;
                totals[2] += 1;
            }
            continue;
        }
        let report = catalog.bind(&b, cases[key]).unwrap();
        assert!(report.table_gap.is_none());
        if numbered {
            assert_eq!(report.level_id, key);
        } else {
            assert_eq!(
                report.set_name.as_deref().unwrap().to_ascii_lowercase(),
                key
            );
        }
        if cat == 2 {
            assert_eq!(report.level_id, "0090");
            assert_eq!(report.level_id_sections.len(), 19);
        }
        for (i, p) in report.placements.iter().enumerate() {
            totals[1] += 1;
            assert_eq!(p.placement_index, i);
            assert_eq!(p.source_offset, 32 + 32 * i);
            semantic.update(p.placement.object_id.to_le_bytes());
            if let Some(index) = p.definition_index {
                assert_eq!(index, usize::from(p.placement.object_id));
                let d = &report.object_definitions[index];
                semantic.update([d.load_flags, d.list]);
                semantic.update(d.flags.to_le_bytes());
                semantic.update(d.distance.to_le_bytes());
                semantic.update(d.function_va.to_le_bytes());
                semantic.update(d.name.as_bytes());
                semantic.update([0]);
            } else {
                totals[2] += 1;
                semantic.update(b"UNKNOWN\0");
            }
        }
        let expected = match name.as_str() {
            "set0011_2p_s.bin" => BTreeMap::from([(92, 38)]),
            "set0046_2p_s.bin" => BTreeMap::from([(89, 1), (92, 1), (94, 1)]),
            _ => BTreeMap::new(),
        };
        assert_eq!(report.unresolved_id_counts, expected);
    }
    assert_eq!(totals, [[209, 58592, 94], [14, 156, 0], [9, 272, 0]]);
    assert_eq!(
        format!("{:x}", source.finalize()),
        "f35a34578d4ca308558f7302cd3ac57c5c3296b45cfc0f98829a96356596bdc9"
    );
    assert_eq!(
        format!("{:x}", semantic.finalize()),
        "78be8cf2eefe3875b191a97b4b97a162bfe9ea9782ac132ed251ba4c84ce9b65"
    );
    let empty = [0; 32];
    for (k, alias) in [("0046", "0047"), ("0029", "0056")] {
        let x = catalog.bind(&empty, cases[k]).unwrap();
        assert!(x.sections_sharing_table.contains(&cases[alias].to_string()));
    }
    for section in [r"Bosses\\Final Hazard", r"Chao World\\Chao Lobby"] {
        assert!(catalog.bind(&empty, section).unwrap().table_gap.is_some());
    }
    assert!(catalog.bind(&empty, "0090").is_err());
    eprintln!("232 source SET files, 59020 placements, 58926 matched to configured executable definitions");
}
