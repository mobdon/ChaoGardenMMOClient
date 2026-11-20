//! A3VE note/instrument/channel allocation and sample-rate calculation.
//! This ports native selection and state writes, not a MIDI interpretation,
//! oscillator, DMA or PCM mixer. Only the proven PSG-rate binding is routed;
//! unknown device callbacks remain explicit effects.
use super::{
    frame::FrameMemory,
    mplay,
    sequencer::{self, Call, Calls, Error},
};

pub const OFFSET: usize = 0x5e684;
pub const LENGTH: usize = 0x200;
pub const SHA256: &str = "d6674254ab734e962dcde6274c080175cc66fd8bba6f855d7d8dee3cf0a10ca1";
pub const PITCH_OFFSET: usize = 0x5e914;
pub const PITCH_LENGTH: usize = 0x64;
pub const PITCH_SHA256: &str = "d0a5917a15e89ff2b3a224d270d63c04e263b3d09c88af825804377d9eacde9c";
pub const MULTIPLY_OFFSET: usize = 0x5dcf8;
pub const MULTIPLY_LENGTH: usize = 0x10;
pub const MULTIPLY_SHA256: &str =
    "1addb8be644cbaa9421a91f6cd6d91407907e9c92320f56ae49804b98fc29a81";
pub const KEY_OFFSET: usize = 0xf5fe8;
pub const KEY_BYTES: usize = 180;
pub const KEY_SHA256: &str = "52e19ba195ffad7aa82ac71bf26543d912f3e3a763e40c755ce0e71b674ef9e7";
pub const SCALE_OFFSET: usize = 0xf609c;
pub const SCALE_BYTES: usize = 48;
pub const SCALE_SHA256: &str = "6b73747d3fb29635eeda13481d08a06e5371958ee746967aa892fa5390509404";
pub const PSG_OFFSET: usize = 0x5f410;
pub const PSG_LENGTH: usize = 0xa8;
pub const PSG_SHA256: &str = "54c17aaded94bc0d4fd4d86ce69df0e0dc15679d7b3046643be4ff45632dc6e1";
pub const PSG_KEY_OFFSET: usize = 0xf60e4;
pub const PSG_KEY_BYTES: usize = 132;
pub const PSG_KEY_SHA256: &str = "2ab14116232b5768777f67dd371a380bd88fdc97858aa050306b132d05faa03e";
pub const PSG_SCALE_OFFSET: usize = 0xf6168;
pub const PSG_SCALE_BYTES: usize = 24;
pub const PSG_SCALE_SHA256: &str =
    "34e0ae7c2f912add79c2e61ed8347ff085000bc39f0173883ccbaff150205ee5";
pub const NOISE_OFFSET: usize = 0xf6180;
pub const NOISE_BYTES: usize = 60;
pub const NOISE_SHA256: &str = "197f0679262884d48dd459dcfe690c0a41e8a73cf76cb62838119fc9cb0e7ab6";
/// 0x5ecf0 loads this literal and installs it into sound-info +0x30.
pub const PSG_BINDING_OFFSET: usize = 0x5ed80;
/// The original initializer loads this sole note-handler pointer into
/// sound-info +0x38. The adjacent 0x5eeb0 internal helper is not note start.
pub const INIT_BINDING_OFFSET: usize = 0x5eea0;

