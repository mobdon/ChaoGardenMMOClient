//! A3VE intro bootstrap/resume/update source wrappers (0x6ef4/0x7840/0x8168).
//!
//! Native scalar setup and all 39 interpolation contexts are translated. The
//! remaining direct/indirect calls are strict typed dependencies, never screen
//! substitutes. Resource loading implements only the verified native type10
//! path through the existing bounded Rust LZ77 decoder; authentic BIOS execution
//! and end-to-end playable intro acceptance remain separate evidence.

use super::interpolation::{ConstructArgs, InterpolationState};
use serde::Serialize;
use std::fmt;

pub const INITIALIZE_OFFSET: usize = 0x6ef4;
pub const INITIALIZE_LENGTH: usize = 0x94c;
pub const INITIALIZE_SHA256: &str =
    "a0db739241c1446a1c9338bcb53afab17a23dcd496bc565466f1d8c591461793";
pub const RESUME_OFFSET: usize = 0x7840;
pub const RESUME_LENGTH: usize = 0x928;
pub const RESUME_SHA256: &str = "43a942924d6eced438fab983091afc289760a7332724e08987e4ebda35fe5e09";
pub const UPDATE_OFFSET: usize = 0x8168;
pub const UPDATE_LENGTH: usize = 0x80;
pub const UPDATE_SHA256: &str = "69a5af14443cc33f536dbcaf5c5dd259484a8679d62ad0ef6fc34fe2cd841296";
pub const CONTEXT_BASE: u32 = 0x0200_1ca8;
pub const CONTEXT_COUNT: usize = 39;
/// Source constructor calls16..20 populate slots34..38; later calls21..38
/// populate slots16..33. This copy permutation is shared by both variants.
pub const CONTEXT_DESTINATION_INDICES: [usize; CONTEXT_COUNT] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 34, 35, 36, 37, 38, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
];
pub const DISPLAY_SHADOW: u32 = 0x0300_0770;
pub const MAX_RESOURCE_BYTES: usize = 0x20000;
/// The native VRAM address window includes 0x18000..0x1ffff accesses. Their
/// mode-dependent backing/write semantics belong to the memory backend. This
/// address bound is independent of the decode budget.
pub const VRAM_WINDOW_BYTES: usize = 0x20000;
const SKIP_STATE: u32 = 0x0200_2190;
const AUXILIARY_STACK: u32 = 0x0300_2060;

