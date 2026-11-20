//! Bounded A2NE MP2K sequence execution and native voice/sample readers.
//! Reuses the local Sonic Advance Rust reader; see docs/NATIVE-AUDIO.md for
//! source identity, independent checks and preview-versus-retail limits.
use crate::{Error, VerifiedRom};
type Result<T> = std::result::Result<T, Error>;
#[path = "native_audio_mixer.rs"]
pub mod mixer;

mod binary {
    use super::{Error, Result};
    pub fn range(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
        crate::bounded_range(bytes, offset, length)
    }
    pub fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            range(bytes, offset, 4)?.try_into().unwrap(),
        ))
    }
    pub fn rom_offset(pointer: u32, length: usize, align: usize) -> Result<usize> {
        let offset = crate::tables::pointer_offset(pointer, 0x08000000, length)?;
        if !offset.is_multiple_of(align) {
            return Err(Error("Unaligned native audio pointer".into()));
        }
        Ok(offset)
    }
}

pub const SONG_TABLE: usize = 0xad4f4c;
pub const SONG_COUNT: usize = 507;
pub const SUPPORTED_CUES: &[u16] = &[
    13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 102, 109, 110, 112, 113, 115, 116, 117,
    118, 119, 138, 139, 140, 141, 149, 152, 153, 157, 221, 232, 279, 298, 401, 407,
];

pub struct Song {
    pub priority: u8,
    pub voicegroup_offset: Option<usize>,
    pub track_offsets: Vec<usize>,
}

pub fn song(rom: &VerifiedRom<'_>, id: u16) -> Result<Song> {
    a2ne_song(rom.rom_bytes(), usize::from(id))
}

const CLOCK: [u8; 49] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 28,
    30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68, 72, 76, 78, 80, 84, 88, 90, 92, 96,
];

fn a2ne_song(rom: &[u8], id: usize) -> Result<Song> {
    if id >= SONG_COUNT {
        return Err(Error("Native song ID outside A2NE table".into()));
    }
    let offset = binary::rom_offset(binary::u32_at(rom, SONG_TABLE + id * 8)?, rom.len(), 4)?;
    let header = crate::tables::song_header(rom, offset, 0x08000000)?;
    Ok(Song {
        priority: header.priority,
        voicegroup_offset: Some(binary::rom_offset(header.voicegroup_pointer, rom.len(), 4)?),
        track_offsets: header
            .track_pointers
            .into_iter()
            .map(|p| binary::rom_offset(p, rom.len(), 1))
            .collect::<Result<_>>()?,
    })
}

/// Render an isolated cue using its actual native sequences and source samples.
/// PCM/PSG output is a bounded preview; it is not retail DAC waveform parity.
pub fn render(rom: &VerifiedRom<'_>, cue: u16, frames: usize) -> Result<mixer::Render> {
    if !SUPPORTED_CUES.contains(&cue) {
        return Err(Error("Native cue is outside verified preview set".into()));
    }
    mixer::render(rom.rom_bytes(), usize::from(cue), frames)
}

pub struct CompleteCue {
    pub timing: CueTiming,
    pub audio: mixer::Render,
}

/// Render the native introduction and first complete score loop, or a complete
/// one-shot through FINE and its preview envelope/sample release. Loop points
/// are driver-frame indices; buffered waveform splicing remains a host policy.
pub fn render_cue(rom: &VerifiedRom<'_>, cue: u16) -> Result<CompleteCue> {
    if !SUPPORTED_CUES.contains(&cue) {
        return Err(Error("Native cue is outside verified preview set".into()));
    }
    let timing = cue_timing(rom, cue)?;
    let audio = if let Some(end) = timing.loop_end_frame {
        mixer::render(rom.rom_bytes(), usize::from(cue), end)?
    } else if timing.fine_frame.is_some() {
        mixer::render_one_shot(rom.rom_bytes(), usize::from(cue))?
    } else {
        return Err(Error(
            "Cue has no shared loop or complete FINE boundary".into(),
        ));
    };
    Ok(CompleteCue { timing, audio })
}