#[derive(Clone)]
pub struct PitchTables {
    keys: [u8; KEY_BYTES],
    scales: [u32; SCALE_BYTES / 4],
    psg_keys: [u8; PSG_KEY_BYTES],
    psg_scales: [i16; PSG_SCALE_BYTES / 2],
    noise: [u8; NOISE_BYTES],
}
impl PitchTables {
    pub fn load(rom: &[u8]) -> Result<Self, Error> {
        crate::catalog::validate_rom(rom).map_err(|_| Error::Source)?;
        let keys = crate::binary::span(rom, KEY_OFFSET, KEY_BYTES).map_err(|_| Error::Source)?;
        let scales =
            crate::binary::span(rom, SCALE_OFFSET, SCALE_BYTES).map_err(|_| Error::Source)?;
        let psg_keys =
            crate::binary::span(rom, PSG_KEY_OFFSET, PSG_KEY_BYTES).map_err(|_| Error::Source)?;
        let psg_scales = crate::binary::span(rom, PSG_SCALE_OFFSET, PSG_SCALE_BYTES)
            .map_err(|_| Error::Source)?;
        let noise =
            crate::binary::span(rom, NOISE_OFFSET, NOISE_BYTES).map_err(|_| Error::Source)?;
        if crate::sha256(keys) != KEY_SHA256
            || crate::sha256(scales) != SCALE_SHA256
            || crate::sha256(psg_keys) != PSG_KEY_SHA256
            || crate::sha256(psg_scales) != PSG_SCALE_SHA256
            || crate::sha256(noise) != NOISE_SHA256
        {
            return Err(Error::Source);
        }
        Ok(Self {
            keys: keys.try_into().expect("exact verified native keys"),
            scales: std::array::from_fn(|i| {
                u32::from_le_bytes(
                    scales[4 * i..4 * i + 4]
                        .try_into()
                        .expect("exact verified scale word"),
                )
            }),
            psg_keys: psg_keys.try_into().expect("verified native PSG keys"),
            psg_scales: std::array::from_fn(|i| {
                i16::from_le_bytes(
                    psg_scales[2 * i..2 * i + 2]
                        .try_into()
                        .expect("verified native PSG scale"),
                )
            }),
            noise: noise.try_into().expect("verified native noise table"),
        })
    }
    fn rate(&self, key: usize) -> Result<u32, Error> {
        let packed = *self.keys.get(key).ok_or(Error::Source)?;
        Ok(*self
            .scales
            .get(usize::from(packed & 15))
            .ok_or(Error::Source)?
            >> (packed >> 4))
    }
    fn psg_rate(&self, key: usize) -> Result<i32, Error> {
        let packed = *self.psg_keys.get(key).ok_or(Error::Source)?;
        Ok(i32::from(
            *self
                .psg_scales
                .get(usize::from(packed & 15))
                .ok_or(Error::Source)?,
        ) >> (packed >> 4))
    }
}

/// Native Thumb→ARM helper 0x5dcf8 executes UMULL and returns its unsigned
/// high word. This is integer math, not a host floating-point resampler.
pub fn multiply_high(first: u32, second: u32) -> u32 {
    ((u64::from(first) * u64::from(second)) >> 32) as u32
}
/// Complete 0x5e914. Key/fine are truncated to bytes before key clamping;
/// an out-of-range truncated key selects key 178 with fine 255.
pub fn sample_pitch(
    memory: &dyn FrameMemory,
    tables: &PitchTables,
    sample: u32,
    key: u32,
    fine: u32,
) -> Result<u32, Error> {
    let (key, fine) = if key as u8 > 178 {
        (178, 255)
    } else {
        (usize::from(key as u8), fine as u8)
    };
    let low = tables.rate(key)?;
    let high = tables.rate(key + 1)?;
    let frequency = memory.read_u32(add(sample, 4)?)?;
    let interpolated =
        low.wrapping_add(multiply_high(high.wrapping_sub(low), u32::from(fine) << 24));
    Ok(multiply_high(frequency, interpolated))
}

