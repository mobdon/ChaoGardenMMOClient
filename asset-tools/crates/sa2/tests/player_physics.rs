use sa2_asset_tools::player_physics;
use sha2::{Digest, Sha256};

#[test]
fn rejects_an_unknown_image() {
    assert!(player_physics::read(b"MZ").is_err());
}

#[test]
#[ignore = "requires SA2_PC_EXE pointing to the pinned Steam executable"]
fn matches_independent_character_parameter_decoder() {
    let bytes = std::fs::read(std::env::var_os("SA2_PC_EXE").unwrap()).unwrap();
    let rows = player_physics::read(&bytes).unwrap();
    assert_eq!(rows.len(), 19);
    assert_eq!(rows[0].character, Some("Sonic"));
    assert_eq!(rows[11].character, Some("B"));
    assert_eq!(rows[16].character, Some("Chaos"));
    assert!(rows[17..].iter().all(|row| row.character.is_none()));
    let mut hash = Sha256::new();
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.slot, i);
        assert_eq!(row.source_va, 0x17391e8 + i as u32 * 132);
        hash.update((i as u32).to_le_bytes());
        let value = serde_json::to_value(&row.parameters).unwrap();
        let fields = value.as_object().unwrap();
        assert_eq!(fields.len(), 33);
        let mut sorted = fields.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|(name, _)| name.as_str());
        for (name, value) in sorted {
            hash.update(name.as_bytes());
            hash.update([0]);
            if name == "jump2_timer" {
                hash.update((value.as_i64().unwrap() as i32).to_le_bytes());
            } else {
                hash.update((value.as_f64().unwrap() as f32).to_le_bytes());
            }
        }
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        "e233e63003d72275a6dfc5f0e615d4f8ae72b53e3242707f3fe272a3b0c119d3"
    );
}
