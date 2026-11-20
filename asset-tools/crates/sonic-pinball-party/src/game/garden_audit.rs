//! Reproducible strict execution audit. A callback probe is never promoted to
//! a complete Garden, device, audio or canonical-service acceptance result.
use super::{
    frame::{FrameError, FrameMemory},
    garden_runtime::{RejectBackend, Runtime},
    memory::{MemoryImage, Region},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub system: &'static str,
    pub scope: &'static str,
    pub completed_calls: usize,
    pub requested_calls: usize,
    pub completed: bool,
    pub first_missing_address: Option<String>,
    pub first_failure_kind: Option<&'static str>,
    pub stopped: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub rom_sha256: &'static str,
    pub payload_sha256: &'static str,
    pub initial_state: &'static str,
    pub probes: Vec<Probe>,
    pub garden_end_to_end_validated: bool,
    pub audio_rendered: bool,
    pub physical_transfer_validated: bool,
    pub canonical_service_round_trip_validated: bool,
    pub whole_game_ported: bool,
}
fn memory<'a>(
    rom: &'a [u8],
    payload: &[u8],
) -> Result<MemoryImage<'a>, super::memory::MemoryError> {
    let mut ewram = vec![0; 0x40000];
    ewram[..payload.len()].copy_from_slice(payload);
    let mut regions = vec![Region {
        base: 0x02000000,
        bytes: ewram,
        writable: true,
    }];
    regions.extend(
        [
            (0x03000000, 0x8000),
            (0x04000000, 0x400),
            (0x05000000, 0x400),
            (0x06000000, 0x18000),
            (0x07000000, 0x400),
        ]
        .into_iter()
        .map(|(base, n)| Region {
            base,
            bytes: vec![0; n],
            writable: true,
        }),
    );
    MemoryImage::new(rom, regions)
}
/// Does not open a save, write a native file, drive an installed game or mutate
/// MMO state. Each probe gets a fresh explicit snapshot and strict backend.
pub fn run(rom: &[u8]) -> Result<Audit, Box<dyn std::error::Error>> {
    let payload = crate::catalog::extract_garden(rom)?.bytes;
    let mut probes = vec![];
    let paths: Vec<(&str, &str, Vec<u32>, bool)> = vec![
        (
            "boot",
            "original Thumb entry168 through first actual device dependency",
            vec![0x168],
            false,
        ),
        (
            "home and care",
            "home installation and 60 direct home callbacks; explicit present Chao",
            std::iter::once(0x18c8)
                .chain(std::iter::repeat_n(0x1d68, 60))
                .collect(),
            true,
        ),
        (
            "Chao Ball",
            "original graphics, full board setup and 180 direct task/frame callbacks",
            std::iter::once(0xd294)
                .chain(std::iter::repeat_n(0xd7cc, 180))
                .collect(),
            true,
        ),
        (
            "other native minigame",
            "original second minigame branch",
            vec![0xa8a4],
            true,
        ),
        (
            "save load",
            "original loader; strict actual flash boundary",
            vec![0x248],
            false,
        ),
        (
            "save write",
            "original writer; strict sound, wait and flash boundaries",
            vec![0x3ac],
            true,
        ),
        (
            "audio",
            "original audio initialization then sound main; no mocked mixer",
            vec![0x105e8, 0x10660],
            false,
        ),
        (
            "transfer",
            "original endpoint initialization and maintenance; no peer supplied",
            vec![0x480, 0x590],
            false,
        ),
    ];
    for (system, scope, entries, initialize_save) in paths {
        let mut m = memory(rom, &payload)?;
        let mut runtime = Runtime::new(&payload, RejectBackend)?;
        if initialize_save {
            runtime.call(&mut m, 0x43c, &[])?;
            m.write_u8(0x03004af8, 0)?;
            m.write_u8(0x03004abf, 88)?;
            m.write_u8(0x03004ac0, 104)?;
            m.write_bytes(0x03006194, &1000u32.to_le_bytes())?;
        }
        m.write_u16(0x04000130, 0x03ff)?;
        let mut done = 0;
        let mut stopped = String::new();
        let mut first_missing_address = None;
        let mut first_failure_kind = None;
        for entry in &entries {
            match runtime.call(&mut m, *entry, &[]) {
                Ok(_) => done += 1,
                Err(e) => {
                    let (kind, address) = match &e {
                        FrameError::Dependency { call, .. }
                        | FrameError::UnportedNativeCall(call) => {
                            ("native dependency", Some(call.entry_offset))
                        }
                        FrameError::MissingRead { address, .. } => ("memory read", Some(*address)),
                        FrameError::MissingWrite { address, .. } => {
                            ("memory write", Some(*address))
                        }
                        _ => ("invalid state", None),
                    };
                    first_failure_kind = Some(kind);
                    first_missing_address = address.map(|a| format!("0x{a:08X}"));
                    stopped = e.to_string();
                    break;
                }
            }
        }
        probes.push(Probe {
            system,
            scope,
            completed_calls: done,
            requested_calls: entries.len(),
            completed: done == entries.len(),
            first_missing_address,
            first_failure_kind,
            stopped,
        });
    }
    Ok(Audit{rom_sha256:crate::catalog::ROM_SHA256,payload_sha256:crate::garden::PAYLOAD_SHA256,
        initial_state:"independent explicit synthetic RAM snapshots and authenticated local payload; no loaded native save or canonical state",
        probes,garden_end_to_end_validated:false,audio_rendered:false,physical_transfer_validated:false,canonical_service_round_trip_validated:false,whole_game_ported:false})
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Requires local SPP_ROM; strict audit does not silently accept missing systems"]
    fn all_system_audit_reports_original_boot_boundary_and_never_claims_acceptance() {
        use std::io::Read;
        let mut rom = vec![];
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("set SPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        let report = super::run(&rom).unwrap();
        assert_eq!(report.probes.len(), 8);
        assert!(!report.probes[0].completed);
        assert_eq!(
            report.probes[0].first_missing_address.as_deref(),
            Some("0x02012028")
        );
        assert!(!report.garden_end_to_end_validated);
        assert!(!report.canonical_service_round_trip_validated);
        assert!(!report.physical_transfer_validated);
    }
}
