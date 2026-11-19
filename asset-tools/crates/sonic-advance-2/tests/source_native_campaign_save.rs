use serde::Deserialize;
use sonic_advance_2_assets::{
    gba,
    native_campaign_save::{self as save, CampaignProfile, CampaignSaveSession, MemoryFlashFaults},
};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    profile_seed: u32,
    header_seed: u32,
    outputs: BTreeMap<String, Fingerprint>,
}
#[derive(Deserialize)]
struct Fingerprint {
    cases: usize,
    bytes: usize,
    sha256: String,
}
fn metadata() -> Oracle {
    serde_json::from_str(include_str!("../catalog/native-campaign-save-parity.json")).unwrap()
}
fn source(name: &str, expected: &Fingerprint) -> Vec<u8> {
    let directory =
        std::env::var_os("A2NE_CAMPAIGN_SAVE_ORACLE").expect("A2NE_CAMPAIGN_SAVE_ORACLE");
    let data =
        std::fs::read(std::path::PathBuf::from(directory).join(format!("{name}.bin"))).unwrap();
    assert_eq!(data.len(), expected.bytes);
    assert_eq!(gba::sha256(&data), expected.sha256);
    data
}
fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}
fn random_word(calls: &mut u32) -> u32 {
    let mut half = || {
        *calls += 1;
        (calls.wrapping_mul(0x9e37) ^ (*calls >> 3)) as u16
    };
    (u32::from(half()) << 16) | u32::from(half())
}
fn put32(raw: &mut [u8], offset: usize, value: u32) {
    raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn same(label: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len(), "{label} length");
    if let Some(index) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        panic!(
            "{label}: first differing byte {index:#x}: Rust {:#x}, original C {:#x}",
            actual[index], expected[index]
        );
    }
}
struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
    fn take(&mut self, bytes: usize) -> &'a [u8] {
        let start = self.position;
        self.position += bytes;
        &self.data[start..self.position]
    }
    fn words<const N: usize>(&mut self) -> [i32; N] {
        let raw = self.take(N * 4);
        std::array::from_fn(|i| i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()))
    }
}

#[test]
#[ignore = "A2NE_CAMPAIGN_SAVE_ORACLE required; exact pinned original C, synthetic profiles and in-memory flash only"]
fn campaign_profiles_pack_unpack_and_completed_state_match_original_c() {
    let metadata = metadata();
    let fingerprint = &metadata.outputs["profiles"];
    let data = source("profiles", fingerprint);
    let mut source = Cursor::new(&data);
    let default = CampaignProfile::new_usa();
    same(
        "GenerateNewSaveGame",
        &default.native_bytes(),
        source.take(save::PROFILE_BYTES),
    );
    let session = CampaignSaveSession::new(&vec![255; 16 * save::SECTOR_BYTES], false).unwrap();
    same(
        "InitSaveGameSectorData",
        session.sector_buffer(),
        source.take(save::PAYLOAD_BYTES),
    );
    let mut seed = metadata.profile_seed;
    for row in 0..fingerprint.cases {
        let mut raw = [0; save::PROFILE_BYTES];
        for byte in &mut raw {
            *byte = (next(&mut seed) >> 24) as u8;
        }
        if row % 16 == 0 {
            put32(&mut raw, 0, 0);
        }
        let mut profile = CampaignProfile::from_native_bytes(&raw).unwrap();
        same(
            "native profile byte round trip",
            &profile.native_bytes(),
            &raw,
        );
        let controls = match row % 4 {
            0 => Some([1u16, 2, 256]),
            1 => Some([256, 1, 2]),
            2 => Some([0, 3, 257]),
            _ => None,
        };
        if let Some([jump, attack, trick]) = controls {
            profile.button_config = save::ButtonConfig {
                jump,
                attack,
                trick,
            };
        }
        let previous_version = next(&mut seed);
        let mut calls = row as u32 * 4;
        let packed = save::pack_sector(&profile, previous_version, random_word(&mut calls));
        same(
            &format!("row {row}: PackSaveSectorData"),
            &packed,
            source.take(save::PAYLOAD_BYTES),
        );
        let unpacked = save::unpack_sector(&packed).unwrap();
        same(
            &format!("row {row}: UnpackSaveSectorData"),
            &unpacked.native_bytes(),
            source.take(save::PROFILE_BYTES),
        );
        profile.generate_completed(&mut || random_word(&mut calls));
        same(
            &format!("row {row}: GenerateCompletedSaveGame"),
            &profile.native_bytes(),
            source.take(save::PROFILE_BYTES),
        );
        assert_eq!(source.words::<1>(), [calls as i32]);
    }
    assert_eq!(source.position, data.len());
}

