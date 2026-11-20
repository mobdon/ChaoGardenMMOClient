use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_animation::AnimationPlayer,
    native_rings::{StageRings, stage_ring_palette},
};

#[derive(Deserialize)]
struct Reference {
    level_ids: Vec<u8>,
    count: usize,
    positions_sha256: String,
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn all_original_stage_ring_csvs_and_shared_hud_palette_match() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let records: Vec<Reference> =
        serde_json::from_str(include_str!("../catalog/native-ring-layouts.json")).unwrap();
    let mut visited = vec![];
    for record in records {
        for id in record.level_ids {
            let stage = StageRings::load(&rom, id).unwrap();
            assert_eq!(stage.placements().len(), record.count);
            let coords: Vec<u8> = stage
                .placements()
                .iter()
                .flat_map(|p| [p.x, p.y])
                .flat_map(i32::to_le_bytes)
                .collect();
            assert_eq!(gba::sha256(&coords), record.positions_sha256, "stage {id}");
            visited.push(id);
        }
    }
    visited.sort_unstable();
    assert_eq!(visited, (0u8..31).collect::<Vec<_>>());
    for id in 31..=34 {
        assert!(StageRings::load(&rom, id).is_err());
    }
    let palette = stage_ring_palette(&rom).unwrap();
    assert_eq!(
        gba::sha256(
            &palette
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        ),
        "19ed9d96dff83a3d55bbf111ee1f1c098ab9aa6122e54ca8e547ddc718b9091f"
    );
    for variant in 0..8 {
        let mut hud = AnimationPlayer::new(&rom, 1127, variant).unwrap();
        hud.advance().unwrap();
        assert_eq!(&hud.palette_words()[96..112], &palette);
    }
}
