//! Native A3VE MPlay player tick. The controller dispatches exact native IDs;
//! note allocation, instrument/PSG callbacks and mixing stay explicit effects.
use super::{
    frame::{FrameError, FrameMemory},
    mplay,
};

pub const OFFSET: usize = 0x5e3a8;
pub const LENGTH: usize = 0x268;
pub const SHA256: &str = "4aed271a94f8dd45897ee0b891ccf1101219125a813807c9d2178f379602e625";
pub const SOUND_INFO_SLOT: u32 = 0x03007ff0;
pub const DISPATCH_OFFSET: usize = 0xf5f58;
pub const DISPATCH_BYTES: usize = 144;
pub const DISPATCH_SHA256: &str =
    "07070902ece8f71e1302f45bc09ae93555c715dc7c578a7eda7901a981f6358f";
pub const WAIT_OFFSET: usize = 0xf61cc;
pub const WAIT_BYTES: usize = 49;
pub const WAIT_SHA256: &str = "952d81c712713fc630d9d622d5f79eec8189f2a4cf05a78442857b635f0d23f8";

/// The 36 native copied function words include 30 command slots (B1..CE)
/// followed by six internal helpers. Addresses are runtime resources, not an
/// assumed opcode map from another game.
pub struct NativeTables {
    entries: [u32; 36],
    waits: [u8; WAIT_BYTES],
}
impl NativeTables {
    pub fn load(rom: &[u8]) -> Result<Self, Error> {
        crate::catalog::validate_rom(rom).map_err(|_| Error::Source)?;
        let entries =
            crate::binary::span(rom, DISPATCH_OFFSET, DISPATCH_BYTES).map_err(|_| Error::Source)?;
        let waits = crate::binary::span(rom, WAIT_OFFSET, WAIT_BYTES).map_err(|_| Error::Source)?;
        if crate::sha256(entries) != DISPATCH_SHA256 || crate::sha256(waits) != WAIT_SHA256 {
            return Err(Error::Source);
        }
        Ok(Self {
            entries: std::array::from_fn(|i| {
                u32::from_le_bytes(
                    entries[i * 4..i * 4 + 4]
                        .try_into()
                        .expect("exact source table"),
                )
            }),
            waits: waits.try_into().expect("exact wait table"),
        })
    }
    pub fn entries(&self) -> &[u32; 36] {
        &self.entries
    }
    pub fn waits(&self) -> &[u8; WAIT_BYTES] {
        &self.waits
    }
}

