//! Exact A3VE intro auxiliary callbacks installed by 0x6ef4/0x7840.
//!
//! The seven installed callback IDs and their direct 0x6754 completion target
//! are translated. Native auxiliary push/pop semantics are included. Audio and
//! palette routines remain typed strict calls. The installed 0x08006825 target
//! remains unported; reaching it never becomes successful idle playback.

use super::intro::{
    IntroCall, IntroCalls, IntroError, IntroMemory, IntroResources, IntroRom, DISPLAY_SHADOW,
    MAX_RESOURCE_BYTES, VRAM_WINDOW_BYTES,
};

pub const AUXILIARY_STACK: u32 = 0x0300_2060;
pub const SKIP_STATE: u32 = 0x0200_2190;
pub const AUDIO_PLAYER_WORD: u32 = 0x086d_d64c;
pub const FADE_IN: u32 = 0x0800_66f9;
pub const COMPLETE: u32 = 0x0800_6755;
pub const PALETTE_FADE: u32 = 0x0800_6765;
pub const WAIT: u32 = 0x0800_67a1;
pub const WAIT_AND_STOP_AUDIO: u32 = 0x0800_67dd;
pub const FADE_OUT: u32 = 0x0800_6df1;
pub const TRANSITION: u32 = 0x0800_6e41;
pub const LOAD_BACKGROUND: u32 = 0x0800_6eb5;
pub const ANIMATION_CALLBACK: u32 = 0x0800_6825;

/// Name, exact source offset, byte length including its literal pool, SHA-256.
pub const SOURCE_SPANS: [(&str, usize, usize, &str); 10] = [
    (
        "fadeIn",
        0x66f8,
        0x5c,
        "f0df2c97ebd89803064dfbb7c21d7c332270c3c12d07d2eb596791a8da49c3ce",
    ),
    (
        "complete",
        0x6754,
        0x10,
        "0a0d209f759fe5f94b0e3749fde74cbc6fd59fe32b65cf14ba1da6e94b59f903",
    ),
    (
        "paletteFade",
        0x6764,
        0x3c,
        "81737580272c408cd1f57a20587df624d80d86bf7e6224f9fe88d4b822dd5b5f",
    ),
    (
        "wait",
        0x67a0,
        0x3c,
        "6b37612df56c63926340c1e8397cf8ab35bd7e0b4b09171ec3c83338cb857054",
    ),
    (
        "waitAndStopAudio",
        0x67dc,
        0x48,
        "b90054dd3e53196d1a134d32dbff6a780b9779059568d5eab3f912951b392e53",
    ),
    (
        "fadeOut",
        0x6df0,
        0x50,
        "5acd0730b152dbfa318d2722a99272929a1f5a1b7fe40408c1c0f283e2966ab8",
    ),
    (
        "transition",
        0x6e40,
        0x74,
        "276f032a02731c27d32f2b16098d2e6d6983ee6b6b8ea85ba7aff5c9e1b784f8",
    ),
    (
        "loadBackground",
        0x6eb4,
        0x40,
        "968e62e34d0f173d9023b0596b53bb13ae47eb4b76fa37afcc1135af1f73125c",
    ),
    (
        "auxiliaryPush",
        0x2f74,
        0x34,
        "507ff2039ba1af4eb05cef37b966a286e62acb2dcc6016ff8527b25be6d59e24",
    ),
    (
        "auxiliaryPop",
        0x2fa8,
        0x14,
        "dae4eb333910023603615b122cb0df837441d1f817f7c43062c48e0bc6609f40",
    ),
];

pub trait IntroCallbackResources {
    fn read_rom_word(&self, address: u32) -> Result<u32, IntroError>;
    fn decode_lz77(&mut self, source_address: u32) -> Result<Vec<u8>, IntroError>;
}

/// Exact local-ROM authentication and bounded shared type10 decoding. No native
/// ROM contents, decoded graphics or audio are embedded in this source.
#[derive(Clone)]
pub struct IntroCallbackRom<'a> {
    rom: &'a [u8],
    intro: IntroRom<'a>,
}
impl<'a> IntroCallbackRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, IntroError> {
        Ok(Self {
            rom,
            intro: IntroRom::new(rom)?,
        })
    }
}
impl IntroCallbackResources for IntroCallbackRom<'_> {
    fn read_rom_word(&self, address: u32) -> Result<u32, IntroError> {
        let offset = crate::binary::pointer_offset(address, 0x0800_0000, self.rom.len())?;
        Ok(crate::binary::u32_le(self.rom, offset)?)
    }
    fn decode_lz77(&mut self, source_address: u32) -> Result<Vec<u8>, IntroError> {
        self.intro.decode_lz77(source_address)
    }
}

