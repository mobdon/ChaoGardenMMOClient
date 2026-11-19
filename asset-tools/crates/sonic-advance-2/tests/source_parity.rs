//! Opt-in actual-source verification. No cartridge or extracted asset fixture.
use serde::Deserialize;
use sonic_advance_2_assets::{VerifiedRom, catalog, gba, graphics, tables};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    schema_version: u32,
    rom_sha1: String,
    rom_sha256: String,
    garden_sha1: String,
    garden_sha256: String,
    garden_compressed_bytes: usize,
    resources: Vec<ResourceOracle>,
}

#[derive(Debug, Deserialize)]
struct ResourceOracle {
    id: String,
    source: String,
    offset: usize,
    length: usize,
    format: String,
    sha256: String,
    decoded: Option<DecodedOracle>,
}

#[derive(Debug, Deserialize)]
struct DecodedOracle {
    format: String,
    bytes: usize,
    units: usize,
    sha256: String,
}

#[test]
#[ignore = "Set A2NE_ROM to the verified local raw cartridge; run independent_parity.py first"]
fn every_catalog_range_and_decoded_output_matches_independent_source_oracle() {
    let path = std::env::var_os("A2NE_ROM").expect("set A2NE_ROM");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 16_777_216);
    let rom_bytes = std::fs::read(path).unwrap();
    let rom = VerifiedRom::parse(&rom_bytes).unwrap();
    let oracle: Oracle = serde_json::from_slice(
        &std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/catalog/parity.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(oracle.schema_version, 1);
    let report = catalog::manifest(&rom).unwrap();
    assert_eq!(report.rom_sha1, oracle.rom_sha1);
    assert_eq!(report.rom_sha256, oracle.rom_sha256);
    assert_eq!(report.garden_sha1, oracle.garden_sha1);
    assert_eq!(report.garden_sha256, oracle.garden_sha256);
    assert_eq!(
        report.garden_compressed_bytes,
        oracle.garden_compressed_bytes
    );
    assert_eq!(report.resources.len(), oracle.resources.len());
    let expected: BTreeMap<_, _> = oracle
        .resources
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    assert_eq!(
        expected.len(),
        oracle.resources.len(),
        "duplicate oracle IDs"
    );
    for actual in report.resources {
        let resource = &actual.resource;
        let oracle = expected
            .get(resource.id.as_str())
            .unwrap_or_else(|| panic!("{} missing from independent oracle", resource.id));
        assert_eq!(resource.source, oracle.source, "{} source", resource.id);
        assert_eq!(resource.offset, oracle.offset, "{} offset", resource.id);
        assert_eq!(resource.length, oracle.length, "{} length", resource.id);
        assert_eq!(resource.format, oracle.format, "{} format", resource.id);
        assert_eq!(actual.sha256, oracle.sha256, "{} source hash", resource.id);
        match (&actual.decoded, &oracle.decoded) {
            (Some(actual), Some(expected)) => {
                assert_eq!(
                    actual.format, expected.format,
                    "{} decoded format",
                    resource.id
                );
                assert_eq!(
                    actual.bytes, expected.bytes,
                    "{} decoded bytes",
                    resource.id
                );
                assert_eq!(
                    actual.units, expected.units,
                    "{} decoded units",
                    resource.id
                );
                assert_eq!(
                    actual.sha256, expected.sha256,
                    "{} decoded hash",
                    resource.id
                );
            }
            (None, None) => {}
            _ => panic!("{} decoded coverage differs", resource.id),
        }
        let raw = rom
            .range(&resource.source, resource.offset, resource.length)
            .unwrap();
        // Repack typed fields independently of the catalog's decoded stream.
        // This proves no raw palette bit, nibble, or unnamed motion byte is lost.
        let repacked: Option<Vec<u8>> = match resource.format.as_str() {
            "palette-bgr555" => Some(
                graphics::parse_palette(raw, 256)
                    .unwrap()
                    .iter()
                    .flat_map(|color| color.raw.to_le_bytes())
                    .collect(),
            ),
            "tiles-4bpp" => Some(
                graphics::decode_4bpp_tiles(raw, 16_384)
                    .unwrap()
                    .iter()
                    .flat_map(|tile| {
                        tile.as_chunks::<2>()
                            .0
                            .iter()
                            .map(|pair| pair[0] | (pair[1] << 4))
                    })
                    .collect(),
            ),
            "tiles-8bpp" => Some(
                graphics::decode_8bpp_tiles(raw, 16_384)
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
            "motion-8" => Some(
                tables::motion_records(raw, 4096)
                    .unwrap()
                    .iter()
                    .flat_map(|row| row.first_word.to_le_bytes().into_iter().chain(row.fields))
                    .collect(),
            ),
            "text-map" => Some(
                raw.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|word| {
                        graphics::TilemapEntry::from_word(u16::from_le_bytes([word[0], word[1]]))
                    })
                    .flat_map(|entry| entry.raw.to_le_bytes())
                    .collect(),
            ),
            "pointer-table" => Some(
                tables::pointer_table(
                    raw,
                    if resource.source == "rom" {
                        0x08000000
                    } else {
                        0x02000000
                    },
                    rom.source_bytes(&resource.source).unwrap().len(),
                )
                .unwrap()
                .iter()
                .flat_map(|row| row.pointer.to_le_bytes())
                .collect(),
            ),
            _ => None,
        };
        if let Some(repacked) = repacked {
            assert_eq!(repacked, raw, "{} typed source round trip", resource.id);
            assert_eq!(gba::sha256(&repacked), oracle.sha256);
        }
    }
}
