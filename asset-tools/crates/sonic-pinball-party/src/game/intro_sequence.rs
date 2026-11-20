//! A3VE 0x6824 intro animation/state sequence, with real 0x663c/0x2614 ports.
//!
//! Transform, palette and audio calls remain strict native-ID dependencies.
//! Timing/descriptor resources are supplied by an authenticated local ROM.
//! Native audio index16 reads the low halfword of saved R4 past its16-entry
//! copied table; the caller must supply that register explicitly when reached.

use super::interpolation::{InterpolationError, InterpolationState};
use super::intro::{IntroCall, IntroCalls, IntroError, IntroMemory, DISPLAY_SHADOW};
use super::intro_callbacks::AUXILIARY_STACK;
use super::oam::{
    OamError, OamSubmissionQueue, Submission, CAPACITY, COUNT_ADDRESS, ORDER_BASE, RECORD_BASE,
    RECORD_WORDS,
};
use super::oam_render::DescriptorSource;
use std::fmt;

pub const CALLBACK_ID: u32 = 0x0800_6825;
pub const OFFSET: usize = 0x6824;
pub const LENGTH: usize = 0x5cc;
pub const SHA256: &str = "0aa6d8f5d6311feaccf5b74b00472d59264109d136336eec24e83d54848998d4";
pub const STATE: u32 = 0x0200_1ca4;
pub const TILE: u32 = 0x0200_2188;
pub const FRAME: u32 = 0x0200_218c;
pub const FADE: u32 = 0x0200_218e;
pub const AUDIO_TIMER: u32 = 0x0200_2194;
pub const AUDIO_INDEX: u32 = 0x0200_2196;
pub const TILE_TABLE: u32 = 0x0806_a2e0;
pub const AUDIO_TABLE: u32 = 0x0806_a32c;
pub const DESCRIPTOR_TABLE: u32 = 0x086e_7134;
pub const PALETTE_FRAMES: u32 = 0x082e_33c0;
pub const RESOURCE_SPANS: [(u32, usize, &str); 4] = [
    (
        TILE_TABLE,
        0x4c,
        "0b573676d8fdd38fff3a116cb7568be9247662397302c380f186c128cbb5e8d5",
    ),
    (
        AUDIO_TABLE,
        0x20,
        "36c88bbca2ecba254c4a9e2f21fc5a631753c5d5f9bbc2178a20c871eba2cd08",
    ),
    (
        DESCRIPTOR_TABLE,
        0x1c,
        "a13249df229cd6f39589dcdee234cedfa4fa57846a12d7e66a2617a1476a8f2c",
    ),
    (
        PALETTE_FRAMES,
        0xa0,
        "d2e343b8b4e5031e26ebfd6b8826845b8c8d8caf0b67dc6c507b26f480e0c0bf",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntroSequenceError {
    Intro(IntroError),
    Interpolation(InterpolationError),
    Queue(OamError),
    TableIndex {
        address: u32,
        index: i32,
        count: usize,
    },
    MissingSavedR4,
}
impl fmt::Display for IntroSequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IntroSequenceError {}
impl From<IntroError> for IntroSequenceError {
    fn from(value: IntroError) -> Self {
        Self::Intro(value)
    }
}
impl From<InterpolationError> for IntroSequenceError {
    fn from(value: InterpolationError) -> Self {
        Self::Interpolation(value)
    }
}
impl From<OamError> for IntroSequenceError {
    fn from(value: OamError) -> Self {
        Self::Queue(value)
    }
}

pub trait IntroSequenceResources {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError>;
    fn read_halfword(&self, address: u32) -> Result<u16, IntroError> {
        let bytes = self.read_bytes(address, 2)?;
        Ok(crate::binary::u16_le(&bytes, 0)?)
    }
    fn read_word(&self, address: u32) -> Result<u32, IntroError> {
        let bytes = self.read_bytes(address, 4)?;
        Ok(crate::binary::u32_le(&bytes, 0)?)
    }
}
#[derive(Clone)]
pub struct IntroSequenceRom<'a> {
    rom: &'a [u8],
}
impl<'a> IntroSequenceRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, IntroError> {
        crate::catalog::validate_rom(rom).map_err(|_| IntroError::WrongRom)?;
        Ok(Self { rom })
    }
}
impl IntroSequenceResources for IntroSequenceRom<'_> {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
        if length > 0x4c {
            return Err(IntroError::ResourceLimit(length));
        }
        let offset = crate::binary::pointer_offset(address, 0x0800_0000, self.rom.len())?;
        Ok(crate::binary::span(self.rom, offset, length)?.to_vec())
    }
}

