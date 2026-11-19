//! Native A3VE title callbacks, resource setup and input/state transitions.
//!
//! Original resources remain local. Frame rendering, affine transforms, palette
//! and audio calls are strict dependencies. The completed interpolation/OAM
//! ports are reused for the independently checked 0x8260/0x8358 copies.

use super::control::{CallbackId, CallbackStack};
use super::interpolation::{ConstructArgs, InterpolationState};
use super::intro::{
    IntroCall, IntroCalls, IntroError, IntroMemory, IntroResources, IntroRom, DISPLAY_SHADOW,
    MAX_RESOURCE_BYTES, VRAM_WINDOW_BYTES,
};
use super::intro_sequence::{IntroSequenceError, IntroSequenceResources};
use super::oam::{
    OamSubmissionQueue, CAPACITY, COUNT_ADDRESS, ORDER_BASE, RECORD_BASE, RECORD_WORDS,
};
use super::oam_render::DescriptorSource;

pub type TitleError = IntroSequenceError;
pub const CALLBACK_ID: u32 = 0x0800_91c5;
pub const NEXT_CALLBACK: u32 = 0x0801_cf4d;
pub const CONTEXT_SLOT: u32 = 0x0200_2320;
pub const CONTEXT: u32 = 0x0201_99a0;
pub const PARTICLES: u32 = 0x0200_21a0;
pub const MAIN_STACK: u32 = 0x0300_0e00;
pub const AUXILIARY_STACK: u32 = 0x0300_2060;
pub const PRESSED: u32 = 0x0300_0516;
pub const SCROLL_TABLE: u32 = 0x0806_a34c;
pub const SOURCE_SPANS: [(&str, usize, usize, &str); 15] = [
    (
        "constructor",
        0x8260,
        0xf8,
        "20f0aee31ce1d955ce0f937d5a505594576d7c2a7f77817f0a97241f70566033",
    ),
    (
        "interpolation",
        0x8358,
        0xbc,
        "6be9df84ca1afb15d80592c35a31e0c26f3a80c83336427a28723671272fad97",
    ),
    (
        "paletteFade",
        0x8414,
        0x3c,
        "5aaf676095101137f31e063b0cab70aee985a2fe64dc3b80681071ca6f1ce206",
    ),
    (
        "specialFade",
        0x8450,
        0x44,
        "6080092411e89b5590b0d9135e9694b338ad1de08bfdc5156c40f220bc6ecf4b",
    ),
    (
        "timeoutFade",
        0x8494,
        0x8c,
        "1d41bba6a53804cd6132bd16a158efa1649778fb0a3087ffeb6275af60e3c35f",
    ),
    (
        "idle",
        0x8520,
        0x38c,
        "7b03442714d8f30d6deac044680c6f8d0506ddd3900c6c0ea5db9c006e5213e9",
    ),
    (
        "localizedResources",
        0x88ac,
        0x158,
        "2a98cd6b2d996c14a60f97a9b51f25d9dd506fc9efd5dbb88a4c00c56ae3c76b",
    ),
    (
        "scroll",
        0x8a04,
        0xa8,
        "b224663ed3f7e7648a47a1622f64a88f2e5d730209940fa78ca45f0de8430625",
    ),
    (
        "wait",
        0x8aac,
        0x34,
        "102d961982ee56340786865cb323c42e4f0fb057b33685bb4bd17f0b81b5fa85",
    ),
    (
        "finish",
        0x8ae0,
        0x40,
        "974c2893370f2fe7848a21a6b5f8e914c2315e58df586c81a8af594ef88d53fd",
    ),
    (
        "initialize",
        0x8b20,
        0x5ac,
        "83d672253cdb7d97fa94847b2251b74c72f820d82c2e2d6860c0325cb4c082f7",
    ),
    (
        "resume",
        0x90cc,
        0xc,
        "20c0317cdef25e3c5d803d63095c2d27c6cb97d1dcac6ef76fb999aebc269d91",
    ),
    (
        "restart",
        0x90d8,
        0x28,
        "682569eb547d69f1056377127e922093a1e83390f8f3935b4167c0a94b138130",
    ),
    (
        "update",
        0x9100,
        0xc4,
        "2e1755108431f28d2b73210c4b80a4760e1284ae560fa8637fa7dffe324fb452",
    ),
    (
        "entry",
        0x91c4,
        0xa0,
        "3f0eb4fb5d93aa1c18cca186038ba5b79cf1a96d3975ad77def00136309f1631",
    ),
];
pub const INSTALLED: [u32; 5] = [
    0x0800_8ae1,
    0x0800_8521,
    0x0800_8415,
    0x0800_88ad,
    0x0800_8a05,
];
pub const ENDPOINTS: [[i16; 2]; 12] = [
    [-32, -32],
    [58, -32],
    [144, -32],
    [240, -32],
    [-32, 44],
    [240, 28],
    [-32, 86],
    [240, 90],
    [-34, 120],
    [50, 160],
    [144, 160],
    [210, 150],
];

pub trait TitleResources: IntroResources + IntroSequenceResources {}
#[derive(Clone)]
pub struct TitleRom<'a> {
    rom: &'a [u8],
    intro: IntroRom<'a>,
}
impl<'a> TitleRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, IntroError> {
        Ok(Self {
            rom,
            intro: IntroRom::new(rom)?,
        })
    }
}
impl IntroResources for TitleRom<'_> {
    fn background_destination(&self, mode: u8) -> Result<u32, IntroError> {
        self.intro.background_destination(mode)
    }
    fn decode_lz77(&mut self, address: u32) -> Result<Vec<u8>, IntroError> {
        self.intro.decode_lz77(address)
    }
}
impl IntroSequenceResources for TitleRom<'_> {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
        if length > 0x48 {
            return Err(IntroError::ResourceLimit(length));
        }
        let offset = crate::binary::pointer_offset(address, 0x0800_0000, self.rom.len())?;
        Ok(crate::binary::span(self.rom, offset, length)?.to_vec())
    }
}
impl TitleResources for TitleRom<'_> {}

