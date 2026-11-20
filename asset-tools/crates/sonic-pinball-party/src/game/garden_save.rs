//! Native A3VE embedded-Garden save control, for isolated local compatibility.
//! Flash/timer/audio hardware must be supplied explicitly. These opaque native
//! records are not a canonical service snapshot or permission to upload gains.
use super::frame::{FrameError, FrameMemory};

pub const SAVE: u32 = 0x0300_4aa0;
pub const SECTOR_INDEX: u32 = 0x0300_0080;
pub const GENERATION: u32 = 0x0300_0084;
pub const RNG: u32 = 0x0300_13a8;
pub const RINGS: u32 = 0x0300_6194;
pub const PROGRAM_CREDITS: u32 = 0x0200_0008;
pub const RECORD_LENGTH: usize = crate::garden::SAVE_RECORD_LENGTH;

/// Only arguments explicitly prepared by the native callers are represented.
/// ReadFlash's returned R0 is not consumed; ProgramFlash's actual R0 controls
/// retries. The backend must mutate the same bounded memory supplied here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    IdentifyFlash,
    BindFlashTimer,
    ReadFlash { sector: u16 },
    ProgramFlash { sector: u16 },
    PauseSound,
    SoundMain,
    WaitVBlank,
    DisableSound,
    EnableSound,
}
impl Call {
    pub fn entry_offset(&self) -> u32 {
        match self {
            Self::IdentifyFlash => 0x12028,
            Self::BindFlashTimer => 0x11c10,
            Self::ReadFlash { .. } => 0x11da0,
            Self::ProgramFlash { .. } => 0x11fe0,
            Self::PauseSound => 0x107a0,
            Self::SoundMain => 0x10660,
            Self::WaitVBlank => 0x11b04,
            Self::DisableSound => 0x10c70,
            Self::EnableSound => 0x10cec,
        }
    }
    pub fn arguments(&self) -> Vec<u32> {
        match *self {
            Self::BindFlashTimer => vec![2, 0x0300_4a88],
            Self::ReadFlash { sector } => {
                vec![u32::from(sector), 0, SAVE, RECORD_LENGTH as u32]
            }
            Self::ProgramFlash { sector } => vec![u32::from(sector), SAVE, RECORD_LENGTH as u32],
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Memory(FrameError),
    Unported(Call),
    Backend { call: Call, reason: String },
    FlashSector(u16),
}
impl From<FrameError> for Error {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub trait Calls {
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error>;
}
pub struct RejectCalls;
impl Calls for RejectCalls {
    fn call(&mut self, _: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        Err(Error::Unported(call))
    }
}

/// Observations of the native loader, not a native return-register contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOutcome {
    pub selected_sector: Option<u16>,
    pub selected_generation: u32,
    pub flash_reads: u8,
}
/// A failed hardware status remains visible after all five native attempts.
/// An outcome is returned only when every delegated hardware call completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOutcome {
    pub attempts: u8,
    pub last_flash_result: u32,
}

fn word(memory: &mut dyn FrameMemory, address: u32, value: u32) -> Result<(), Error> {
    memory.write_u16(address, value as u16)?;
    memory.write_u16(address + 2, (value >> 16) as u16)?;
    Ok(())
}
fn record_bytes(memory: &dyn FrameMemory) -> Result<Vec<u8>, Error> {
    (0..RECORD_LENGTH)
        .map(|i| Ok(memory.read_u8(SAVE + i as u32)?))
        .collect()
}
fn read_sector(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    sector: u16,
) -> Result<(), Error> {
    if !(10..=15).contains(&sector) {
        return Err(Error::FlashSector(sector));
    }
    calls.call(memory, Call::ReadFlash { sector })?;
    Ok(())
}

/// Payload043C plus its exact small random-delay dependency08810/01554.
/// Only save bytes08..57,58..69,10..13 and1C are changed; the opaque tail,
/// checksum/header, generation and other native fields are retained verbatim.
pub fn initialize(memory: &mut dyn FrameMemory) -> Result<(), Error> {
    for offset in 8..0x58 {
        memory.write_u8(SAVE + offset, 0)?;
    }
    for offset in (0x58..=0x69).rev() {
        memory.write_u8(SAVE + offset, 0xff)?;
    }
    let seed = memory
        .read_u32(RNG)?
        .wrapping_mul(0x41c6_4e6d)
        .wrapping_add(12345);
    word(memory, RNG, seed)?;
    let random = (seed >> 16) & 0x7fff;
    word(memory, SAVE + 0x10, (random % 30 + 60) * 60)?;
    memory.write_u8(SAVE + 0x1c, 0xff)?;
    Ok(())
}

/// Payload0248. Each scan read overwrites the live record just as native code
/// does; after selection the winner is read again. No validity recheck, flash
/// read status interpretation, generation wrap ordering or tail reset is added.
pub fn load(memory: &mut dyn FrameMemory, calls: &mut dyn Calls) -> Result<LoadOutcome, Error> {
    calls.call(memory, Call::IdentifyFlash)?;
    calls.call(memory, Call::BindFlashTimer)?;
    let mut selected: Option<u16> = None;
    let mut generation = 0;
    for sector in 10..=15 {
        read_sector(memory, calls, sector)?;
        if memory.read_u16(SAVE + 2)? != crate::garden::SAVE_MARKER {
            continue;
        }
        let bytes = record_bytes(memory)?;
        if crate::garden::crc16_x25(&bytes[2..]) != memory.read_u16(SAVE)? {
            continue;
        }
        let current = memory.read_u32(SAVE + 4)?;
        if selected.is_none() || current > generation {
            selected = Some(sector);
            generation = current;
        }
    }
    if let Some(sector) = selected {
        memory.write_u8(SECTOR_INDEX, (sector - 10) as u8)?;
        word(memory, GENERATION, generation)?;
        read_sector(memory, calls, sector)?;
        let saved_rng = memory.read_u32(SAVE + 0x18)?;
        word(memory, RNG, saved_rng)?;
    } else {
        memory.write_u8(SECTOR_INDEX, 6)?;
        word(memory, GENERATION, 0)?;
        initialize(memory)?;
        memory.write_u8(SAVE + 0x59, 0)?;
        memory.write_u8(SAVE + 0x1f, 0x58)?;
        memory.write_u8(SAVE + 0x20, 0x68)?;
    }
    let saved_credits = memory.read_u32(SAVE + 8)?;
    let program_credits = memory.read_u32(PROGRAM_CREDITS)?;
    let saved_rings = memory.read_u32(SAVE + 0xc)?;
    if saved_credits >= program_credits {
        word(memory, RINGS, saved_rings)?;
    } else {
        // Native addition wraps before the unsigned cap comparison.
        let value = saved_rings
            .wrapping_add(program_credits.wrapping_sub(saved_credits))
            .min(99999);
        word(memory, RINGS, value)?;
        word(memory, SAVE + 8, program_credits)?;
    }
    let rings = memory.read_u32(RINGS)?;
    word(memory, SAVE + 0x64, rings.swap_bytes())?;
    memory.write_u8(SAVE + 0x68, 0)?;
    Ok(LoadOutcome {
        selected_sector: selected,
        selected_generation: generation,
        flash_reads: if selected.is_some() { 7 } else { 6 },
    })
}

/// Payload03AC. Header/RNG/rings and checksum are updated before sound and
/// flash calls. Each nonzero ProgramFlash result advances to the next sector;
/// zero ends retries, and five nonzero results exhaust the native loop. All
/// delegated errors retain already performed source-order RAM/backend effects.
pub fn write(memory: &mut dyn FrameMemory, calls: &mut dyn Calls) -> Result<WriteOutcome, Error> {
    memory.write_u16(SAVE + 2, crate::garden::SAVE_MARKER)?;
    let generation = memory.read_u32(GENERATION)?.wrapping_add(1);
    word(memory, GENERATION, generation)?;
    word(memory, SAVE + 4, generation)?;
    let rings = memory.read_u32(RINGS)?;
    word(memory, SAVE + 0xc, rings)?;
    let rng = memory.read_u32(RNG)?;
    word(memory, SAVE + 0x18, rng)?;
    let bytes = record_bytes(memory)?;
    memory.write_u16(SAVE, crate::garden::crc16_x25(&bytes[2..]))?;
    for call in [
        Call::PauseSound,
        Call::SoundMain,
        Call::WaitVBlank,
        Call::DisableSound,
    ] {
        calls.call(memory, call)?;
    }
    let mut result = WriteOutcome {
        attempts: 0,
        last_flash_result: 0,
    };
    for _ in 0..5 {
        let next = memory.read_u8(SECTOR_INDEX)?.wrapping_add(1);
        memory.write_u8(SECTOR_INDEX, next)?;
        let next = if next > 5 {
            memory.write_u8(SECTOR_INDEX, 0)?;
            0
        } else {
            next
        };
        let sector = u16::from(next) + 10;
        result.last_flash_result = calls.call(memory, Call::ProgramFlash { sector })?;
        result.attempts += 1;
        if result.last_flash_result == 0 {
            break;
        }
    }
    calls.call(memory, Call::EnableSound)?;
    calls.call(memory, Call::WaitVBlank)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn number(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn field(value: &Value, key: &str, default: u32) -> u32 {
        value.get(key).map_or(default, number)
    }
    fn pattern(seed: u32, size: usize) -> Vec<u8> {
        (0..size)
            .map(|i| seed.wrapping_add(i as u32 * 37) as u8)
            .collect()
    }
    fn record(spec: &Value) -> Vec<u8> {
        let mut bytes = pattern(field(spec, "seed", 19), RECORD_LENGTH);
        bytes[2..4].copy_from_slice(&crate::garden::SAVE_MARKER.to_le_bytes());
        for (key, offset) in [
            ("generation", 4),
            ("credits", 8),
            ("rings", 12),
            ("rng", 24),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&field(spec, key, 0).to_le_bytes());
        }
        let crc = crate::garden::crc16_x25(&bytes[2..]);
        bytes[..2].copy_from_slice(&crc.to_le_bytes());
        match spec.get("invalid").and_then(Value::as_str) {
            Some("marker") => bytes[2] ^= 1,
            Some("checksum") => bytes[RECORD_LENGTH - 1] ^= 0x80,
            _ => {}
        }
        bytes
    }

    struct Ram {
        bytes: Vec<u8>,
        program_credits: u32,
    }
    impl Ram {
        fn new(case: &Value, seed: u32) -> Self {
            let mut value = Self {
                bytes: pattern(seed, 0x8000),
                program_credits: number(&case["programCredits"]),
            };
            let initial = &case["initial"];
            value
                .write_bytes(SAVE, &record(&initial["record"]))
                .unwrap();
            value
                .write_u8(SECTOR_INDEX, number(&initial["sectorIndex"]) as u8)
                .unwrap();
            for (key, address) in [
                ("generation", GENERATION),
                ("rng", RNG),
                ("workingRings", RINGS),
            ] {
                value
                    .write_bytes(address, &number(&initial[key]).to_le_bytes())
                    .unwrap();
            }
            value
        }
        fn snapshot(&self) -> String {
            crate::sha256(&self.bytes[..0x7400])
        }
    }
    impl FrameMemory for Ram {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            if let Some(offset) = address
                .checked_sub(PROGRAM_CREDITS)
                .filter(|&offset| offset < 4)
            {
                return Ok(self.program_credits.to_le_bytes()[offset as usize]);
            }
            address
                .checked_sub(0x0300_0000)
                .and_then(|offset| self.bytes.get(offset as usize))
                .copied()
                .ok_or(FrameError::MissingRead { address, width: 1 })
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes([
                self.read_u8(address)?,
                self.read_u8(address + 1)?,
            ]))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes([
                self.read_u8(address)?,
                self.read_u8(address + 1)?,
                self.read_u8(address + 2)?,
                self.read_u8(address + 3)?,
            ]))
        }
        fn write_u8(&mut self, address: u32, byte: u8) -> Result<(), FrameError> {
            self.write_bytes(address, &[byte])
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.write_bytes(address, &value.to_le_bytes())
        }
        fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), FrameError> {
            let offset = address
                .checked_sub(0x0300_0000)
                .map(|value| value as usize)
                .ok_or(FrameError::MissingWrite { address, width: 1 })?;
            let end = offset
                .checked_add(bytes.len())
                .filter(|&end| end <= self.bytes.len())
                .ok_or(FrameError::MissingWrite { address, width: 1 })?;
            self.bytes[offset..end].copy_from_slice(bytes);
            Ok(())
        }
    }

    /// Explicit in-memory test hardware only. Neither this backend nor its
    /// torn-write model is exported as a real flash or durable-service adapter.
    struct SyntheticCalls {
        case: Value,
        flash: Vec<u8>,
        calls: Vec<Value>,
        reads: u32,
        writes: usize,
    }
    impl SyntheticCalls {
        fn new(case: &Value) -> Self {
            let mut flash = vec![255; crate::garden::FLASH_LENGTH];
            if let Some(records) = case.get("flashRecords").and_then(Value::as_array) {
                for spec in records {
                    let offset = (10 + number(&spec["slot"]) as usize) * 0x1000;
                    flash[offset..offset + RECORD_LENGTH].copy_from_slice(&record(spec));
                }
            }
            Self {
                case: case.clone(),
                flash,
                calls: vec![],
                reads: 0,
                writes: 0,
            }
        }
        fn interrupted(&mut self, call: Call, mut trace: Value) -> Result<u32, Error> {
            trace["aborted"] = json!(true);
            self.calls.push(trace);
            Err(Error::Backend {
                call,
                reason: "synthetic hardware interruption".into(),
            })
        }
    }
    impl Calls for SyntheticCalls {
        fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
            let mut trace = json!({"entry":call.entry_offset(),"args":call.arguments()});
            let ordinal = self.calls.len() as u32 + 1;
            if let Some(mutations) = self.case.get("mutations").and_then(Value::as_array) {
                for mutation in mutations {
                    if number(&mutation["call"]) == ordinal {
                        memory.write_bytes(
                            number(&mutation["address"]),
                            &number(&mutation["value"]).to_le_bytes()
                                [..number(&mutation["width"]) as usize],
                        )?;
                    }
                }
            }
            if self
                .case
                .get("abortAtCall")
                .is_some_and(|value| number(value) == ordinal)
            {
                return self.interrupted(call, trace);
            }
            let result = match call {
                Call::IdentifyFlash => field(&self.case, "identifyResult", 0),
                Call::BindFlashTimer => field(&self.case, "timerResult", 0),
                Call::ReadFlash { sector } => {
                    assert!((10..=15).contains(&sector));
                    self.reads += 1;
                    let abort = self.case.get("abortRead");
                    let interrupted =
                        abort.is_some_and(|abort| number(&abort["ordinal"]) == self.reads);
                    let prefix = if interrupted {
                        number(&abort.unwrap()["prefix"]) as usize
                    } else {
                        RECORD_LENGTH
                    };
                    assert!(prefix <= RECORD_LENGTH);
                    let offset = usize::from(sector) * 0x1000;
                    memory.write_bytes(SAVE, &self.flash[offset..offset + prefix])?;
                    if interrupted {
                        return self.interrupted(call, trace);
                    }
                    field(&self.case, "readResult", 0)
                }
                Call::ProgramFlash { sector } => {
                    assert!((10..=15).contains(&sector));
                    let plans = self.case.get("writePlans").and_then(Value::as_array);
                    let default = json!({"result":0,"prefix":RECORD_LENGTH,"erase":true});
                    let plan = plans
                        .and_then(|plans| plans.get(self.writes))
                        .unwrap_or(&default)
                        .clone();
                    self.writes += 1;
                    let bytes = record_bytes(memory)?;
                    trace["recordSha256"] = json!(crate::sha256(&bytes));
                    let offset = usize::from(sector) * 0x1000;
                    if plan["erase"].as_bool().unwrap_or(false) {
                        self.flash[offset..offset + 0x1000].fill(255);
                    }
                    let prefix = number(&plan["prefix"]) as usize;
                    assert!(prefix <= RECORD_LENGTH);
                    self.flash[offset..offset + prefix].copy_from_slice(&bytes[..prefix]);
                    if plan["abort"].as_bool().unwrap_or(false) {
                        return self.interrupted(call, trace);
                    }
                    number(&plan["result"])
                }
                _ => 0,
            };
            trace["result"] = json!(result);
            self.calls.push(trace);
            Ok(result)
        }
    }

    fn fixture() -> Value {
        serde_json::from_str(vectors!("garden-save-vectors.json")).unwrap()
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_synthetic_flash_vectors_match_full_ram_flash_and_calls() {
        let data = fixture();
        assert_eq!(data["actualFlashHardwareValidated"], false);
        assert_eq!(data["canonicalServiceValidated"], false);
        for case in data["cases"].as_array().unwrap() {
            let mut ram = Ram::new(case, number(&case["ramSeed"]));
            let mut calls = SyntheticCalls::new(case);
            for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
                if let Some(seed) = step.get("resetRamSeed") {
                    ram = Ram::new(case, number(seed));
                }
                let result = match step["routine"].as_str().unwrap() {
                    "load" => load(&mut ram, &mut calls).map(|_| ()),
                    "write" => write(&mut ram, &mut calls).map(|_| ()),
                    "initialize" => initialize(&mut ram),
                    _ => panic!("unknown fixture routine"),
                };
                let expected = &case["outcomes"][index];
                assert_eq!(
                    result.is_err(),
                    expected["aborted"].as_bool().unwrap(),
                    "{} step{index}: {result:?}",
                    case["name"]
                );
                if let Err(error) = result {
                    assert!(
                        matches!(error, Error::Backend { .. }),
                        "{}: {error:?}",
                        case["name"]
                    );
                }
                assert_eq!(
                    ram.snapshot(),
                    expected["ramSha256"].as_str().unwrap(),
                    "{} step{index} RAM",
                    case["name"]
                );
                assert_eq!(
                    crate::sha256(&calls.flash),
                    expected["flashSha256"].as_str().unwrap(),
                    "{} step{index} flash",
                    case["name"]
                );
            }
            assert_eq!(
                calls.calls,
                *case["calls"].as_array().unwrap(),
                "{} calls",
                case["name"]
            );
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn unbound_hardware_rejects_and_preserves_native_prefix_mutations() {
        let data = fixture();
        let case = &data["cases"][0];
        let mut ram = Ram::new(case, number(&case["ramSeed"]));
        let before = ram.snapshot();
        assert_eq!(
            load(&mut ram, &mut RejectCalls),
            Err(Error::Unported(Call::IdentifyFlash))
        );
        assert_eq!(ram.snapshot(), before);
        assert_eq!(
            write(&mut ram, &mut RejectCalls),
            Err(Error::Unported(Call::PauseSound))
        );
        let bytes = record_bytes(&ram).unwrap();
        assert!(crate::garden::read_record(&bytes).is_ok());
        assert_eq!(
            ram.read_u32(SAVE + 0xc).unwrap(),
            ram.read_u32(RINGS).unwrap()
        );
        assert_ne!(ram.snapshot(), before);
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn truncated_record_stops_before_hardware_write() {
        let data = fixture();
        let case = &data["cases"][0];
        let mut ram = Ram::new(case, number(&case["ramSeed"]));
        ram.bytes
            .truncate((SAVE - 0x0300_0000) as usize + RECORD_LENGTH - 1);
        let mut calls = SyntheticCalls::new(case);
        assert!(matches!(
            write(&mut ram, &mut calls),
            Err(Error::Memory(FrameError::MissingRead { .. }))
        ));
        assert!(calls.calls.is_empty());
        assert!(calls.flash.iter().all(|&byte| byte == 255));
    }
}