fn invoke(
    calls: &mut dyn IntroCalls,
    memory: &mut dyn IntroMemory,
    entry_offset: usize,
    arguments: &[u32],
) -> Result<(), IntroSequenceError> {
    calls.invoke(
        IntroCall::Direct {
            entry_offset,
            arguments: arguments.to_vec(),
        },
        memory,
    )?;
    Ok(())
}
fn table_halfword(
    bytes: &[u8],
    address: u32,
    index: i32,
    count: usize,
) -> Result<u16, IntroSequenceError> {
    if index < 0 || index as usize >= count {
        return Err(IntroSequenceError::TableIndex {
            address,
            index,
            count,
        });
    }
    crate::binary::u16_le(bytes, index as usize * 2)
        .map_err(|error| IntroError::Resource(error).into())
}
fn read_queue(memory: &dyn IntroMemory) -> Result<OamSubmissionQueue, IntroSequenceError> {
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
fn write_accepted_submission(
    memory: &mut dyn IntroMemory,
    old_count: u8,
    queue: &OamSubmissionQueue,
) -> Result<(), IntroSequenceError> {
    if queue.count() != old_count {
        let count_halfword = memory.read_halfword(COUNT_ADDRESS)?;
        memory.write_halfword(
            COUNT_ADDRESS,
            (count_halfword & 0xff00) | u16::from(queue.count()),
        )?;
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
    Ok(())
}
fn draw(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    descriptor: u32,
) -> Result<(), IntroSequenceError> {
    let mut queue = read_queue(memory)?;
    let old_count = queue.count();
    let halfword = if old_count & 0x80 == 0 {
        resources.read_halfword(
            descriptor
                .checked_add(4)
                .ok_or(IntroError::Resource(crate::binary::Error::Range))?,
        )?
    } else {
        0
    };
    queue.submit(Submission {
        sort_key: 0,
        x: 104,
        y: 58,
        descriptor_address: descriptor,
        descriptor_halfword_at_4: halfword,
        flags: 0,
        auxiliary: 0,
    });
    write_accepted_submission(memory, old_count, &queue)
}
struct Descriptors<'a>(&'a dyn IntroSequenceResources);
impl DescriptorSource for Descriptors<'_> {
    fn halfword(&self, address: u32) -> Option<u16> {
        self.0.read_halfword(address).ok()
    }
}
fn interpolate(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    slot: usize,
) -> Result<(), IntroSequenceError> {
    let address = super::intro::CONTEXT_BASE + slot as u32 * 32;
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
    state.update(&mut queue, &Descriptors(resources))?;
    memory.write_bytes(address, state.as_bytes())?;
    write_accepted_submission(memory, old_count, &queue)
}
fn transform(
    calls: &mut dyn IntroCalls,
    memory: &mut dyn IntroMemory,
    index: u32,
    angle: u8,
    scale: u16,
) -> Result<(), IntroSequenceError> {
    invoke(
        calls,
        memory,
        0x236c,
        &[
            index,
            u32::from(angle),
            u32::from(scale),
            u32::from(scale),
            0,
        ],
    )
}
fn tile_index(memory: &dyn IntroMemory) -> Result<i32, IntroSequenceError> {
    Ok(i32::from(memory.read_halfword(TILE)? as i16))
}
fn draw_tile(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    table: &[u8],
) -> Result<(), IntroSequenceError> {
    let index = table_halfword(table, TILE_TABLE, tile_index(memory)?, 19)?;
    if index >= 7 {
        return Err(IntroSequenceError::TableIndex {
            address: DESCRIPTOR_TABLE,
            index: i32::from(index),
            count: 7,
        });
    }
    draw(
        memory,
        resources,
        resources.read_word(DESCRIPTOR_TABLE + u32::from(index) * 4)?,
    )
}
fn advance_tile(memory: &mut dyn IntroMemory, table: &[u8]) -> Result<bool, IntroSequenceError> {
    let timer = memory.read_halfword(TILE + 2)?;
    let duration = table_halfword(&table[0x26..], TILE_TABLE + 0x26, tile_index(memory)?, 19)?;
    if i32::from(timer as i16) < i32::from(duration) {
        memory.write_halfword(TILE + 2, timer.wrapping_add(1))?;
        Ok(false)
    } else {
        memory.write_halfword(TILE + 2, 0)?;
        let index = memory.read_halfword(TILE)?;
        memory.write_halfword(TILE, index.wrapping_add(1))?;
        Ok(true)
    }
}

