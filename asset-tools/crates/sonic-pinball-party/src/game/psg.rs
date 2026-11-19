//! A3VE PSG key-off, gain/pan and four-channel register update source ports.
//! Register ports must preserve native width/order and read-after-write effects.
//! A captured register trace is not oscillator execution or audible playback.
use super::{
    frame::{FrameError, FrameMemory},
    mplay,
    sequencer::{self, Call, Calls, Error},
};

pub const OFFSET: usize = 0x5f570;
pub const LENGTH: usize = 0x44c;
pub const SHA256: &str = "1a723c2dca0fd2363118819467f369cd5814accdc367a432e760158af600e5a3";
pub const KEY_OFF_OFFSET: usize = 0x5f4b8;
pub const KEY_OFF_LENGTH: usize = 0x50;
pub const KEY_OFF_SHA256: &str = "67a2670376360f300df5a7d18b458792da50002de9bcde7580342dcf8c366fcf";
pub const GAIN_OFFSET: usize = 0x5f508;
pub const GAIN_LENGTH: usize = 0x68;
pub const GAIN_SHA256: &str = "584762a3caa5fe9d71f2afd7fe58f2e5a8d2ef8efb2c4d68df84a621b84c9b88";
pub const UPDATE_BINDING_OFFSET: usize = 0x5ed78;
pub const KEY_OFF_BINDING_OFFSET: usize = 0x5ed7c;
pub const WAVE_CURVE_OFFSET: usize = 0xf61bc;
pub const WAVE_CURVE_BYTES: usize = 16;
pub const WAVE_CURVE_SHA256: &str =
    "86480498f3bc57863a295f2c9a9752be09fd91cb28c998650fa2b7ae3bdefd24";
/// The native lookup adds an unrestricted u8 channel amplitude. Preserve its
/// bounded 256-byte read extent, including bytes after the 16-byte curve;
/// those adjacent bytes are not asserted to be additional amplitude entries.
pub const LOOKUP_BYTES: usize = 256;
pub const LOOKUP_SHA256: &str = "2a91d7b83d8f677f2901d30d3c05074db85ee857a6416fdad13db84e45e318a7";

#[derive(Clone)]
pub struct Tables {
    wave: [u8; LOOKUP_BYTES],
}
impl Tables {
    pub fn load(rom: &[u8]) -> Result<Self, Error> {
        crate::catalog::validate_rom(rom).map_err(|_| Error::Source)?;
        let wave =
            crate::binary::span(rom, WAVE_CURVE_OFFSET, LOOKUP_BYTES).map_err(|_| Error::Source)?;
        if crate::sha256(wave) != LOOKUP_SHA256
            || crate::sha256(&wave[..WAVE_CURVE_BYTES]) != WAVE_CURVE_SHA256
        {
            return Err(Error::Source);
        }
        Ok(Self {
            wave: wave
                .try_into()
                .expect("exact verified source lookup extent"),
        })
    }
}

