//! Bounded MP2K sequence-time execution and uncompressed Direct Sound voices.
use crate::{binary, tables, Error, Result};
pub mod mixer;

const CLOCK: [u8; 49] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 28,
    30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68, 72, 76, 78, 80, 84, 88, 90, 92, 96,
];

pub fn asoe_song(rom: &[u8], song: usize) -> Result<tables::SongHeader> {
    binary::verify_asoe(rom)?;
    if song >= 308 {
        return Err(Error("song index outside table".into()));
    }
    let entry = tables::song_table(rom, 0x6d76ac + 8 * song, 1, 1)?;
    let offset = entry[0]
        .header_offset
        .ok_or_else(|| Error("empty song entry".into()))?;
    tables::song_header(rom, offset, 16)
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub enum Command {
    Control {
        opcode: u8,
        value: u8,
    },
    Extended {
        opcode: u8,
        value: u8,
    },
    Note {
        key: u8,
        velocity: u8,
        gate: u8,
        voice: u8,
    },
    EndTie {
        key: u8,
    },
    Fine,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Event {
    pub tick: u32,
    pub command: Command,
}

/// Execute one track through an inclusive sequence tick. Tempo and other
/// controller writes are timestamped in score time, before mixing or envelopes.
pub fn trace(rom: &[u8], start: usize, through_tick: u32) -> Result<Vec<Event>> {
    let mut pc = start;
    let mut tick = 0;
    let (mut running, mut key, mut velocity, mut voice) = (0, 0, 0, 0);
    let mut stack = Vec::new();
    let mut repeats = 0u8;
    let mut events = Vec::new();
    for _ in 0..100_000 {
        let first = byte(rom, pc)?;
        let op = if first < 128 {
            running
        } else {
            pc += 1;
            if first >= 0xbd {
                running = first;
            }
            first
        };
        let command = match op {
            0x80..=0xb0 => {
                tick += u32::from(CLOCK[(op - 0x80) as usize]);
                if tick > through_tick {
                    return Ok(events);
                }
                continue;
            }
            0xb1 => Command::Fine,
            0xb2 | 0xb3 | 0xb5 => {
                let count = if op == 0xb5 { take(rom, &mut pc)? } else { 0 };
                let pointer = binary::u32_at(rom, pc)?;
                pc += 4;
                if op == 0xb5 && count != 0 {
                    repeats += 1;
                    if repeats >= count {
                        repeats = 0;
                        continue;
                    }
                }
                if op == 0xb3 {
                    if stack.len() == 3 {
                        return Err(Error("MP2K pattern nesting exceeds three".into()));
                    }
                    stack.push(pc);
                }
                pc = binary::rom_offset(pointer, rom.len(), 1)?;
                continue;
            }
            0xb4 => {
                if let Some(ret) = stack.pop() {
                    pc = ret;
                }
                continue;
            }
            0xba..=0xc5 | 0xc8 => {
                let value = take(rom, &mut pc)?;
                if op == 0xbd {
                    voice = value;
                }
                Command::Control { opcode: op, value }
            }
            0xcd => {
                let opcode = take(rom, &mut pc)?;
                if !matches!(opcode, 8 | 9) {
                    return Err(Error(format!("unsupported extended command {opcode:#x}")));
                }
                Command::Extended {
                    opcode,
                    value: take(rom, &mut pc)?,
                }
            }
            0xce => {
                if byte(rom, pc)? < 128 {
                    key = take(rom, &mut pc)?;
                }
                Command::EndTie { key }
            }
            0xcf..=0xff => {
                let mut gate = CLOCK[(op - 0xcf) as usize];
                if byte(rom, pc)? < 128 {
                    key = take(rom, &mut pc)?;
                    if byte(rom, pc)? < 128 {
                        velocity = take(rom, &mut pc)?;
                        if byte(rom, pc)? < 128 {
                            gate += take(rom, &mut pc)?;
                        }
                    }
                }
                Command::Note {
                    key,
                    velocity,
                    gate,
                    voice,
                }
            }
            _ => {
                return Err(Error(format!(
                    "unsupported MP2K status {op:#04x} at {pc:#x}"
                )))
            }
        };
        events.push(Event { tick, command });
        if op == 0xb1 {
            return Ok(events);
        }
    }
    Err(Error("track instruction limit exceeded".into()))
}

fn byte(rom: &[u8], offset: usize) -> Result<u8> {
    rom.get(offset)
        .copied()
        .ok_or_else(|| Error("truncated MP2K track".into()))
}
fn take(rom: &[u8], pc: &mut usize) -> Result<u8> {
    let v = byte(rom, *pc)?;
    *pc += 1;
    Ok(v)
}

pub struct Sample<'a> {
    pub pitch_1024: u32,
    pub loop_start: Option<usize>,
    pub signed_pcm: &'a [u8],
}

/// Read an ordinary directly indexed 8-bit PCM instrument from a voicegroup.
/// Drum/key splits, PSG voices and compressed samples require different readers.
pub fn direct_sample(rom: &[u8], group: usize, voice: u8) -> Result<Sample<'_>> {
    if voice >= 128 {
        return Err(Error("voice outside bank".into()));
    }
    let at = group
        .checked_add(usize::from(voice) * 12)
        .ok_or_else(|| Error("voice address overflow".into()))?;
    let tone = binary::range(rom, at, 12)?;
    if !matches!(tone[0], 0 | 8) {
        return Err(Error(format!("unsupported tone type {:#x}", tone[0])));
    }
    let wave = binary::rom_offset(binary::u32_at(tone, 4)?, rom.len(), 4)?;
    let header = binary::range(rom, wave, 16)?;
    let flags = binary::u32_at(header, 0)?;
    if flags & !0x40000000 != 0 {
        return Err(Error("unsupported PCM encoding".into()));
    }
    let pitch_1024 = binary::u32_at(header, 4)?;
    let len = binary::u32_at(header, 12)? as usize;
    let loop_start = (flags != 0)
        .then(|| binary::u32_at(header, 8).map(|n| n as usize))
        .transpose()?;
    if pitch_1024 < 1024 || len == 0 || loop_start.is_some_and(|n| n >= len) {
        return Err(Error("invalid PCM header".into()));
    }
    Ok(Sample {
        pitch_1024,
        loop_start,
        signed_pcm: binary::range(rom, wave + 16, len)?,
    })
}