/// Incoming R0 is unused in the native body. `saved_r4` represents the actual
/// preserved caller register, not a new ROM timing. The real 0x8168 caller keeps
/// its context pointer in R4; index16/17 can therefore read that explicit word.
/// Further native stack overreads are rejected. A completed state99 pops this
/// callback; the existing6754 completion callback and81e8 wrapper then perform
/// the native next-screen transition to0x080091c5.
pub fn update(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn IntroCalls,
    saved_r4: Option<u32>,
) -> Result<(), IntroSequenceError> {
    let table = resources.read_bytes(TILE_TABLE, 0x4c)?;
    let audio = resources.read_bytes(AUDIO_TABLE, 0x20)?;
    if table.len() != 0x4c || audio.len() != 0x20 {
        return Err(IntroError::Resource(crate::binary::Error::Range).into());
    }
    let state = memory.read_halfword(STATE)?;
    match state {
        0 => {
            let display = memory.read_halfword(DISPLAY_SHADOW)?;
            memory.write_halfword(DISPLAY_SHADOW, display & 0xe0ff)?;
            let display = memory.read_halfword(DISPLAY_SHADOW)?;
            memory.write_halfword(DISPLAY_SHADOW, display | 0x1200)?;
            memory.write_halfword(DISPLAY_SHADOW + 0x4a, 0x1340)?;
            memory.write_halfword(DISPLAY_SHADOW + 0x4c, 0x0407)?;
            memory.write_halfword(TILE, 0)?;
            memory.write_halfword(TILE + 2, 0)?;
            let frame = memory.read_halfword(FRAME)?;
            if frame <= 59 {
                memory.write_halfword(FRAME, frame.wrapping_add(1))?;
            } else {
                memory.write_halfword(STATE, 1)?;
                memory.write_halfword(FRAME, 0)?;
                memory.write_halfword(AUDIO_TIMER, 0)?;
                memory.write_halfword(AUDIO_INDEX, 0)?;
                memory.write_word(0x0200_2198, 0)?;
                invoke(calls, memory, 0x389c, &[122])?;
            }
        }
        1 => {
            let tile = table_halfword(&table, TILE_TABLE, tile_index(memory)?, 19)?;
            let (x, y) = match tile {
                0 => (256, 512),
                1 => (231, 306),
                3 => (281, 231),
                _ => (256, 256),
            };
            invoke(calls, memory, 0x236c, &[0, 0, x, y, 0])?;
            draw_tile(memory, resources, &table)?;
            let index = memory.read_halfword(AUDIO_INDEX)?;
            let duration = if index < 16 {
                table_halfword(&audio, AUDIO_TABLE, i32::from(index), 16)?
            } else if index <= 17 {
                let saved = saved_r4.ok_or(IntroSequenceError::MissingSavedR4)?;
                (saved >> ((index - 16) * 16)) as u16
            } else {
                return Err(IntroSequenceError::TableIndex {
                    address: AUDIO_TABLE,
                    index: i32::from(index),
                    count: 18,
                });
            };
            let timer = memory.read_halfword(AUDIO_TIMER)?;
            if timer < duration {
                memory.write_halfword(AUDIO_TIMER, timer.wrapping_add(1))?;
            } else {
                invoke(calls, memory, 0x389c, &[u32::from(index.wrapping_add(89))])?;
                memory.write_halfword(AUDIO_TIMER, 0)?;
                let index = memory.read_halfword(AUDIO_INDEX)?;
                memory.write_halfword(AUDIO_INDEX, index.wrapping_add(1))?;
            }
            if advance_tile(memory, &table)? && memory.read_halfword(TILE)? as i16 == 16 {
                memory.write_halfword(STATE, 2)?;
                invoke(calls, memory, 0x389c, &[123])?;
            }
        }
        2 => {
            draw_tile(memory, resources, &table)?;
            if advance_tile(memory, &table)? {
                invoke(calls, memory, 0x389c, &[123])?;
                if memory.read_halfword(TILE)? as i16 == 19 {
                    memory.write_halfword(STATE, 3)?;
                    invoke(calls, memory, 0x389c, &[124])?;
                }
            }
            animated(memory, resources, calls)?;
        }
        3 => animated(memory, resources, calls)?,
        _ => {
            let count = memory.read_halfword(AUXILIARY_STACK)?;
            if count > 10 {
                return Err(IntroError::AuxiliaryDepth(count).into());
            }
            if count != 0 {
                memory.write_halfword(AUXILIARY_STACK, count - 1)?;
            }
        }
    }
    Ok(())
}