/// Each native row is endX/endY/descriptor/flags/speed. StartX/startY are
/// exactly120/80 for every constructor call in both bootstrap variants.
pub const CONTEXT_SETUP_ROWS: [[u32; 5]; CONTEXT_COUNT] = [
    [65472, 65472, 0x086e7244, 0x22, 2],
    [240, 65504, 0x086e724c, 0x42, 2],
    [8, 160, 0x086e7254, 0x62, 2],
    [240, 160, 0x086e725c, 0x80, 2],
    [65472, 105, 0x086e7264, 0xa0, 3],
    [80, 65472, 0x086e726c, 0xc0, 2],
    [104, 160, 0x086e7274, 0xe0, 2],
    [240, 72, 0x086e727c, 0x100, 3],
    [65504, 25, 0x086e7284, 0x120, 3],
    [148, 160, 0x086e728c, 0x140, 2],
    [160, 65504, 0x086e7294, 0x160, 2],
    [64, 65504, 0x086e729c, 0x181, 2],
    [190, 65504, 0x086e72d4, 0x1a1, 2],
    [65504, 40, 0x086e730c, 0x1c1, 2],
    [240, 104, 0x086e729c, 0x1e1, 2],
    [65478, 160, 0x086e729c, 0x1e1, 2],
    [64, 65504, 0x086e72cc, 0x201, 3],
    [190, 65504, 0x086e7304, 0x201, 4],
    [65504, 40, 0x086e733c, 0x201, 3],
    [240, 104, 0x086e7304, 0x201, 4],
    [65478, 160, 0x086e72cc, 0x201, 3],
    [48, 65504, 0x086e72b4, 0x201, 1],
    [96, 65504, 0x086e72ec, 0x201, 1],
    [144, 65504, 0x086e72a4, 0x201, 2],
    [192, 65504, 0x086e72ec, 0x201, 2],
    [240, 65504, 0x086e72a4, 0x201, 1],
    [48, 240, 0x086e72b4, 0x201, 1],
    [96, 240, 0x086e72ec, 0x201, 2],
    [144, 240, 0x086e72a4, 0x201, 2],
    [192, 240, 0x086e72ec, 0x201, 2],
    [240, 240, 0x086e72a4, 0x201, 1],
    [65504, 40, 0x086e72ec, 0x201, 1],
    [65504, 80, 0x086e72ec, 0x201, 2],
    [65504, 120, 0x086e72ec, 0x201, 2],
    [65504, 160, 0x086e72ec, 0x201, 1],
    [160, 40, 0x086e72ec, 0x201, 1],
    [160, 80, 0x086e72ec, 0x201, 2],
    [160, 120, 0x086e72ec, 0x201, 2],
    [160, 160, 0x086e72ec, 0x201, 1],
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum IntroCall {
    Direct {
        #[serde(rename = "entryOffset")]
        entry_offset: usize,
        arguments: Vec<u32>,
    },
    Callback {
        #[serde(rename = "callbackId")]
        callback_id: u32,
        arguments: [u32; 1],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntroError {
    Memory { address: u32, length: usize },
    UnportedCall(IntroCall),
    WrongRom,
    Resource(crate::binary::Error),
    ResourceLimit(usize),
    InvalidResourceDestination(u32),
    AuxiliaryDepth(u16),
    MissingConstructorStackPrefix,
}
impl fmt::Display for IntroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IntroError {}
impl From<crate::binary::Error> for IntroError {
    fn from(value: crate::binary::Error) -> Self {
        Self::Resource(value)
    }
}

/// A production backend must preserve GBA mode-dependent VRAM backing/write
/// semantics in the native 0x06000000..0x0601ffff address window. An oracle's
/// linear window proves requested transactions, not hardware or PPU behavior.
pub trait IntroMemory {
    fn read_halfword(&self, address: u32) -> Result<u16, IntroError>;
    fn read_word(&self, address: u32) -> Result<u32, IntroError>;
    fn write_halfword(&mut self, address: u32, value: u16) -> Result<(), IntroError>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), IntroError>;
    fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), IntroError>;
}

/// A dependency implementation must call a completed Rust port or stop. It may
/// mutate mapped memory, as a real native dependency can. Return R0 is not used
/// by the external calls represented here; the native3628 result is handled by
/// IntroResources instead.
pub trait IntroCalls {
    fn invoke(&mut self, call: IntroCall, memory: &mut dyn IntroMemory) -> Result<(), IntroError>;
}
#[derive(Debug, Default)]
pub struct RejectUnportedIntroCalls;
impl IntroCalls for RejectUnportedIntroCalls {
    fn invoke(&mut self, call: IntroCall, _memory: &mut dyn IntroMemory) -> Result<(), IntroError> {
        Err(IntroError::UnportedCall(call))
    }
}

pub trait IntroResources {
    fn background_destination(&self, mode: u8) -> Result<u32, IntroError>;
    fn decode_lz77(&mut self, source_address: u32) -> Result<Vec<u8>, IntroError>;
}

/// Authenticated local-only reader; no resource bytes are embedded in source.
#[derive(Clone)]
pub struct IntroRom<'a> {
    rom: &'a [u8],
}
impl<'a> IntroRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, IntroError> {
        crate::catalog::validate_rom(rom).map_err(|_| IntroError::WrongRom)?;
        Ok(Self { rom })
    }
}
impl IntroResources for IntroRom<'_> {
    fn background_destination(&self, mode: u8) -> Result<u32, IntroError> {
        // Native3628 masks the low three bits after truncating its argument.
        Ok(crate::binary::u32_le(
            self.rom,
            0x66c54 + usize::from(mode & 7) * 4,
        )?)
    }
    fn decode_lz77(&mut self, source_address: u32) -> Result<Vec<u8>, IntroError> {
        let offset = crate::binary::pointer_offset(source_address, 0x0800_0000, self.rom.len())?;
        let bytes = &self.rom[offset..];
        if bytes[0] != 0x10 {
            return Err(crate::binary::Error::InvalidTag(bytes[0]).into());
        }
        Ok(crate::binary::decode(bytes, MAX_RESOURCE_BYTES)?.bytes)
    }
}