/// These are actual native device accesses. Implementations must bind reads
/// and perform writes with the stated width. Synthetic storage used by tests
/// is explicitly capture-only; missing hardware is never a successful no-op.
pub trait RegisterIo {
    fn read_byte(&mut self, address: u32) -> Result<u8, FrameError>;
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), FrameError>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), FrameError>;
}
pub struct RejectRegisterIo;
impl RegisterIo for RejectRegisterIo {
    fn read_byte(&mut self, address: u32) -> Result<u8, FrameError> {
        Err(FrameError::MissingRead { address, width: 1 })
    }
    fn write_byte(&mut self, address: u32, _: u8) -> Result<(), FrameError> {
        Err(FrameError::MissingWrite { address, width: 1 })
    }
    fn write_word(&mut self, address: u32, _: u32) -> Result<(), FrameError> {
        Err(FrameError::MissingWrite { address, width: 4 })
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub phase_steps: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self { phase_steps: 64 }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    pub active_channels: usize,
    pub stopped_channels: usize,
    pub phase_steps: usize,
}

fn add(address: u32, amount: u32) -> Result<u32, Error> {
    address.checked_add(amount).ok_or(Error::Address(address))
}
fn ram(address: u32, bytes: u32) -> Result<(), Error> {
    let end = add(address, bytes)?;
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
fn or_byte(memory: &mut dyn FrameMemory, address: u32, mask: u8) -> Result<(), Error> {
    let value = memory.read_u8(address)?;
    memory.write_u8(address, value | mask)?;
    Ok(())
}

/// Complete native 0x5f4b8. Inputs are byte-truncated; kinds other than 1..3
/// take the native noise-channel branch, including zero and values above four.
pub fn key_off(io: &mut dyn RegisterIo, kind: u32) -> Result<(), Error> {
    let (volume, trigger) = match kind as u8 {
        1 => (0x04000063, 0x04000065),
        2 => (0x04000069, 0x0400006d),
        3 => {
            io.write_byte(0x04000070, 0)?;
            return Ok(());
        }
        _ => (0x04000079, 0x0400007d),
    };
    io.write_byte(volume, 8)?;
    io.write_byte(trigger, 0x80)?;
    Ok(())
}

/// Complete pure native 0x5f508. The balanced-pan branch intentionally does
/// not share the one-sided branch's 15 cap; native truncation is preserved.
pub fn gain_pan(memory: &mut dyn FrameMemory, channel: u32) -> Result<(), Error> {
    ram(channel, mplay::CHANNEL_BYTES as u32)?;
    let left = memory.read_u8(channel + 2)?;
    let right = memory.read_u8(channel + 3)?;
    let side = if left >= right && left >> 1 >= right {
        Some(0x0f)
    } else if left < right && right >> 1 >= left {
        Some(0xf0)
    } else {
        None
    };
    memory.write_u8(channel + 0x1b, side.unwrap_or(0xff))?;
    let mut volume = (u32::from(left) + u32::from(right)) >> 4;
    if side.is_some() {
        volume = volume.min(15);
    }
    memory.write_u8(channel + 0x0a, volume as u8)?;
    let sustain = (u32::from(memory.read_u8(channel + 6)?) * volume + 15) >> 4;
    memory.write_u8(channel + 0x19, sustain as u8)?;
    let mask = memory.read_u8(channel + 0x1c)?;
    let pan = memory.read_u8(channel + 0x1b)?;
    memory.write_u8(channel + 0x1b, mask & pan)?;
    Ok(())
}

#[derive(Clone, Copy)]
struct Registers {
    control: u32,
    length: u32,
    envelope: u32,
    frequency: u32,
    trigger: u32,
}
impl Registers {
    fn for_kind(kind: u32) -> Self {
        match kind {
            1 => Self {
                control: 0x04000060,
                length: 0x04000062,
                envelope: 0x04000063,
                frequency: 0x04000064,
                trigger: 0x04000065,
            },
            2 => Self {
                control: 0x04000061,
                length: 0x04000068,
                envelope: 0x04000069,
                frequency: 0x0400006c,
                trigger: 0x0400006d,
            },
            3 => Self {
                control: 0x04000070,
                length: 0x04000072,
                envelope: 0x04000073,
                frequency: 0x04000074,
                trigger: 0x04000075,
            },
            _ => Self {
                control: 0x04000071,
                length: 0x04000078,
                envelope: 0x04000079,
                frequency: 0x0400007c,
                trigger: 0x0400007d,
            },
        }
    }
}
#[derive(Clone, Copy)]
enum Phase {
    Check,
    DecayEnd,
    DecayStart,
    Sustain,
    Echo,
    Advance,
    Effects,
    Stop,
}
struct ChannelSlot {
    address: u32,
    kind: u32,
}

fn channel_update(
    memory: &mut dyn FrameMemory,
    io: &mut dyn RegisterIo,
    tables: &Tables,
    slot: ChannelSlot,
    tick_clock: u8,
    limits: Limits,
    outcome: &mut Outcome,
) -> Result<(), Error> {
    let channel = slot.address;
    let kind = slot.kind;
    let registers = Registers::for_kind(kind);
    let mut clock = u32::from(tick_clock);
    // Native envelope read precedes the new-note/release decision.
    let mut envelope = u32::from(io.read_byte(registers.envelope)?);
    let flags = memory.read_u8(channel)?;
    let mut phase;
    if flags & 0x80 != 0 {
        if flags & 0x40 != 0 {
            phase = Phase::Stop;
        } else {
            memory.write_u8(channel, 3)?;
            memory.write_u8(channel + 0x1d, 3)?;
            gain_pan(memory, channel)?;
            match kind {
                1 | 2 => {
                    if kind == 1 {
                        io.write_byte(registers.control, memory.read_u8(channel + 0x1f)?)?;
                    }
                    let duty = memory
                        .read_u32(channel + 0x24)?
                        .wrapping_shl(6)
                        .wrapping_add(u32::from(memory.read_u8(channel + 0x1e)?));
                    io.write_byte(registers.length, duty as u8)?;
                }
                3 => {
                    let wave = memory.read_u32(channel + 0x24)?;
                    if wave != memory.read_u32(channel + 0x28)? {
                        io.write_byte(registers.control, 0x40)?;
                        // The source reloads the pointer after the control write.
                        let wave = memory.read_u32(channel + 0x24)?;
                        for index in 0..4 {
                            let value = memory.read_u32(add(wave, index * 4)?)?;
                            io.write_word(0x04000090 + index * 4, value)?;
                        }
                        word(memory, channel + 0x28, wave)?;
                    }
                    io.write_byte(registers.control, 0)?;
                    let length = memory.read_u8(channel + 0x1e)?;
                    io.write_byte(registers.length, length)?;
                    memory.write_u8(channel + 0x1a, if length == 0 { 0x80 } else { 0xc0 })?;
                }
                _ => {
                    io.write_byte(registers.length, memory.read_u8(channel + 0x1e)?)?;
                    let noise = memory.read_u32(channel + 0x24)?.wrapping_shl(3);
                    io.write_byte(registers.frequency, noise as u8)?;
                }
            }
            if kind != 3 {
                envelope = u32::from(memory.read_u8(channel + 4)?) + 8;
                let length = memory.read_u8(channel + 0x1e)?;
                memory.write_u8(channel + 0x1a, if length == 0 { 0 } else { 0x40 })?;
            }
            let attack = memory.read_u8(channel + 4)?;
            memory.write_u8(channel + 0x0b, attack)?;
            if attack == 0 {
                phase = Phase::DecayStart;
            } else {
                memory.write_u8(channel + 9, 0)?;
                phase = Phase::Advance;
            }
        }
    } else if flags & 4 != 0 {
        let remaining = memory.read_u8(channel + 0x0d)?.wrapping_sub(1);
        memory.write_u8(channel + 0x0d, remaining)?;
        phase = if remaining as i8 <= 0 {
            Phase::Stop
        } else {
            Phase::Effects
        };
    } else if flags & 0x40 != 0 && flags & 3 != 0 {
        memory.write_u8(channel, flags & 0xfc)?;
        let release = memory.read_u8(channel + 7)?;
        memory.write_u8(channel + 0x0b, release)?;
        if release == 0 {
            phase = Phase::Echo;
        } else {
            or_byte(memory, channel + 0x1d, 1)?;
            if kind != 3 {
                envelope = u32::from(release);
            }
            phase = Phase::Advance;
        }
    } else {
        phase = Phase::Check;
    }
    let mut steps = 0;
    loop {
        if steps >= limits.phase_steps {
            return Err(Error::Limit {
                kind: "PSG phase steps",
                maximum: limits.phase_steps,
            });
        }
        steps += 1;
        outcome.phase_steps += 1;
        phase = match phase {
            Phase::Check => {
                if memory.read_u8(channel + 0x0b)? != 0 {
                    Phase::Advance
                } else {
                    if kind == 3 {
                        or_byte(memory, channel + 0x1d, 1)?;
                    }
                    gain_pan(memory, channel)?;
                    match memory.read_u8(channel)? & 3 {
                        0 => {
                            let level = memory.read_u8(channel + 9)?.wrapping_sub(1);
                            memory.write_u8(channel + 9, level)?;
                            if level as i8 > 0 {
                                let release = memory.read_u8(channel + 7)?;
                                memory.write_u8(channel + 0x0b, release)?;
                                Phase::Advance
                            } else {
                                Phase::Echo
                            }
                        }
                        1 => Phase::Sustain,
                        2 => {
                            let level = memory.read_u8(channel + 9)?.wrapping_sub(1);
                            memory.write_u8(channel + 9, level)?;
                            let sustain = memory.read_u8(channel + 0x19)?;
                            if level as i8 > sustain as i8 {
                                let decay = memory.read_u8(channel + 5)?;
                                memory.write_u8(channel + 0x0b, decay)?;
                                Phase::Advance
                            } else {
                                Phase::DecayEnd
                            }
                        }
                        _ => {
                            let level = memory.read_u8(channel + 9)?.wrapping_add(1);
                            memory.write_u8(channel + 9, level)?;
                            if level < memory.read_u8(channel + 0x0a)? {
                                let attack = memory.read_u8(channel + 4)?;
                                memory.write_u8(channel + 0x0b, attack)?;
                                Phase::Advance
                            } else {
                                Phase::DecayStart
                            }
                        }
                    }
                }
            }
            Phase::DecayStart => {
                let flags = memory.read_u8(channel)?.wrapping_sub(1);
                memory.write_u8(channel, flags)?;
                let decay = memory.read_u8(channel + 5)?;
                memory.write_u8(channel + 0x0b, decay)?;
                if decay == 0 {
                    Phase::DecayEnd
                } else {
                    or_byte(memory, channel + 0x1d, 1)?;
                    let volume = memory.read_u8(channel + 0x0a)?;
                    memory.write_u8(channel + 9, volume)?;
                    if kind != 3 {
                        envelope = u32::from(decay);
                    }
                    Phase::Advance
                }
            }
            Phase::DecayEnd => {
                if memory.read_u8(channel + 6)? == 0 {
                    let flags = memory.read_u8(channel)?;
                    memory.write_u8(channel, flags & 0xfc)?;
                    Phase::Echo
                } else {
                    let flags = memory.read_u8(channel)?.wrapping_sub(1);
                    memory.write_u8(channel, flags)?;
                    or_byte(memory, channel + 0x1d, 1)?;
                    if kind != 3 {
                        envelope = 8;
                    }
                    Phase::Sustain
                }
            }
            Phase::Sustain => {
                let sustain = memory.read_u8(channel + 0x19)?;
                memory.write_u8(channel + 9, sustain)?;
                memory.write_u8(channel + 0x0b, 7)?;
                Phase::Advance
            }
            Phase::Echo => {
                let volume = (u32::from(memory.read_u8(channel + 0x0c)?)
                    * u32::from(memory.read_u8(channel + 0x0a)?)
                    + 255)
                    >> 8;
                memory.write_u8(channel + 9, volume as u8)?;
                if volume as u8 == 0 {
                    Phase::Stop
                } else {
                    or_byte(memory, channel, 4)?;
                    or_byte(memory, channel + 0x1d, 1)?;
                    if kind != 3 {
                        envelope = 8;
                    }
                    Phase::Effects
                }
            }
            Phase::Advance => {
                let countdown = memory.read_u8(channel + 0x0b)?.wrapping_sub(1);
                memory.write_u8(channel + 0x0b, countdown)?;
                if clock == 0 {
                    clock = u32::MAX;
                    Phase::Check
                } else {
                    Phase::Effects
                }
            }
            Phase::Stop => {
                key_off(io, kind)?;
                memory.write_u8(channel, 0)?;
                memory.write_u8(channel + 0x1d, 0)?;
                outcome.stopped_channels += 1;
                return Ok(());
            }
            Phase::Effects => {
                break;
            }
        };
    }
    if memory.read_u8(channel + 0x1d)? & 2 != 0 {
        if kind <= 3 && memory.read_u8(channel + 1)? & 8 != 0 {
            let bias = io.read_byte(0x04000089)?;
            let delta = if bias <= 0x3f {
                Some((2, 0x7fc))
            } else if bias <= 0x7f {
                Some((1, 0x7fe))
            } else {
                None
            };
            if let Some((delta, mask)) = delta {
                let pitch = memory.read_u32(channel + 0x20)?.wrapping_add(delta) & mask;
                word(memory, channel + 0x20, pitch)?;
            }
        }
        let pitch = memory.read_u32(channel + 0x20)?;
        if kind == 4 {
            let low = io.read_byte(registers.frequency)? & 8;
            io.write_byte(registers.frequency, (pitch | u32::from(low)) as u8)?;
        } else {
            io.write_byte(registers.frequency, pitch as u8)?;
        }
        let high =
            (memory.read_u8(channel + 0x1a)? & 0xc0).wrapping_add(memory.read_u8(channel + 0x21)?);
        memory.write_u8(channel + 0x1a, high)?;
        io.write_byte(registers.trigger, high)?;
    }
    if memory.read_u8(channel + 0x1d)? & 1 != 0 {
        let pan = (io.read_byte(0x04000081)? & !memory.read_u8(channel + 0x1c)?)
            | memory.read_u8(channel + 0x1b)?;
        io.write_byte(0x04000081, pan)?;
        if kind == 3 {
            let level = memory.read_u8(channel + 9)?;
            io.write_byte(registers.envelope, tables.wave[usize::from(level)])?;
            if memory.read_u8(channel + 0x1a)? & 0x80 != 0 {
                io.write_byte(registers.control, 0x80)?;
                let high = memory.read_u8(channel + 0x1a)?;
                io.write_byte(registers.trigger, high)?;
                memory.write_u8(channel + 0x1a, high & 0x7f)?;
            }
        } else {
            envelope &= 15;
            let value = (u32::from(memory.read_u8(channel + 9)?) << 4) + envelope;
            io.write_byte(registers.envelope, value as u8)?;
            let high = memory.read_u8(channel + 0x1a)? | 0x80;
            io.write_byte(registers.trigger, high)?;
            if kind == 1 && io.read_byte(registers.control)? & 8 == 0 {
                let high = memory.read_u8(channel + 0x1a)? | 0x80;
                io.write_byte(registers.trigger, high)?;
            }
        }
    }
    memory.write_u8(channel + 0x1d, 0)?;
    Ok(())
}

/// Complete native 0x5f570. Exactly four consecutive PSG records are visited;
/// inactive records retain their dirty byte. Shared clock and original device
/// reads/writes occur in source order, including duplicate trigger writes.
pub fn update(
    memory: &mut dyn FrameMemory,
    io: &mut dyn RegisterIo,
    tables: &Tables,
    limits: Limits,
) -> Result<Outcome, Error> {
    let sound = memory.read_u32(sequencer::SOUND_INFO_SLOT)?;
    ram(sound, 0x40)?;
    let clock = memory.read_u8(sound + 0x0a)?;
    memory.write_u8(sound + 0x0a, if clock == 0 { 14 } else { clock - 1 })?;
    let base = memory.read_u32(sound + 0x1c)?;
    ram(base, 4 * mplay::CHANNEL_BYTES as u32)?;
    let mut outcome = Outcome::default();
    for kind in 1..=4 {
        let channel = base + (kind - 1) * mplay::CHANNEL_BYTES as u32;
        if memory.read_u8(channel)? & 0xc7 == 0 {
            continue;
        }
        outcome.active_channels += 1;
        let clock = memory.read_u8(sound + 0x0a)?;
        channel_update(
            memory,
            io,
            tables,
            ChannelSlot {
                address: channel,
                kind,
            },
            clock,
            limits,
            &mut outcome,
        )?;
    }
    Ok(outcome)
}

/// May be installed as the sequencer's callback port. Only actual known Thumb
/// bindings are routed; unknown or ARM pointers reach the strict delegate.
pub struct NativeCalls<I, C = sequencer::RejectCalls> {
    pub io: I,
    pub tables: Tables,
    pub callbacks: C,
    pub limits: Limits,
}
impl<I: RegisterIo, C: Calls> Calls for NativeCalls<I, C> {
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        match call.target {
            0x0805f571 => {
                update(memory, &mut self.io, &self.tables, self.limits)?;
                Ok(0)
            }
            0x0805f4b9 => {
                key_off(
                    &mut self.io,
                    call.r0.ok_or_else(|| Error::Operand(call.clone()))?,
                )?;
                Ok(0)
            }
            _ => self.callbacks.call(memory, call),
        }
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
    fn vectors() -> Value {
        serde_json::from_str(include_str!("../../tests/psg-vectors.json")).unwrap()
    }
    fn synthetic_tables(vectors: &Value) -> Tables {
        Tables {
            wave: bytes(&vectors["syntheticLookup"]).try_into().unwrap(),
        }
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
    /// Matches only the oracle's mapped synthetic storage. In particular this
    /// does not simulate write-only bits, wave-bank selection or oscillators.
    struct CapturedRegisters {
        memory: Memory,
        accesses: Vec<Value>,
    }
    impl CapturedRegisters {
        fn new(case: &Value) -> Self {
            let mut memory = Memory::default();
            memory.bind(0x04000060, &bytes(&case["device"]));
            Self {
                memory,
                accesses: vec![],
            }
        }
        fn check(&self, case: &Value, context: &str) {
            assert_eq!(
                self.accesses,
                *case["accesses"].as_array().unwrap(),
                "{context}: exact MMIO width/order/value"
            );
            let expected = bytes(&case["device_after"]);
            let actual = (0..expected.len())
                .map(|i| self.memory.read_u8(0x04000060 + i as u32).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{context}: mapped register storage");
        }
    }
    impl RegisterIo for CapturedRegisters {
        fn read_byte(&mut self, address: u32) -> Result<u8, FrameError> {
            let value = self.memory.read_u8(address)?;
            self.accesses.push(json!(["read", address, 1, value]));
            Ok(value)
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            self.memory.write_u8(address, value)?;
            self.accesses.push(json!(["write", address, 1, value]));
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), FrameError> {
            self.memory.write(address, &value.to_le_bytes())?;
            self.accesses.push(json!(["write", address, 4, value]));
            Ok(())
        }
    }
    #[test]
    fn key_off_and_gain_match_768_unhooked_original_calls() {
        let vectors = vectors();
        let cases = vectors["key_off"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (index, case) in cases.iter().enumerate() {
            let mut io = CapturedRegisters::new(case);
            key_off(&mut io, case["kind"].as_u64().unwrap() as u32).unwrap();
            io.check(case, &format!("key-off {index}"));
        }
        let cases = vectors["gain"].as_array().unwrap();
        assert_eq!(cases.len(), 512);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            gain_pan(&mut memory, 0x02002000).unwrap();
            memory.check(&case["after"], &format!("gain/pan {index}"));
            assert!(case["accesses"].as_array().unwrap().is_empty());
        }
    }
    fn check_updates(cases: &Value, tables: &Tables) {
        for (index, case) in cases.as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let mut io = CapturedRegisters::new(case);
            update(&mut memory, &mut io, tables, Limits::default()).unwrap();
            memory.check(&case["after"], &format!("PSG update {index}"));
            io.check(case, &format!("PSG update {index}"));
        }
    }
    #[test]
    fn update_matches_1024_unhooked_original_calls_with_synthetic_lookup() {
        let vectors = vectors();
        assert_eq!(vectors["synthetic"].as_array().unwrap().len(), 1024);
        let cases = vectors["synthetic"].as_array().unwrap();
        let wave_words = cases
            .iter()
            .flat_map(|case| case["accesses"].as_array().unwrap())
            .filter(|row| row[0] == "write" && row[2] == 4)
            .count();
        assert_eq!(
            wave_words, 160,
            "source cases must exercise real-width wave uploads"
        );
        let duplicate_triggers = cases
            .iter()
            .map(|case| {
                case["accesses"]
                    .as_array()
                    .unwrap()
                    .windows(3)
                    .filter(|rows| {
                        rows[0] == rows[2]
                            && rows[0][0] == "write"
                            && rows[0][1] == 0x04000065
                            && rows[1][0] == "read"
                            && rows[1][1] == 0x04000060
                    })
                    .count()
            })
            .sum::<usize>();
        assert_eq!(
            duplicate_triggers, 238,
            "source cases must preserve duplicate sweep trigger writes"
        );
        check_updates(&vectors["synthetic"], &synthetic_tables(&vectors));
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; native amplitude lookup extent remains local"]
    fn update_matches_512_unhooked_original_calls_with_native_lookup() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let tables = Tables::load(&rom).unwrap();
        let vectors = vectors();
        for source in vectors["routines"].as_array().unwrap() {
            let offset = source["offset"].as_u64().unwrap() as usize;
            let length = source["length"].as_u64().unwrap() as usize;
            assert_eq!(
                crate::sha256(&rom[offset..offset + length]),
                source["sha256"].as_str().unwrap()
            );
        }
        assert_eq!(
            u32::from_le_bytes(
                rom[UPDATE_BINDING_OFFSET..UPDATE_BINDING_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0805f571
        );
        assert_eq!(
            u32::from_le_bytes(
                rom[KEY_OFF_BINDING_OFFSET..KEY_OFF_BINDING_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0805f4b9
        );
        assert_eq!(vectors["native"].as_array().unwrap().len(), 512);
        check_updates(&vectors["native"], &tables);
    }
    #[test]
    fn missing_hardware_wave_data_and_budgets_stop_without_completion() {
        let vectors = vectors();
        let tables = synthetic_tables(&vectors);
        let case = &vectors["synthetic"][0];
        let mut memory = Memory::from_spans(&case["input"]);
        let old_clock = memory.read_u8(0x0200100a).unwrap();
        assert!(matches!(
            update(
                &mut memory,
                &mut RejectRegisterIo,
                &tables,
                Limits::default()
            ),
            Err(Error::Memory(FrameError::MissingRead { width: 1, .. }))
        ));
        assert_eq!(
            memory.read_u8(0x0200100a).unwrap(),
            if old_clock == 0 { 14 } else { old_clock - 1 }
        );
        let mut memory = Memory::from_spans(&case["input"]);
        let mut io = CapturedRegisters::new(case);
        assert_eq!(
            update(&mut memory, &mut io, &tables, Limits { phase_steps: 0 }),
            Err(Error::Limit {
                kind: "PSG phase steps",
                maximum: 0
            })
        );
        let mut memory = Memory::from_spans(&case["input"]);
        for index in 0..4 {
            memory.write_u8(0x02002000 + index * 64, 0).unwrap();
        }
        memory.write_u8(0x02002080, 0x80).unwrap();
        word(&mut memory, 0x020020a4, 0x0203f000).unwrap();
        word(&mut memory, 0x020020a8, 0).unwrap();
        let mut io = CapturedRegisters::new(case);
        assert!(matches!(
            update(&mut memory, &mut io, &tables, Limits::default()),
            Err(Error::Memory(FrameError::MissingRead {
                address: 0x0203f000,
                width: 4
            }))
        ));
        assert!(io.accesses.contains(&json!(["write", 0x04000070, 1, 0x40])));
        assert!(matches!(
            key_off(&mut RejectRegisterIo, 1),
            Err(Error::Memory(FrameError::MissingWrite {
                address: 0x04000063,
                width: 1
            }))
        ));
        let mut calls = NativeCalls {
            io: RejectRegisterIo,
            tables,
            callbacks: sequencer::RejectCalls,
            limits: Limits::default(),
        };
        let unknown = Call {
            target: 0x0805f4b8,
            r0: Some(1),
            r1: None,
            r2: None,
            r4: None,
            r5: None,
        };
        assert_eq!(
            calls.call(&mut memory, unknown.clone()),
            Err(Error::UnportedCall(unknown))
        );
        assert!(Tables::load(&[0; 256]).is_err());
    }
}