fn depth(memory: &dyn IntroMemory) -> Result<u16, IntroError> {
    let count = memory.read_halfword(AUXILIARY_STACK)?;
    if count > 10 {
        Err(IntroError::AuxiliaryDepth(count))
    } else {
        Ok(count)
    }
}

fn top_parameter(memory: &dyn IntroMemory) -> Result<u32, IntroError> {
    let count = depth(memory)?;
    if count == 0 {
        return Err(IntroError::AuxiliaryDepth(count));
    }
    Ok(AUXILIARY_STACK + 0x2c + u32::from(count - 1) * 2)
}

fn pop(memory: &mut dyn IntroMemory) -> Result<(), IntroError> {
    let count = depth(memory)?;
    if count != 0 {
        memory.write_halfword(AUXILIARY_STACK, count - 1)?;
    }
    Ok(())
}

fn push(memory: &mut dyn IntroMemory, callback: u32) -> Result<(), IntroError> {
    let count = depth(memory)?;
    if count < 10 {
        memory.write_word(AUXILIARY_STACK + 4 + u32::from(count) * 4, callback)?;
        let count = depth(memory)?;
        memory.write_halfword(AUXILIARY_STACK + 0x2c + u32::from(count) * 2, 0)?;
        let count = depth(memory)?;
        memory.write_halfword(AUXILIARY_STACK, count + 1)?;
    }
    Ok(())
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
    resources: &mut dyn IntroCallbackResources,
    source: u32,
    destination: u32,
) -> Result<(), IntroError> {
    let decoded = resources.decode_lz77(source)?;
    if decoded.len() > MAX_RESOURCE_BYTES {
        return Err(IntroError::ResourceLimit(decoded.len()));
    }
    let within_window = destination
        .checked_sub(0x0600_0000)
        .and_then(|offset| (offset as usize).checked_add(decoded.len()))
        .is_some_and(|end| end <= VRAM_WINDOW_BYTES);
    if !within_window {
        return Err(IntroError::InvalidResourceDestination(destination));
    }
    memory.write_bytes(destination, &decoded)
}

