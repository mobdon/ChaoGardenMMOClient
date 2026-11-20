//! A3VE mode-menu callbacks and native next-screen dispatch.
//!
//! Large resource/layout initializers and rendering remain explicit native-ID
//! dependencies. No missing screen is replaced by a generated menu. In
//! particular the Garden transfer's original loader boundary remains strict;
//! this source does not claim an ARM payload/game-engine port.

use super::control::{CallbackId, CallbackStack};
use super::intro::{IntroCall, IntroError, IntroMemory, DISPLAY_SHADOW};
use super::intro_sequence::IntroSequenceResources;
use std::fmt;

pub const CALLBACK_ID: u32 = 0x0801_cf4d;
pub const CONTEXT_SLOT: u32 = 0x0201_2bbc;
pub const CONTEXT: u32 = 0x0201_99a0;
pub const MAIN_STACK: u32 = 0x0300_0e00;
pub const AUXILIARY_STACK: u32 = 0x0300_2060;
pub const SELECTION: u32 = 0x0201_2c0a;
pub const EXIT_FADE: u32 = 0x0201_2c10;
pub const LOOKUP_TABLE: u32 = 0x080a_b29a;
pub const MAX_RESOURCE_BYTES: usize = 0x2000;
/// Exact translated native spans, including each routine's literal pool.
pub const SOURCE_SPANS: [(&str, usize, usize, &str); 11] = [
    (
        "entry",
        0x1cf4c,
        0x2f0,
        "0e62bcfd63813183ed28473f883ca92981cddb57b93f9b4626cf8acbcd7f4901",
    ),
    (
        "resume",
        0x1b69c,
        0xc,
        "63813c56101a64f13c7cc33ffd0ddf548a13abb410f5ce7485d9cbc4f9bf5fb7",
    ),
    (
        "resumeSecond",
        0x1b6a8,
        0x28,
        "7fef9a6dcea4e035ea8b6704fe3d7f0f6bc667edcdb99e6f793212ad1231cbc6",
    ),
    (
        "resumeThird",
        0x1b6d0,
        0x28,
        "729926b9e7a7fa47c3f9ee2680e3cdb6ca73e5229000026a3173d298ae1a0565",
    ),
    (
        "lookup",
        0x1b6f8,
        0x44,
        "a53e5dd163a24c155a6b4729cf1cc83a0189fcaa9baee598c8b8b0b8862bce54",
    ),
    (
        "selection",
        0x195e0,
        0x81c,
        "be6d4acefdb8d62c9e4c5ee0539f2be0eebce3adedc96bcb5570cd72ef20aad6",
    ),
    (
        "selectionFade",
        0x15e20,
        0x38,
        "1036c98fd5245ffa72c781292597575d677de409156e71d18d6a53990f3927ef",
    ),
    (
        "slideOut",
        0x15e58,
        0x48,
        "8ae5130affa7d59ece08957cb2765e0968e20aaa9ac621edbb8dc19c85ecef2f",
    ),
    (
        "slideIn",
        0x15ea0,
        0x48,
        "83d1b4c921949287e3b133eb6453fb1ac241e266523b7d967f001b8629a0234c",
    ),
    (
        "cancelFade",
        0x15ee8,
        0x54,
        "f6ccdc11a2eec9b363756d5f93b2d5a3f49afd917cb5ef888d31ca34395f39ec",
    ),
    (
        "paletteIntro",
        0x19dfc,
        0x17c,
        "c24286469d9a3a9514b6cc4f8a48f0276718615e8b0b34dd2222bd31de9cbf18",
    ),
];
pub const GARDEN_PAYLOAD_ADDRESS: u32 = 0x080d_0970;
pub const GARDEN_STORED_LENGTH: usize = 152552;
pub const GARDEN_STORED_SHA256: &str =
    "72fd1b2645fc1358ad131a697dac1c53b4ae57f195759ff78e1535ebda70d29f";
pub const GARDEN_DECODED_LENGTH: usize = 188716;
pub const GARDEN_DECODED_SHA256: &str =
    "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeMenuError {
    Intro(IntroError),
    LookupValue(u16),
}
impl fmt::Display for ModeMenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ModeMenuError {}
impl From<IntroError> for ModeMenuError {
    fn from(value: IntroError) -> Self {
        Self::Intro(value)
    }
}

/// Native R0 is explicit because Garden setup consumes the 0x36D4 result.
/// Implementations must route completed Rust ports or reject the exact call.
pub trait ModeMenuCalls {
    fn invoke(
        &mut self,
        call: IntroCall,
        memory: &mut dyn IntroMemory,
    ) -> Result<u32, ModeMenuError>;
}
#[derive(Default)]
pub struct RejectUnportedModeCalls;
impl ModeMenuCalls for RejectUnportedModeCalls {
    fn invoke(
        &mut self,
        call: IntroCall,
        _memory: &mut dyn IntroMemory,
    ) -> Result<u32, ModeMenuError> {
        Err(IntroError::UnportedCall(call).into())
    }
}
pub struct ModeMenuRom<'a> {
    rom: &'a [u8],
}
impl<'a> ModeMenuRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, IntroError> {
        crate::catalog::validate_rom(rom).map_err(|_| IntroError::WrongRom)?;
        Ok(Self { rom })
    }
}
impl IntroSequenceResources for ModeMenuRom<'_> {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
        if length > MAX_RESOURCE_BYTES {
            return Err(IntroError::ResourceLimit(length));
        }
        let offset = crate::binary::pointer_offset(address, 0x0800_0000, self.rom.len())?;
        Ok(crate::binary::span(self.rom, offset, length)?.to_vec())
    }
}