fn invoke(
    calls: &mut dyn IntroCalls,
    memory: &mut dyn IntroMemory,
    entry_offset: usize,
    arguments: &[u32],
) -> Result<(), TitleError> {
    calls.invoke(
        IntroCall::Direct {
            entry_offset,
            arguments: arguments.to_vec(),
        },
        memory,
    )?;
    Ok(())
}
fn stack(memory: &dyn IntroMemory, base: u32) -> Result<CallbackStack, TitleError> {
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
) -> Result<(), TitleError> {
    let bytes: Vec<_> = value
        .to_halfwords()
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    memory.write_bytes(base, &bytes)?;
    Ok(())
}
fn pop(memory: &mut dyn IntroMemory) -> Result<(), TitleError> {
    let mut value = stack(memory, AUXILIARY_STACK)?;
    value.pop();
    save_stack(memory, AUXILIARY_STACK, &value)
}
fn push(memory: &mut dyn IntroMemory, callback: u32) -> Result<(), TitleError> {
    let mut value = stack(memory, AUXILIARY_STACK)?;
    value.push(CallbackId(callback), 0);
    save_stack(memory, AUXILIARY_STACK, &value)
}
fn clear_auxiliary(memory: &mut dyn IntroMemory) -> Result<(), TitleError> {
    let mut value = stack(memory, AUXILIARY_STACK)?;
    value.clear();
    save_stack(memory, AUXILIARY_STACK, &value)
}
fn replace_main(
    memory: &mut dyn IntroMemory,
    callback: u32,
    parameter: u32,
) -> Result<(), TitleError> {
    let mut value = stack(memory, MAIN_STACK)?;
    value.replace(CallbackId(callback), parameter);
    save_stack(memory, MAIN_STACK, &value)
}
fn top_parameter(memory: &dyn IntroMemory, base: u32) -> Result<u32, TitleError> {
    let count = memory.read_halfword(base)?;
    if !(1..=10).contains(&count) {
        return Err(IntroError::AuxiliaryDepth(count).into());
    }
    Ok(base + 0x2c + u32::from(count - 1) * 2)
}
fn copy_bytes(
    memory: &mut dyn IntroMemory,
    destination: u32,
    bytes: &[u8],
) -> Result<(), TitleError> {
    if !destination
        .checked_sub(0x0600_0000)
        .and_then(|offset| (offset as usize).checked_add(bytes.len()))
        .is_some_and(|end| end <= VRAM_WINDOW_BYTES)
    {
        return Err(IntroError::InvalidResourceDestination(destination).into());
    }
    memory.write_bytes(destination, bytes)?;
    Ok(())
}
fn resource(
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    source: u32,
    destination: u32,
) -> Result<(), TitleError> {
    let bytes = resources.decode_lz77(source)?;
    if bytes.len() > MAX_RESOURCE_BYTES {
        return Err(IntroError::ResourceLimit(bytes.len()).into());
    }
    copy_bytes(memory, destination, &bytes)
}
fn tile_destination(memory: &dyn IntroMemory, offset: u32) -> Result<u32, TitleError> {
    Ok(0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + offset)? & 0x0c) << 12))
}
fn screen_destination(memory: &dyn IntroMemory, offset: u32) -> Result<u32, TitleError> {
    Ok(0x0600_0000 | (u32::from(memory.read_halfword(DISPLAY_SHADOW + offset)? & 0x1f00) << 3))
}
fn language(memory: &dyn IntroMemory) -> Result<u16, TitleError> {
    Ok(memory.read_halfword(memory.read_word(0x0300_40b0)?.wrapping_add(0x6e0))?)
}