#[test]
#[ignore = "A2NE_CAMPAIGN_SAVE_ORACLE required; original C header selection including rollover quirks"]
fn campaign_header_selection_matches_original_c() {
    let metadata = metadata();
    let fingerprint = &metadata.outputs["headers"];
    let data = source("headers", fingerprint);
    let mut source = Cursor::new(&data);
    let mut seed = metadata.header_seed;
    let special = [0u32, 1, 0xffff, 0x10000, 0xffff0000, 0xfffffffe, 0xffffffff];
    for row in 0..fingerprint.cases {
        let mut flash = vec![0xa5; 16 * save::SECTOR_BYTES];
        for sector in 0..save::SECTOR_COUNT {
            let bits = next(&mut seed);
            put32(
                &mut flash,
                sector * save::SECTOR_BYTES,
                if bits & 3 != 0 { save::SECURITY } else { bits },
            );
            let bits = next(&mut seed);
            put32(
                &mut flash,
                sector * save::SECTOR_BYTES + 4,
                if bits % 8 == 7 {
                    bits
                } else {
                    special[bits as usize % 8]
                },
            );
        }
        let expected = source.words::<3>();
        let report = save::inspect_flash(&flash).unwrap();
        assert_eq!(
            [
                report.newest_candidate as i32,
                report.oldest_candidate.map_or(-1, |sector| sector as i32),
                i32::from(report.header_save_exists)
            ],
            expected,
            "header case {row}"
        );
    }
    assert_eq!(source.position, data.len());
}

fn fixture(scenario: usize) -> (CampaignSaveSession, MemoryFlashFaults) {
    let mut flash = vec![if scenario == 2 { 0 } else { 255 }; 16 * save::SECTOR_BYTES];
    flash[10 * save::SECTOR_BYTES..].fill(0xa5);
    let mut calls = 0;
    if matches!(scenario, 1 | 3 | 7) {
        let count = if scenario == 1 { 3 } else { 10 };
        for sector in 0..count {
            let mut profile = CampaignProfile::new_usa();
            profile.id = 0x100 + sector as u32;
            profile.score = (sector as u32 + 1) * 111;
            let mut packed = save::pack_sector(&profile, sector as u32, random_word(&mut calls));
            if scenario == 3 {
                put32(
                    &mut packed,
                    4,
                    if sector == 0 {
                        u32::MAX
                    } else if sector == 1 {
                        0
                    } else {
                        sector as u32
                    },
                );
                let sum = save::checksum(&packed).unwrap();
                put32(&mut packed, save::CHECKSUM_OFFSET, sum);
            }
            flash[sector * save::SECTOR_BYTES..sector * save::SECTOR_BYTES + save::PAYLOAD_BYTES]
                .copy_from_slice(&packed);
            if scenario == 7 {
                flash[sector * save::SECTOR_BYTES + 80] ^= 1;
            }
        }
        if scenario == 1 {
            flash[2 * save::SECTOR_BYTES + 80] ^= 1;
        }
    }
    let mut session = CampaignSaveSession::new(&flash, scenario != 4).unwrap();
    session.loaded.language = (3 + scenario) as u8;
    session.loaded.score = 900 + scenario as u32;
    let mut faults = MemoryFlashFaults::default();
    if scenario == 5 {
        faults.program_error = 7;
    }
    if scenario == 6 {
        faults.erase_errors[2] = 8;
    }
    (session, faults)
}