/// Encode the local preview as ordinary stereo signed-16 PCM WAV.
pub fn wav(render: &mixer::Render) -> Result<Vec<u8>> {
    let bytes = u32::try_from(
        render
            .stereo
            .len()
            .checked_mul(4)
            .ok_or_else(|| Error("Native preview WAV length overflow".into()))?,
    )
    .map_err(|_| Error("Native preview WAV exceeds RIFF bounds".into()))?;
    let size = bytes
        .checked_add(36)
        .ok_or_else(|| Error("Native preview RIFF length overflow".into()))?;
    let mut out = Vec::with_capacity(bytes as usize + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&mixer::SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(mixer::SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    out.extend(
        render
            .stereo
            .iter()
            .flat_map(|pair| pair.iter().flat_map(|v| v.to_le_bytes())),
    );
    Ok(out)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ScoreLoop {
    pub target_offset: usize,
    pub start_tick: u32,
    pub end_tick: u32,
}

#[derive(Debug, serde::Serialize)]
pub struct CueTiming {
    pub track_loops: Vec<Option<ScoreLoop>>,
    pub loop_start_frame: Option<usize>,
    pub loop_end_frame: Option<usize>,
    pub fine_frame: Option<usize>,
}

/// Follow actual native GOTO targets to the first repeated musical interval.
/// Driver-frame points map native score ticks through the cue's tempo events.
/// They are sequence bounds, not proof of a phase-continuous waveform splice.
pub fn cue_timing(rom: &VerifiedRom<'_>, cue: u16) -> Result<CueTiming> {
    let header = song(rom, cue)?;
    let mut loops = Vec::new();
    let mut tempos = Vec::new();
    let mut end = Some(0);
    for (index, offset) in header.track_offsets.into_iter().enumerate() {
        let (events, score_loop) = execute(rom.rom_bytes(), offset, u32::MAX, true)?;
        let fine = events
            .iter()
            .find_map(|event| matches!(event.command, Command::Fine).then_some(event.tick));
        end = end.zip(fine).map(|(a, b)| a.max(b));
        for event in events {
            if let Command::Control {
                opcode: 0xbb,
                value,
            } = event.command
            {
                tempos.push((event.tick, index, value));
            }
        }
        loops.push(score_loop);
    }
    tempos.sort_by_key(|(tick, track, _)| (*tick, *track));
    let frame_at = |target: u32| -> Result<usize> {
        let (mut tick, mut clock, mut tempo, mut next_tempo) = (0, 0, 150, 0);
        for frame in 0..mixer::MAX_FRAMES {
            clock += tempo;
            while clock >= 150 {
                clock -= 150;
                if tick == target {
                    return Ok(frame);
                }
                while next_tempo < tempos.len() && tempos[next_tempo].0 == tick {
                    tempo = u32::from(tempos[next_tempo].2) * 2;
                    next_tempo += 1;
                }
                tick += 1;
            }
        }
        Err(Error("Cue timing exceeds bounded driver duration".into()))
    };
    let common = loops.first().copied().flatten().filter(|first| {
        loops.iter().all(|entry| {
            entry.is_some_and(|other| {
                other.start_tick == first.start_tick && other.end_tick == first.end_tick
            })
        })
    });
    Ok(CueTiming {
        loop_start_frame: common.map(|v| frame_at(v.start_tick)).transpose()?,
        loop_end_frame: common.map(|v| frame_at(v.end_tick)).transpose()?,
        fine_frame: end.map(frame_at).transpose()?,
        track_loops: loops,
    })
}

/// Execute one track through an inclusive sequence tick. Tempo and other
/// controller writes are timestamped in score time, before mixing or envelopes.
pub fn trace(rom: &[u8], start: usize, through_tick: u32) -> Result<Vec<Event>> {
    Ok(execute(rom, start, through_tick, false)?.0)
}

fn execute(
    rom: &[u8],
    start: usize,
    through_tick: u32,
    stop_on_loop: bool,
) -> Result<(Vec<Event>, Option<ScoreLoop>)> {
    let mut pc = start;
    let mut tick = 0;
    let (mut running, mut key, mut velocity, mut voice) = (0, 0, 0, 0);
    let mut stack = Vec::new();
    let mut repeats = 0u8;
    let mut events = Vec::new();
    let mut visits = std::collections::BTreeMap::new();
    let mut score_loop = None;
    for _ in 0..100_000 {
        visits.entry(pc).or_insert(tick);
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
                    return Ok((events, score_loop));
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
                if op == 0xb2
                    && let Some(&start_tick) = visits.get(&pc)
                    && tick > start_tick
                {
                    score_loop.get_or_insert(ScoreLoop {
                        target_offset: pc,
                        start_tick,
                        end_tick: tick,
                    });
                    if stop_on_loop {
                        return Ok((events, score_loop));
                    }
                }
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
                            gate = gate.checked_add(take(rom, &mut pc)?).ok_or_else(|| {
                                Error("Native note gate exceeds byte range".into())
                            })?;
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
                )));
            }
        };
        events.push(Event { tick, command });
        if op == 0xb1 {
            return Ok((events, score_loop));
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
    pub wave_offset: usize,
    pub data_offset: usize,
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
        wave_offset: wave,
        data_offset: wave + 16,
        pitch_1024,
        loop_start,
        signed_pcm: binary::range(rom, wave + 16, len)?,
    })
}