fn invoke(
    calls: &mut dyn ModeMenuCalls,
    memory: &mut dyn IntroMemory,
    entry_offset: usize,
    arguments: &[u32],
) -> Result<u32, ModeMenuError> {
    calls.invoke(
        IntroCall::Direct {
            entry_offset,
            arguments: arguments.to_vec(),
        },
        memory,
    )
}
fn stack(memory: &dyn IntroMemory, base: u32) -> Result<CallbackStack, ModeMenuError> {
    let mut words = [0; 32];
    for (index, value) in words.iter_mut().enumerate() {
        *value = memory.read_halfword(base + index as u32 * 2)?;
    }
    if words[0] > 10 {
        return Err(IntroError::AuxiliaryDepth(words[0]).into());
    }
    CallbackStack::from_halfwords(&words).map_err(|_| IntroError::AuxiliaryDepth(words[0]).into())
}
fn save_stack(
    memory: &mut dyn IntroMemory,
    base: u32,
    value: &CallbackStack,
) -> Result<(), ModeMenuError> {
    let bytes: Vec<_> = value
        .to_halfwords()
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    memory.write_bytes(base, &bytes)?;
    Ok(())
}
fn replace_main(
    memory: &mut dyn IntroMemory,
    callback: u32,
    parameter: u32,
) -> Result<(), ModeMenuError> {
    let mut main = stack(memory, MAIN_STACK)?;
    main.replace(CallbackId(callback), parameter);
    save_stack(memory, MAIN_STACK, &main)
}
fn clear_auxiliary(memory: &mut dyn IntroMemory) -> Result<(), ModeMenuError> {
    let mut auxiliary = stack(memory, AUXILIARY_STACK)?;
    auxiliary.clear();
    save_stack(memory, AUXILIARY_STACK, &auxiliary)
}
fn byte(memory: &dyn IntroMemory, address: u32) -> Result<u8, ModeMenuError> {
    Ok((memory.read_halfword(address & !1)? >> ((address & 1) * 8)) as u8)
}
fn put_byte(memory: &mut dyn IntroMemory, address: u32, value: u8) -> Result<(), ModeMenuError> {
    let aligned = address & !1;
    let shift = (address & 1) * 8;
    let old = memory.read_halfword(aligned)?;
    memory.write_halfword(
        aligned,
        (old & !(0xff << shift)) | (u16::from(value) << shift),
    )?;
    Ok(())
}
fn finish(
    memory: &mut dyn IntroMemory,
    callback: u32,
    parameter: u32,
) -> Result<(), ModeMenuError> {
    let context = memory.read_word(CONTEXT_SLOT)?;
    memory.write_word(context.wrapping_add(4), callback)?;
    replace_main(memory, callback, parameter)?;
    clear_auxiliary(memory)
}

/// Exact1B69C resume wrapper; the native1A8F8 layout initializer stays strict.
pub fn resume(
    context: u32,
    memory: &mut dyn IntroMemory,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    invoke(calls, memory, 0x1a8f8, &[context])?;
    Ok(())
}
/// Exact1B6A8 and1B6D0 re-entry setup, retaining original mode/selection flags.
pub fn resume_mode(
    context: u32,
    third_state: bool,
    memory: &mut dyn IntroMemory,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    memory.write_halfword(0x0201_2bc0, 2)?;
    memory.write_halfword(SELECTION, if third_state { 2 } else { 0 })?;
    memory.write_halfword(0x0201_2bde, u16::from(third_state))?;
    invoke(calls, memory, 0x1af84, &[context])?;
    Ok(())
}

/// Native1B6F8 performs an unbounded stack-table search. The host limits reads
/// to its actual ten halfwords and rejects values absent from that table.
pub fn lookup(value: u16, resources: &dyn IntroSequenceResources) -> Result<u16, ModeMenuError> {
    let table = resources.read_bytes(LOOKUP_TABLE, 20)?;
    if table.len() != 20 {
        return Err(IntroError::Resource(crate::binary::Error::Range).into());
    }
    for index in 0..10 {
        let item = crate::binary::u16_le(&table, index * 2).map_err(IntroError::Resource)?;
        if item == value {
            return Ok(match index {
                8 => 16,
                9 => 17,
                _ => index as u16,
            });
        }
    }
    Err(ModeMenuError::LookupValue(value))
}

