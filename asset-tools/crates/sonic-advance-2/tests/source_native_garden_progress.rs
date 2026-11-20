use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_garden_progress::{
        self as garden, BounceReturnState, GardenLoadServices, GardenSaveSession, SignedGameExit,
    },
};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    payload_sha256: String,
    routines: BTreeMap<String, Routine>,
    outputs: BTreeMap<String, Output>,
}
#[derive(Deserialize)]
struct Routine {
    offset: usize,
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    cases: usize,
    row_bytes: usize,
    bytes: usize,
    sha256: String,
}
fn metadata() -> Oracle {
    serde_json::from_str(include_str!(
        "../catalog/native-garden-progress-parity.json"
    ))
    .unwrap()
}
fn source(name: &str, expected: &Output) -> Vec<u8> {
    let root =
        std::env::var_os("A2NE_GARDEN_PROGRESS_ORACLE").expect("A2NE_GARDEN_PROGRESS_ORACLE");
    let bytes = std::fs::read(std::path::PathBuf::from(root).join(format!("{name}.bin"))).unwrap();
    assert_eq!(bytes.len(), expected.bytes);
    assert_eq!(bytes.len(), expected.cases * expected.row_bytes);
    assert_eq!(gba::sha256(&bytes), expected.sha256);
    bytes
}
fn words<const N: usize>(bytes: &[u8]) -> [u32; N] {
    assert_eq!(bytes.len(), N * 4);
    std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

#[test]
#[ignore = "A2NE_ROM required; independent actual-payload routine byte identities"]
fn tiny_progress_routine_bytes_match_verified_payload() {
    let m = metadata();
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let payload = rom.garden_bytes();
    assert_eq!(gba::sha256(payload), m.payload_sha256);
    for (name, r) in m.routines {
        assert_eq!(
            gba::sha256(&payload[r.offset..r.offset + r.bytes]),
            r.sha256,
            "{name}"
        );
    }
}

#[test]
#[ignore = "A2NE_GARDEN_PROGRESS_ORACLE required; actual0x8810/0x1554 Thumb with bounded BIOS Mod"]
fn tiny_initial_word_and_post_draw_seed_match_actual_payload() {
    let m = metadata();
    let data = source("initializer", &m.outputs["initializer"]);
    let mut values = std::collections::BTreeSet::new();
    for (row, bytes) in data.as_chunks::<12>().0.iter().enumerate() {
        let input = words::<3>(bytes);
        let services = GardenLoadServices::from_seed(137, input[0]);
        assert_eq!(
            [services.initial_word, services.rng_seed_before],
            [input[1], input[2]],
            "initializer row{row}"
        );
        assert_eq!(services.config_counter, 137);
        values.insert(services.initial_word);
    }
    assert_eq!(values.len(), 30);
    assert_eq!(values.first(), Some(&3600));
    assert_eq!(values.last(), Some(&5340));
}

#[test]
#[ignore = "A2NE_GARDEN_PROGRESS_ORACLE required; actual Thumb signed Memory credit and both return blocks"]
fn tiny_memory_credit_and_ready_return_match_actual_payload() {
    let m = metadata();
    let data = source("signed", &m.outputs["signed"]);
    let mut reward_bytes = std::collections::BTreeSet::new();
    for (row, bytes) in data.as_chunks::<12>().0.iter().enumerate() {
        let r = words::<3>(bytes);
        reward_bytes.insert(r[1]);
        assert_eq!(
            garden::credit_signed_byte(r[0], r[1] as u8),
            r[2],
            "credit row{row}"
        );
    }
    assert_eq!(reward_bytes.len(), 256);
    let data = source("memory-exit", &m.outputs["memory-exit"]);
    let mut modes = std::collections::BTreeSet::new();
    for (row, bytes) in data.as_chunks::<48>().0.iter().enumerate() {
        let r = words::<12>(bytes);
        modes.insert(r[0]);
        let mut state = SignedGameExit {
            active_byte: r[1] as u8,
            phase: r[2],
            main_callback: r[3],
        };
        let mut rings = r[4];
        let returned = garden::signed_game_exit_step(&mut state, &mut rings, r[5] as u8, r[6] != 0);
        assert_eq!(
            [
                u32::from(state.active_byte),
                state.phase,
                state.main_callback,
                rings
            ],
            [r[7], r[8], r[9], r[10]],
            "return row{row}"
        );
        assert_eq!(returned, r[6] != 0);
        assert_eq!(r[11], 1);
    }
    assert_eq!(modes.len(), 2);
}

#[test]
#[ignore = "A2NE_GARDEN_PROGRESS_ORACLE required; actual Thumb Bounce credit/return and native signed fade counter"]
fn tiny_bounce_credit_and_garden_restore_match_actual_payload() {
    let m = metadata();
    let data = source("bounce", &m.outputs["bounce"]);
    for (row, bytes) in data.as_chunks::<40>().0.iter().enumerate() {
        let r = words::<10>(bytes);
        let mut state = BounceReturnState {
            main_callback: garden::BOUNCE_RETURN_CALLBACK,
            completion_callback: r[3],
            fade_counter: r[2] as u16 as i16,
        };
        let mut rings = r[0];
        let step = garden::bounce_return_step(&mut state, &mut rings, r[1] as u16);
        assert_eq!(
            [
                rings,
                u32::from(state.fade_counter as u16),
                u32::from(step.brightness_register),
                state.main_callback,
                u32::from(step.restore_garden_tasks),
                3 + u32::from(step.restore_garden_tasks)
            ],
            [r[4], r[5], r[6], r[7], r[8], r[9]],
            "Bounce return row{row}"
        );
        assert_eq!(step.returned, state.fade_counter > 15);
        assert_eq!(step.reward_applied, step.restore_garden_tasks);
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let at = self.at;
        self.at += n;
        &self.data[at..at + n]
    }
    fn words<const N: usize>(&mut self) -> [u32; N] {
        words(self.take(N * 4))
    }
}
#[test]
#[ignore = "A2NE_GARDEN_PROGRESS_ORACLE required; exact native C load/reconcile and memory flash save lifecycle"]
fn tiny_garden_load_ring_reconciliation_and_sector_writes_match_original_c() {
    let m = metadata();
    let data = source("save", &m.outputs["save"]);
    let mut source = Cursor { data: &data, at: 0 };
    let mut failures = std::collections::BTreeSet::new();
    for scenario in 0..m.outputs["save"].cases {
        let h = source.words::<8>();
        assert_eq!(h[6], scenario as u32);
        let input = source.take(65536);
        let expected_load = source.words::<4>();
        let load_record = source.take(0x858);
        let expected_write = source.words::<11>();
        let write_record = source.take(0x858);
        let output_flash = source.take(65536);
        let mut session = GardenSaveSession::load(
            input,
            GardenLoadServices {
                config_counter: h[0],
                initial_word: h[1],
                rng_seed_before: h[2],
            },
        )
        .unwrap();
        assert_eq!(
            [
                u32::from(session.local_sector),
                session.version,
                session.rings,
                session.rng_seed
            ],
            expected_load,
            "load state{scenario}"
        );
        assert_eq!(session.record_bytes(), load_record, "load record{scenario}");
        assert_eq!(session.flash_bytes(), input, "load read-only{scenario}");
        assert_eq!(session.initialized, session.local_sector == 6);
        session.rings = h[3];
        session.rng_seed = h[4];
        let result = session.write_memory(h[5] as u8).unwrap();
        failures.insert(h[5]);
        let mut actual = vec![
            u32::from(session.local_sector),
            session.version,
            session.rings,
            session.rng_seed,
            result.attempted_physical_sectors.len() as u32,
            u32::from(result.succeeded),
        ];
        actual.extend((0..5).map(|i| {
            u32::from(
                result
                    .attempted_physical_sectors
                    .get(i)
                    .copied()
                    .unwrap_or(255),
            )
        }));
        assert_eq!(actual, expected_write, "write state{scenario}");
        assert_eq!(
            session.record_bytes(),
            write_record,
            "write record{scenario}"
        );
        assert_eq!(
            session.flash_bytes(),
            output_flash,
            "all flash bytes{scenario}"
        );
    }
    assert_eq!(source.at, data.len());
    assert_eq!(failures.len(), 64);
}