fn construct_particles(
    memory: &mut dyn IntroMemory,
    stack_prefix: Option<u32>,
) -> Result<(), TitleError> {
    let prefix = stack_prefix.ok_or(IntroError::MissingConstructorStackPrefix)?;
    for (index, endpoint) in ENDPOINTS.iter().enumerate() {
        let state = InterpolationState::construct(
            ConstructArgs {
                start_x: 120,
                start_y: 80,
                end_x: endpoint[0] as u16 as u32,
                end_y: endpoint[1] as u16 as u32,
                descriptor_address: if index % 2 == 0 {
                    0x086e_7124
                } else {
                    0x086e_712c
                },
                flags: 1,
                speed: 1,
            },
            prefix,
        );
        memory.write_bytes(PARTICLES + index as u32 * 32, state.as_bytes())?;
    }
    Ok(())
}
fn read_queue(memory: &dyn IntroMemory) -> Result<OamSubmissionQueue, TitleError> {
    let mut records = [[0; RECORD_WORDS]; CAPACITY];
    for (index, row) in records.iter_mut().enumerate() {
        for (word, value) in row.iter_mut().enumerate() {
            *value = memory.read_word(RECORD_BASE + (index * 28 + word * 4) as u32)?;
        }
    }
    let mut order = [0; CAPACITY + 1];
    for (index, value) in order.iter_mut().enumerate() {
        *value = memory.read_word(ORDER_BASE + index as u32 * 4)?;
    }
    Ok(OamSubmissionQueue::from_words(
        memory.read_halfword(COUNT_ADDRESS)? as u8,
        &records,
        &order,
    )?)
}
struct Descriptors<'a>(&'a dyn TitleResources);
impl DescriptorSource for Descriptors<'_> {
    fn halfword(&self, address: u32) -> Option<u16> {
        self.0.read_halfword(address).ok()
    }
}
fn interpolate(
    memory: &mut dyn IntroMemory,
    resources: &dyn TitleResources,
    slot: usize,
) -> Result<bool, TitleError> {
    let address = PARTICLES + slot as u32 * 32;
    let mut bytes = [0; 32];
    for (index, chunk) in bytes.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        chunk.copy_from_slice(
            &memory
                .read_halfword(address + index as u32 * 2)?
                .to_le_bytes(),
        );
    }
    let mut state = InterpolationState::from_bytes(&bytes)?;
    let mut queue = read_queue(memory)?;
    let old_count = queue.count();
    let complete = state.update(&mut queue, &Descriptors(resources))?;
    memory.write_bytes(address, state.as_bytes())?;
    if queue.count() != old_count {
        let old = memory.read_halfword(COUNT_ADDRESS)?;
        memory.write_halfword(COUNT_ADDRESS, (old & 0xff00) | u16::from(queue.count()))?;
        memory.write_word(
            ORDER_BASE + u32::from(queue.count()) * 4,
            queue.order()[usize::from(queue.count())],
        )?;
        let bytes: Vec<_> = queue.records()[usize::from(old_count)]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        memory.write_bytes(RECORD_BASE + u32::from(old_count) * 28, &bytes)?;
    }
    Ok(complete)
}