fn animated(
    memory: &mut dyn IntroMemory,
    resources: &dyn IntroSequenceResources,
    calls: &mut dyn IntroCalls,
) -> Result<(), IntroSequenceError> {
    let frame = memory.read_halfword(FRAME)?.wrapping_add(1);
    memory.write_halfword(FRAME, frame)?;
    if frame <= 49 {
        invoke(
            calls,
            memory,
            0x5724,
            &[PALETTE_FRAMES + u32::from(frame / 10) * 32, 240, 16],
        )?;
    }
    let frame = memory.read_halfword(FRAME)?;
    transform(
        calls,
        memory,
        4,
        frame.wrapping_mul(3).wrapping_neg() as u8,
        frame.wrapping_sub(10).wrapping_mul(5),
    )?;
    let frame = memory.read_halfword(FRAME)?;
    transform(calls, memory, 5, 0, frame.wrapping_sub(10).wrapping_mul(4))?;
    let frame = memory.read_halfword(FRAME)?;
    transform(calls, memory, 6, 0, frame.wrapping_sub(10).wrapping_mul(6))?;
    for (index, factor, angle_factor) in [
        (7, 6, 0i16),
        (8, 8, 0),
        (9, 6, 0),
        (10, 7, 0),
        (11, 7, 4),
        (12, 4, 3),
        (13, 4, -3),
        (14, 4, 3),
        (15, 4, -3),
    ] {
        let frame = memory.read_halfword(FRAME)?;
        transform(
            calls,
            memory,
            index,
            frame.wrapping_mul(angle_factor as u16) as u8,
            frame.wrapping_mul(factor),
        )?;
    }
    let frame = memory.read_halfword(FRAME)?;
    transform(calls, memory, 16, frame.wrapping_mul(3) as u8, 256)?;
    for (threshold, transform_index, slot) in [(50, 1, 0), (40, 2, 1), (30, 3, 2)] {
        let frame = memory.read_halfword(FRAME)?;
        if frame > threshold {
            transform(
                calls,
                memory,
                transform_index,
                0,
                frame.wrapping_sub(threshold).wrapping_mul(8),
            )?;
            interpolate(memory, resources, slot)?;
        }
    }
    if memory.read_halfword(FRAME)? > 10 {
        for slot in 3..6 {
            interpolate(memory, resources, slot)?;
        }
    }
    for slot in 6..11 {
        interpolate(memory, resources, slot)?;
    }
    if memory.read_halfword(FRAME)? > 31 {
        for slot in 11..16 {
            interpolate(memory, resources, slot)?;
        }
    }
    for slot in 16..34 {
        interpolate(memory, resources, slot)?;
    }
    if memory.read_halfword(FRAME)? > 61 {
        for slot in 34..38 {
            interpolate(memory, resources, slot)?;
        }
    }
    if memory.read_halfword(FRAME)? > 69 {
        let fade = memory.read_halfword(FADE)?.wrapping_add(1);
        memory.write_halfword(FADE, fade)?;
        if fade <= 32 {
            invoke(
                calls,
                memory,
                0x5784,
                &[0, 32, u32::from(fade.wrapping_add(32) as u8)],
            )?;
        } else {
            memory.write_halfword(STATE, 99)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::interpolation::ConstructArgs;
    use super::super::intro::{CONTEXT_BASE, CONTEXT_DESTINATION_INDICES, CONTEXT_SETUP_ROWS};
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn fixture() -> Value {
        serde_json::from_str(vectors!("intro-sequence-vectors.json")).unwrap()
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
        fn new(case: &Value, data: &Value, synthetic: bool) -> Self {
            let seed = integer(&case["seed"]);
            let mut result = Self {
                ewram: pattern(seed, 0x40000),
                iwram: pattern(seed ^ 0xa55a_1234, 0x8000),
            };
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
                    integer(&data["constructorStackPrefix"]),
                );
                result
                    .write_bytes(
                        CONTEXT_BASE + CONTEXT_DESTINATION_INDICES[index] as u32 * 32,
                        state.as_bytes(),
                    )
                    .unwrap();
            }
            if synthetic {
                for index in 0..39 {
                    result
                        .write_word(
                            CONTEXT_BASE + index * 32 + 12,
                            integer(&data["syntheticDescriptorBase"]) + index * 8,
                        )
                        .unwrap();
                }
            }
            for (address, field) in [
                (STATE, "state"),
                (TILE, "tile"),
                (TILE + 2, "tileTimer"),
                (FRAME, "frame"),
                (FADE, "fade"),
                (AUDIO_TIMER, "audioTimer"),
                (AUDIO_INDEX, "audioIndex"),
                (AUXILIARY_STACK, "depth"),
            ] {
                result
                    .write_halfword(address, integer(&case[field]) as u16)
                    .unwrap();
            }
            let old = result.read_halfword(COUNT_ADDRESS).unwrap();
            result
                .write_halfword(
                    COUNT_ADDRESS,
                    (old & 0xff00) | integer(&case["queueCount"]) as u16,
                )
                .unwrap();
            result
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
    struct SyntheticResources;
    impl IntroSequenceResources for SyntheticResources {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, IntroError> {
            let tiles: Vec<_> = (0..19u16)
                .map(|index| index % 7)
                .chain((0..19u16).map(|index| 2 + index % 5))
                .flat_map(u16::to_le_bytes)
                .collect();
            let audio: Vec<_> = (0..16u16)
                .flat_map(|index| (3 + index % 4).to_le_bytes())
                .collect();
            let pointers: Vec<_> = (0..7u32)
                .flat_map(|index| (0x0870_0000 + index * 8).to_le_bytes())
                .collect();
            let descriptors: Vec<_> = (0..39u16)
                .flat_map(|index| [0, 0, (index % 4) << 10, 0])
                .flat_map(u16::to_le_bytes)
                .collect();
            for (base, bytes) in [
                (TILE_TABLE, &tiles),
                (AUDIO_TABLE, &audio),
                (DESCRIPTOR_TABLE, &pointers),
                (0x0870_0000, &descriptors),
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

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_state_sequence_matches_real_interpolation_and_oam_with_declared_synthetic_resources()
    {
        let data = fixture();
        assert_eq!(data["routines"]["sequence"]["sha256"], SHA256);
        assert_eq!(
            integer(&data["routines"]["sequence"]["offset"]) as usize,
            OFFSET
        );
        assert_eq!(
            integer(&data["routines"]["sequence"]["length"]) as usize,
            LENGTH
        );
        for (index, &(address, length, hash)) in RESOURCE_SPANS.iter().enumerate() {
            assert_eq!(integer(&data["resourceSpans"][index]["address"]), address);
            assert_eq!(
                integer(&data["resourceSpans"][index]["length"]) as usize,
                length
            );
            assert_eq!(data["resourceSpans"][index]["sha256"], hash);
        }
        for (index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::new(case, &data, true);
            let mut calls = Calls::default();
            update(
                &mut memory,
                &SyntheticResources,
                &mut calls,
                Some(integer(&case["savedR4"])),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(&calls.0).unwrap(),
                case["calls"],
                "case{index}"
            );
            memory.assert_snapshot(case);
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn unknown_resources_indices_and_required_native_saved_register_stop_bounded() {
        let data = fixture();
        let mut case = data["cases"][0].clone();
        case["state"] = 1.into();
        case["audioIndex"] = 16.into();
        case["queueCount"] = 0.into();
        let mut memory = Memory::new(&case, &data, true);
        let mut calls = Calls::default();
        assert_eq!(
            update(&mut memory, &SyntheticResources, &mut calls, None),
            Err(IntroSequenceError::MissingSavedR4)
        );
        assert_eq!(memory.read_halfword(COUNT_ADDRESS).unwrap() as u8, 1);
        assert_eq!(memory.read_halfword(AUDIO_INDEX).unwrap(), 16);
        case["tile"] = 0xffff.into();
        let mut memory = Memory::new(&case, &data, true);
        assert!(matches!(
            update(
                &mut memory,
                &SyntheticResources,
                &mut Calls::default(),
                None
            ),
            Err(IntroSequenceError::TableIndex { index: -1, .. })
        ));
        assert_eq!(memory.read_halfword(COUNT_ADDRESS).unwrap() as u8, 0);
        struct Truncated;
        impl IntroSequenceResources for Truncated {
            fn read_bytes(&self, _address: u32, _length: usize) -> Result<Vec<u8>, IntroError> {
                Ok(Vec::new())
            }
        }
        assert_eq!(
            update(&mut memory, &Truncated, &mut Calls::default(), None),
            Err(IntroSequenceError::Intro(IntroError::Resource(
                crate::binary::Error::Range
            )))
        );
        assert!(matches!(
            IntroSequenceRom::new(&[]),
            Err(IntroError::WrongRom)
        ));
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn strict_external_dependency_stops_after_actual_pre_call_state_only() {
        let data = fixture();
        let mut case = data["cases"][0].clone();
        case["state"] = 3.into();
        case["frame"] = 0.into();
        let mut memory = Memory::new(&case, &data, true);
        let contexts = memory.span(CONTEXT_BASE, 39 * 32).unwrap().to_vec();
        let error = update(
            &mut memory,
            &SyntheticResources,
            &mut super::super::intro::RejectUnportedIntroCalls,
            None,
        )
        .unwrap_err();
        assert_eq!(
            error,
            IntroSequenceError::Intro(IntroError::UnportedCall(IntroCall::Direct {
                entry_offset: 0x5724,
                arguments: vec![PALETTE_FRAMES, 240, 16]
            }))
        );
        assert_eq!(memory.read_halfword(FRAME).unwrap(), 1);
        assert_eq!(memory.span(CONTEXT_BASE, 39 * 32).unwrap(), contexts);
    }

    #[test]
    #[ignore = "Requires local SPP_ROM; real timings/descriptors stay local"]
    fn authentic_sequence_runs_387_ticks_and_composes_existing_completion_and_next_screen_ports() {
        use super::super::control::{CallbackId, CallbackStack};
        use super::super::startup::{self, StartupCall, StartupCalls, StartupError, StartupMemory};
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("SetSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        let resources = IntroSequenceRom::new(&bytes).unwrap();
        let data = fixture();
        for &(address, length, hash) in &RESOURCE_SPANS {
            let offset = (address - 0x0800_0000) as usize;
            assert_eq!(crate::sha256(&bytes[offset..offset + length]), hash);
        }
        assert!(resources.read_bytes(0x087f_fffe, 4).is_err());
        assert!(resources.read_bytes(TILE_TABLE, 77).is_err());
        let sequence = &data["authenticSequence"];
        assert_eq!(sequence["ticks"].as_array().unwrap().len(), 387);
        let mut memory = Memory::new(&sequence["initial"], &data, false);
        memory.write_word(0x0200_1ca0, 0x0201_99a0).unwrap();
        memory.write_word(0x0201_99a0, 0).unwrap();
        memory.write_halfword(0x0300_0e00, 1).unwrap();
        memory.write_word(0x0300_0e04, 0x0800_81e9).unwrap();
        memory.write_halfword(0x0300_0e2c, 2).unwrap();
        memory
            .write_word(AUXILIARY_STACK + 4, super::super::intro_callbacks::COMPLETE)
            .unwrap();
        memory.write_word(AUXILIARY_STACK + 8, CALLBACK_ID).unwrap();
        memory.write_halfword(AUXILIARY_STACK + 0x2c, 0).unwrap();
        memory.write_halfword(AUXILIARY_STACK + 0x2e, 0).unwrap();
        struct CallbackResources;
        impl super::super::intro_callbacks::IntroCallbackResources for CallbackResources {
            fn read_rom_word(&self, _address: u32) -> Result<u32, IntroError> {
                unreachable!()
            }
            fn decode_lz77(&mut self, _source: u32) -> Result<Vec<u8>, IntroError> {
                unreachable!()
            }
        }
        for tick in sequence["ticks"].as_array().unwrap() {
            let old = memory.read_halfword(COUNT_ADDRESS).unwrap();
            memory.write_halfword(COUNT_ADDRESS, old & 0xff00).unwrap();
            let depth = memory.read_halfword(AUXILIARY_STACK).unwrap();
            let callback = memory
                .read_word(AUXILIARY_STACK + 4 + u32::from(depth - 1) * 4)
                .unwrap();
            assert_eq!(callback, integer(&tick["callbackId"]));
            let mut calls = Calls::default();
            if callback == CALLBACK_ID {
                update(&mut memory, &resources, &mut calls, Some(0x0201_99a0)).unwrap();
            } else {
                super::super::intro_callbacks::dispatch(
                    callback,
                    0x0201_99a0,
                    &mut memory,
                    &mut CallbackResources,
                    &mut calls,
                )
                .unwrap();
            }
            assert_eq!(serde_json::to_value(calls.0).unwrap(), tick["calls"]);
            memory.assert_snapshot(tick);
        }
        assert_eq!(memory.read_word(0x0201_99a0).unwrap(), 1);
        fn stack(memory: &Memory, address: u32) -> CallbackStack {
            CallbackStack::from_halfwords(
                &(0..32)
                    .map(|index| memory.read_halfword(address + index * 2).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        }
        let mut main = stack(&memory, 0x0300_0e00);
        let mut auxiliary = stack(&memory, AUXILIARY_STACK);
        struct StartupAdapter<'a>(&'a mut Memory);
        impl StartupMemory for StartupAdapter<'_> {
            fn read_word(&self, address: u32) -> Result<u32, StartupError> {
                self.0
                    .read_word(address)
                    .map_err(|_| StartupError::MissingWord(address))
            }
            fn write_word(&mut self, address: u32, value: u32) -> Result<(), StartupError> {
                self.0
                    .write_word(address, value)
                    .map_err(|_| StartupError::MissingWord(address))
            }
        }
        struct AlreadyRunUpdate(Vec<StartupCall>);
        impl StartupCalls for AlreadyRunUpdate {
            fn invoke(
                &mut self,
                call: StartupCall,
                _memory: &mut dyn StartupMemory,
            ) -> Result<(), StartupError> {
                assert_eq!(call.entry_offset, 0x8168);
                self.0.push(call);
                Ok(())
            }
        }
        let mut calls = AlreadyRunUpdate(Vec::new());
        assert_eq!(
            startup::initial_callback(
                &mut main,
                &mut auxiliary,
                &mut StartupAdapter(&mut memory),
                &mut calls
            )
            .unwrap(),
            startup::StartupOutcome::ReplacedByNextCallback
        );
        assert_eq!(main.top().unwrap().callback, CallbackId(0x0800_91c5));
        for (address, stack) in [(0x0300_0e00, main), (AUXILIARY_STACK, auxiliary)] {
            let bytes: Vec<_> = stack
                .to_halfwords()
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect();
            memory.write_bytes(address, &bytes).unwrap();
        }
        assert_eq!(calls.0.len(), 1);
        memory.assert_snapshot(&sequence["transition"]);
    }
}