fn invoke(
    calls: &mut dyn IntroCalls,
    memory: &mut dyn IntroMemory,
    entry_offset: usize,
    arguments: &[u32],
) -> Result<(), IntroError> {
    calls.invoke(
        IntroCall::Direct {
            entry_offset,
            arguments: arguments.to_vec(),
        },
        memory,
    )
}

fn copy_resource(
    memory: &mut dyn IntroMemory,
    resources: &mut dyn IntroResources,
    source: u32,
    destination: u32,
) -> Result<(), IntroError> {
    let decoded = resources.decode_lz77(source)?;
    if decoded.len() > MAX_RESOURCE_BYTES {
        return Err(IntroError::ResourceLimit(decoded.len()));
    }
    let within_vram = destination
        .checked_sub(0x0600_0000)
        .and_then(|offset| (offset as usize).checked_add(decoded.len()))
        .is_some_and(|end| end <= VRAM_WINDOW_BYTES);
    if !within_vram {
        return Err(IntroError::InvalidResourceDestination(destination));
    }
    memory.write_bytes(destination, &decoded)
}

fn bootstrap(
    initial: bool,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn IntroResources,
    calls: &mut dyn IntroCalls,
    constructor_stack_prefix: Option<u32>,
) -> Result<(), IntroError> {
    memory.write_word(0x0300_40b0, 0x0300_2980)?;
    if initial {
        invoke(calls, memory, 0x3250, &[])?;
        invoke(calls, memory, 0x5f0bc, &[])?;
    }
    memory.write_word(SKIP_STATE, 0)?;
    let context = memory.read_word(0x0200_1ca0)?;
    memory.write_word(context, 0)?;
    if initial {
        for target in [0x0884, 0x5f040, 0x3278, 0x3010] {
            invoke(calls, memory, target, &[])?;
        }
    }
    let display = memory.read_halfword(DISPLAY_SHADOW)?;
    memory.write_halfword(DISPLAY_SHADOW, display & 0xe0ff)?;
    let display = memory.read_halfword(DISPLAY_SHADOW)?;
    memory.write_halfword(DISPLAY_SHADOW, display | 0x0300)?;
    memory.write_halfword(DISPLAY_SHADOW + 0x4a, 0x0241)?;
    memory.write_halfword(DISPLAY_SHADOW + 0x4c, 0x0f00)?;
    memory.write_halfword(DISPLAY_SHADOW + 4, 0x1f00)?;
    memory.write_halfword(DISPLAY_SHADOW + 6, 0x1e09)?;
    for offset in [0x14, 0x16, 0x18, 0x1a, 0x0c, 0x0e, 0x10, 0x12] {
        memory.write_halfword(DISPLAY_SHADOW + offset, 0)?;
    }
    invoke(calls, memory, 0x30fc, &[])?;
    memory.write_halfword(0x0200_1ca4, 0)?;
    let first = 0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 4)? & 0x0c) << 12);
    copy_resource(memory, resources, 0x082d_453c, first)?;
    let second = 0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 4)? & 0x1f00) << 3);
    copy_resource(memory, resources, 0x082d_5468, second)?;
    let third = 0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 6)? & 0x0c) << 12);
    copy_resource(memory, resources, 0x082e_1be0, third)?;
    let fourth = 0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 6)? & 0x1f00) << 3);
    copy_resource(memory, resources, 0x082e_2d88, fourth)?;
    let fifth =
        resources.background_destination((memory.read_halfword(DISPLAY_SHADOW)? & 7) as u8)?;
    copy_resource(memory, resources, 0x082e_3480, fifth)?;
    memory.write_halfword(0x0200_218c, 0)?;
    memory.write_halfword(0x0200_218e, 0)?;
    let constructor_stack_prefix =
        constructor_stack_prefix.ok_or(IntroError::MissingConstructorStackPrefix)?;
    for (index, row) in CONTEXT_SETUP_ROWS.iter().enumerate() {
        let state = InterpolationState::construct(
            ConstructArgs {
                start_x: 120,
                start_y: 80,
                end_x: row[0],
                end_y: row[1],
                descriptor_address: row[2],
                flags: row[3],
                speed: row[4],
            },
            constructor_stack_prefix,
        );
        memory.write_bytes(
            CONTEXT_BASE + CONTEXT_DESTINATION_INDICES[index] as u32 * 32,
            state.as_bytes(),
        )?;
    }
    invoke(calls, memory, 0x5618, &[0x0300_00f0])?;
    invoke(calls, memory, 0x5784, &[0, 32, 0])?;
    invoke(calls, memory, 0x5724, &[0x082e_3280, 0, 256])?;
    invoke(calls, memory, 0x5724, &[0x082e_5868, 256, 256])?;
    invoke(calls, memory, 0x5668, &[])?;
    invoke(calls, memory, 0x1dc4, &[0])?;
    invoke(calls, memory, 0x2560, &[])?;
    invoke(calls, memory, 0x48dcc, &[])?;
    if initial {
        invoke(calls, memory, 0x3250, &[])?;
        invoke(calls, memory, 0x5f0bc, &[])?;
    }
    invoke(calls, memory, 0x37d4, &[0, 0])?;
    invoke(calls, memory, 0x2f68, &[])?;
    for callback in [
        0x0800_6e41,
        0x0800_67dd,
        0x0800_66f9,
        0x0800_6eb5,
        0x0800_6df1,
        0x0800_67a1,
        0x0800_66f9,
        0x0800_6765,
    ] {
        invoke(calls, memory, 0x2f74, &[callback, 0])?;
    }
    Ok(())
}