/// Original8B20. Constructor residue is required exactly where native8260 reads
/// its uninitialized stack-prefix word; None never supplies a zero default.
pub fn initialize(
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    constructor_stack_prefix: Option<u32>,
) -> Result<(), TitleError> {
    memory.write_word(context, 0)?;
    memory.write_word(context.wrapping_add(4), 0)?;
    memory.write_halfword(0x0200_232c, 0)?;
    memory.write_word(0x0203_19a4, 0)?;
    let display = memory.read_halfword(DISPLAY_SHADOW)?;
    memory.write_halfword(DISPLAY_SHADOW, display & 0xe0ff)?;
    let display = memory.read_halfword(DISPLAY_SHADOW)?;
    memory.write_halfword(DISPLAY_SHADOW, display | 0x1740)?;
    for (offset, value) in [
        (0x4a, 0),
        (0x4c, 0),
        (4, 0x1a00),
        (6, 0x9e01),
        (8, 0x9c02),
        (10, 0x1b0b),
        (0x14, 0),
        (0x16, 160),
        (0x18, 96),
        (0x1a, 0),
        (0xc, 0),
        (0xe, 0),
        (0x10, 0),
        (0x12, 0),
    ] {
        memory.write_halfword(DISPLAY_SHADOW + offset, value)?;
    }
    invoke(calls, memory, 0x30fc, &[])?;
    for address in [
        0x0200_2324,
        0x0200_2338,
        0x0200_233a,
        0x0200_233c,
        0x0200_233e,
        0x0300_2948,
        0x0300_3068,
        0x0300_2960,
        0x0300_2968,
        0x0300_294c,
    ] {
        memory.write_halfword(address, 0)?;
    }
    for address in [0x0200_2328, 0x0200_2330, 0x0200_2334] {
        memory.write_word(address, 0)?;
    }
    memory.write_word(0x0300_2940, 1)?;
    memory.write_bytes(context.wrapping_add(8), &[0; 0x800])?;
    construct_particles(memory, constructor_stack_prefix)?;
    match language(memory)? {
        0 => {
            let destination = tile_destination(memory, 4)?;
            resource(memory, resources, 0x082d_7808, destination)?;
            let destination = screen_destination(memory, 6)?;
            resource(memory, resources, 0x082d_c684, destination)?;
        }
        1..=5 => {
            let destination = tile_destination(memory, 4)?;
            resource(memory, resources, 0x082d_87e0, destination)?;
            let destination = screen_destination(memory, 6)?;
            resource(memory, resources, 0x082d_c974, destination)?;
        }
        _ => {}
    }
    let destination = tile_destination(memory, 4)?.wrapping_add(0x2000);
    resource(memory, resources, 0x082d_9924, destination)?;
    let destination = tile_destination(memory, 10)?;
    resource(memory, resources, 0x082d_cf1c, destination)?;
    let destination =
        resources.background_destination((memory.read_halfword(DISPLAY_SHADOW)? & 7) as u8)?;
    resource(memory, resources, 0x082e_18d4, destination)?;
    let source: Vec<_> = (0..0x400)
        .map(|index| memory.read_halfword(context.wrapping_add(8 + index * 2)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    for offset in [0x4800, 0x4c00, 0x4000] {
        let destination = tile_destination(memory, 4)?.wrapping_add(offset);
        copy_bytes(memory, destination, &source)?;
    }
    let destination = screen_destination(memory, 4)?;
    resource(memory, resources, 0x082e_0adc, destination)?;
    let destination = screen_destination(memory, 6)?.wrapping_add(0x800);
    copy_bytes(memory, destination, &source)?;
    let destination = screen_destination(memory, 8)?;
    copy_bytes(memory, destination, &source)?;
    let destination = screen_destination(memory, 8)?.wrapping_add(0x800);
    resource(memory, resources, 0x082d_cc60, destination)?;
    let destination = screen_destination(memory, 10)?;
    resource(memory, resources, 0x082e_0530, destination)?;
    invoke(calls, memory, 0x5618, &[0x0300_00f0])?;
    invoke(calls, memory, 0x5784, &[0, 32, 32])?;
    invoke(calls, memory, 0x5784, &[12, 1, 64])?;
    match language(memory)? {
        0 => invoke(calls, memory, 0x5724, &[0x082e_0c54, 0, 256])?,
        1..=5 => invoke(calls, memory, 0x5724, &[0x082e_0e54, 0, 256])?,
        _ => {}
    }
    invoke(calls, memory, 0x5724, &[0x082e_19e0, 256, 256])?;
    for (entry, args) in [
        (0x5668, &[][..]),
        (0x1dc4, &[0][..]),
        (0x3250, &[][..]),
        (0x5f0bc, &[][..]),
        (0x37d4, &[0, 1][..]),
    ] {
        invoke(calls, memory, entry, args)?;
    }
    invoke(
        calls,
        memory,
        0x5ebd4,
        &[resources.read_word(0x086d_d878)?, 6],
    )?;
    clear_auxiliary(memory)?;
    for callback in INSTALLED {
        push(memory, callback)?;
    }
    Ok(())
}

pub fn resume(
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    prefix: Option<u32>,
) -> Result<(), TitleError> {
    initialize(context, memory, resources, calls, prefix)
}
pub fn restart(
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    prefix: Option<u32>,
) -> Result<(), TitleError> {
    for entry in [0x3250, 0x5f0bc, 0x0884, 0x5f040, 0x3278, 0x3010] {
        invoke(calls, memory, entry, &[])?;
    }
    initialize(context, memory, resources, calls, prefix)
}

/// Original9100 frame wrapper. Callback dispatch remains a typed dependency so
/// a host must route the exact installed ID through `auxiliary` or stop.
pub fn update(
    context: u32,
    memory: &mut dyn IntroMemory,
    calls: &mut dyn IntroCalls,
) -> Result<(), TitleError> {
    invoke(calls, memory, 0x1dc4, &[0])?;
    invoke(calls, memory, 0x2580, &[])?;
    if memory.read_word(0x0200_2330)? != 1 {
        memory.write_halfword(DISPLAY_SHADOW, 0x1e40)?;
    } else {
        let blink = memory.read_halfword(0x0200_2324)?;
        if blink.wrapping_sub(16) <= 15 {
            memory.write_halfword(DISPLAY_SHADOW, 0x1f40)?;
        } else if blink > 31 {
            memory.write_halfword(DISPLAY_SHADOW, 0x1e40)?;
            memory.write_halfword(0x0200_2324, 0)?;
        }
        if memory.read_word(0x0200_2328)? != 0 {
            let blink = memory.read_halfword(0x0200_2324)?;
            memory.write_halfword(0x0200_2324, blink.wrapping_add(8))?;
        }
        let blink = memory.read_halfword(0x0200_2324)?;
        memory.write_halfword(0x0200_2324, blink.wrapping_add(1))?;
        if memory.read_word(0x0200_2328)? != 1 {
            let timer = memory.read_halfword(0x0200_232c)?;
            memory.write_halfword(0x0200_232c, timer.wrapping_add(1))?;
        }
    }
    let active = stack(memory, AUXILIARY_STACK)?;
    if let Some(top) = active.top() {
        calls.invoke(
            IntroCall::Callback {
                callback_id: top.callback.0,
                arguments: [context],
            },
            memory,
        )?;
    }
    invoke(calls, memory, 0x5668, &[])?;
    invoke(calls, memory, 0x2700, &[])?;
    Ok(())
}

/// Original91C4 main callback. Completion targets actual mode-selection entry
/// 0x0801CF4D; the target is retained as a strict next-screen boundary.
pub fn entry(
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    prefix: Option<u32>,
) -> Result<(), TitleError> {
    let address = top_parameter(memory, MAIN_STACK)?;
    match memory.read_halfword(address)? as i16 {
        state @ 0..=2 => {
            memory.write_halfword(address, 3)?;
            memory.write_word(CONTEXT_SLOT, CONTEXT)?;
            match state {
                0 => initialize(CONTEXT, memory, resources, calls, prefix)?,
                1 => resume(CONTEXT, memory, resources, calls, prefix)?,
                _ => restart(CONTEXT, memory, resources, calls, prefix)?,
            }
        }
        _ => {
            update(memory.read_word(CONTEXT_SLOT)?, memory, calls)?;
            if memory.read_word(memory.read_word(CONTEXT_SLOT)?)? != 0 {
                replace_main(memory, NEXT_CALLBACK, 0)?;
                clear_auxiliary(memory)?;
            }
        }
    }
    Ok(())
}

/// Known native title auxiliaries only. Unknown IDs and missing resources stop
/// without substituting another screen, graphics, input or timing sequence.
pub fn auxiliary(
    callback: u32,
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    prefix: Option<u32>,
) -> Result<(), TitleError> {
    if !matches!(
        callback,
        0x0800_8415
            | 0x0800_8451
            | 0x0800_8495
            | 0x0800_8521
            | 0x0800_88ad
            | 0x0800_8a05
            | 0x0800_8aad
            | 0x0800_8ae1
    ) {
        return Err(IntroError::UnportedCall(IntroCall::Callback {
            callback_id: callback,
            arguments: [context],
        })
        .into());
    }
    let parameter_address = top_parameter(memory, AUXILIARY_STACK)?;
    match callback {
        0x0800_8415 | 0x0800_8451 | 0x0800_8495 | 0x0800_8ae1 => {
            let parameter = memory.read_halfword(parameter_address)?;
            memory.write_halfword(parameter_address, parameter.wrapping_add(1))?;
            if callback == 0x0800_8495
                && memory.read_halfword(PRESSED)? & 8 != 0
                && memory.read_word(context.wrapping_add(4))? != 1
            {
                invoke(calls, memory, 0x389c, &[53])?;
                memory.write_word(0x0200_2328, 1)?;
            }
            let arguments = if callback == 0x0800_8415 {
                [12, 1, u32::from(64u16.wrapping_sub(parameter) as u8)]
            } else {
                [0, 32, u32::from(32u16.wrapping_sub(parameter) as u8)]
            };
            invoke(calls, memory, 0x5784, &arguments)?;
            if parameter as i16 > 31 {
                match callback {
                    0x0800_8415 => pop(memory)?,
                    0x0800_8451 => replace_main(memory, 0x0805_a349, 1)?,
                    0x0800_8495 => {
                        if memory.read_word(0x0200_2328)? == 1 {
                            replace_main(memory, NEXT_CALLBACK, 0)?;
                        } else {
                            replace_main(memory, 0x0800_81e9, 1)?;
                        }
                        clear_auxiliary(memory)?;
                    }
                    _ => {
                        memory.write_word(context, 1)?;
                        pop(memory)?;
                    }
                }
            }
        }
        0x0800_88ad => {
            let destination = tile_destination(memory, 4)?.wrapping_add(0x4800);
            resource(memory, resources, 0x082d_c058, destination)?;
            let lang = language(memory)?;
            if lang <= 5 {
                let source = [
                    0x082d_acd4,
                    0x082d_b160,
                    0x082d_b414,
                    0x082d_b6cc,
                    0x082d_baa0,
                    0x082d_bd88,
                ][usize::from(lang)];
                let destination = tile_destination(memory, 4)?.wrapping_add(0x4000);
                resource(memory, resources, source, destination)?;
            }
            pop(memory)?;
        }
        0x0800_8a05 => {
            let table = resources.read_bytes(SCROLL_TABLE, 0x48)?;
            if table.len() != 0x48 {
                return Err(IntroError::Resource(crate::binary::Error::Range).into());
            }
            let vertical = memory.read_halfword(DISPLAY_SHADOW + 0x16)?;
            if vertical == 0 {
                let index = memory.read_halfword(0x0200_2338)?;
                if index >= 18 {
                    return Err(TitleError::TableIndex {
                        address: SCROLL_TABLE,
                        index: i32::from(index),
                        count: 18,
                    });
                }
                let amount = crate::binary::u32_le(&table, usize::from(index) * 4)
                    .map_err(IntroError::Resource)? as u16;
                for offset in [0xe, 0x10] {
                    let old = memory.read_halfword(DISPLAY_SHADOW + offset)?;
                    memory.write_halfword(DISPLAY_SHADOW + offset, old.wrapping_add(amount))?;
                }
                let next = memory.read_halfword(0x0200_2338)?.wrapping_add(1);
                memory.write_halfword(0x0200_2338, next)?;
                if next > 17 {
                    memory.write_word(0x0200_2334, 1)?;
                }
            } else {
                memory.write_halfword(DISPLAY_SHADOW + 0x16, vertical.wrapping_sub(4))?;
                let other = memory.read_halfword(DISPLAY_SHADOW + 0x18)?;
                memory.write_halfword(DISPLAY_SHADOW + 0x18, other.wrapping_add(4))?;
            }
            if memory.read_halfword(PRESSED)? != 0 || memory.read_word(0x0200_2334)? == 1 {
                memory.write_halfword(DISPLAY_SHADOW + 0x16, 0)?;
                memory.write_halfword(DISPLAY_SHADOW + 0x18, 256)?;
                pop(memory)?;
            }
        }
        0x0800_8aad => {
            let parameter = memory.read_halfword(parameter_address)?.wrapping_sub(1);
            memory.write_halfword(parameter_address, parameter)?;
            let address = top_parameter(memory, AUXILIARY_STACK)?;
            if memory.read_halfword(address)? as i16 <= 0 {
                pop(memory)?;
            }
        }
        0x0800_8521 => idle(context, memory, resources, calls, prefix)?,
        _ => unreachable!(),
    }
    Ok(())
}
fn idle(
    context: u32,
    memory: &mut dyn IntroMemory,
    resources: &mut dyn TitleResources,
    calls: &mut dyn IntroCalls,
    prefix: Option<u32>,
) -> Result<(), TitleError> {
    let delay = memory.read_halfword(0x0200_233e)?;
    if delay > 89 {
        memory.write_word(0x0200_2330, 1)?;
    } else {
        memory.write_halfword(0x0200_233e, delay.wrapping_add(1))?;
    }
    let ready = memory.read_word(0x0200_2330)?;
    if ready == 1 {
        if memory.read_halfword(PRESSED)? & 8 != 0
            && memory.read_word(context.wrapping_add(4))? != 1
        {
            invoke(calls, memory, 0x389c, &[53])?;
            pop(memory)?;
            memory.write_word(0x0200_2328, ready)?;
        }
        if memory.read_halfword(PRESSED)? & 2 != 0 {
            memory.write_halfword(0x0300_3068, 1)?;
            pop(memory)?;
            push(memory, 0x0800_8451)?;
        }
    }
    let angle = memory.read_halfword(0x0200_233a)?.wrapping_shl(1) as u8;
    invoke(calls, memory, 0x236c, &[0, u32::from(angle), 256, 256, 0])?;
    let angle = memory.read_halfword(0x0200_233a)?;
    memory.write_halfword(0x0200_233a, angle.wrapping_add(1))?;
    if memory.read_halfword(0x0200_233c)? == 0 {
        let mut complete = 0;
        for slot in [0, 2, 5, 6, 9, 11] {
            complete += u8::from(interpolate(memory, resources, slot)?);
        }
        if complete == 6 {
            memory.write_halfword(0x0200_233c, 1)?;
        }
    }
    if memory.read_halfword(0x0200_233c)? == 1 {
        let mut complete = 0;
        for slot in [1, 3, 4, 7, 8, 10] {
            complete += u8::from(interpolate(memory, resources, slot)?);
        }
        if complete == 6 {
            memory.write_halfword(0x0200_233c, 0)?;
            construct_particles(memory, prefix)?;
        }
    }
    if memory.read_halfword(0x0200_232c)? > 1319 && memory.read_word(0x0200_2328)? != 1 {
        pop(memory)?;
        push(memory, 0x0800_8495)?;
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
        serde_json::from_str(include_str!("../../tests/title-vectors.json")).unwrap()
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
        fn new(case: &Value, data: &Value) -> Self {
            let seed = integer(&case["seed"]);
            let mut result = Self {
                ewram: pattern(seed, 0x40000),
                iwram: pattern(seed ^ 0xa55a_1234, 0x8000),
                vram: pattern(seed ^ 0x7ace_1200, 0x20000),
            };
            construct_particles(&mut result, Some(integer(&data["initialParticlePrefix"])))
                .unwrap();
            if case["allComplete"].as_bool().unwrap() {
                for index in 0..12 {
                    result
                        .write_halfword(PARTICLES + index * 32 + 0x1c, 0xffff)
                        .unwrap();
                }
            }
            for (address, field) in [
                (0x0300_3060, "language"),
                (0x0300_0e2c, "mainParameter"),
                (AUXILIARY_STACK, "depth"),
                (0x0200_2324, "blink"),
                (0x0200_232c, "idleTimer"),
                (0x0200_2338, "scrollIndex"),
                (0x0200_233a, "angle"),
                (0x0200_233c, "phase"),
                (0x0200_233e, "delay"),
                (DISPLAY_SHADOW + 0x16, "vertical"),
                (DISPLAY_SHADOW + 0x18, "otherVertical"),
                (PRESSED, "pressed"),
            ] {
                result
                    .write_halfword(address, integer(&case[field]) as u16)
                    .unwrap();
            }
            result.write_halfword(MAIN_STACK, 1).unwrap();
            for (address, value) in [
                (0x0300_40b0, 0x0300_2980),
                (MAIN_STACK + 4, CALLBACK_ID),
                (CONTEXT_SLOT, CONTEXT),
                (CONTEXT, integer(&case["completion"])),
                (CONTEXT + 4, integer(&case["contextFlag"])),
                (0x0200_2330, integer(&case["ready"])),
                (0x0200_2328, integer(&case["accepted"])),
                (0x0200_2334, integer(&case["scrollDone"])),
            ] {
                result.write_word(address, value).unwrap();
            }
            let display = result.read_halfword(DISPLAY_SHADOW).unwrap();
            result
                .write_halfword(
                    DISPLAY_SHADOW,
                    (display & !7) | integer(&case["mode"]) as u16,
                )
                .unwrap();
            let count = result.read_halfword(COUNT_ADDRESS).unwrap();
            result
                .write_halfword(
                    COUNT_ADDRESS,
                    (count & 0xff00) | integer(&case["queueCount"]) as u16,
                )
                .unwrap();
            let depth = integer(&case["depth"]);
            if depth != 0 {
                result
                    .write_halfword(
                        AUXILIARY_STACK + 0x2c + (depth - 1) * 2,
                        integer(&case["parameter"]) as u16,
                    )
                    .unwrap();
                result
                    .write_word(AUXILIARY_STACK + 4 + (depth - 1) * 4, 0x0300_6101)
                    .unwrap();
            }
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
    struct Calls(Vec<IntroCall>);
    impl IntroCalls for Calls {
        fn invoke(
            &mut self,
            call: IntroCall,
            _memory: &mut dyn IntroMemory,
        ) -> Result<(), IntroError> {
            self.0.push(call);
            Ok(())
        }
    }
    struct SyntheticResources {
        lookup: [u32; 8],
        player: u32,
        lengths: Vec<(u32, usize)>,
        requests: Vec<(u32, usize)>,
    }
    impl SyntheticResources {
        fn new(data: &Value) -> Self {
            Self {
                lookup: std::array::from_fn(|index| {
                    integer(&data["scalarResources"]["backgroundDestinations"][index])
                }),
                player: integer(&data["scalarResources"]["titleAudioPlayer"]),
                lengths: data["authenticAssets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|asset| {
                        (
                            integer(&asset["sourceAddress"]),
                            integer(&asset["decodedLength"]) as usize,
                        )
                    })
                    .collect(),
                requests: Vec::new(),
            }
        }
    }
    fn synthetic_asset(source: u32, length: usize) -> Vec<u8> {
        (0..length)
            .map(|index| ((source >> 1).wrapping_add((index as u32).wrapping_mul(37))) as u8)
            .collect()
    }
    impl IntroResources for SyntheticResources {
        fn background_destination(&self, mode: u8) -> Result<u32, IntroError> {
            Ok(self.lookup[usize::from(mode & 7)])
        }
        fn decode_lz77(&mut self, address: u32) -> Result<Vec<u8>, IntroError> {
            let length = self
                .lengths
                .iter()
                .find(|row| row.0 == address)
                .ok_or(IntroError::Resource(crate::binary::Error::Range))?
                .1;
            self.requests.push((address, length));
            Ok(synthetic_asset(address, length))
        }
    }
    impl IntroSequenceResources for SyntheticResources {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
            let table: Vec<_> = (0..18u32)
                .flat_map(|index| (1 + index % 3).to_le_bytes())
                .collect();
            let player = self.player.to_le_bytes();
            for (base, bytes) in [
                (SCROLL_TABLE, table.as_slice()),
                (0x086d_d878, player.as_slice()),
                (0x086e_7128, &[0, 0][..]),
                (0x086e_7130, &[0, 4][..]),
            ] {
                if let Some(start) = address.checked_sub(base).map(|offset| offset as usize) {
                    if let Some(end) = start.checked_add(length) {
                        if let Some(found) = bytes.get(start..end) {
                            return Ok(found.to_vec());
                        }
                    }
                }
            }
            Err(crate::binary::Error::Range.into())
        }
    }
    impl TitleResources for SyntheticResources {}
    fn prefix(case: &Value) -> Option<u32> {
        case["constructorPrefix"].as_u64().map(|value| value as u32)
    }
    fn expected_requests(case: &Value) -> Vec<(u32, usize)> {
        case["resourceRequests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    integer(&row["sourceAddress"]),
                    integer(&row["length"]) as usize,
                )
            })
            .collect()
    }

    #[test]
    fn all_title_routines_match_native_ram_input_edges_calls_and_resource_transactions() {
        let data = fixture();
        for &(name, offset, length, hash) in &SOURCE_SPANS {
            assert_eq!(integer(&data["routines"][name]["offset"]) as usize, offset);
            assert_eq!(integer(&data["routines"][name]["length"]) as usize, length);
            assert_eq!(data["routines"][name]["sha256"], hash);
        }
        for (index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::new(case, &data);
            let mut resources = SyntheticResources::new(&data);
            let mut calls = Calls::default();
            let routine = case["routine"].as_str().unwrap();
            match routine {
                "initialize" => initialize(
                    CONTEXT,
                    &mut memory,
                    &mut resources,
                    &mut calls,
                    prefix(case),
                )
                .unwrap(),
                "resume" => resume(
                    CONTEXT,
                    &mut memory,
                    &mut resources,
                    &mut calls,
                    prefix(case),
                )
                .unwrap(),
                "restart" => restart(
                    CONTEXT,
                    &mut memory,
                    &mut resources,
                    &mut calls,
                    prefix(case),
                )
                .unwrap(),
                "update" => update(CONTEXT, &mut memory, &mut calls).unwrap(),
                "entry" => entry(&mut memory, &mut resources, &mut calls, prefix(case)).unwrap(),
                _ => {
                    let callback = 0x0800_0001 + integer(&data["routines"][routine]["offset"]);
                    auxiliary(
                        callback,
                        CONTEXT,
                        &mut memory,
                        &mut resources,
                        &mut calls,
                        prefix(case),
                    )
                    .unwrap();
                }
            }
            assert_eq!(
                serde_json::to_value(&calls.0).unwrap(),
                case["calls"],
                "case{index}:{routine}"
            );
            assert_eq!(
                resources.requests,
                expected_requests(case),
                "case{index}:{routine}"
            );
            memory.assert_snapshot(case);
        }
    }

    #[test]
    fn malformed_states_and_strict_dependencies_preserve_real_stop_positions() {
        let data = fixture();
        let mut case = data["cases"][0].clone();
        let mut memory = Memory::new(&case, &data);
        let mut resources = SyntheticResources::new(&data);
        let error = initialize(
            CONTEXT,
            &mut memory,
            &mut resources,
            &mut super::super::intro::RejectUnportedIntroCalls,
            None,
        )
        .unwrap_err();
        assert_eq!(
            error,
            TitleError::Intro(IntroError::UnportedCall(IntroCall::Direct {
                entry_offset: 0x30fc,
                arguments: Vec::new()
            }))
        );
        assert_eq!(memory.read_word(CONTEXT).unwrap(), 0);
        let mut memory = Memory::new(&case, &data);
        assert_eq!(
            initialize(
                CONTEXT,
                &mut memory,
                &mut resources,
                &mut Calls::default(),
                None
            ),
            Err(TitleError::Intro(IntroError::MissingConstructorStackPrefix))
        );
        assert!(resources.requests.is_empty());
        case["vertical"] = 0.into();
        case["scrollIndex"] = 18.into();
        let mut memory = Memory::new(&case, &data);
        assert!(matches!(
            auxiliary(
                0x0800_8a05,
                CONTEXT,
                &mut memory,
                &mut resources,
                &mut Calls::default(),
                None
            ),
            Err(TitleError::TableIndex { index: 18, .. })
        ));
        memory.write_halfword(AUXILIARY_STACK, 11).unwrap();
        assert_eq!(
            auxiliary(
                0x0800_8415,
                CONTEXT,
                &mut memory,
                &mut resources,
                &mut Calls::default(),
                None
            ),
            Err(TitleError::Intro(IntroError::AuxiliaryDepth(11)))
        );
        assert!(matches!(TitleRom::new(&[]), Err(IntroError::WrongRom)));
        assert!(matches!(
            auxiliary(
                NEXT_CALLBACK,
                CONTEXT,
                &mut memory,
                &mut resources,
                &mut Calls::default(),
                None
            ),
            Err(TitleError::Intro(IntroError::UnportedCall(_)))
        ));
    }

    struct NativeResources<'a> {
        rom: TitleRom<'a>,
        requests: Vec<(u32, usize)>,
    }
    impl IntroResources for NativeResources<'_> {
        fn background_destination(&self, mode: u8) -> Result<u32, IntroError> {
            self.rom.background_destination(mode)
        }
        fn decode_lz77(&mut self, source: u32) -> Result<Vec<u8>, IntroError> {
            let decoded = self.rom.decode_lz77(source)?;
            self.requests.push((source, decoded.len()));
            Ok(synthetic_asset(source, decoded.len()))
        }
    }
    impl IntroSequenceResources for NativeResources<'_> {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
            self.rom.read_bytes(address, length)
        }
    }
    impl TitleResources for NativeResources<'_> {}
    struct FrameCalls<'a, 'b> {
        resources: &'a mut NativeResources<'b>,
        calls: Vec<IntroCall>,
        prefix: Option<u32>,
    }
    impl IntroCalls for FrameCalls<'_, '_> {
        fn invoke(
            &mut self,
            call: IntroCall,
            memory: &mut dyn IntroMemory,
        ) -> Result<(), IntroError> {
            self.calls.push(call.clone());
            match call {
                IntroCall::Callback {
                    callback_id,
                    arguments,
                } => {
                    // Move the resource borrow out of self during recursive direct
                    // dependency recording; no nested callback needs this resource.
                    let mut direct = Calls::default();
                    let result = auxiliary(
                        callback_id,
                        arguments[0],
                        memory,
                        self.resources,
                        &mut direct,
                        self.prefix,
                    );
                    self.calls.extend(direct.0);
                    result.map_err(|error| match error {
                        TitleError::Intro(error) => error,
                        _ => IntroError::Resource(crate::binary::Error::Range),
                    })
                }
                IntroCall::Direct {
                    entry_offset: 0x2580,
                    ..
                } => {
                    let old = memory.read_halfword(COUNT_ADDRESS)?;
                    memory.write_halfword(COUNT_ADDRESS, old & 0xff00)
                }
                _ => Ok(()),
            }
        }
    }
    fn local_rom() -> Vec<u8> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("SetSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; native scroll/priorities and assets remain local"]
    fn native_start_sequence_runs254_ticks_to_actual_mode_selection_callback() {
        let bytes = local_rom();
        let data = fixture();
        let sequence = &data["nativeTableStartSequence"];
        let mut memory = Memory::new(&sequence["initial"], &data);
        let mut init_resources = NativeResources {
            rom: TitleRom::new(&bytes).unwrap(),
            requests: Vec::new(),
        };
        let mut calls = Calls::default();
        entry(
            &mut memory,
            &mut init_resources,
            &mut calls,
            prefix(&sequence["initialized"]),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(calls.0).unwrap(),
            sequence["initialized"]["calls"]
        );
        assert_eq!(
            init_resources.requests,
            expected_requests(&sequence["initialized"])
        );
        memory.assert_snapshot(&sequence["initialized"]);
        let mut frame_resources = NativeResources {
            rom: TitleRom::new(&bytes).unwrap(),
            requests: Vec::new(),
        };
        assert_eq!(sequence["ticks"].as_array().unwrap().len(), 254);
        for (index, tick) in sequence["ticks"].as_array().unwrap().iter().enumerate() {
            memory
                .write_halfword(PRESSED, integer(&tick["pressed"]) as u16)
                .unwrap();
            frame_resources.requests.clear();
            let mut calls = FrameCalls {
                resources: &mut frame_resources,
                calls: Vec::new(),
                prefix: prefix(tick),
            };
            entry(&mut memory, &mut init_resources, &mut calls, prefix(tick)).unwrap();
            assert_eq!(
                serde_json::to_value(calls.calls).unwrap(),
                tick["calls"],
                "frame{index}"
            );
            assert_eq!(frame_resources.requests, expected_requests(tick));
            memory.assert_snapshot(tick);
        }
        assert_eq!(memory.read_word(MAIN_STACK + 4).unwrap(), NEXT_CALLBACK);
        assert_eq!(memory.read_halfword(AUXILIARY_STACK).unwrap(), 0);
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; independent authentic graphics hashes only"]
    fn authentic_assets_and_exact_source_spans_match_local_independent_hashes() {
        let bytes = local_rom();
        let data = fixture();
        let mut resources = TitleRom::new(&bytes).unwrap();
        for &(_, offset, length, hash) in &SOURCE_SPANS {
            assert_eq!(crate::sha256(&bytes[offset..offset + length]), hash);
        }
        for span in data["resourceSpans"].as_array().unwrap() {
            let offset = (integer(&span["address"]) - 0x0800_0000) as usize;
            assert_eq!(
                crate::sha256(&bytes[offset..offset + integer(&span["length"]) as usize]),
                span["sha256"].as_str().unwrap()
            );
        }
        for asset in data["authenticAssets"].as_array().unwrap() {
            let address = integer(&asset["sourceAddress"]);
            let decoded = resources.decode_lz77(address).unwrap();
            assert_eq!(decoded.len(), integer(&asset["decodedLength"]) as usize);
            assert_eq!(
                crate::sha256(&decoded),
                asset["decodedSha256"].as_str().unwrap()
            );
            let offset = (address - 0x0800_0000) as usize;
            let parsed = crate::binary::decode(&bytes[offset..], MAX_RESOURCE_BYTES).unwrap();
            assert_eq!(parsed.consumed, integer(&asset["storedLength"]) as usize);
            assert_eq!(
                crate::sha256(&bytes[offset..offset + parsed.consumed]),
                asset["storedSha256"].as_str().unwrap()
            );
        }
        assert!(resources.read_bytes(SCROLL_TABLE, 73).is_err());
        assert!(resources.read_bytes(0x087f_fffe, 4).is_err());
    }
}