/// Complete pure 0x5f410 callback. Native kind 4 uses the noise lookup without
/// interpolation; other kinds use signed table values and byte-truncated
/// key/fine inputs. This calculates register data without writing a device.
pub fn psg_pitch(tables: &PitchTables, kind: u32, key: u32, fine: u32) -> Result<u32, Error> {
    let key = key as u8;
    if kind as u8 == 4 {
        let index = if key <= 20 { 0 } else { (key - 21).min(59) };
        return Ok(u32::from(tables.noise[usize::from(index)]));
    }
    let (index, fine) = if key <= 35 {
        (0, 0)
    } else if key - 36 > 130 {
        (130, 255)
    } else {
        (usize::from(key - 36), fine as u8)
    };
    let low = tables.psg_rate(index)?;
    let high = tables.psg_rate(index + 1)?;
    let delta = high.wrapping_sub(low).wrapping_mul(i32::from(fine)) >> 8;
    Ok(low.wrapping_add(delta).wrapping_add(2048) as u32)
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub pcm_channels: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self { pcm_channels: 12 }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    NestedBank,
    MissingPsgBank,
    NoEligibleChannel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Started {
        channel: u32,
        instrument: u32,
        kind: u8,
    },
    Skipped(Skip),
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
fn standard(target: u32, args: &[u32]) -> Call {
    Call {
        target,
        r0: args.first().copied(),
        r1: args.get(1).copied(),
        r2: args.get(2).copied(),
        r4: None,
        r5: None,
    }
}
fn invoke(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    target: u32,
    args: &[u32],
) -> Result<u32, Error> {
    calls.call(memory, standard(target, args))
}

/// Full native 0x5e684. Reads the game's actual duration table, consumes at
/// most three raw optional bytes, resolves at most one native bank layer and
/// chooses a bounded native channel. Synchronous dependencies mutate the same
/// canonical memory. Failures retain preceding native-order effects.
pub fn start(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    duration_index: u32,
    player: u32,
    track: u32,
    limits: Limits,
) -> Result<Outcome, Error> {
    ram(player, mplay::PLAYER_BYTES as u32)?;
    ram(track, mplay::TRACK_BYTES as u32)?;
    if duration_index >= sequencer::WAIT_BYTES as u32 {
        return Err(Error::Limit {
            kind: "note duration index",
            maximum: sequencer::WAIT_BYTES,
        });
    }
    let sound = memory.read_u32(sequencer::SOUND_INFO_SLOT)?;
    ram(sound, 0x40)?;
    let duration = memory.read_u8(0x08000000 + sequencer::WAIT_OFFSET as u32 + duration_index)?;
    memory.write_u8(track + 4, duration)?;
    let mut cursor = memory.read_u32(track + 0x40)?;
    let key = memory.read_u8(cursor)?;
    if key < 0x80 {
        memory.write_u8(track + 5, key)?;
        cursor = add(cursor, 1)?;
        let velocity = memory.read_u8(cursor)?;
        if velocity < 0x80 {
            memory.write_u8(track + 6, velocity)?;
            cursor = add(cursor, 1)?;
            let extension = memory.read_u8(cursor)?;
            if extension < 0x80 {
                let gate = memory.read_u8(track + 4)?.wrapping_add(extension);
                memory.write_u8(track + 4, gate)?;
                cursor = add(cursor, 1)?;
            }
        }
        word(memory, track + 0x40, cursor)?;
    }
    let base = track + 0x24;
    let root_type = memory.read_u8(base)?;
    let mut pan = 0i32;
    let (instrument, effective_key) = if root_type & 0xc0 != 0 {
        let key = memory.read_u8(track + 5)?;
        let index = if root_type & 0x40 != 0 {
            let map = memory.read_u32(track + 0x2c)?;
            memory.read_u8(add(map, u32::from(key))?)?
        } else {
            key
        };
        let bank = memory.read_u32(track + 0x28)?;
        let selected = add(bank, u32::from(index) * 12)?;
        add(selected, 11)?;
        if memory.read_u8(selected)? & 0xc0 != 0 {
            return Ok(Outcome::Skipped(Skip::NestedBank));
        }
        let key = if root_type & 0x80 != 0 {
            let native_pan = memory.read_u8(selected + 3)?;
            if native_pan & 0x80 != 0 {
                pan = (i32::from(native_pan) - 0xc0) * 2;
            }
            memory.read_u8(selected + 1)?
        } else {
            key
        };
        (selected, key)
    } else {
        (base, memory.read_u8(track + 5)?)
    };
    let priority = (u32::from(memory.read_u8(player + 9)?)
        + u32::from(memory.read_u8(track + 0x1d)?))
    .min(255) as u8;
    let kind = memory.read_u8(instrument)? & 7;
    let channel = if kind != 0 {
        let bank = memory.read_u32(sound + 0x1c)?;
        if bank == 0 {
            return Ok(Outcome::Skipped(Skip::MissingPsgBank));
        }
        let channel = add(bank, u32::from(kind - 1) * mplay::CHANNEL_BYTES as u32)?;
        ram(channel, mplay::CHANNEL_BYTES as u32)?;
        let flags = memory.read_u8(channel)?;
        if flags & 0xc7 != 0 && flags & 0x40 == 0 {
            let old_priority = memory.read_u8(channel + 0x13)?;
            if old_priority > priority
                || old_priority == priority && memory.read_u32(channel + 0x2c)? < track
            {
                return Ok(Outcome::Skipped(Skip::NoEligibleChannel));
            }
        }
        channel
    } else {
        // Like the original signed loop, count zero still examines one slot.
        let count = usize::from(memory.read_u8(sound + 6)?).max(1);
        if count > limits.pcm_channels {
            return Err(Error::Limit {
                kind: "PCM channels",
                maximum: limits.pcm_channels,
            });
        }
        let base = add(sound, 0x50)?;
        ram(base, (count * mplay::CHANNEL_BYTES) as u32)?;
        let mut selected = None;
        let mut best_priority = priority;
        let mut best_owner = track;
        let mut release = false;
        for index in 0..count {
            let channel = base + (index * mplay::CHANNEL_BYTES) as u32;
            let flags = memory.read_u8(channel)?;
            if flags & 0xc7 == 0 {
                selected = Some(channel);
                break;
            }
            if flags & 0x40 != 0 && !release {
                release = true;
                best_priority = memory.read_u8(channel + 0x13)?;
                best_owner = memory.read_u32(channel + 0x2c)?;
                selected = Some(channel);
                continue;
            }
            if flags & 0x40 == 0 && release {
                continue;
            }
            let old_priority = memory.read_u8(channel + 0x13)?;
            if old_priority <= best_priority {
                let owner = memory.read_u32(channel + 0x2c)?;
                if old_priority < best_priority || owner >= best_owner {
                    best_priority = old_priority;
                    best_owner = owner;
                    selected = Some(channel);
                }
            }
        }
        match selected {
            Some(channel) => channel,
            None => return Ok(Outcome::Skipped(Skip::NoEligibleChannel)),
        }
    };
    invoke(memory, calls, 0x0805ed90, &[channel])?;
    word(memory, channel + 0x30, 0)?;
    let head = memory.read_u32(track + 0x20)?;
    word(memory, channel + 0x34, head)?;
    if head != 0 {
        ram(head, mplay::CHANNEL_BYTES as u32)?;
        word(memory, head + 0x30, channel)?;
    }
    word(memory, track + 0x20, channel)?;
    word(memory, channel + 0x2c, track)?;
    let delay = memory.read_u8(track + 0x1b)?;
    memory.write_u8(track + 0x1c, delay)?;
    if delay != 0 {
        // Inline exact 0x5e8c4, whose r1 operand is the track.
        memory.write_u8(track + 0x16, 0)?;
        memory.write_u8(track + 0x1a, 0)?;
        let mask = if memory.read_u8(track + 0x18)? == 0 {
            0x0c
        } else {
            3
        };
        let flags = memory.read_u8(track)?;
        memory.write_u8(track, flags | mask)?;
    }
    invoke(memory, calls, 0x0805f35c, &[player, track])?;
    let gate_key_velocity = memory.read_u32(track + 4)?;
    word(memory, channel + 0x10, gate_key_velocity)?;
    memory.write_u8(channel + 0x13, priority)?;
    memory.write_u8(channel + 8, effective_key)?;
    memory.write_u8(channel + 0x14, pan as u8)?;
    let native_type = memory.read_u8(instrument)?;
    memory.write_u8(channel + 1, native_type)?;
    let sample = memory.read_u32(instrument + 4)?;
    word(memory, channel + 0x24, sample)?;
    let envelope = memory.read_u32(instrument + 8)?;
    word(memory, channel + 4, envelope)?;
    let echo = memory.read_u16(track + 0x1e)?;
    memory.write_u16(channel + 0x0c, echo)?;
    calls.call(
        memory,
        Call {
            target: 0x0805e654,
            r0: None,
            r1: None,
            r2: None,
            r4: Some(channel),
            r5: Some(track),
        },
    )?;
    let pitch = (i32::from(memory.read_u8(channel + 8)?)
        + i32::from(memory.read_u8(track + 8)? as i8))
    .max(0) as u32;
    let fine = u32::from(memory.read_u8(track + 9)?);
    let rate = if kind != 0 {
        let sweep = memory.read_u8(instrument + 2)?;
        memory.write_u8(channel + 0x1e, sweep)?;
        let raw = memory.read_u8(instrument + 3)?;
        memory.write_u8(
            channel + 0x1f,
            if raw & 0x80 != 0 || raw & 0x70 == 0 {
                8
            } else {
                raw
            },
        )?;
        let target = memory.read_u32(sound + 0x30)?;
        invoke(memory, calls, target, &[u32::from(kind), pitch, fine])?
    } else {
        invoke(memory, calls, 0x0805e914, &[sample, pitch, fine])?
    };
    word(memory, channel + 0x20, rate)?;
    memory.write_u8(channel, 0x80)?;
    let flags = memory.read_u8(track)?;
    memory.write_u8(track, flags & 0xf0)?;
    Ok(Outcome::Started {
        channel,
        instrument,
        kind,
    })
}

/// Composition for the tick's real A3VE note and PCM/PSG-rate bindings.
/// Existing sequencer leaves are reused. Unknown PSG/device callbacks keep
/// their exact pointer/operands and reach the supplied strict callback port.
pub struct NativeCalls<C = sequencer::RejectCalls, R = super::sequence_extended::RejectRegisterIo> {
    pub sequence: sequencer::NativeCalls<C, R>,
    pub tables: PitchTables,
    pub limits: Limits,
}
impl NativeCalls {
    pub fn new(tables: PitchTables) -> Self {
        Self {
            sequence: sequencer::NativeCalls::default(),
            tables,
            limits: Limits::default(),
        }
    }
}
impl<C: Calls, R: super::sequence_extended::RegisterIo> Calls for NativeCalls<C, R> {
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        let operand = |value: Option<u32>| value.ok_or_else(|| Error::Operand(call.clone()));
        match call.target {
            0x0805e685 => {
                let limits = self.limits;
                start(
                    memory,
                    self,
                    operand(call.r0)?,
                    operand(call.r1)?,
                    operand(call.r2)?,
                    limits,
                )?;
                Ok(0)
            }
            0x0805e914 => sample_pitch(
                memory,
                &self.tables,
                operand(call.r0)?,
                operand(call.r1)?,
                operand(call.r2)?,
            ),
            0x0805f411 => psg_pitch(
                &self.tables,
                operand(call.r0)?,
                operand(call.r1)?,
                operand(call.r2)?,
            ),
            _ => self.sequence.call(memory, call),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::frame::FrameError;
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
        serde_json::from_str(vectors!("notes-vectors.json")).unwrap()
    }
    fn synthetic_tables(vectors: &Value) -> PitchTables {
        let scales = bytes(&vectors["syntheticScales"]);
        let psg_scales = bytes(&vectors["syntheticPsgScales"]);
        PitchTables {
            keys: bytes(&vectors["syntheticKeys"]).try_into().unwrap(),
            scales: std::array::from_fn(|i| {
                u32::from_le_bytes(scales[4 * i..4 * i + 4].try_into().unwrap())
            }),
            psg_keys: bytes(&vectors["syntheticPsgKeys"]).try_into().unwrap(),
            psg_scales: std::array::from_fn(|i| {
                i16::from_le_bytes(psg_scales[2 * i..2 * i + 2].try_into().unwrap())
            }),
            noise: bytes(&vectors["syntheticNoise"]).try_into().unwrap(),
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
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn note_selection_matches_512_original_controller_cases() {
        let vectors = vectors();
        let cases = vectors["controller"].as_array().unwrap();
        assert_eq!(cases.len(), 512);
        let mut started = 0;
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let mut calls = Scripted::new(case);
            let outcome = start(
                &mut memory,
                &mut calls,
                case["duration"].as_u64().unwrap() as u32,
                0x02001000,
                case["track"].as_u64().unwrap() as u32,
                Limits::default(),
            )
            .unwrap();
            started += usize::from(matches!(outcome, Outcome::Started { .. }));
            assert_eq!(
                calls.calls,
                *case["calls"].as_array().unwrap(),
                "native dependency operands/order case {index}"
            );
            memory.check(&case["after"], &format!("note controller {index}"));
        }
        assert!(started > 300, "allocation branches must be exercised");
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn synthetic_pitch_and_arm_multiply_match_4352_original_calls() {
        let vectors = vectors();
        let tables = synthetic_tables(&vectors);
        let cases = vectors["pitch_synthetic"].as_array().unwrap();
        assert_eq!(cases.len(), 2048);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::default();
            let frequency = case["frequency"].as_u64().unwrap() as u32;
            memory.bind(0x02009004, &frequency.to_le_bytes());
            assert_eq!(
                sample_pitch(
                    &memory,
                    &tables,
                    0x02009000,
                    case["key"].as_u64().unwrap() as u32,
                    case["fine"].as_u64().unwrap() as u32
                )
                .unwrap(),
                case["result"].as_u64().unwrap() as u32,
                "native synthetic rate case {index}"
            );
        }
        let cases = vectors["multiply"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (index, case) in cases.iter().enumerate() {
            assert_eq!(
                multiply_high(
                    case["first"].as_u64().unwrap() as u32,
                    case["second"].as_u64().unwrap() as u32
                ),
                case["result"].as_u64().unwrap() as u32,
                "original Thumb/ARM UMULL case {index}"
            );
        }
        let cases = vectors["psg_synthetic"].as_array().unwrap();
        assert_eq!(cases.len(), 2048);
        for (index, case) in cases.iter().enumerate() {
            assert_eq!(
                psg_pitch(
                    &tables,
                    case["kind"].as_u64().unwrap() as u32,
                    case["key"].as_u64().unwrap() as u32,
                    case["fine"].as_u64().unwrap() as u32
                )
                .unwrap(),
                case["result"].as_u64().unwrap() as u32,
                "native synthetic PSG-rate case {index}"
            );
        }
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; exact native key/scale/duration tables remain local"]
    fn complete_native_notes_pitch_and_ticks_match_4736_original_calls() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let tables = PitchTables::load(&rom).unwrap();
        let vectors = vectors();
        assert_eq!(
            u32::from_le_bytes(
                rom[INIT_BINDING_OFFSET..INIT_BINDING_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0805e685
        );
        assert_eq!(
            u32::from_le_bytes(
                rom[PSG_BINDING_OFFSET..PSG_BINDING_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0805f411
        );
        for source in vectors["routines"].as_array().unwrap() {
            let offset = source["offset"].as_u64().unwrap() as usize;
            let length = source["length"].as_u64().unwrap() as usize;
            assert_eq!(
                crate::sha256(&rom[offset..offset + length]),
                source["sha256"].as_str().unwrap()
            );
        }
        let cases = vectors["pitch_native"].as_array().unwrap();
        assert_eq!(cases.len(), 2048);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::default();
            memory.bind(
                0x02009004,
                &(case["frequency"].as_u64().unwrap() as u32).to_le_bytes(),
            );
            assert_eq!(
                sample_pitch(
                    &memory,
                    &tables,
                    0x02009000,
                    case["key"].as_u64().unwrap() as u32,
                    case["fine"].as_u64().unwrap() as u32
                )
                .unwrap(),
                case["result"].as_u64().unwrap() as u32,
                "native resource rate case {index}"
            );
        }
        let cases = vectors["psg_native"].as_array().unwrap();
        assert_eq!(cases.len(), 2048);
        for (index, case) in cases.iter().enumerate() {
            assert_eq!(
                psg_pitch(
                    &tables,
                    case["kind"].as_u64().unwrap() as u32,
                    case["key"].as_u64().unwrap() as u32,
                    case["fine"].as_u64().unwrap() as u32
                )
                .unwrap(),
                case["result"].as_u64().unwrap() as u32,
                "native resource PSG-rate case {index}"
            );
        }
        let cases = vectors["native"].as_array().unwrap();
        assert_eq!(cases.len(), 512);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            memory.bind(
                0x08000000 + sequencer::WAIT_OFFSET as u32,
                &rom[sequencer::WAIT_OFFSET..sequencer::WAIT_OFFSET + sequencer::WAIT_BYTES],
            );
            let mut calls = NativeCalls {
                sequence: sequencer::NativeCalls {
                    callbacks: Scripted::new(case),
                    registers: super::super::sequence_extended::RejectRegisterIo,
                    limits: sequencer::Limits::default(),
                },
                tables: tables.clone(),
                limits: Limits::default(),
            };
            calls
                .call(
                    &mut memory,
                    standard(
                        0x0805e685,
                        &[
                            case["duration"].as_u64().unwrap() as u32,
                            0x02001000,
                            case["track"].as_u64().unwrap() as u32,
                        ],
                    ),
                )
                .unwrap();
            assert_eq!(
                calls.sequence.callbacks.calls,
                *case["calls"].as_array().unwrap(),
                "native PSG callback boundary {index}"
            );
            memory.check(&case["after"], &format!("complete native note {index}"));
        }
        let dispatch = sequencer::NativeTables::load(&rom).unwrap();
        let dispatch = dispatch
            .entries()
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let cases = vectors["ticks"].as_array().unwrap();
        assert_eq!(cases.len(), 128);
        for (index, case) in cases.iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            memory.bind(
                0x08000000 + sequencer::WAIT_OFFSET as u32,
                &rom[sequencer::WAIT_OFFSET..sequencer::WAIT_OFFSET + sequencer::WAIT_BYTES],
            );
            memory.bind(0x0200b000, &dispatch);
            let mut calls = NativeCalls {
                sequence: sequencer::NativeCalls {
                    callbacks: Scripted::new(case),
                    registers: super::super::sequence_extended::RejectRegisterIo,
                    limits: sequencer::Limits::default(),
                },
                tables: tables.clone(),
                limits: Limits::default(),
            };
            sequencer::tick(
                &mut memory,
                &mut calls,
                0x02001000,
                sequencer::Limits::default(),
            )
            .unwrap();
            assert_eq!(
                calls.sequence.callbacks.calls,
                *case["calls"].as_array().unwrap(),
                "complete native tick callback boundary {index}"
            );
            memory.check(
                &case["after"],
                &format!("complete tick with native notes {index}"),
            );
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn malformed_inputs_and_unimplemented_psg_stop_explicitly() {
        let vectors = vectors();
        let tables = synthetic_tables(&vectors);
        assert_eq!(
            sample_pitch(&Memory::default(), &tables, u32::MAX, 0, 0),
            Err(Error::Address(u32::MAX))
        );
        assert!(matches!(
            sample_pitch(&Memory::default(), &tables, 0x02009000, 0, 0),
            Err(Error::Memory(FrameError::MissingRead {
                address: 0x02009004,
                width: 4
            }))
        ));
        assert!(PitchTables::load(&[0; 128]).is_err());
        let case = &vectors["controller"][0];
        let mut memory = Memory::from_spans(&case["input"]);
        assert_eq!(
            start(
                &mut memory,
                &mut sequencer::RejectCalls,
                49,
                0x02001000,
                0x02002000,
                Limits::default()
            ),
            Err(Error::Limit {
                kind: "note duration index",
                maximum: 49
            })
        );
        memory.write_u8(0x02002024, 0).unwrap();
        memory.write_u8(0x02004006, 13).unwrap();
        assert_eq!(
            start(
                &mut memory,
                &mut sequencer::RejectCalls,
                0,
                0x02001000,
                0x02002000,
                Limits::default()
            ),
            Err(Error::Limit {
                kind: "PCM channels",
                maximum: 12
            })
        );
        let case = vectors["native"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| !case["calls"].as_array().unwrap().is_empty())
            .unwrap();
        let mut memory = Memory::from_spans(&case["input"]);
        memory.bind(
            0x08000000 + sequencer::WAIT_OFFSET as u32,
            &[1; sequencer::WAIT_BYTES],
        );
        let mut calls = NativeCalls::new(tables);
        assert!(matches!(
            calls.call(
                &mut memory,
                standard(
                    0x0805e685,
                    &[
                        case["duration"].as_u64().unwrap() as u32,
                        0x02001000,
                        case["track"].as_u64().unwrap() as u32
                    ]
                )
            ),
            Err(Error::UnportedCall(Call {
                target: 0x0200a001,
                ..
            }))
        ));
    }
}