#[test]
#[ignore = "A2NE_CAMPAIGN_SAVE_ORACLE required; original C load/write/new-save lifecycle against a defined memory adapter"]
fn campaign_memory_session_lifecycle_matches_original_c() {
    let metadata = metadata();
    let fingerprint = &metadata.outputs["sessions"];
    assert_eq!(fingerprint.cases, 64);
    let data = source("sessions", fingerprint);
    let mut source = Cursor::new(&data);
    let name = [11, 22, 33, 44, 55, 0xffff];
    for scenario in 0..8 {
        let (mut session, faults) = fixture(scenario);
        let mut calls = 0u32;
        for step in 0..8 {
            let (success, attempted_sector, loaded_sector, erases) = if matches!(step, 0 | 6) {
                let report = session.load_latest(&faults);
                (
                    report.success,
                    None,
                    report.loaded_sector,
                    report.erase_attempts,
                )
            } else {
                let mut random = || random_word(&mut calls);
                let report = match step {
                    1 => {
                        session.loaded.score += 17;
                        session.loaded.multiplayer_wins = 98;
                        session.loaded.player_name = name;
                        session.write(&mut random, &faults)
                    }
                    2 | 4 => session.write(&mut random, &faults),
                    3 => {
                        session.loaded.id = 0;
                        session.loaded.language = 6;
                        session.loaded.score = 1234;
                        session.new_save(&mut random, &faults)
                    }
                    5 => {
                        session.loaded.generate_completed(&mut random);
                        session.write(&mut random, &faults)
                    }
                    7 => {
                        session.loaded.insert_multiplayer_profile(42, name);
                        session.loaded.record_own_result(0);
                        session.loaded.record_multiplayer_result(42, name, 1);
                        session.write(&mut random, &faults)
                    }
                    _ => unreachable!(),
                };
                (
                    report.success,
                    report.written_sector,
                    None,
                    report.erase_attempts,
                )
            };
            let attempted_mask = erases
                .iter()
                .fold(0i32, |mask, attempt| mask | (1 << attempt.sector));
            let success_mask = erases
                .iter()
                .filter(|attempt| attempt.error == 0)
                .fold(0i32, |mask, attempt| mask | (1 << attempt.sector));
            let header = [
                i32::from(success),
                attempted_sector.map_or(-1, |sector| sector as i32),
                attempted_mask,
                success_mask,
                calls as i32,
                i32::from(session.has_changes()),
                i32::from(session.save_exists()),
                save::find_newest_sector(session.flash_bytes()).unwrap() as i32,
                save::find_oldest_sector(session.flash_bytes())
                    .unwrap()
                    .map_or(-1, |sector| sector as i32),
                loaded_sector.map_or(-1, |sector| sector as i32),
            ];
            assert_eq!(
                header,
                source.words::<10>(),
                "scenario {scenario}, step {step}"
            );
            let label = format!("scenario {scenario}, step {step}");
            same(
                &format!("{label}: current"),
                &session.loaded.native_bytes(),
                source.take(save::PROFILE_BYTES),
            );
            same(
                &format!("{label}: last-written"),
                &session.last_written().native_bytes(),
                source.take(save::PROFILE_BYTES),
            );
            same(
                &format!("{label}: sector buffer"),
                session.sector_buffer(),
                source.take(save::PAYLOAD_BYTES),
            );
            same(
                &format!("{label}: all flash bytes"),
                session.flash_bytes(),
                source.take(16 * save::SECTOR_BYTES),
            );
        }
    }
    assert_eq!(source.position, data.len());
}