/// Execute exactly one known native callback. Active depths outside1..10 stop
/// before source writes; native underflow is not exposed as host memory access.
/// Unported dependencies stop at their actual call position, retaining earlier
/// source mutations and leaving later stack operations pending.
pub fn dispatch(
    callback_id: u32,
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn IntroCallbackResources,
    calls: &mut dyn IntroCalls,
) -> Result<(), IntroError> {
    if !matches!(
        callback_id,
        FADE_IN
            | COMPLETE
            | PALETTE_FADE
            | WAIT
            | WAIT_AND_STOP_AUDIO
            | FADE_OUT
            | TRANSITION
            | LOAD_BACKGROUND
    ) {
        return Err(IntroError::UnportedCall(IntroCall::Callback {
            callback_id,
            arguments: [context],
        }));
    }
    let parameter_address = top_parameter(memory)?;
    match callback_id {
        FADE_IN => {
            let parameter = memory.read_halfword(parameter_address)?;
            if (parameter as i16) <= 16 {
                let blend = (16u32.wrapping_sub(u32::from(parameter)) << 8) | u32::from(parameter);
                memory.read_halfword(DISPLAY_SHADOW + 0x4c)?;
                memory.write_halfword(DISPLAY_SHADOW + 0x4c, blend as u16)?;
            }
            let address = top_parameter(memory)?;
            let current = memory.read_halfword(address)?;
            memory.write_halfword(address, current.wrapping_add(1))?;
            if (parameter as i16) > 15 {
                memory.write_halfword(context.wrapping_add(4), 120)?;
                pop(memory)?;
                invoke(calls, memory, 0x389c, &[124])?;
            }
        }
        COMPLETE => {
            memory.write_word(context, 1)?;
            pop(memory)?;
        }
        PALETTE_FADE => {
            let parameter = memory.read_halfword(parameter_address)?;
            memory.write_halfword(parameter_address, parameter.wrapping_add(1))?;
            invoke(calls, memory, 0x5784, &[0, 32, u32::from(parameter as u8)])?;
            if (parameter as i16) > 31 {
                memory.write_halfword(context.wrapping_add(4), 120)?;
                pop(memory)?;
            }
        }
        WAIT | WAIT_AND_STOP_AUDIO => {
            memory.read_halfword(DISPLAY_SHADOW + 0x4c)?;
            memory.write_halfword(DISPLAY_SHADOW + 0x4c, 31)?;
            let parameter = memory.read_halfword(parameter_address)?;
            memory.write_halfword(parameter_address, parameter.wrapping_sub(1))?;
            let timer = memory
                .read_halfword(context.wrapping_add(4))?
                .wrapping_sub(1);
            memory.write_halfword(context.wrapping_add(4), timer)?;
            if (timer as i16) <= 0 {
                if callback_id == WAIT_AND_STOP_AUDIO {
                    let player = resources.read_rom_word(AUDIO_PLAYER_WORD)?;
                    invoke(calls, memory, 0x5ebd4, &[player, 3])?;
                }
                pop(memory)?;
            }
        }
        FADE_OUT | TRANSITION => {
            let parameter = memory.read_halfword(parameter_address)?;
            if (parameter as i16) <= 8 {
                let doubled = u32::from(parameter.wrapping_shl(1));
                let blend = (doubled << 8) | 16u32.wrapping_sub(doubled);
                memory.read_halfword(DISPLAY_SHADOW + 0x4c)?;
                memory.write_halfword(DISPLAY_SHADOW + 0x4c, blend as u16)?;
            }
            let address = top_parameter(memory)?;
            let current = memory.read_halfword(address)?;
            memory.write_halfword(address, current.wrapping_add(1))?;
            if (parameter as i16) > 7 {
                if callback_id == TRANSITION {
                    memory.write_word(SKIP_STATE, 1)?;
                }
                pop(memory)?;
                if callback_id == TRANSITION {
                    push(memory, COMPLETE)?;
                    push(memory, ANIMATION_CALLBACK)?;
                }
            }
        }
        LOAD_BACKGROUND => {
            let display = memory.read_halfword(DISPLAY_SHADOW + 4)?;
            let destination = 0x0600_0000 | (u32::from(display & 0x0c) << 12);
            copy_resource(memory, resources, 0x082d_4d48, destination)?;
            let display = memory.read_halfword(DISPLAY_SHADOW + 4)?;
            let destination = 0x0600_0000 | (u32::from(display & 0x1f00) << 3);
            copy_resource(memory, resources, 0x082d_5670, destination)?;
            pop(memory)?;
        }
        _ => unreachable!("known callback validated above"),
    }
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
        fn new(seed: u32, count: u16, parameter: u16, timer: u16, display: u16) -> Self {
            let mut result = Self {
                ewram: pattern(seed, 0x40000),
                iwram: pattern(seed ^ 0xa55a_1234, 0x8000),
                vram: pattern(seed ^ 0x7ace_1200, 0x20000),
            };
            result.write_halfword(AUXILIARY_STACK, count).unwrap();
            if count != 0 && count <= 10 {
                result
                    .write_halfword(AUXILIARY_STACK + 0x2c + u32::from(count - 1) * 2, parameter)
                    .unwrap();
            }
            result.write_halfword(0x0201_99a4, timer).unwrap();
            result.write_halfword(DISPLAY_SHADOW + 4, display).unwrap();
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
                case["vramTransactionWindowSha256"].as_str().unwrap()
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
    struct Calls {
        calls: Vec<IntroCall>,
        boundaries: Vec<Value>,
    }
    impl IntroCalls for Calls {
        fn invoke(
            &mut self,
            call: IntroCall,
            memory: &mut dyn IntroMemory,
        ) -> Result<(), IntroError> {
            self.calls.push(call);
            self.boundaries.push(serde_json::json!({"depth": memory.read_halfword(AUXILIARY_STACK)?, "timer": memory.read_halfword(0x0201_99a4)?}));
            Ok(())
        }
    }
    struct Resources {
        player: u32,
        requests: Vec<(u32, usize)>,
    }
    impl Resources {
        fn new(data: &Value) -> Self {
            Self {
                player: integer(&data["audioPlayerWord"]["value"]),
                requests: Vec::new(),
            }
        }
    }
    impl IntroCallbackResources for Resources {
        fn read_rom_word(&self, address: u32) -> Result<u32, IntroError> {
            if address == AUDIO_PLAYER_WORD {
                Ok(self.player)
            } else {
                Err(crate::binary::Error::Range.into())
            }
        }
        fn decode_lz77(&mut self, source: u32) -> Result<Vec<u8>, IntroError> {
            let length = match source {
                0x082d_4d48 => 3072,
                0x082d_5670 => 2048,
                _ => return Err(crate::binary::Error::Range.into()),
            };
            self.requests.push((source, length));
            Ok((0..length)
                .map(|index| ((source >> 1).wrapping_add((index as u32).wrapping_mul(37))) as u8)
                .collect())
        }
    }
    fn fixture() -> Value {
        serde_json::from_str(vectors!("intro-callbacks-vectors.json")).unwrap()
    }
    fn assert_observations(case: &Value, memory: &Memory, calls: &Calls, resources: &Resources) {
        memory.assert_snapshot(case);
        assert_eq!(serde_json::to_value(&calls.calls).unwrap(), case["calls"]);
        assert_eq!(
            serde_json::to_value(&calls.boundaries).unwrap(),
            case["callBoundaries"]
        );
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
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn original_callbacks_match_all_ram_writes_call_order_and_resource_transactions() {
        let data = fixture();
        for &(name, offset, length, hash) in &SOURCE_SPANS {
            let source = &data["routines"][name];
            assert_eq!(integer(&source["offset"]) as usize, offset);
            assert_eq!(integer(&source["length"]) as usize, length);
            assert_eq!(source["sha256"], hash);
        }
        for case in data["cases"].as_array().unwrap() {
            let callback = 0x0800_0001
                + integer(&data["routines"][case["routine"].as_str().unwrap()]["offset"]);
            let mut memory = Memory::new(
                integer(&case["seed"]),
                integer(&case["depth"]) as u16,
                integer(&case["parameter"]) as u16,
                integer(&case["timer"]) as u16,
                integer(&case["display"]) as u16,
            );
            let mut calls = Calls::default();
            let mut resources = Resources::new(&data);
            dispatch(
                callback,
                0x0201_99a0,
                &mut memory,
                &mut resources,
                &mut calls,
            )
            .unwrap();
            assert_observations(case, &memory, &calls, &resources);
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn installed_stack_runs_326_native_ticks_then_stops_at_actual_unported_animation() {
        let data = fixture();
        let sequence = &data["sequence"];
        let installed = sequence["installed"].as_array().unwrap();
        let mut memory = Memory::new(
            integer(&sequence["seed"]),
            installed.len() as u16,
            0,
            integer(&sequence["timer"]) as u16,
            integer(&sequence["display"]) as u16,
        );
        for (index, callback) in installed.iter().enumerate() {
            memory
                .write_word(AUXILIARY_STACK + 4 + index as u32 * 4, integer(callback))
                .unwrap();
            memory
                .write_halfword(AUXILIARY_STACK + 0x2c + index as u32 * 2, 0)
                .unwrap();
        }
        assert_eq!(sequence["ticks"].as_array().unwrap().len(), 326);
        for tick in sequence["ticks"].as_array().unwrap() {
            let count = memory.read_halfword(AUXILIARY_STACK).unwrap();
            let callback = memory
                .read_word(AUXILIARY_STACK + 4 + u32::from(count - 1) * 4)
                .unwrap();
            assert_eq!(callback, integer(&tick["callbackId"]));
            let mut calls = Calls::default();
            let mut resources = Resources::new(&data);
            dispatch(
                callback,
                0x0201_99a0,
                &mut memory,
                &mut resources,
                &mut calls,
            )
            .unwrap();
            assert_observations(tick, &memory, &calls, &resources);
        }
        let count = memory.read_halfword(AUXILIARY_STACK).unwrap();
        assert_eq!(
            memory
                .read_word(AUXILIARY_STACK + 4 + u32::from(count - 1) * 4)
                .unwrap(),
            ANIMATION_CALLBACK
        );
        assert_eq!(
            dispatch(
                ANIMATION_CALLBACK,
                0x0201_99a0,
                &mut memory,
                &mut Resources::new(&data),
                &mut Calls::default()
            ),
            Err(IntroError::UnportedCall(IntroCall::Callback {
                callback_id: ANIMATION_CALLBACK,
                arguments: [0x0201_99a0]
            }))
        );
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn strict_stops_preserve_mutations_at_the_real_call_position_and_bound_depths() {
        use super::super::intro::RejectUnportedIntroCalls;
        let data = fixture();
        let mut resources = Resources::new(&data);
        for count in [0, 11, u16::MAX] {
            let mut memory = Memory::new(1, count, 0, 55, 0x1f00);
            let before = memory.iwram.clone();
            assert_eq!(
                dispatch(
                    FADE_IN,
                    0x0201_99a0,
                    &mut memory,
                    &mut resources,
                    &mut Calls::default()
                ),
                Err(IntroError::AuxiliaryDepth(count))
            );
            assert_eq!(memory.iwram, before);
        }
        for (callback, parameter, timer, entry, expected_depth, expected_timer) in [
            (FADE_IN, 16, 55, 0x389c, 1, 120),
            (PALETTE_FADE, 32, 55, 0x5784, 2, 55),
            (WAIT_AND_STOP_AUDIO, 7, 1, 0x5ebd4, 2, 0),
        ] {
            let mut memory = Memory::new(2, 2, parameter, timer, 0x1f00);
            let error = dispatch(
                callback,
                0x0201_99a0,
                &mut memory,
                &mut resources,
                &mut RejectUnportedIntroCalls,
            )
            .unwrap_err();
            assert!(
                matches!(error, IntroError::UnportedCall(IntroCall::Direct { entry_offset, .. }) if entry_offset == entry)
            );
            assert_eq!(
                memory.read_halfword(AUXILIARY_STACK).unwrap(),
                expected_depth
            );
            assert_eq!(memory.read_halfword(0x0201_99a4).unwrap(), expected_timer);
            assert_eq!(
                memory.read_halfword(AUXILIARY_STACK + 0x2e).unwrap(),
                if callback == WAIT_AND_STOP_AUDIO {
                    parameter - 1
                } else {
                    parameter + 1
                }
            );
        }
        assert!(matches!(
            IntroCallbackRom::new(&[]),
            Err(IntroError::WrongRom)
        ));
    }

    #[test]
    fn resource_copy_is_bounded_and_never_pops_after_a_failed_resource() {
        struct Oversized(usize);
        impl IntroCallbackResources for Oversized {
            fn read_rom_word(&self, _address: u32) -> Result<u32, IntroError> {
                unreachable!()
            }
            fn decode_lz77(&mut self, _source: u32) -> Result<Vec<u8>, IntroError> {
                Ok(vec![0; self.0])
            }
        }
        let mut memory = Memory::new(1, 1, 0, 0, 0x1f00);
        let before = memory.vram.clone();
        assert_eq!(
            dispatch(
                LOAD_BACKGROUND,
                0x0201_99a0,
                &mut memory,
                &mut Oversized(MAX_RESOURCE_BYTES + 1),
                &mut Calls::default()
            ),
            Err(IntroError::ResourceLimit(MAX_RESOURCE_BYTES + 1))
        );
        assert_eq!(memory.read_halfword(AUXILIARY_STACK).unwrap(), 1);
        assert_eq!(memory.vram, before);
        assert_eq!(
            copy_resource(&mut memory, &mut Oversized(2), 0, 0x0601_ffff),
            Err(IntroError::InvalidResourceDestination(0x0601_ffff))
        );
        assert_eq!(memory.vram, before);
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; hashes authentic assets without embedding them"]
    fn authenticated_resources_and_audio_pointer_match_independent_native_source_hashes() {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("SetSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        let mut rom = IntroCallbackRom::new(&bytes).unwrap();
        let data = fixture();
        assert_eq!(
            rom.read_rom_word(AUDIO_PLAYER_WORD).unwrap(),
            integer(&data["audioPlayerWord"]["value"])
        );
        assert!(rom.read_rom_word(0x087f_fffe).is_err());
        assert!(rom.read_rom_word(0x07ff_ffff).is_err());
        assert_eq!(
            crate::sha256(&bytes[0x6dd64c..0x6dd650]),
            data["audioPlayerWord"]["sha256"].as_str().unwrap()
        );
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
    }
}
