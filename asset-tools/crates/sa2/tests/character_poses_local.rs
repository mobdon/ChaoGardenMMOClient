use sa2_asset_tools::{character_motions::character_pose, model, motion, pose};
use std::{env, fs, path::PathBuf};

#[test]
#[ignore = "requires installed game and splitMDL/splitMTN exports under SA2_CHARACTER_SPLITS"]
fn native_and_split_story_rigs_agree() {
    let game = PathBuf::from(env::var_os("SA2_GAME_PATH").unwrap());
    let gd = game.join("resource/gd_PC");
    let refs = PathBuf::from(env::var_os("SA2_CHARACTER_SPLITS").unwrap());
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    for (name, stem, state, model_id, motion_id) in [
        ("Tails", "miles", 6, 208, 96),
        ("MechEggman", "ewalk", 6, 248, 91),
        ("MechTails", "twalk", 6, 293, 143),
        ("Eggman", "egg", 6, 230, 91),
        ("Knuckles", "knuck", 0, 142, 128),
        ("Rouge", "rouge", 0, 171, 168),
        ("Sonic", "sonic", 87, 0, 87),
        ("Shadow", "terios", 0, 65, 169),
        ("SuperSonic", "ssonic", 0, 328, 87),
        ("SuperShadow", "sshadow", 0, 355, 87),
    ] {
        let mdl = fs::read(gd.join(format!("{stem}mdl.prs"))).unwrap();
        let bank = fs::read(gd.join(format!("{stem}mtn.prs"))).unwrap();
        let native =
            character_pose(&bank, &exe, &mdl, state, 0.5).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(native["character"], name);
        assert_eq!(native["row"]["model"], model_id);
        assert_eq!(native["row"]["motion"], motion_id);
        let split_model = model::read_model(
            &fs::read(refs.join(format!("{stem}mdl/{model_id}.sa2mdl"))).unwrap(),
        )
        .unwrap();
        let split_motion = motion::read_motion(
            &fs::read(refs.join(format!("{stem}mtn/{motion_id}.saanim"))).unwrap(),
        )
        .unwrap();
        let expected = serde_json::to_value(
            pose::evaluate_model_pose(&split_model, Some(&split_motion), 0.5).unwrap(),
        )
        .unwrap();
        let nodes = native["pose"]["nodes"].as_array().unwrap();
        let reference = expected["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), reference.len());
        for (a, b) in nodes.iter().zip(reference) {
            for key in ["localMatrix", "worldMatrix", "motionPartIndex", "flags"] {
                assert_eq!(a[key], b[key], "{name} {key}");
            }
        }
        if name == "Shadow" {
            let sonic = fs::read(gd.join("sonicmtn.prs")).unwrap();
            assert!(character_pose(&sonic, &exe, &mdl, state, 0.5).is_err());
        }
    }
}
