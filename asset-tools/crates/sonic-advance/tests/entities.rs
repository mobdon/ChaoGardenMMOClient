use sonic_advance_assets::{
    binary,
    entities::{EntityClass, EntityLayout, StageEntities},
};

fn fixture(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 16];
    bytes.extend_from_slice(payload);
    let length = (bytes.len() as u32) << 8;
    for (dst, word) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .take(4)
        .zip([length, 1, 1, 12])
    {
        dst.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[test]
fn placement_width_markers_and_invalid_pointers_are_checked() {
    let raw = fixture(&[1, 2, 5, 255]);
    let layout = EntityLayout::parse(&raw, EntityClass::ItemBox).unwrap();
    let p = &layout.records[0];
    assert_eq!((p.x, p.y, p.kind, p.record_offset), (8, 16, 5, 16));
    assert!(EntityLayout::parse(&raw, EntityClass::Enemy).is_err());
    for data in [&[253, 0, 0, 255][..], &[254, 0, 0, 255], &[2, 2, 4]] {
        assert!(EntityLayout::parse(&fixture(data), EntityClass::ItemBox).is_err());
    }
    for (at, value) in [(0, 1u32), (4, 0), (8, 257), (12, 1), (12, u32::MAX)] {
        let mut invalid = raw.clone();
        invalid[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(EntityLayout::parse(&invalid, EntityClass::ItemBox).is_err());
    }
    let mut alias = fixture(&[0; 8]);
    alias[4..8].copy_from_slice(&2u32.to_le_bytes());
    alias[12..16].copy_from_slice(&16u32.to_le_bytes());
    alias[16..20].copy_from_slice(&16u32.to_le_bytes());
    alias[23] = 255;
    assert!(EntityLayout::parse(&alias, EntityClass::ItemBox).is_err());
}

#[test]
fn trigger_uses_player_center_unsigned_extents_and_preserves_other_layer_bits() {
    let placements = EntityLayout::parse(
        &fixture(&[1, 2, 18, 128, 127, 2, 3, 4, 6, 17, 0, 0, 0, 0, 255]),
        EntityClass::Interactable,
    )
    .unwrap();
    let empty = EntityLayout::parse(&fixture(&[255]), EntityClass::Enemy).unwrap();
    let stage = StageEntities {
        interactables: placements,
        item_boxes: empty.clone(),
        enemies: empty,
    };
    for point in [(8, 16), (24, 40)] {
        assert_eq!(stage.collision_layer_at(point.0, point.1, 0x80), 0x81);
    }
    for point in [(7, 16), (25, 40), (8, 15), (24, 41)] {
        assert_eq!(stage.collision_layer_at(point.0, point.1, 0x80), 0x80);
    }
    assert_eq!(stage.collision_layer_at(32, 48, 0x81), 0x80);
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn all_native_entity_maps_match_pinned_csv_records_and_source_extents() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let lines: Vec<_> = include_str!("entity-evidence.tsv")
        .lines()
        .skip(1)
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .collect();
    assert_eq!(lines.len(), 54);
    for stage in 0..18 {
        let entities = StageEntities::load(&rom, stage).unwrap();
        for (group, (class, map, label)) in [
            (
                EntityClass::Interactable,
                &entities.interactables,
                "interactables",
            ),
            (EntityClass::ItemBox, &entities.item_boxes, "itemboxes"),
            (EntityClass::Enemy, &entities.enemies, "enemies"),
        ]
        .into_iter()
        .enumerate()
        {
            let row = &lines[group * 18 + stage];
            let num = |i: usize| row[i].parse::<usize>().unwrap();
            let hex =
                |i: usize| usize::from_str_radix(row[i].trim_start_matches("0x"), 16).unwrap();
            assert_eq!((row[0], num(1)), (label, stage));
            let table = class.table_offset();
            assert_eq!(table, hex(2));
            assert_eq!(binary::sha256(&rom[table..table + 72]), row[3]);
            let off = binary::rom_offset(
                binary::u32_at(&rom, table + stage * 4).unwrap(),
                rom.len(),
                4,
            )
            .unwrap();
            assert_eq!(off, hex(4));
            let decoded = binary::decompress(&rom[off..], 0x40000).unwrap();
            assert_eq!(decoded.consumed, num(5));
            assert_eq!(binary::sha256(&rom[off..off + decoded.consumed]), row[6]);
            assert_eq!(decoded.bytes.len(), num(7));
            assert_eq!(binary::sha256(&decoded.bytes), row[8]);
            assert_eq!(
                (map.width_regions, map.height_regions, map.records.len()),
                (num(9), num(10), num(11))
            );
            let mut packed = Vec::new();
            for p in &map.records {
                packed.extend(p.x.to_le_bytes());
                packed.extend(p.y.to_le_bytes());
                packed.push(p.kind);
                if class != EntityClass::ItemBox {
                    packed.extend(p.data);
                }
            }
            assert_eq!(binary::sha256(&packed), row[12]);
        }
    }
}

#[test]
fn camera_entry_changes_overlap_order_and_despawn_runs_after_the_trigger() {
    use sonic_advance_assets::entities::LayerTriggers;
    let mut raw = Vec::new();
    // Three regions. Front is in region zero, back in region one.
    for word in [40u32 << 8, 3, 1, 20, 28, 0] {
        raw.extend(word.to_le_bytes());
    }
    raw.extend([12, 5, 17, 0, 0, 32, 1, 255, 0, 5, 18, 0, 0, 20, 1, 255]);
    let layout = EntityLayout::parse(&raw, EntityClass::Interactable).unwrap();
    let mut triggers = LayerTriggers::default();
    assert_eq!(triggers.update(&layout, (350, 0), (280, 40), 0), 1);
    // Front enters later from the left. Its task runs last despite appearing first in the ROM.
    assert_eq!(triggers.update(&layout, (200, 0), (280, 40), 1), 0);
    // Destructors check the player once on their final tick.
    assert_eq!(triggers.update(&layout, (700, 0), (280, 40), 1), 0);
    assert_eq!(triggers.update(&layout, (700, 0), (280, 40), 1), 1);
}