/// Original1CF4C wrapper and seven-way exit dispatch. Calls may mutate globals;
/// pointer, counter and selection reloads preserve native post-call behavior.
pub fn entry(
    memory: &mut dyn IntroMemory,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    let count = memory.read_halfword(MAIN_STACK)?;
    if !(1..=10).contains(&count) {
        return Err(IntroError::AuxiliaryDepth(count).into());
    }
    let parameter = MAIN_STACK + 0x2c + u32::from(count - 1) * 2;
    match memory.read_halfword(parameter)? as i16 {
        state @ 0..=3 => {
            memory.write_halfword(parameter, 4)?;
            memory.write_word(CONTEXT_SLOT, CONTEXT)?;
            match state {
                0 => {
                    invoke(calls, memory, 0x1a100, &[CONTEXT])?;
                }
                1 => resume(CONTEXT, memory, calls)?,
                2 => resume_mode(CONTEXT, false, memory, calls)?,
                _ => resume_mode(CONTEXT, true, memory, calls)?,
            }
        }
        _ => {
            let context = memory.read_word(CONTEXT_SLOT)?;
            invoke(calls, memory, 0x1b73c, &[context])?;
            let context = memory.read_word(CONTEXT_SLOT)?;
            if memory.read_word(context)? != 0 {
                let fade = memory.read_halfword(EXIT_FADE)?.wrapping_add(1);
                memory.write_halfword(EXIT_FADE, fade)?;
                let selected = memory.read_halfword(SELECTION)? as i16;
                memory.write_halfword(DISPLAY_SHADOW + 0x4c, 31)?;
                let amount = if selected == 4 {
                    fade.wrapping_add(33)
                } else {
                    32u16.wrapping_sub(fade)
                } as u8;
                invoke(calls, memory, 0x5784, &[0, 32, u32::from(amount)])?;
                if memory.read_halfword(EXIT_FADE)? > 31 {
                    match memory.read_halfword(SELECTION)? as i16 {
                        0 => {
                            for address in [
                                0x0300_2948,
                                0x0300_2960,
                                0x0300_2968,
                                0x0300_294c,
                                0x0300_2954,
                            ] {
                                memory.write_halfword(address, 0)?;
                            }
                            finish(memory, 0x0800_abb5, 0)?;
                        }
                        1 => {
                            if memory.read_word(0x0201_2c30)? == 1 {
                                let context = memory.read_word(CONTEXT_SLOT)?;
                                memory.write_word(context.wrapping_add(4), 0x0801_0289)?;
                                memory.write_word(
                                    0x0300_296c,
                                    u32::from(byte(memory, 0x0300_3064)?),
                                )?;
                                memory.write_halfword(
                                    0x0300_2958,
                                    u16::from(byte(memory, 0x0300_3065)?),
                                )?;
                                replace_main(memory, 0x0801_0289, 4)?;
                                clear_auxiliary(memory)?;
                            } else {
                                if byte(memory, 0x0300_3064)? != 0 {
                                    put_byte(memory, 0x0300_3064, 0)?;
                                    put_byte(memory, 0x0300_3065, 0)?;
                                    invoke(calls, memory, 0x49af4, &[])?;
                                }
                                finish(memory, 0x0801_0289, 0)?;
                            }
                        }
                        2 => finish(memory, 0x0801_4dd9, 0)?,
                        3 => finish(memory, 0x0805_b599, 0)?,
                        4 => {
                            invoke(calls, memory, 0x5f040, &[])?;
                            let save = memory.read_word(0x0300_40b4)?;
                            invoke(calls, memory, 0x4973c, &[save])?;
                            invoke(calls, memory, 0x5fecc, &[0x080d_0970, 0x0200_0000])?;
                            memory.write_word(0x0200_0008, memory.read_word(0x0300_40b4)?)?;
                            let settings = memory.read_word(0x0300_40b0)?;
                            let language = memory.read_halfword(settings.wrapping_add(0x6e0))?;
                            if language <= 5 {
                                memory.write_word(
                                    0x0200_000c,
                                    [0, 1, 3, 2, 4, 1][usize::from(language)],
                                )?;
                            }
                            let seed = invoke(calls, memory, 0x36d4, &[0xffff])? as u16;
                            memory.write_word(0x0200_0010, u32::from(seed))?;
                            invoke(calls, memory, 0x5ff00, &[0])?;
                        }
                        5 => finish(memory, 0x0801_ef41, 0)?,
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

/// Exact195E0 controller. A/B consume pressed edges; Left/Right consume held
/// bits with the original counter gate. Cache transfers remain strict native
/// calls after physical source/destination bounds are established.
pub fn selection_update(
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    memory.write_halfword(DISPLAY_SHADOW + 0x14, 0)?;
    memory.write_halfword(DISPLAY_SHADOW + 0x0c, 0)?;
    if memory.read_halfword(0x0300_0516)? & 1 != 0
        && memory.read_word(0x0201_2bfc)? != 1
        && memory.read_word(0x0201_2bf8)? != 1
    {
        invoke(calls, memory, 0x389c, &[53])?;
        memory.write_word(0x0203_1a40, 1)?;
        match memory.read_halfword(SELECTION)? as i16 {
            0 => {
                invoke(calls, memory, 0x5724, &[0x082f_3f8c, 128, 4])?;
                memory.write_halfword(0x0300_3068, 1)?;
                selection_pop(memory)?;
                if invoke(calls, memory, 0x49be8, &[])? != 0 {
                    selection_push(memory, 0x0801_8dc1)?;
                    selection_push(memory, 0x0801_937d)?;
                } else {
                    selection_push(memory, 0x0801_5f3d)?;
                }
                selection_push(memory, 0x0801_5e59)?;
            }
            1 => {
                invoke(calls, memory, 0x5724, &[0x082f_3fac, 128, 4])?;
                selection_pop(memory)?;
                if byte(memory, 0x0300_3064)? != 0 && byte(memory, 0x0300_3065)? != 0 {
                    selection_push(memory, 0x0801_893d)?;
                    selection_push(memory, 0x0801_5e59)?;
                } else {
                    memory.write_word(context, 1)?;
                }
            }
            2 => {
                memory.write_halfword(0x0300_3068, 1)?;
                invoke(calls, memory, 0x5724, &[0x082f_3fcc, 128, 4])?;
                selection_pop(memory)?;
                selection_push(memory, 0x0801_66e1)?;
                selection_push(memory, 0x0801_5e59)?;
            }
            3 => {
                invoke(calls, memory, 0x5724, &[0x082f_3fec, 128, 4])?;
                memory.write_halfword(0x0300_3068, 7)?;
                selection_pop(memory)?;
                let source = context
                    .checked_add(0xac)
                    .ok_or(IntroError::Resource(crate::binary::Error::Range))?;
                let mut tilemap = vec![0; 0x800];
                for chunk in tilemap.as_chunks_mut::<2>().0.iter_mut() {
                    *chunk = 0x0200u16.to_le_bytes();
                }
                let mut value = 0x5000u16;
                for row in 18..=20 {
                    for column in 9..=22 {
                        let at = (row * 32 + column) * 2;
                        tilemap[at..at + 2].copy_from_slice(&value.to_le_bytes());
                        value = value.wrapping_add(1);
                    }
                }
                memory.write_bytes(source, &tilemap)?;
                let destination = 0x0600_0000
                    | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 4)? & 0x1f00) << 3);
                selection_copy(calls, memory, destination, source, 0x800)?;
                selection_push(memory, 0x0801_64a9)?;
                selection_push(memory, 0x0801_5e59)?;
            }
            4 | 5 => {
                selection_pop(memory)?;
                memory.write_word(context, 1)?;
            }
            6 => {
                invoke(calls, memory, 0x5724, &[0x082f_400c, 128, 4])?;
                selection_pop(memory)?;
                put_byte(memory, 0x0201_2c1c, 0)?;
                put_byte(memory, 0x0201_2c1d, 0)?;
                selection_push(memory, 0x0801_7649)?;
                selection_push(memory, 0x0801_5e59)?;
            }
            _ => {}
        }
    }
    // Native A handling falls through into B and direction handling. A+B can
    // therefore pop a newly installed child callback; no early return is added.
    if memory.read_halfword(0x0300_0516)? & 2 != 0 {
        selection_pop(memory)?;
        selection_push(memory, 0x0801_5ee9)?;
    }
    let held = memory.read_halfword(0x0300_0514)?;
    if held & 0x10 != 0 {
        if memory.read_word(0x0201_2bfc)? != 1
            && memory.read_word(0x0201_2bf8)? != 1
            && memory.read_word(0x0203_1a40)? != 1
            && selection_repeat(memory)?
        {
            invoke(calls, memory, 0x389c, &[87])?;
            memory.write_word(0x0201_2bfc, 1)?;
            let edge = memory.read_halfword(0x0201_2bea)?.wrapping_add(48);
            memory.write_halfword(0x0201_2bec, edge)?;
            for address in [0x0201_2c08, SELECTION, 0x0201_2c0c] {
                let next = memory.read_halfword(address)?.wrapping_add(1);
                memory.write_halfword(address, if next as i16 == 7 { 0 } else { next })?;
            }
            let incoming = memory.read_halfword(0x0201_2c0c)?;
            memory.write_halfword(0x0201_2c0e, if incoming as i16 == 7 { 0 } else { incoming })?;
            let destination = selection_background(memory, resources)?.wrapping_add(0x31a0);
            let source = selection_cache(memory.read_halfword(0x0201_2c0e)?, 0x0202_79a0, 0x800)?;
            selection_copy(calls, memory, destination, source, 0x800)?;
        }
    } else if held & 0x20 != 0 {
        if memory.read_word(0x0201_2bf8)? != 1
            && memory.read_word(0x0201_2bfc)? != 1
            && memory.read_word(0x0203_1a40)? != 1
            && selection_repeat(memory)?
        {
            invoke(calls, memory, 0x389c, &[87])?;
            memory.write_word(0x0201_2bf8, 1)?;
            let edge = memory.read_halfword(0x0201_2be6)?.wrapping_sub(48);
            memory.write_halfword(0x0201_2bec, edge)?;
            for address in [0x0201_2c08, SELECTION, 0x0201_2c0c] {
                let next = memory.read_halfword(address)?.wrapping_sub(1);
                memory.write_halfword(address, if next as i16 == -1 { 6 } else { next })?;
            }
            let incoming = memory.read_halfword(0x0201_2c08)?;
            memory.write_halfword(
                0x0201_2c0e,
                if incoming as i16 == -1 { 6 } else { incoming },
            )?;
            let destination = selection_background(memory, resources)?.wrapping_add(0x31a0);
            let source = selection_cache(memory.read_halfword(0x0201_2c0e)?, 0x0202_79a0, 0x800)?;
            selection_copy(calls, memory, destination, source, 0x800)?;
        }
    } else {
        memory.write_halfword(0x0201_2c3a, 0)?;
    }
    if memory.read_word(0x0201_2bfc)? == 1 {
        selection_animate_right(memory, resources, calls)?;
    }
    if memory.read_word(0x0201_2bf8)? == 1 {
        selection_animate_left(memory, resources, calls)?;
    }
    Ok(())
}

fn selection_pop(memory: &mut dyn IntroMemory) -> Result<(), ModeMenuError> {
    let mut value = stack(memory, AUXILIARY_STACK)?;
    value.pop();
    save_stack(memory, AUXILIARY_STACK, &value)
}
fn selection_push(memory: &mut dyn IntroMemory, callback: u32) -> Result<(), ModeMenuError> {
    let mut value = stack(memory, AUXILIARY_STACK)?;
    value.push(CallbackId(callback), 0);
    save_stack(memory, AUXILIARY_STACK, &value)
}
fn selection_parameter(memory: &dyn IntroMemory) -> Result<u32, ModeMenuError> {
    let count = memory.read_halfword(AUXILIARY_STACK)?;
    if !(1..=10).contains(&count) {
        return Err(IntroError::AuxiliaryDepth(count).into());
    }
    Ok(AUXILIARY_STACK + 0x2c + u32::from(count - 1) * 2)
}
fn selection_repeat(memory: &mut dyn IntroMemory) -> Result<bool, ModeMenuError> {
    let counter = memory.read_halfword(0x0201_2c3a)?;
    if counter == 0 {
        memory.write_halfword(0x0201_2c3a, 1)?;
        Ok(true)
    } else if counter == 20 {
        Ok(true)
    } else {
        if counter <= 19 {
            memory.write_halfword(0x0201_2c3a, counter.wrapping_add(1))?;
        }
        Ok(false)
    }
}
fn selection_background(
    memory: &dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
) -> Result<u32, ModeMenuError> {
    let mode = memory.read_halfword(DISPLAY_SHADOW)? & 7;
    Ok(resources.read_word(0x0806_6c54 + u32::from(mode) * 4)?)
}
fn selection_cache(index: u16, base: u32, stride: u32) -> Result<u32, ModeMenuError> {
    if index > 6 {
        return Err(ModeMenuError::LookupValue(index));
    }
    base.checked_add(u32::from(index) * stride)
        .ok_or_else(|| IntroError::Resource(crate::binary::Error::Range).into())
}
fn selection_copy(
    calls: &mut dyn ModeMenuCalls,
    memory: &mut dyn IntroMemory,
    destination: u32,
    source: u32,
    length: usize,
) -> Result<(), ModeMenuError> {
    let source_ok = source
        .checked_sub(0x0200_0000)
        .and_then(|offset| (offset as usize).checked_add(length))
        .is_some_and(|end| end <= 0x40000);
    let destination_ok = destination
        .checked_sub(0x0600_0000)
        .and_then(|offset| (offset as usize).checked_add(length))
        .is_some_and(|end| end <= super::intro::VRAM_WINDOW_BYTES);
    if !source_ok || length > 0xc00 {
        return Err(IntroError::Resource(crate::binary::Error::Range).into());
    }
    if !destination_ok {
        return Err(IntroError::InvalidResourceDestination(destination).into());
    }
    invoke(calls, memory, 0x162c, &[destination, source, length as u32])?;
    Ok(())
}
fn selection_tile_destination(memory: &dyn IntroMemory) -> Result<u32, ModeMenuError> {
    Ok(
        (0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + 10)? & 0x0c) << 12))
            .wrapping_add(0x4800),
    )
}
fn selection_cache_refresh(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    for (offset, index_address) in [
        (0, 0x0201_2c08),
        (0x800, SELECTION),
        (0x1000, 0x0201_2c0c),
        (0x31a0, 0x0201_2c0e),
    ] {
        let destination = selection_background(memory, resources)?.wrapping_add(offset);
        let source = selection_cache(memory.read_halfword(index_address)?, 0x0202_79a0, 0x800)?;
        selection_copy(calls, memory, destination, source, 0x800)?;
    }
    let destination = selection_tile_destination(memory)?;
    let source = selection_cache(memory.read_halfword(SELECTION)?, 0x0202_19a0, 0xc00)?;
    selection_copy(calls, memory, destination, source, 0xc00)
}
fn selection_positions(
    memory: &mut dyn IntroMemory,
    flag: u32,
    center: u16,
) -> Result<(), ModeMenuError> {
    memory.write_word(flag, 0)?;
    for (address, value) in [
        (0x0201_2bce, 128),
        (0x0201_2bd0, 256),
        (0x0201_2bd2, 128),
        (0x0201_2bd4, 128),
        (0x0201_2be6, 84),
        (0x0201_2be8, center),
        (0x0201_2bea, 192),
        (0x0201_2bec, 240),
        (0x0201_2bee, 98),
        (0x0201_2bf0, 86),
        (0x0201_2bf2, 98),
        (0x0201_2bf4, 98),
    ] {
        memory.write_halfword(address, value)?;
    }
    Ok(())
}
fn selection_animate_right(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    let destination = selection_tile_destination(memory)?;
    selection_copy(calls, memory, destination, 0x0202_6da0, 0xc00)?;
    let scale = memory.read_halfword(0x0201_2bd2)?;
    if scale as i16 != 256 {
        let other = memory.read_halfword(0x0201_2bd0)?.wrapping_sub(16);
        memory.write_halfword(0x0201_2bd0, other)?;
        memory.write_halfword(0x0201_2bd2, scale.wrapping_add(16))?;
    }
    let step = (i32::from(memory.read_halfword(0x0201_2bd2)? as i16) - 128) / 16;
    memory.write_halfword(0x0201_2bf2, (98 - step) as u16)?;
    let step = (i32::from(memory.read_halfword(0x0201_2bd2)? as i16) - 128) / 16;
    memory.write_halfword(0x0201_2bf0, (86 + step) as u16)?;
    for (address, amount) in [(0x0201_2be6, 8), (0x0201_2be8, 4)] {
        let value = memory.read_halfword(address)?.wrapping_sub(amount);
        memory.write_halfword(address, value)?;
    }
    let center = memory.read_halfword(0x0201_2bea)?.wrapping_sub(8);
    memory.write_halfword(0x0201_2bea, center)?;
    let edge = memory.read_halfword(0x0201_2bec)?;
    if edge as i16 > 176 {
        memory.write_halfword(0x0201_2bec, edge.wrapping_sub(2))?;
    }
    if center as i16 == 120 {
        selection_positions(memory, 0x0201_2bfc, center)?;
        selection_cache_refresh(memory, resources, calls)?;
    }
    Ok(())
}
fn selection_animate_left(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    let destination = selection_tile_destination(memory)?;
    selection_copy(calls, memory, destination, 0x0202_6da0, 0xc00)?;
    let scale = memory.read_halfword(0x0201_2bce)?;
    if scale as i16 != 256 {
        memory.write_halfword(0x0201_2bce, scale.wrapping_add(16))?;
        let other = memory.read_halfword(0x0201_2bd0)?.wrapping_sub(16);
        memory.write_halfword(0x0201_2bd0, other)?;
    }
    let step = (i32::from(memory.read_halfword(0x0201_2bce)? as i16) - 128) / 16;
    memory.write_halfword(0x0201_2bee, (98 - step) as u16)?;
    let step = (i32::from(memory.read_halfword(0x0201_2bce)? as i16) - 128) / 16;
    memory.write_halfword(0x0201_2bf0, (86 + step) as u16)?;
    let center = memory.read_halfword(0x0201_2be6)?.wrapping_add(4);
    memory.write_halfword(0x0201_2be6, center)?;
    for address in [0x0201_2be8, 0x0201_2bea] {
        let value = memory.read_halfword(address)?.wrapping_add(8);
        memory.write_halfword(address, value)?;
    }
    let edge = memory.read_halfword(0x0201_2bec)?;
    if edge as i16 <= 67 {
        memory.write_halfword(0x0201_2bec, edge.wrapping_add(2))?;
    }
    if center as i16 == 120 {
        selection_positions(memory, 0x0201_2bf8, center)?;
        selection_cache_refresh(memory, resources, calls)?;
    }
    Ok(())
}

