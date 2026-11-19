use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_entities::StageEntities,
    native_player::{Character, PlayerState},
};

#[derive(Deserialize)]
struct Reference {
    level_ids: Vec<u8>,
    count: usize,
    positions_sha256: String,
}
fn bytes() -> Vec<u8> {
    std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap()
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn all_stage_interactables_match_independent_authoring_records() {
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let references: Vec<Reference> =
        serde_json::from_str(include_str!("../catalog/native-interactable-layouts.json")).unwrap();
    let mut slots = Vec::new();
    for record in references {
        for id in record.level_ids {
            let stage = StageEntities::load(&rom, id).unwrap();
            assert_eq!(stage.placements().len(), record.count);
            let mut packed = Vec::new();
            for e in stage.placements() {
                packed.extend(e.x.to_le_bytes());
                packed.extend(e.y.to_le_bytes());
                packed.push(e.kind);
                packed.extend(e.data);
            }
            assert_eq!(gba::sha256(&packed), record.positions_sha256, "stage {id}");
            slots.push(id);
        }
    }
    slots.sort_unstable();
    assert_eq!(slots, (0..31).collect::<Vec<_>>());
    for id in 31..=34 {
        assert!(StageEntities::load(&rom, id).is_err());
    }
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn leaf_forest_adjacent_triggers_resolve_in_native_list_order() {
    // The two adjacent 4-by-4-tile switches in region (13,2) include
    // their shared boundary. Foreground occurs first in the original list.
    let bytes = bytes();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut entities = StageEntities::load(&rom, 0).unwrap();
    let mut p = PlayerState::new_at(Character::Sonic, 3360 * 256, 744 * 256);
    p.layer = 0x80;
    p.move_state |= 0x01000000;
    entities.update_layers(&mut p, 3260, 675);
    assert_eq!(entities.update_layers(&mut p, 3260, 675), 2);
    assert_eq!(p.layer, 0x81);
    assert_eq!(p.move_state & 0x01000000, 0);
    p.q_world_x += 256;
    assert_eq!(entities.update_layers(&mut p, 3260, 675), 1);
    assert_eq!(p.layer, 0x80);
    p.q_world_y = 777 * 256;
    assert_eq!(entities.update_layers(&mut p, 3260, 675), 0);
    assert_eq!(p.layer, 0x80);
    // Already-active tasks perform one last collision before despawning,
    // and the original data may be activated again when the camera returns.
    p.q_world_y = 744 * 256;
    p.layer = 1;
    assert_eq!(entities.update_layers(&mut p, 1200, 675), 1);
    assert_eq!(p.layer, 0);
    p.layer = 1;
    assert_eq!(entities.update_layers(&mut p, 1200, 675), 0);
    entities.update_layers(&mut p, 3260, 675);
    assert_eq!(entities.update_layers(&mut p, 3260, 675), 1);
    assert_eq!(p.layer, 0);
}