/// Only explicitly consumed operands are supplied. Most calls use r0..r2;
/// the native 0x5e654 helper instead consumes channel in r4 and track in r5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub target: u32,
    pub r0: Option<u32>,
    pub r1: Option<u32>,
    pub r2: Option<u32>,
    pub r4: Option<u32>,
    pub r5: Option<u32>,
}
impl Call {
    fn standard(target: u32, args: &[u32]) -> Self {
        Self {
            target,
            r0: args.first().copied(),
            r1: args.get(1).copied(),
            r2: args.get(2).copied(),
            r4: None,
            r5: None,
        }
    }
    fn volume(channel: u32, track: u32) -> Self {
        Self {
            target: 0x0805e654,
            r0: None,
            r1: None,
            r2: None,
            r4: Some(channel),
            r5: Some(track),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Memory(FrameError),
    Source,
    Address(u32),
    UnportedCall(Call),
    Operand(Call),
    Sequence(super::sequence::SequenceError),
    Extended(super::sequence_extended::Error),
    Limit { kind: &'static str, maximum: usize },
    ChannelCycle(u32),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<FrameError> for Error {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
impl From<super::sequence::SequenceError> for Error {
    fn from(value: super::sequence::SequenceError) -> Self {
        Self::Sequence(value)
    }
}
impl From<super::sequence_extended::Error> for Error {
    fn from(value: super::sequence_extended::Error) -> Self {
        Self::Extended(value)
    }
}

pub trait Calls {
    /// Helpers are synchronous and may mutate the supplied canonical memory.
    /// Only sample/PSG pitch callbacks' return words are consumed by the tick.
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error>;
}
pub struct RejectCalls;
impl Calls for RejectCalls {
    fn call(&mut self, _: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        Err(Error::UnportedCall(call))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub tracks: usize,
    pub commands: usize,
    pub channel_visits: usize,
    pub event_ticks: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            tracks: 16,
            commands: 4096,
            channel_visits: 4096,
            event_ticks: 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    pub accepted: bool,
    pub event_ticks: usize,
    pub commands: usize,
    pub channel_visits: usize,
}
fn count(value: &mut usize, maximum: usize, kind: &'static str) -> Result<(), Error> {
    if *value >= maximum {
        return Err(Error::Limit { kind, maximum });
    }
    *value += 1;
    Ok(())
}
fn add(address: u32, amount: u32) -> Result<u32, Error> {
    address.checked_add(amount).ok_or(Error::Address(address))
}
fn ram(address: u32, length: u32) -> Result<(), Error> {
    let end = add(address, length)?;
    if address & 3 == 0
        && ((0x02000000..0x02040000).contains(&address) && end <= 0x02040000
            || (0x03000000..0x03008000).contains(&address) && end <= 0x03008000)
    {
        Ok(())
    } else {
        Err(Error::Address(address))
    }
}
fn word(memory: &mut dyn FrameMemory, address: u32, value: u32) -> Result<(), Error> {
    memory.write_u16(address, value as u16)?;
    memory.write_u16(add(address, 2)?, (value >> 16) as u16)?;
    Ok(())
}
fn invoke(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    target: u32,
    args: &[u32],
) -> Result<u32, Error> {
    calls.call(memory, Call::standard(target, args))
}
fn visits(memory: &dyn FrameMemory, player: u32, limits: Limits) -> Result<(usize, u32), Error> {
    // The native main loops execute once even when the u8 count is zero.
    let count = usize::from(memory.read_u8(player + 8)?).max(1);
    if count > limits.tracks {
        return Err(Error::Limit {
            kind: "tracks",
            maximum: limits.tracks,
        });
    }
    let base = memory.read_u32(player + 0x2c)?;
    ram(base, (count * mplay::TRACK_BYTES) as u32)?;
    Ok((count, base))
}
fn next_channel(
    current: u32,
    seen: &mut Vec<u32>,
    outcome: &mut Outcome,
    limits: Limits,
) -> Result<(), Error> {
    ram(current, mplay::CHANNEL_BYTES as u32)?;
    if seen.contains(&current) {
        return Err(Error::ChannelCycle(current));
    }
    seen.push(current);
    count(
        &mut outcome.channel_visits,
        limits.channel_visits,
        "channel visits",
    )?;
    Ok(())
}

fn lfo(memory: &mut dyn FrameMemory, track: u32) -> Result<(), Error> {
    let speed = u32::from(memory.read_u8(track + 0x19)?);
    if speed == 0 {
        return Ok(());
    }
    let depth = i32::from(memory.read_u8(track + 0x17)?);
    if depth == 0 {
        return Ok(());
    }
    let delay = memory.read_u8(track + 0x1c)?;
    if delay != 0 {
        memory.write_u8(track + 0x1c, delay - 1)?;
        return Ok(());
    }
    let sum = u32::from(memory.read_u8(track + 0x1a)?) + speed;
    memory.write_u8(track + 0x1a, sum as u8)?;
    let wave = if (sum.wrapping_sub(0x40) as u8 as i8) < 0 {
        i32::from(sum as u8 as i8)
    } else {
        0x80 - sum as i32
    };
    let value = (depth * wave) >> 6;
    if memory.read_u8(track + 0x16)? != value as u8 {
        memory.write_u8(track + 0x16, value as u8)?;
        let flags = memory.read_u8(track)?;
        let mask = if memory.read_u8(track + 0x18)? == 0 {
            0x0c
        } else {
            3
        };
        memory.write_u8(track, flags | mask)?;
    }
    Ok(())
}

/// Complete 0x5e3a8 controller. A stopped dependency retains earlier native-
/// order effects (including the signature lock), and is never a completed tick.
pub fn tick(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    player: u32,
    limits: Limits,
) -> Result<Outcome, Error> {
    ram(player, mplay::PLAYER_BYTES as u32)?;
    let mut outcome = Outcome::default();
    if memory.read_u32(player + 0x34)? != mplay::SIGNATURE {
        return Ok(outcome);
    }
    word(memory, player + 0x34, mplay::SIGNATURE + 1)?;
    outcome.accepted = true;
    let callback = memory.read_u32(player + 0x38)?;
    if callback != 0 {
        let argument = memory.read_u32(player + 0x3c)?;
        invoke(memory, calls, callback, &[argument])?;
    }
    if memory.read_u32(player + 4)? as i32 >= 0 {
        let sound = memory.read_u32(SOUND_INFO_SLOT)?;
        ram(sound, 0x3c)?;
        invoke(memory, calls, 0x0805f294, &[player])?;
        if memory.read_u32(player + 4)? as i32 >= 0 {
            let mut accumulator = u32::from(memory.read_u16(player + 0x22)?)
                + u32::from(memory.read_u16(player + 0x20)?);
            loop {
                memory.write_u16(player + 0x22, accumulator as u16)?;
                if accumulator < 150 {
                    break;
                }
                count(&mut outcome.event_ticks, limits.event_ticks, "event ticks")?;
                let (tracks, base) = visits(memory, player, limits)?;
                let mut active = 0u32;
                for index in 0..tracks {
                    let track = add(base, (index * mplay::TRACK_BYTES) as u32)?;
                    if memory.read_u8(track)? & 0x80 == 0 {
                        continue;
                    }
                    active |= 1u32.checked_shl(index as u32).unwrap_or(0);
                    let mut channel = memory.read_u32(track + 0x20)?;
                    let mut seen = Vec::new();
                    while channel != 0 {
                        next_channel(channel, &mut seen, &mut outcome, limits)?;
                        let flags = memory.read_u8(channel)?;
                        if flags & 0xc7 != 0 {
                            let gate = memory.read_u8(channel + 0x10)?;
                            if gate != 0 {
                                memory.write_u8(channel + 0x10, gate - 1)?;
                                if gate == 1 {
                                    memory.write_u8(channel, flags | 0x40)?;
                                }
                            }
                        } else {
                            invoke(memory, calls, 0x0805ed90, &[channel])?;
                        }
                        channel = memory.read_u32(channel + 0x34)?;
                    }
                    if memory.read_u8(track)? & 0x40 != 0 {
                        invoke(memory, calls, 0x0805eda4, &[track])?;
                        for (offset, value) in
                            [(0, 0x80), (0x0f, 2), (0x13, 0x40), (0x19, 0x16), (0x24, 1)]
                        {
                            memory.write_u8(track + offset, value)?;
                        }
                    }
                    let mut terminated = false;
                    while memory.read_u8(track + 1)? == 0 {
                        count(&mut outcome.commands, limits.commands, "commands")?;
                        let cursor = memory.read_u32(track + 0x40)?;
                        let byte = memory.read_u8(cursor)?;
                        let opcode = if byte < 0x80 {
                            memory.read_u8(track + 7)?
                        } else {
                            word(memory, track + 0x40, add(cursor, 1)?)?;
                            if byte >= 0xbd {
                                memory.write_u8(track + 7, byte)?;
                            }
                            byte
                        };
                        if opcode >= 0xcf {
                            let callback = memory.read_u32(sound + 0x38)?;
                            invoke(
                                memory,
                                calls,
                                callback,
                                &[u32::from(opcode - 0xcf), player, track],
                            )?;
                        } else if opcode > 0xb0 {
                            let command = u32::from(opcode - 0xb1);
                            memory.write_u8(player + 0x0a, command as u8)?;
                            let table = memory.read_u32(sound + 0x34)?;
                            let callback = memory.read_u32(add(table, command * 4)?)?;
                            invoke(memory, calls, callback, &[player, track])?;
                            if memory.read_u8(track)? == 0 {
                                terminated = true;
                                break;
                            }
                        } else {
                            // Native running status below 0x80 would underflow
                            // this source index. Reject its unproved address.
                            let index = opcode
                                .checked_sub(0x80)
                                .ok_or(Error::Address(0x08000000 + WAIT_OFFSET as u32))?;
                            let wait = memory
                                .read_u8(0x08000000 + WAIT_OFFSET as u32 + u32::from(index))?;
                            memory.write_u8(track + 1, wait)?;
                        }
                    }
                    if !terminated {
                        let wait = memory.read_u8(track + 1)?;
                        memory.write_u8(track + 1, wait.wrapping_sub(1))?;
                        lfo(memory, track)?;
                    }
                }
                let counter = memory.read_u32(player + 0x0c)?.wrapping_add(1);
                word(memory, player + 0x0c, counter)?;
                if active == 0 {
                    word(memory, player + 4, 0x80000000)?;
                    word(memory, player + 0x34, mplay::SIGNATURE)?;
                    return Ok(outcome);
                }
                word(memory, player + 4, active)?;
                accumulator = u32::from(memory.read_u16(player + 0x22)?).wrapping_sub(150);
            }
            let (tracks, base) = visits(memory, player, limits)?;
            for index in 0..tracks {
                let track = add(base, (index * mplay::TRACK_BYTES) as u32)?;
                let flags = memory.read_u8(track)?;
                if flags & 0x80 == 0 || flags & 0x0f == 0 {
                    continue;
                }
                invoke(memory, calls, 0x0805f35c, &[player, track])?;
                let mut channel = memory.read_u32(track + 0x20)?;
                let mut seen = Vec::new();
                while channel != 0 {
                    next_channel(channel, &mut seen, &mut outcome, limits)?;
                    if memory.read_u8(channel)? & 0xc7 == 0 {
                        invoke(memory, calls, 0x0805ed90, &[channel])?;
                    } else {
                        let kind = memory.read_u8(channel + 1)? & 7;
                        if memory.read_u8(track)? & 3 != 0 {
                            calls.call(memory, Call::volume(channel, track))?;
                            if kind != 0 {
                                let flags = memory.read_u8(channel + 0x1d)?;
                                memory.write_u8(channel + 0x1d, flags | 1)?;
                            }
                        }
                        if memory.read_u8(track)? & 0x0c != 0 {
                            let pitch = i32::from(memory.read_u8(channel + 8)?)
                                + i32::from(memory.read_u8(track + 8)? as i8);
                            let pitch = pitch.max(0) as u32;
                            let fine = u32::from(memory.read_u8(track + 9)?);
                            let result = if kind != 0 {
                                let callback = memory.read_u32(sound + 0x30)?;
                                let result = invoke(
                                    memory,
                                    calls,
                                    callback,
                                    &[u32::from(kind), pitch, fine],
                                )?;
                                // Store rate before dirty-flag mutation, as native.
                                word(memory, channel + 0x20, result)?;
                                let flags = memory.read_u8(channel + 0x1d)?;
                                memory.write_u8(channel + 0x1d, flags | 2)?;
                                None
                            } else {
                                let sample = memory.read_u32(channel + 0x24)?;
                                Some(invoke(memory, calls, 0x0805e914, &[sample, pitch, fine])?)
                            };
                            if let Some(result) = result {
                                word(memory, channel + 0x20, result)?;
                            }
                        }
                    }
                    channel = memory.read_u32(channel + 0x34)?;
                }
                let flags = memory.read_u8(track)?;
                memory.write_u8(track, flags & 0xf0)?;
            }
        }
    }
    word(memory, player + 0x34, mplay::SIGNATURE)?;
    Ok(outcome)
}

/// Pure 0x5e654 helper; native operands are channel r4 and track r5.
pub fn channel_volume(memory: &mut dyn FrameMemory, channel: u32, track: u32) -> Result<(), Error> {
    ram(channel, mplay::CHANNEL_BYTES as u32)?;
    ram(track, mplay::TRACK_BYTES as u32)?;
    let velocity = i32::from(memory.read_u8(channel + 0x12)?);
    let pan = i32::from(memory.read_u8(channel + 0x14)? as i8);
    for (offset, track_offset, factor) in [(2, 0x10, 128 + pan), (3, 0x11, 127 - pan)] {
        let value = (velocity * factor * i32::from(memory.read_u8(track + track_offset)?)) >> 14;
        memory.write_u8(
            channel + offset,
            if value as u32 > 255 { 255 } else { value as u8 },
        )?;
    }
    Ok(())
}

/// Pure 0x5f35c helper; player r0 is not consumed by this native routine.
pub fn track_parameters(memory: &mut dyn FrameMemory, track: u32) -> Result<(), Error> {
    ram(track, mplay::TRACK_BYTES as u32)?;
    if memory.read_u8(track)? & 1 != 0 {
        let mut volume = (u32::from(memory.read_u8(track + 0x13)?)
            * u32::from(memory.read_u8(track + 0x12)?))
            >> 5;
        let mode = memory.read_u8(track + 0x18)?;
        if mode == 1 {
            volume = (volume * (128 + i32::from(memory.read_u8(track + 0x16)? as i8)) as u32) >> 7;
        }
        let mut pan = i32::from(memory.read_u8(track + 0x14)? as i8) * 2
            + i32::from(memory.read_u8(track + 0x15)? as i8);
        if mode == 2 {
            pan += i32::from(memory.read_u8(track + 0x16)? as i8);
        }
        pan = pan.clamp(-128, 127);
        memory.write_u8(track + 0x10, (((pan + 128) as u32 * volume) >> 8) as u8)?;
        memory.write_u8(track + 0x11, (((127 - pan) as u32 * volume) >> 8) as u8)?;
    }
    let flags = memory.read_u8(track)?;
    if flags & 4 != 0 {
        let signed =
            |offset| -> Result<i32, Error> { Ok(i32::from(memory.read_u8(track + offset)? as i8)) };
        let mut pitch = 4
            * (signed(0x0c)? + signed(0x0e)? * i32::from(memory.read_u8(track + 0x0f)?))
            + 256 * (signed(0x0a)? + signed(0x0b)?)
            + i32::from(memory.read_u8(track + 0x0d)?);
        if memory.read_u8(track + 0x18)? == 0 {
            pitch += 16 * signed(0x16)?;
        }
        memory.write_u8(track + 8, (pitch >> 8) as u8)?;
        memory.write_u8(track + 9, pitch as u8)?;
    }
    memory.write_u8(track, flags & 0xfa)?;
    Ok(())
}

/// Complete native fade controller. Track stopping remains a synchronous call
/// so PSG key-off is never silently discarded. Its count-zero loop does not
/// share the main tick's execute-once behavior.
pub fn fade(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    player: u32,
    limits: Limits,
) -> Result<(), Error> {
    ram(player, mplay::PLAYER_BYTES as u32)?;
    let interval = memory.read_u16(player + 0x24)?;
    if interval == 0 {
        return Ok(());
    }
    let countdown = memory.read_u16(player + 0x26)?.wrapping_sub(1);
    memory.write_u16(player + 0x26, countdown)?;
    if countdown != 0 {
        return Ok(());
    }
    memory.write_u16(player + 0x26, interval)?;
    let old = memory.read_u16(player + 0x28)?;
    let value = if old & 2 != 0 {
        old.wrapping_add(16)
    } else {
        old.wrapping_sub(16)
    };
    memory.write_u16(player + 0x28, value)?;
    let terminal = old & 2 == 0 && value as i16 <= 0;
    if old & 2 != 0 && value >= 256 {
        memory.write_u16(player + 0x28, 256)?;
        memory.write_u16(player + 0x24, 0)?;
    }
    let tracks = usize::from(memory.read_u8(player + 8)?);
    if tracks > limits.tracks {
        return Err(Error::Limit {
            kind: "tracks",
            maximum: limits.tracks,
        });
    }
    let base = memory.read_u32(player + 0x2c)?;
    if tracks != 0 {
        ram(base, (tracks * mplay::TRACK_BYTES) as u32)?;
    }
    for index in 0..tracks {
        let track = base + (index * mplay::TRACK_BYTES) as u32;
        if terminal {
            invoke(memory, calls, 0x0805e610, &[player, track])?;
            if memory.read_u16(player + 0x28)? & 1 == 0 {
                memory.write_u8(track, 0)?;
            }
        } else {
            let flags = memory.read_u8(track)?;
            if flags & 0x80 != 0 {
                let volume = memory.read_u16(player + 0x28)? >> 2;
                memory.write_u8(track + 0x13, volume as u8)?;
                memory.write_u8(track, flags | 3)?;
            }
        }
    }
    if terminal {
        let status = if memory.read_u16(player + 0x28)? & 1 != 0 {
            memory.read_u32(player + 4)? | 0x80000000
        } else {
            0x80000000
        };
        word(memory, player + 4, status)?;
        memory.write_u16(player + 0x24, 0)?;
    }
    Ok(())
}

/// Native trackStop 0x5e610. The PSG callback is read from the current global
/// sound-info binding on every required key-off, as in the original.
pub fn stop_track(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    track: u32,
    limits: Limits,
) -> Result<(), Error> {
    ram(track, mplay::TRACK_BYTES as u32)?;
    if memory.read_u8(track)? & 0x80 == 0 {
        return Ok(());
    }
    let mut channel = memory.read_u32(track + 0x20)?;
    let mut seen = Vec::new();
    let mut outcome = Outcome::default();
    while channel != 0 {
        next_channel(channel, &mut seen, &mut outcome, limits)?;
        if memory.read_u8(channel)? != 0 {
            let kind = memory.read_u8(channel + 1)? & 7;
            if kind != 0 {
                let sound = memory.read_u32(SOUND_INFO_SLOT)?;
                let target = memory.read_u32(add(sound, 0x2c)?)?;
                invoke(memory, calls, target, &[u32::from(kind)])?;
            }
            memory.write_u8(channel, 0)?;
        }
        word(memory, channel + 0x2c, 0)?;
        channel = memory.read_u32(channel + 0x34)?;
    }
    word(memory, track + 0x20, 0)
}

fn unlink(memory: &mut dyn FrameMemory, channel: u32) -> Result<(), Error> {
    ram(channel, mplay::CHANNEL_BYTES as u32)?;
    let owner = memory.read_u32(channel + 0x2c)?;
    if owner == 0 {
        return Ok(());
    }
    let next = memory.read_u32(channel + 0x34)?;
    let previous = memory.read_u32(channel + 0x30)?;
    if previous != 0 {
        ram(previous, mplay::CHANNEL_BYTES as u32)?;
        word(memory, previous + 0x34, next)?;
    } else {
        ram(owner, mplay::TRACK_BYTES as u32)?;
        word(memory, owner + 0x20, next)?;
    }
    if next != 0 {
        ram(next, mplay::CHANNEL_BYTES as u32)?;
        word(memory, next + 0x30, previous)?;
    }
    word(memory, channel + 0x2c, 0)
}
fn finish(memory: &mut dyn FrameMemory, track: u32, limits: Limits) -> Result<(), Error> {
    ram(track, mplay::TRACK_BYTES as u32)?;
    let mut channel = memory.read_u32(track + 0x20)?;
    let mut seen = Vec::new();
    let mut outcome = Outcome::default();
    while channel != 0 {
        next_channel(channel, &mut seen, &mut outcome, limits)?;
        let flags = memory.read_u8(channel)?;
        if flags & 0xc7 != 0 {
            memory.write_u8(channel, flags | 0x40)?;
        }
        unlink(memory, channel)?;
        channel = memory.read_u32(channel + 0x34)?;
    }
    memory.write_u8(track, 0)?;
    Ok(())
}
fn clear_lfo(memory: &mut dyn FrameMemory, track: u32) -> Result<(), Error> {
    memory.write_u8(track + 0x16, 0)?;
    memory.write_u8(track + 0x1a, 0)?;
    let mask = if memory.read_u8(track + 0x18)? == 0 {
        0x0c
    } else {
        3
    };
    let flags = memory.read_u8(track)?;
    memory.write_u8(track, flags | mask)?;
    Ok(())
}
fn lfo_command(memory: &mut dyn FrameMemory, track: u32, offset: u32) -> Result<(), Error> {
    ram(track, mplay::TRACK_BYTES as u32)?;
    let cursor = memory.read_u32(track + 0x40)?;
    // 0x5e8e0 writes the cursor before its raw, unprotected byte load.
    word(memory, track + 0x40, add(cursor, 1)?)?;
    let value = memory.read_u8(cursor)?;
    memory.write_u8(track + offset, value)?;
    if value == 0 {
        clear_lfo(memory, track)?;
    }
    Ok(())
}
fn end_tie(memory: &mut dyn FrameMemory, track: u32, limits: Limits) -> Result<(), Error> {
    ram(track, mplay::TRACK_BYTES as u32)?;
    let cursor = memory.read_u32(track + 0x40)?;
    let raw = memory.read_u8(cursor)?;
    let key = if raw < 0x80 {
        memory.write_u8(track + 5, raw)?;
        word(memory, track + 0x40, add(cursor, 1)?)?;
        raw
    } else {
        memory.read_u8(track + 5)?
    };
    let mut channel = memory.read_u32(track + 0x20)?;
    let mut seen = Vec::new();
    let mut outcome = Outcome::default();
    while channel != 0 {
        next_channel(channel, &mut seen, &mut outcome, limits)?;
        let flags = memory.read_u8(channel)?;
        if flags & 0x83 != 0 && flags & 0x40 == 0 && memory.read_u8(channel + 0x11)? == key {
            memory.write_u8(channel, flags | 0x40)?;
            break;
        }
        channel = memory.read_u32(channel + 0x34)?;
    }
    Ok(())
}

fn record<const N: usize>(memory: &dyn FrameMemory, address: u32) -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    for (index, value) in bytes.iter_mut().enumerate() {
        *value = memory.read_u8(add(address, index as u32)?)?;
    }
    Ok(bytes)
}
fn commit(memory: &mut dyn FrameMemory, address: u32, bytes: &[u8]) -> Result<(), Error> {
    for (index, value) in bytes.iter().enumerate() {
        memory.write_u8(add(address, index as u32)?, *value)?;
    }
    Ok(())
}
fn get_word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed native field"),
    )
}
#[derive(Default)]
struct Source(Vec<(u32, Vec<u8>)>);
impl Source {
    fn capture(
        &mut self,
        memory: &dyn FrameMemory,
        address: u32,
        length: usize,
    ) -> Result<(), Error> {
        let mut bytes = Vec::with_capacity(length);
        for index in 0..length {
            bytes.push(memory.read_u8(add(address, index as u32)?)?);
        }
        self.0.push((address, bytes));
        Ok(())
    }
}
impl super::sequence::SequenceData for Source {
    fn read(&self, address: u32, length: usize) -> Option<&[u8]> {
        self.0.iter().find_map(|(base, bytes)| {
            let index = address.checked_sub(*base)? as usize;
            bytes.get(index..index.checked_add(length)?)
        })
    }
}

/// Routes the proven A3VE leaves against the same canonical bounded memory as
/// the controller. Remaining note allocation, sample-rate and PSG callbacks
/// are delegated strictly. Register writes require a separate real port.
/// Command/instrument sources follow SequenceData's immutable non-MMIO contract.
pub struct NativeCalls<C = RejectCalls, R = super::sequence_extended::RejectRegisterIo> {
    pub callbacks: C,
    pub registers: R,
    pub limits: Limits,
}
impl Default for NativeCalls {
    fn default() -> Self {
        Self {
            callbacks: RejectCalls,
            registers: super::sequence_extended::RejectRegisterIo,
            limits: Limits::default(),
        }
    }
}
impl<C: Calls, R: super::sequence_extended::RegisterIo> Calls for NativeCalls<C, R> {
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        let operand = |value: Option<u32>| value.ok_or_else(|| Error::Operand(call.clone()));
        let limits = self.limits;
        match call.target {
            0x0805f294 => fade(memory, self, operand(call.r0)?, limits)?,
            0x0805f35c => track_parameters(memory, operand(call.r1)?)?,
            0x0805e654 => channel_volume(memory, operand(call.r4)?, operand(call.r5)?)?,
            0x0805e610 | 0x0805e611 => stop_track(memory, self, operand(call.r1)?, limits)?,
            0x0805ed90 | 0x0805eda4 => {
                let slot = if call.target == 0x0805ed90 {
                    0x020387c8
                } else {
                    0x020387cc
                };
                let target = memory.read_u32(slot)?;
                // These wrappers execute BX. An even pointer is ARM code,
                // not permission to run the adjacent Thumb source port.
                if target & 1 == 0 {
                    return self
                        .callbacks
                        .call(memory, Call::standard(target, &[operand(call.r0)?]));
                }
                return self.call(memory, Call::standard(target, &[operand(call.r0)?]));
            }
            0x0805e149 => unlink(memory, operand(call.r0)?)?,
            0x0805e169 => finish(memory, operand(call.r1)?, limits)?,
            0x0805e131 => {
                let address = operand(call.r0)?;
                ram(address, 64)?;
                for index in (0..64).step_by(4) {
                    word(memory, address + index, 0)?;
                }
            }
            0x0805e8ed => lfo_command(memory, operand(call.r1)?, 0x19)?,
            0x0805e901 => lfo_command(memory, operand(call.r1)?, 0x17)?,
            0x0805e885 => end_tie(memory, operand(call.r1)?, limits)?,
            target if target & 1 != 0 => {
                let entry = (target & !1).wrapping_sub(0x08000000);
                let simple = super::sequence::Handler::from_entry(entry);
                let extended = super::sequence_extended::Handler::from_entry(entry);
                if simple.is_err() && extended.is_err() {
                    return self.callbacks.call(memory, call);
                }
                let player = operand(call.r0)?;
                let track = operand(call.r1)?;
                ram(player, mplay::PLAYER_BYTES as u32)?;
                ram(track, mplay::TRACK_BYTES as u32)?;
                let mut p = record::<{ mplay::PLAYER_BYTES }>(memory, player)?;
                let mut t = record::<{ mplay::TRACK_BYTES }>(memory, track)?;
                let cursor = get_word(&t, 0x40);
                let mut source = Source::default();
                if let Ok(handler) = simple {
                    use super::sequence::Handler;
                    match handler {
                        Handler::Pattern if t[2] >= 3 => {
                            finish(memory, track, limits)?;
                            return Ok(0);
                        }
                        Handler::Goto | Handler::Pattern => source.capture(memory, cursor, 4)?,
                        Handler::Return => {}
                        Handler::Repeat => {
                            source.capture(memory, cursor, 1)?;
                            let raw = memory.read_u8(cursor)?;
                            let protected = if cursor < 0x02000000 { 0 } else { raw };
                            if raw == 0 || u32::from(t[3]) + 1 < u32::from(protected) {
                                source.capture(memory, add(cursor, 1)?, 4)?;
                            }
                        }
                        Handler::Voice => {
                            source.capture(memory, cursor, 1)?;
                            let base =
                                add(get_word(&p, 0x30), u32::from(memory.read_u8(cursor)?) * 12)?;
                            add(base, 11)?;
                            source.capture(memory, base, 12)?;
                        }
                        _ => source.capture(memory, cursor, 1)?,
                    }
                    super::sequence::apply(handler, &source, &mut p, &mut t)?;
                    commit(memory, player, &p)?;
                } else {
                    let handler = extended.expect("recognized extended entry");
                    source.capture(
                        memory,
                        cursor,
                        if matches!(handler, super::sequence_extended::Handler::RegisterByte) {
                            2
                        } else {
                            1
                        },
                    )?;
                    super::sequence_extended::apply(handler, &source, &mut t, &mut self.registers)?;
                }
                commit(memory, track, &t)?;
            }
            _ => return self.callbacks.call(memory, call),
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    fn bytes(value: &Value) -> Vec<u8> {
        let hex = value.as_str().unwrap();
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
            .collect()
    }
    #[derive(Default)]
    struct Memory(BTreeMap<u32, u8>);
    impl Memory {
        fn from_spans(spans: &Value) -> Self {
            let mut result = Self::default();
            for row in spans.as_array().unwrap() {
                result.bind(
                    row["address"].as_u64().unwrap() as u32,
                    &bytes(&row["bytes"]),
                );
            }
            result
        }
        fn bind(&mut self, address: u32, data: &[u8]) {
            for (index, value) in data.iter().enumerate() {
                assert!(
                    self.0.insert(address + index as u32, *value).is_none(),
                    "overlapping synthetic memory"
                );
            }
        }
        fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], FrameError> {
            let mut bytes = [0; N];
            for (i, value) in bytes.iter_mut().enumerate() {
                *value = *self
                    .0
                    .get(&(address + i as u32))
                    .ok_or(FrameError::MissingRead {
                        address,
                        width: N as u8,
                    })?;
            }
            Ok(bytes)
        }
        fn write(&mut self, address: u32, data: &[u8]) -> Result<(), FrameError> {
            for i in 0..data.len() {
                if !self.0.contains_key(&(address + i as u32)) {
                    return Err(FrameError::MissingWrite {
                        address,
                        width: data.len() as u8,
                    });
                }
            }
            for (i, value) in data.iter().enumerate() {
                self.0.insert(address + i as u32, *value);
            }
            Ok(())
        }
        fn check(&self, spans: &Value, context: &str) {
            for row in spans.as_array().unwrap() {
                let address = row["address"].as_u64().unwrap() as u32;
                let expected = bytes(&row["bytes"]);
                let actual = (0..expected.len())
                    .map(|i| self.read_u8(address + i as u32).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "{context}: canonical span {address:08x}");
            }
        }
    }
    impl FrameMemory for Memory {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            Ok(self.read::<1>(address)?[0])
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes(self.read(address)?))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes(self.read(address)?))
        }
        fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            self.write(address, &[value])
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.write(address, &value.to_le_bytes())
        }
    }
    struct Scripted<'a> {
        actions: &'a [Value],
        calls: Vec<Value>,
        occurrences: BTreeMap<u32, usize>,
    }
    impl<'a> Scripted<'a> {
        fn new(case: &'a Value) -> Self {
            Self {
                actions: case["actions"].as_array().unwrap(),
                calls: vec![],
                occurrences: BTreeMap::new(),
            }
        }
    }
    impl Calls for Scripted<'_> {
        fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
            self.calls.push(json!({"target":call.target,"r0":call.r0,"r1":call.r1,"r2":call.r2,"r4":call.r4,"r5":call.r5}));
            let occurrence = self.occurrences.entry(call.target).or_default();
            *occurrence += 1;
            let mut result = 0;
            for action in self.actions {
                if action["target"].as_u64().unwrap() as u32 != call.target
                    || action["occurrence"].as_u64().unwrap() as usize != *occurrence
                {
                    continue;
                }
                if let Some(writes) = action["writes"].as_array() {
                    for write in writes {
                        let address = write[0].as_u64().unwrap() as u32;
                        let width = write[1].as_u64().unwrap() as usize;
                        let data = (write[2].as_u64().unwrap() as u32).to_le_bytes();
                        for (i, value) in data[..width].iter().enumerate() {
                            memory.write_u8(address + i as u32, *value)?;
                        }
                    }
                }
                if let Some(value) = action["return"].as_u64() {
                    result = value as u32;
                }
            }
            Ok(result)
        }
    }
    fn vectors() -> Value {
        serde_json::from_str(include_str!("../../tests/sequencer-vectors.json")).unwrap()
    }
    #[test]
    fn tick_matches_256_original_controller_cases_with_intercepted_dependencies() {
        let vectors = vectors();
        let cases = vectors["controller"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let mut calls = Scripted::new(case);
            let outcome = tick(&mut memory, &mut calls, 0x02001000, Limits::default()).unwrap();
            assert_eq!(
                calls.calls,
                *case["calls"].as_array().unwrap(),
                "native call operands/order case {index}"
            );
            memory.check(&case["after"], &format!("controller {index}"));
            assert_eq!(
                outcome.accepted,
                memory.read_u32(0x02001034).unwrap() == mplay::SIGNATURE
            );
        }
    }
    #[test]
    fn helpers_match_768_original_instruction_cases() {
        let vectors = vectors();
        let cases = vectors["leaves"].as_array().unwrap();
        assert_eq!(cases.len(), 768);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let mut calls = Scripted::new(case);
            match case["entry"].as_u64().unwrap() {
                0x5f35c => track_parameters(&mut memory, 0x02002000).unwrap(),
                0x5e654 => channel_volume(&mut memory, 0x02003000, 0x02002000).unwrap(),
                0x5e8ec => lfo_command(&mut memory, 0x02002000, 0x19).unwrap(),
                0x5e900 => lfo_command(&mut memory, 0x02002000, 0x17).unwrap(),
                0x5e884 => end_tie(&mut memory, 0x02002000, Limits::default()).unwrap(),
                0x5f294 => fade(&mut memory, &mut calls, 0x02001000, Limits::default()).unwrap(),
                _ => panic!("unknown native fixture entry"),
            }
            assert_eq!(
                calls.calls,
                *case["calls"].as_array().unwrap(),
                "native helper boundary case {index}"
            );
            memory.check(&case["after"], &format!("helper {index}"));
        }
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; original A3VE dispatch/wait resources remain local"]
    fn native_leaf_compositions_match_128_original_ticks() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let tables = NativeTables::load(&rom).unwrap();
        let vectors = vectors();
        for source in vectors["routines"].as_array().unwrap() {
            let offset = source["offset"].as_u64().unwrap() as usize;
            let length = source["length"].as_u64().unwrap() as usize;
            assert_eq!(
                crate::sha256(&rom[offset..offset + length]),
                source["sha256"].as_str().unwrap()
            );
        }
        let cases = vectors["native"].as_array().unwrap();
        assert_eq!(cases.len(), 128);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let table = tables
                .entries()
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            memory.bind(0x02006000, &table);
            memory.bind(0x08000000 + WAIT_OFFSET as u32, tables.waits());
            let mut calls = NativeCalls {
                callbacks: Scripted::new(case),
                registers: super::super::sequence_extended::RejectRegisterIo,
                limits: Limits::default(),
            };
            tick(&mut memory, &mut calls, 0x02001000, Limits::default()).unwrap();
            assert_eq!(
                calls.callbacks.calls,
                *case["calls"].as_array().unwrap(),
                "native remaining callback order case {index}"
            );
            memory.check(&case["after"], &format!("native composition {index}"));
        }
    }
    #[test]
    fn budgets_cycles_and_unknown_callbacks_stop_explicitly() {
        let fixture = vectors();
        let case = &fixture["controller"][15];
        let mut memory = Memory::from_spans(&case["input"]);
        let mut calls = Scripted::new(case);
        let limits = Limits {
            commands: 2,
            ..Limits::default()
        };
        assert_eq!(
            tick(&mut memory, &mut calls, 0x02001000, limits),
            Err(Error::Limit {
                kind: "commands",
                maximum: 2
            })
        );
        assert_eq!(
            memory.read_u32(0x02001034).unwrap(),
            mplay::SIGNATURE + 1,
            "incomplete native tick retains signature lock"
        );
        let mut memory = Memory::from_spans(&fixture["controller"][6]["input"]);
        word(&mut memory, 0x02001038, 0).unwrap();
        let mut calls = NativeCalls::default();
        assert!(matches!(
            tick(&mut memory, &mut calls, 0x02001000, Limits::default()),
            Err(Error::UnportedCall(Call {
                target: 0x0200a011,
                ..
            }))
        ));
        let mut memory = Memory::from_spans(&fixture["leaves"][512]["input"]);
        memory.write_u8(0x02008000, 128).unwrap();
        memory.write_u8(0x02003000, 0).unwrap();
        word(&mut memory, 0x02003034, 0x02003000).unwrap();
        assert_eq!(
            end_tie(&mut memory, 0x02002000, Limits::default()),
            Err(Error::ChannelCycle(0x02003000))
        );
        let mut memory = Memory::default();
        memory.bind(0x020387c8, &0x0805e148u32.to_le_bytes());
        assert!(matches!(
            calls.call(&mut memory, Call::standard(0x0805ed90, &[0x02003000])),
            Err(Error::UnportedCall(Call {
                target: 0x0805e148,
                ..
            }))
        ));
        assert!(NativeTables::load(&[0; 144]).is_err());
    }
}