/// Original incoming R0 is ignored by both initializer routines. Context is
/// reloaded from the actual global pointer written by the startup wrapper.
/// An unavailable native stack-prefix word stops explicitly at construction;
/// None never silently becomes zero.
pub fn initialize(
    memory: &mut dyn IntroMemory,
    resources: &mut dyn IntroResources,
    calls: &mut dyn IntroCalls,
    constructor_stack_prefix: Option<u32>,
) -> Result<(), IntroError> {
    bootstrap(true, memory, resources, calls, constructor_stack_prefix)
}
pub fn resume(
    memory: &mut dyn IntroMemory,
    resources: &mut dyn IntroResources,
    calls: &mut dyn IntroCalls,
    constructor_stack_prefix: Option<u32>,
) -> Result<(), IntroError> {
    bootstrap(false, memory, resources, calls, constructor_stack_prefix)
}

/// Exact8168 control wrapper. The auxiliary callback remains a strict native-ID
/// dependency; no unported callback is treated as an idle/completed screen.
pub fn update(
    context_argument: u32,
    memory: &mut dyn IntroMemory,
    calls: &mut dyn IntroCalls,
) -> Result<(), IntroError> {
    invoke(calls, memory, 0x1dc4, &[0])?;
    invoke(calls, memory, 0x2580, &[])?;
    if memory.read_word(SKIP_STATE)? == 1 && memory.read_halfword(0x0300_0514)? != 0 {
        let display = memory.read_halfword(DISPLAY_SHADOW)?;
        memory.write_halfword(DISPLAY_SHADOW, display & 0xe0ff)?;
        let display = memory.read_halfword(DISPLAY_SHADOW)?;
        memory.write_halfword(DISPLAY_SHADOW, display)?;
        memory.write_halfword(0x0200_1ca4, 99)?;
        invoke(calls, memory, 0x5784, &[0, 32, 64])?;
    }
    let count = memory.read_halfword(AUXILIARY_STACK)?;
    if count > 10 {
        return Err(IntroError::AuxiliaryDepth(count));
    }
    if count != 0 {
        let callback = memory.read_word(AUXILIARY_STACK + 4 + u32::from(count - 1) * 4)?;
        calls.invoke(
            IntroCall::Callback {
                callback_id: callback,
                arguments: [context_argument],
            },
            memory,
        )?;
    }
    invoke(calls, memory, 0x5668, &[])?;
    invoke(calls, memory, 0x2700, &[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn pattern(seed: u32, length: usize) -> Vec<u8> {
        (0..length / 4)
            .flat_map(|index| {
                seed.wrapping_add((index as u32).wrapping_mul(0x0102_0304))
                    .to_le_bytes()
            })
            .collect()
    }

    struct Memory {
        ewram: Vec<u8>,
        iwram: Vec<u8>,
        vram: Vec<u8>,
    }
    impl Memory {
        fn new(seed: u32) -> Self {
            let mut result = Self {
                ewram: pattern(seed, 0x40000),
                iwram: pattern(seed ^ 0xa55a_1234, 0x8000),
                vram: pattern(seed ^ 0x7ace_1200, 0x20000),
            };
            result.write_word(0x0200_1ca0, 0x0201_99a0).unwrap();
            result
        }
        fn span(&self, address: u32, length: usize) -> Result<&[u8], IntroError> {
            for (base, region) in [
                (0x0200_0000, &self.ewram),
                (0x0300_0000, &self.iwram),
                (0x0600_0000, &self.vram),
            ] {
                if let Some(start) = address.checked_sub(base).map(|offset| offset as usize) {
                    if let Some(end) = start.checked_add(length) {
                        if let Some(bytes) = region.get(start..end) {
                            return Ok(bytes);
                        }
                    }
                }
            }
            Err(IntroError::Memory { address, length })
        }
        fn assert_snapshot(&self, case: &Value) {
            assert_eq!(
                crate::sha256(&self.ewram),
                case["ewramSha256"].as_str().unwrap()
            );
            assert_eq!(
                crate::sha256(&self.iwram[..0x7800]),
                case["iwramWithoutStackSha256"].as_str().unwrap()
            );
            assert_eq!(
                crate::sha256(&self.vram),
                case["vramSha256"].as_str().unwrap()
            );
        }
    }
    impl IntroMemory for Memory {
        fn read_halfword(&self, address: u32) -> Result<u16, IntroError> {
            Ok(u16::from_le_bytes(
                self.span(address, 2)?.try_into().unwrap(),
            ))
        }
        fn read_word(&self, address: u32) -> Result<u32, IntroError> {
            Ok(u32::from_le_bytes(
                self.span(address, 4)?.try_into().unwrap(),
            ))
        }
        fn write_halfword(&mut self, address: u32, value: u16) -> Result<(), IntroError> {
            self.write_bytes(address, &value.to_le_bytes())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), IntroError> {
            self.write_bytes(address, &value.to_le_bytes())
        }
        fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), IntroError> {
            for (base, region) in [
                (0x0200_0000, &mut self.ewram),
                (0x0300_0000, &mut self.iwram),
                (0x0600_0000, &mut self.vram),
            ] {
                if let Some(start) = address.checked_sub(base).map(|offset| offset as usize) {
                    if let Some(end) = start.checked_add(bytes.len()) {
                        if let Some(output) = region.get_mut(start..end) {
                            output.copy_from_slice(bytes);
                            return Ok(());
                        }
                    }
                }
            }
            Err(IntroError::Memory {
                address,
                length: bytes.len(),
            })
        }
    }

    #[derive(Default)]
    struct InterceptedCalls(Vec<IntroCall>);
    impl IntroCalls for InterceptedCalls {
        fn invoke(
            &mut self,
            call: IntroCall,
            _memory: &mut dyn IntroMemory,
        ) -> Result<(), IntroError> {
            self.0.push(call);
            Ok(())
        }
    }
    struct InterceptedResources {
        lookup: [u32; 8],
        requests: Vec<(u32, usize)>,
    }
    impl InterceptedResources {
        fn new(data: &Value) -> Self {
            Self {
                lookup: std::array::from_fn(|index| integer(&data["lookup"]["values"][index])),
                requests: Vec::new(),
            }
        }
    }
    impl IntroResources for InterceptedResources {
        fn background_destination(&self, mode: u8) -> Result<u32, IntroError> {
            Ok(self.lookup[usize::from(mode & 7)])
        }
        fn decode_lz77(&mut self, source: u32) -> Result<Vec<u8>, IntroError> {
            let length = match source {
                0x082d_453c => 4096,
                0x082d_5468 => 2048,
                0x082e_1be0 => 8192,
                0x082e_2d88 => 2048,
                0x082e_3480 => 17408,
                _ => return Err(IntroError::Resource(crate::binary::Error::Range)),
            };
            self.requests.push((source, length));
            Ok((0..length)
                .map(|index| ((source >> 1).wrapping_add((index as u32).wrapping_mul(37))) as u8)
                .collect())
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_bootstrap_resume_match_all_contexts_ram_hashes_and_declared_call_boundaries() {
        let data: Value = serde_json::from_str(vectors!("intro-vectors.json")).unwrap();
        assert_eq!(data["routines"]["initialize"]["sha256"], INITIALIZE_SHA256);
        assert_eq!(data["routines"]["resume"]["sha256"], RESUME_SHA256);
        assert_eq!(
            serde_json::to_value(CONTEXT_DESTINATION_INDICES.as_slice()).unwrap(),
            data["contextDestinationIndices"]
        );
        for case in data["initializers"].as_array().unwrap() {
            let mut memory = Memory::new(integer(&case["seed"]));
            let display = memory.read_halfword(DISPLAY_SHADOW).unwrap();
            memory
                .write_halfword(
                    DISPLAY_SHADOW,
                    (display & !7) | integer(&case["mode"]) as u16,
                )
                .unwrap();
            let mut resources = InterceptedResources::new(&data);
            let mut calls = InterceptedCalls::default();
            if case["routine"] == "initialize" {
                initialize(
                    &mut memory,
                    &mut resources,
                    &mut calls,
                    Some(integer(&case["stackPrefix"])),
                )
                .unwrap();
            } else {
                resume(
                    &mut memory,
                    &mut resources,
                    &mut calls,
                    Some(integer(&case["stackPrefix"])),
                )
                .unwrap();
            }
            for (index, expected) in case["contextStates"].as_array().unwrap().iter().enumerate() {
                let expected: Vec<_> = expected
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|word| (integer(word) as u16).to_le_bytes())
                    .collect();
                assert_eq!(
                    memory.span(CONTEXT_BASE + index as u32 * 32, 32).unwrap(),
                    expected,
                    "native context row{index}"
                );
            }
            memory.assert_snapshot(case);
            assert_eq!(serde_json::to_value(&calls.0).unwrap(), case["calls"]);
            let expected: Vec<_> = case["resourceRequests"]
                .as_array()
                .unwrap()
                .iter()
                .map(|request| {
                    (
                        integer(&request["sourceAddress"]),
                        integer(&request["length"]) as usize,
                    )
                })
                .collect();
            assert_eq!(resources.requests, expected);
            for (index, expected) in data["contextArguments"][case["routine"].as_str().unwrap()]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let row = CONTEXT_SETUP_ROWS[index];
                assert_eq!(
                    row,
                    [
                        integer(&expected["endX"]),
                        integer(&expected["endY"]),
                        integer(&expected["descriptorAddress"]),
                        integer(&expected["flags"]),
                        integer(&expected["speed"])
                    ]
                );
            }
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_update_matches_skip_input_and_auxiliary_dispatch_control() {
        let data: Value = serde_json::from_str(vectors!("intro-vectors.json")).unwrap();
        assert_eq!(data["routines"]["update"]["sha256"], UPDATE_SHA256);
        for case in data["updates"].as_array().unwrap() {
            let mut memory = Memory::new(integer(&case["seed"]));
            memory
                .write_word(SKIP_STATE, integer(&case["skipState"]))
                .unwrap();
            memory
                .write_halfword(0x0300_0514, integer(&case["pressed"]) as u16)
                .unwrap();
            let depth = integer(&case["auxiliaryDepth"]);
            memory
                .write_halfword(AUXILIARY_STACK, depth as u16)
                .unwrap();
            if depth != 0 {
                memory
                    .write_word(
                        AUXILIARY_STACK + 4 + (depth - 1) * 4,
                        integer(&data["interceptedCallbackId"]),
                    )
                    .unwrap();
            }
            let mut calls = InterceptedCalls::default();
            update(0x0201_99a0, &mut memory, &mut calls).unwrap();
            memory.assert_snapshot(case);
            assert_eq!(serde_json::to_value(&calls.0).unwrap(), case["calls"]);
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn strict_unported_stop_and_malformed_memory_preserve_the_actual_source_boundary() {
        let data: Value = serde_json::from_str(vectors!("intro-vectors.json")).unwrap();
        let mut memory = Memory::new(0);
        let before_skip = memory.read_word(SKIP_STATE).unwrap();
        let mut resources = InterceptedResources::new(&data);
        let expected = IntroCall::Direct {
            entry_offset: 0x3250,
            arguments: Vec::new(),
        };
        assert_eq!(
            initialize(
                &mut memory,
                &mut resources,
                &mut RejectUnportedIntroCalls,
                None
            ),
            Err(IntroError::UnportedCall(expected))
        );
        assert_eq!(memory.read_word(0x0300_40b0).unwrap(), 0x0300_2980);
        assert_eq!(memory.read_word(SKIP_STATE).unwrap(), before_skip);
        let before_context = memory.span(CONTEXT_BASE, 32).unwrap().to_vec();
        let mut calls = InterceptedCalls::default();
        assert_eq!(
            resume(&mut memory, &mut resources, &mut calls, None),
            Err(IntroError::MissingConstructorStackPrefix)
        );
        assert_eq!(memory.span(CONTEXT_BASE, 32).unwrap(), before_context);
        memory.write_halfword(AUXILIARY_STACK, 11).unwrap();
        memory.write_word(SKIP_STATE, 0).unwrap();
        let mut calls = InterceptedCalls::default();
        assert_eq!(
            update(0, &mut memory, &mut calls),
            Err(IntroError::AuxiliaryDepth(11))
        );
        assert_eq!(calls.0.len(), 2);
        assert!(matches!(
            memory.read_word(u32::MAX),
            Err(IntroError::Memory { .. })
        ));
        assert!(matches!(IntroRom::new(&[]), Err(IntroError::WrongRom)));
    }

    #[test]
    fn resource_destination_uses_native_address_window_independent_of_decode_budget() {
        struct Bytes(usize);
        impl IntroResources for Bytes {
            fn background_destination(&self, _mode: u8) -> Result<u32, IntroError> {
                unreachable!()
            }
            fn decode_lz77(&mut self, _source: u32) -> Result<Vec<u8>, IntroError> {
                Ok(vec![0xa5; self.0])
            }
        }
        let mut memory = Memory::new(7);
        // Actual native mode3..5 request; the memory backend owns write semantics.
        copy_resource(&mut memory, &mut Bytes(0x4400), 0, 0x0601_4000).unwrap();
        assert_eq!(
            memory.span(0x0601_4000, 0x4400).unwrap(),
            vec![0xa5; 0x4400]
        );
        copy_resource(&mut memory, &mut Bytes(4), 0, 0x0601_fffc).unwrap();
        assert_eq!(memory.span(0x0601_fffc, 4).unwrap(), &[0xa5; 4]);
        for (destination, length) in [
            (0x0601_fffd, 4),
            (0x0602_0000, 1),
            (0x05ff_ffff, 1),
            (u32::MAX, 2),
        ] {
            let before = memory.vram.clone();
            assert_eq!(
                copy_resource(&mut memory, &mut Bytes(length), 0, destination),
                Err(IntroError::InvalidResourceDestination(destination))
            );
            assert_eq!(memory.vram, before);
        }
        assert_eq!(
            copy_resource(
                &mut memory,
                &mut Bytes(MAX_RESOURCE_BYTES + 1),
                0,
                0x0600_0000
            ),
            Err(IntroError::ResourceLimit(MAX_RESOURCE_BYTES + 1))
        );
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; hashes authentic assets without embedding them"]
    fn authenticated_type10_resources_match_independent_local_decode_hashes() {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("SetSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        let mut rom = IntroRom::new(&bytes).unwrap();
        let data: Value = serde_json::from_str(vectors!("intro-vectors.json")).unwrap();
        for resource in data["authenticLz77Resources"].as_array().unwrap() {
            let decoded = rom
                .decode_lz77(integer(&resource["sourceAddress"]))
                .unwrap();
            assert_eq!(decoded.len(), integer(&resource["decodedLength"]) as usize);
            assert_eq!(
                crate::sha256(&decoded),
                resource["decodedSha256"].as_str().unwrap()
            );
            let offset = integer(&resource["offset"]) as usize;
            let parsed = crate::binary::decode(&bytes[offset..], MAX_RESOURCE_BYTES).unwrap();
            assert_eq!(parsed.consumed, integer(&resource["storedLength"]) as usize);
            assert_eq!(
                crate::sha256(&bytes[offset..offset + parsed.consumed]),
                resource["storedSha256"].as_str().unwrap()
            );
        }
        for mode in 0..8 {
            assert_eq!(
                rom.background_destination(mode).unwrap(),
                integer(&data["lookup"]["values"][usize::from(mode)])
            );
        }
    }
}