/// Installed selection auxiliaries and directly used fades/slides. Child menu
/// IDs remain original stack entries and reject until their own ports exist.
pub fn selection_auxiliary(
    callback_id: u32,
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    match callback_id {
        0x0801_95e1 => selection_update(context, memory, resources, calls),
        0x0801_5e21 | 0x0801_5ee9 => {
            let address = selection_parameter(memory)?;
            let parameter = memory.read_halfword(address)?;
            memory.write_halfword(address, parameter.wrapping_add(1))?;
            let amount = if callback_id == 0x0801_5ee9 {
                memory.write_halfword(DISPLAY_SHADOW + 0x4c, 31)?;
                parameter.wrapping_add(32) as u8
            } else {
                parameter as u8
            };
            invoke(calls, memory, 0x5784, &[0, 32, u32::from(amount)])?;
            if parameter as i16 > 31 {
                selection_pop(memory)?;
                if callback_id == 0x0801_5ee9 {
                    replace_main(memory, 0x0800_91c5, 0)?;
                }
            }
            Ok(())
        }
        0x0801_5e59 => {
            let value = memory.read_halfword(0x0201_2be2)?;
            let complete = if value as i16 > 0 {
                let next = value.wrapping_sub(32);
                memory.write_halfword(0x0201_2be2, next)?;
                let other = memory.read_halfword(0x0201_2be4)?.wrapping_sub(30);
                memory.write_halfword(0x0201_2be4, other)?;
                memory.read_halfword(0x0201_2be2)? as i16 <= 0
            } else {
                true
            };
            if complete {
                memory.write_word(0x0201_2c28, 1)?;
                memory.write_halfword(0x0201_2be4, 32)?;
                memory.write_halfword(0x0201_2be2, 1)?;
                selection_pop(memory)?;
            }
            Ok(())
        }
        0x0801_5ea1 => {
            memory.write_word(0x0201_2c28, 0)?;
            let value = memory.read_halfword(0x0201_2be2)?;
            let complete = if value as i16 <= 255 {
                let next = value.wrapping_add(32);
                memory.write_halfword(0x0201_2be2, next)?;
                let other = memory.read_halfword(0x0201_2be4)?.wrapping_add(30);
                memory.write_halfword(0x0201_2be4, other)?;
                next as i16 > 255
            } else {
                true
            };
            if complete {
                memory.write_halfword(0x0201_2be4, 256)?;
                memory.write_halfword(0x0201_2be2, 256)?;
                selection_pop(memory)?;
            }
            Ok(())
        }
        0x0801_9dfd => selection_palette_intro(memory, calls),
        _ => Err(IntroError::UnportedCall(IntroCall::Callback {
            callback_id,
            arguments: [context],
        })
        .into()),
    }
}
fn selection_palette_intro(
    memory: &mut dyn IntroMemory,
    calls: &mut dyn ModeMenuCalls,
) -> Result<(), ModeMenuError> {
    if memory.read_halfword(0x0201_2bc2)? as i16 == 0 {
        invoke(calls, memory, 0x37d4, &[0, 2])?;
    }
    let frame = memory.read_halfword(0x0201_2bc2)?;
    let requests: &[(u32, u32)] = match frame {
        10..=19 => &[(0x082f_458c, 32)],
        20..=29 => &[(0x082f_45ac, 32)],
        30..=39 => &[(0x082f_428c, 0), (0x0830_4720, 288)],
        40..=49 => &[(0x082f_42ac, 0), (0x082f_3f2c, 80)],
        50..=59 => &[(0x082f_42cc, 0), (0x082f_468c, 48), (0x082f_474c, 64)],
        60..=69 => &[(0x082f_42ec, 0), (0x082f_3eac, 16), (0x082f_3f4c, 96)],
        70..=79 => &[(0x082f_448c, 32)],
        _ => &[],
    };
    for &(source, start) in requests {
        invoke(calls, memory, 0x5724, &[source, start, 16])?;
    }
    let next = memory.read_halfword(0x0201_2bc2)?.wrapping_add(1);
    memory.write_halfword(0x0201_2bc2, next)?;
    if next as i16 > 79 {
        memory.write_halfword(0x0201_2bc0, 1)?;
        let destination = selection_tile_destination(memory)?;
        let source = selection_cache(memory.read_halfword(SELECTION)?, 0x0202_19a0, 0xc00)?;
        selection_copy(calls, memory, destination, source, 0xc00)?;
        selection_pop(memory)?;
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
    fn fixture() -> Value {
        serde_json::from_str(vectors!("mode-menu-vectors.json")).unwrap()
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
    }
    impl Memory {
        fn new(case: &Value) -> Self {
            let mut memory = Self {
                ewram: pattern(integer(&case["seed"]), 0x40000),
                iwram: pattern(integer(&case["seed"]) ^ 0xa55a_1234, 0x8000),
            };
            for (address, value) in [
                (MAIN_STACK, 1),
                (MAIN_STACK + 0x2c, integer(&case["mainParameter"]) as u16),
                (AUXILIARY_STACK, integer(&case["depth"]) as u16),
                (SELECTION, integer(&case["selection"]) as u16),
                (EXIT_FADE, integer(&case["fade"]) as u16),
                (0x0300_3060, integer(&case["language"]) as u16),
            ] {
                memory.write_halfword(address, value).unwrap();
            }
            for (address, value) in [
                (MAIN_STACK + 4, CALLBACK_ID),
                (CONTEXT_SLOT, CONTEXT),
                (CONTEXT, 0),
                (0x0201_2c30, integer(&case["resumeSaved"])),
                (0x0300_40b0, 0x0300_2980),
                (0x0300_40b4, 0x0203_0000),
            ] {
                memory.write_word(address, value).unwrap();
            }
            memory
                .write_halfword(
                    0x0300_3064,
                    integer(&case["savedByte0"]) as u16
                        | ((integer(&case["savedByte1"]) as u16) << 8),
                )
                .unwrap();
            memory
        }
        fn span(&self, address: u32, length: usize) -> Result<&[u8], IntroError> {
            for (base, region) in [(0x0200_0000, &self.ewram), (0x0300_0000, &self.iwram)] {
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
    struct Calls {
        log: Vec<IntroCall>,
        after_pointer: u32,
        completion: u32,
        random: u32,
        unlock: u32,
    }
    impl Calls {
        fn new(case: &Value) -> Self {
            Self {
                log: Vec::new(),
                after_pointer: integer(&case["afterUpdatePointer"]),
                completion: integer(&case["completion"]),
                random: integer(&case["randomResult"]),
                unlock: case["unlockResult"].as_u64().unwrap_or(0) as u32,
            }
        }
    }
    impl ModeMenuCalls for Calls {
        fn invoke(
            &mut self,
            call: IntroCall,
            memory: &mut dyn IntroMemory,
        ) -> Result<u32, ModeMenuError> {
            let mut result = 0;
            if let IntroCall::Direct { entry_offset, .. } = &call {
                if *entry_offset == 0x1b73c {
                    memory.write_word(CONTEXT_SLOT, self.after_pointer)?;
                    memory.write_word(self.after_pointer, self.completion)?;
                }
                if *entry_offset == 0x36d4 {
                    result = self.random;
                }
                if *entry_offset == 0x49be8 {
                    result = self.unlock;
                }
            }
            self.log.push(call);
            Ok(result)
        }
    }
    struct Lookup(Vec<u8>);
    impl IntroSequenceResources for Lookup {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
            if address != LOOKUP_TABLE || length != 20 {
                return Err(crate::binary::Error::Range.into());
            }
            Ok(self.0.clone())
        }
    }

    struct Background([u32; 8]);
    impl IntroSequenceResources for Background {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
            let bytes: Vec<_> = self
                .0
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let offset = address
                .checked_sub(0x0806_6c54)
                .ok_or(IntroError::Resource(crate::binary::Error::Range))?
                as usize;
            Ok(crate::binary::span(&bytes, offset, length)?.to_vec())
        }
    }
    fn selection_memory(case: &Value) -> Memory {
        let mut memory = Memory::new(case);
        memory.write_bytes(0x0201_2bc0, &[0; 0x80]).unwrap();
        memory.write_word(CONTEXT_SLOT, CONTEXT).unwrap();
        for (address, field) in [
            (0x0300_0516, "pressed"),
            (0x0300_0514, "held"),
            (0x0201_2c3a, "repeat"),
            (0x0201_2c08, "previous"),
            (SELECTION, "selection"),
            (0x0201_2c0c, "next"),
            (0x0201_2c0e, "incoming"),
            (0x0201_2bc2, "paletteFrame"),
            (0x0201_2be2, "slideScale"),
            (0x0201_2be4, "slidePosition"),
            (0x0201_2bce, "leftScale"),
            (0x0201_2bd0, "middleScale"),
            (0x0201_2bd2, "rightScale"),
            (0x0201_2be6, "leftX"),
            (0x0201_2be8, "middleX"),
            (0x0201_2bea, "rightX"),
            (0x0201_2bec, "edgeX"),
        ] {
            memory
                .write_halfword(address, integer(&case[field]) as u16)
                .unwrap();
        }
        for (address, value) in [
            (0x0201_2bd4, 128),
            (0x0201_2bee, 98),
            (0x0201_2bf0, 86),
            (0x0201_2bf2, 98),
            (0x0201_2bf4, 98),
        ] {
            memory.write_halfword(address, value).unwrap();
        }
        for (address, field) in [
            (0x0201_2bf8, "movingLeft"),
            (0x0201_2bfc, "movingRight"),
            (0x0203_1a40, "accepted"),
        ] {
            memory.write_word(address, integer(&case[field])).unwrap();
        }
        let display = memory.read_halfword(DISPLAY_SHADOW).unwrap();
        memory
            .write_halfword(
                DISPLAY_SHADOW,
                (display & !7) | integer(&case["mode"]) as u16,
            )
            .unwrap();
        let depth = integer(&case["depth"]);
        if depth != 0 {
            memory
                .write_halfword(
                    AUXILIARY_STACK + 0x2c + (depth - 1) * 2,
                    integer(&case["parameter"]) as u16,
                )
                .unwrap();
        }
        memory
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn actual_selection_input_animation_and_auxiliaries_match_native_snapshots() {
        let data = fixture();
        let resources = Background(std::array::from_fn(|index| {
            integer(&data["backgroundDestinations"][index])
        }));
        for (index, case) in data["selectionCases"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let mut memory = selection_memory(case);
            let mut calls = Calls::new(case);
            let routine = case["routine"].as_str().unwrap();
            if routine == "selection" {
                selection_update(CONTEXT, &mut memory, &resources, &mut calls).unwrap();
            } else {
                selection_auxiliary(
                    0x0800_0001 + integer(&data["routines"][routine]["offset"]),
                    CONTEXT,
                    &mut memory,
                    &resources,
                    &mut calls,
                )
                .unwrap();
            }
            assert_eq!(
                serde_json::to_value(&calls.log).unwrap(),
                case["calls"],
                "selection case{index}:{routine}"
            );
            memory.assert_snapshot(case);
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn held_direction_sequences_preserve_movement_completion_repeat_delay_and_wrapping() {
        let data = fixture();
        let resources = Background(std::array::from_fn(|index| {
            integer(&data["backgroundDestinations"][index])
        }));
        for sequence in data["holdSequences"].as_array().unwrap() {
            let initial = &sequence["initial"];
            let mut memory = selection_memory(initial);
            for (index, tick) in sequence["ticks"].as_array().unwrap().iter().enumerate() {
                let mut calls = Calls::new(initial);
                selection_update(CONTEXT, &mut memory, &resources, &mut calls).unwrap();
                assert_eq!(
                    serde_json::to_value(calls.log).unwrap(),
                    tick["calls"],
                    "held tick{index}"
                );
                memory.assert_snapshot(tick);
            }
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn original_mode_wrappers_match_transition_targets_save_flags_rng_and_call_order() {
        let data = fixture();
        for &(name, offset, length, hash) in &SOURCE_SPANS {
            assert_eq!(integer(&data["routines"][name]["offset"]) as usize, offset);
            assert_eq!(integer(&data["routines"][name]["length"]) as usize, length);
            assert_eq!(data["routines"][name]["sha256"], hash);
        }
        for (index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::new(case);
            let mut calls = Calls::new(case);
            match case["routine"].as_str().unwrap() {
                "entry" => entry(&mut memory, &mut calls).unwrap(),
                "resume" => resume(CONTEXT, &mut memory, &mut calls).unwrap(),
                "resumeSecond" => resume_mode(CONTEXT, false, &mut memory, &mut calls).unwrap(),
                "resumeThird" => resume_mode(CONTEXT, true, &mut memory, &mut calls).unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(
                serde_json::to_value(calls.log).unwrap(),
                case["calls"],
                "case{index}"
            );
            memory.assert_snapshot(case);
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn lookup_is_bounded_and_preserves_native_special_indices() {
        let data = fixture();
        let bytes: Vec<_> = data["lookup"]["syntheticValues"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|value| (integer(value) as u16).to_le_bytes())
            .collect();
        let resources = Lookup(bytes);
        for case in data["lookup"]["cases"].as_array().unwrap() {
            assert_eq!(
                lookup(integer(&case["input"]) as u16, &resources).unwrap(),
                integer(&case["output"]) as u16
            );
        }
        assert_eq!(
            lookup(0xffff, &resources),
            Err(ModeMenuError::LookupValue(0xffff))
        );
        assert_eq!(
            lookup(0, &Lookup(Vec::new())),
            Err(ModeMenuError::Intro(IntroError::Resource(
                crate::binary::Error::Range
            )))
        );
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn strict_native_boundaries_preserve_only_pre_call_mutations() {
        let data = fixture();
        let case = &data["cases"][0];
        let mut memory = Memory::new(case);
        let context_before = memory.read_word(CONTEXT).unwrap();
        assert_eq!(
            entry(&mut memory, &mut RejectUnportedModeCalls),
            Err(ModeMenuError::Intro(IntroError::UnportedCall(
                IntroCall::Direct {
                    entry_offset: 0x1a100,
                    arguments: vec![CONTEXT]
                }
            )))
        );
        assert_eq!(memory.read_halfword(MAIN_STACK + 0x2c).unwrap(), 4);
        assert_eq!(memory.read_word(CONTEXT_SLOT).unwrap(), CONTEXT);
        assert_eq!(memory.read_word(CONTEXT).unwrap(), context_before);
        memory.write_halfword(MAIN_STACK, 0).unwrap();
        assert_eq!(
            entry(&mut memory, &mut RejectUnportedModeCalls),
            Err(ModeMenuError::Intro(IntroError::AuxiliaryDepth(0)))
        );
        assert!(matches!(ModeMenuRom::new(&[]), Err(IntroError::WrongRom)));
        assert_eq!(
            selection_cache(7, 0x0202_79a0, 0x800),
            Err(ModeMenuError::LookupValue(7))
        );
        assert!(matches!(
            selection_auxiliary(
                0x0801_5f3d,
                CONTEXT,
                &mut memory,
                &Lookup(Vec::new()),
                &mut RejectUnportedModeCalls
            ),
            Err(ModeMenuError::Intro(IntroError::UnportedCall(
                IntroCall::Callback { .. }
            )))
        ));
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; Garden payload is decoded and hashed, never executed"]
    fn authentic_source_lookup_and_garden_payload_hashes_remain_local() {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("SetSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        let resources = ModeMenuRom::new(&bytes).unwrap();
        let data = fixture();
        for span in data["paletteResources"]
            .as_array()
            .unwrap()
            .iter()
            .chain(std::iter::once(&data["backgroundLookup"]))
        {
            let offset = (integer(&span["address"]) - 0x0800_0000) as usize;
            assert_eq!(
                crate::sha256(&bytes[offset..offset + integer(&span["length"]) as usize]),
                span["sha256"].as_str().unwrap()
            );
        }
        for source in data["routines"].as_object().unwrap().values() {
            let offset = integer(&source["offset"]) as usize;
            let length = integer(&source["length"]) as usize;
            assert_eq!(
                crate::sha256(&bytes[offset..offset + length]),
                source["sha256"].as_str().unwrap()
            );
        }
        let table = resources.read_bytes(LOOKUP_TABLE, 20).unwrap();
        assert_eq!(
            crate::sha256(&table),
            data["lookup"]["sha256"].as_str().unwrap()
        );
        assert!(resources.read_bytes(0x087f_fffe, 4).is_err());
        assert!(resources
            .read_bytes(LOOKUP_TABLE, MAX_RESOURCE_BYTES + 1)
            .is_err());
        let asset = &data["authenticGardenPayload"];
        assert_eq!(integer(&asset["sourceAddress"]), GARDEN_PAYLOAD_ADDRESS);
        assert_eq!(
            integer(&asset["storedLength"]) as usize,
            GARDEN_STORED_LENGTH
        );
        assert_eq!(asset["storedSha256"], GARDEN_STORED_SHA256);
        assert_eq!(
            integer(&asset["decodedLength"]) as usize,
            GARDEN_DECODED_LENGTH
        );
        assert_eq!(asset["decodedSha256"], GARDEN_DECODED_SHA256);
        let offset = (integer(&asset["sourceAddress"]) - 0x0800_0000) as usize;
        let decoded = crate::binary::decode(&bytes[offset..], 0x40000).unwrap();
        assert_eq!(
            decoded.bytes.len(),
            integer(&asset["decodedLength"]) as usize
        );
        assert_eq!(decoded.consumed, integer(&asset["storedLength"]) as usize);
        assert_eq!(
            crate::sha256(&decoded.bytes),
            asset["decodedSha256"].as_str().unwrap()
        );
        assert_eq!(
            crate::sha256(&bytes[offset..offset + decoded.consumed]),
            asset["storedSha256"].as_str().unwrap()
        );
    }
}
