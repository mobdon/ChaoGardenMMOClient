use sonic_advance_2_assets::{VerifiedRom, gba, graphics::RenderedMap, native_garden_home_art};

fn empty() -> Vec<u8> {
    let mut record = vec![0; 0x858];
    record[0x1c] = 255;
    record[0x58] = 255;
    record[0x59] = 255;
    record[0x5a..0x62].fill(255);
    record
}
fn canvas() -> RenderedMap {
    RenderedMap {
        width: 240,
        height: 160,
        rgba: vec![0; 240 * 160 * 4],
    }
}

#[test]
#[ignore = "requires verified local A2NE_ROM; native constructor/OAM pixel fingerprints"]
fn original_home_actor_pixels_and_source_spans_match_native_proof() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let evidence: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-garden-home-parity.json")).unwrap();
    assert_eq!(gba::sha256(rom.garden_bytes()), evidence["payloadSha256"]);
    for span in evidence["sourceSpans"].as_array().unwrap() {
        let at = span["offset"].as_u64().unwrap() as usize;
        let length = span["bytes"].as_u64().unwrap() as usize;
        assert_eq!(
            gba::sha256(&rom.garden_bytes()[at..at + length]),
            span["sha256"]
        );
    }
    for sample in evidence["samples"].as_array().unwrap() {
        let name = sample["name"].as_str().unwrap();
        let mut record = empty();
        if name == "chao-unset-position" {
            record[0x58] = 0;
            record[0x74] = 100;
        } else {
            let parts = name.split('-').collect::<Vec<_>>();
            let position = [
                parts[2].parse::<u8>().unwrap(),
                parts[3].parse::<u8>().unwrap(),
            ];
            match parts[0] {
                "egg" => {
                    record[0x59] = 0;
                    record[0x1f..0x21].copy_from_slice(&position);
                }
                "fruit" => {
                    record[0x5a] = 0;
                    record[0x21..0x23].copy_from_slice(&position);
                }
                _ => panic!("Unknown evidence sample"),
            }
        }
        let before = record.clone();
        let mut frame = canvas();
        let report = native_garden_home_art::paint(&rom, &record, &mut frame).unwrap();
        assert_eq!(record, before);
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            sample["report"],
            "{name}"
        );
        assert_eq!(gba::sha256(&frame.rgba), sample["rgbaSha256"], "{name}");
    }
}

#[test]
#[ignore = "requires verified local A2NE_ROM; malformed-frame rollback and unsupported appearance omission"]
fn home_inspection_is_read_only_and_never_guesses_transferred_appearance() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut record = empty();
    let mut frame = canvas();
    let before = frame.clone();
    assert!(native_garden_home_art::paint(&rom, &record[..0x857], &mut frame).is_err());
    assert_eq!(frame, before);
    record[0x59] = 0;
    record[0x1f] = 255;
    assert!(native_garden_home_art::paint(&rom, &record, &mut frame).is_err());
    assert_eq!(frame, before);
    record = empty();
    record[0x58] = 0;
    record[0x72] = 12;
    let report = native_garden_home_art::paint(&rom, &record, &mut frame).unwrap();
    assert!(report.actors.is_empty());
    assert_eq!(report.omitted_chao_appearance, Some(12));
    assert_eq!(frame, before);
    frame.rgba.pop();
    assert!(native_garden_home_art::paint(&rom, &record, &mut frame).is_err());
}
