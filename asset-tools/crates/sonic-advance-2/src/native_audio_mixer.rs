//! Offline stereo preview using A2NE native sequences, instruments and PCM.
//! Reused bounded preview synthesis; hardware waveform fidelity is separate.
use super::{Command, Result, Sample, a2ne_song, binary, direct_sample, trace};
use crate::Error;

pub const SAMPLE_RATE: u32 = 10512;
pub const SAMPLES_PER_FRAME: usize = 176;
pub const MAX_FRAMES: usize = 10_800;
pub struct Render {
    pub stereo: Vec<[i16; 2]>,
    pub notes: [u32; 5],
    pub peak_polyphony: usize,
}

enum Sound<'a> {
    Pcm(Sample<'a>, bool),
    Square(u8, u8),
    Wave(&'a [u8]),
    Noise(bool, u16),
}
impl Sound<'_> {
    fn kind(&self) -> usize {
        match self {
            Self::Pcm(..) => 0,
            Self::Square(_, channel) => usize::from(*channel),
            Self::Wave(_) => 3,
            Self::Noise(..) => 4,
        }
    }
}
struct Voice<'a> {
    source: Sound<'a>,
    track: usize,
    priority: u8,
    raw_key: u8,
    key: u8,
    velocity: u8,
    pan: i16,
    gate: u32,
    released: bool,
    phase: f64,
    adsr: [u8; 4],
    env: u16,
    state: u8,
    age: u16,
}
fn voice(rom: &[u8], bank: usize, inst: u8, key: u8) -> Result<(Sound<'_>, u8, i16, [u8; 4])> {
    let mut at = bank + usize::from(inst) * 12;
    let tone = binary::range(rom, at, 12)?;
    let mut pitch = key;
    let mut pan = 0;
    let ptr =
        |tone: &[u8], i, align| binary::rom_offset(binary::u32_at(tone, i)?, rom.len(), align);
    if matches!(tone[0], 0x40 | 0x80) {
        let index = if tone[0] == 0x40 {
            binary::range(rom, ptr(tone, 8, 1)? + usize::from(key), 1)?[0]
        } else {
            key
        };
        at = ptr(tone, 4, 4)? + usize::from(index) * 12;
        let sub = binary::range(rom, at, 12)?;
        if tone[0] == 0x80 {
            pitch = sub[1];
            if sub[3] & 128 != 0 {
                pan = (i16::from(sub[3] & 127) - 64) * 2;
            }
        }
    }
    let t = binary::range(rom, at, 12)?;
    let data = binary::u32_at(t, 4)?;
    let src = match t[0] {
        0 | 8 => Sound::Pcm(direct_sample(rom, at, 0)?, t[0] == 8),
        1 | 9 if t[2] == 0 && t[3] == 0 && data < 4 => Sound::Square(data as u8, 1),
        2 | 10 if t[2] == 0 && data < 4 => Sound::Square(data as u8, 2),
        3 | 11 if t[2] == 0 => Sound::Wave(binary::range(rom, ptr(t, 4, 4)?, 16)?),
        4 | 12 if t[2] == 0 && data < 2 => Sound::Noise(data == 1, 0x7fff),
        x => {
            return Err(Error(format!(
                "unsupported tone {x:#x} or PSG length/parameters at {at:#x}"
            )));
        }
    };
    Ok((src, pitch, pan, [t[8], t[9], t[10], t[11]]))
}
#[derive(Default)]
struct Track {
    ended: bool,
    vol: u8,
    pan: i16,
    shift: i8,
    bend: i16,
    range: u8,
    tune: i16,
    priority: u8,
    speed: u8,
    depth: u8,
    mode: u8,
    phase: u8,
    modulation: i16,
    delay: u8,
    count: u8,
}
impl Track {
    fn control(&mut self, op: u8, v: u8, tempo: &mut u16) -> Result<()> {
        let signed = i16::from(v) - 64;
        match op {
            0xba => self.priority = v,
            0xbb => *tempo = u16::from(v) * 2,
            0xbc => self.shift = v as i8,
            0xbd => (),
            0xbe => self.vol = v,
            0xbf => self.pan = signed,
            0xc0 => self.bend = signed,
            0xc1 => self.range = v,
            0xc2 => self.speed = v,
            0xc3 => self.delay = v,
            0xc4 => self.depth = v,
            0xc5 if v <= 2 => self.mode = v,
            0xc8 => self.tune = signed,
            _ => return Err(Error(format!("unsupported control {op:#x}={v}"))),
        };
        if self.depth == 0 || self.speed == 0 {
            self.phase = 0;
            self.modulation = 0;
        }
        Ok(())
    }
    fn lfo(&mut self) {
        if self.depth == 0 || self.speed == 0 {
            return;
        }
        if self.count > 0 {
            self.count -= 1;
            return;
        }
        self.phase = self.phase.wrapping_add(self.speed);
        let r = match self.phase {
            64..=191 => 128 - i16::from(self.phase),
            x => i16::from(x as i8),
        };
        self.modulation = (i16::from(self.depth) * r) >> 6;
    }
}
impl Voice<'_> {
    fn envelope(&mut self) {
        let [a, d, s, r] = self.adsr;
        if self.source.kind() == 0 {
            if self.released {
                self.env = self.env * u16::from(r) / 256;
                if self.env == 0 {
                    self.state = 0;
                }
            } else if self.state == 3 {
                self.env += u16::from(a);
                if self.env >= 255 {
                    self.env = 255;
                    self.state = 2;
                }
            } else if self.state == 2 {
                self.env = (self.env * u16::from(d) / 256).max(u16::from(s));
                if self.env == u16::from(s) {
                    self.state = u8::from(s != 0);
                }
            }
        } else {
            // PSG envelope steps are previewed at the driver frame cadence.
            if self.released && r == 0 {
                self.state = 0;
                return;
            }
            let period = if self.released {
                r
            } else if self.state == 3 {
                a
            } else {
                d
            };
            if self.age == 0 || period == 0 || self.age.is_multiple_of(u16::from(period)) {
                if self.released {
                    self.env = self.env.saturating_sub(17);
                    if self.env == 0 {
                        self.state = 0;
                    }
                } else if self.state == 3 {
                    self.env = if a == 0 {
                        255
                    } else {
                        (self.env + 17).min(255)
                    };
                    if self.env == 255 {
                        self.state = 2;
                    }
                } else if self.state == 2 {
                    let target = u16::from(s) * 17;
                    self.env = if d == 0 {
                        target
                    } else {
                        self.env.saturating_sub(17).max(target)
                    };
                    if self.env == target {
                        self.state = u8::from(target != 0);
                    }
                }
            }
            self.age = self.age.saturating_add(1);
        }
    }
    fn mix(&mut self, t: &Track, out: &mut [[f64; 2]]) {
        let m = t.modulation;
        let pitch = (f64::from(self.key)
            + f64::from(t.shift)
            + f64::from(t.tune + t.bend * i16::from(t.range)) / 64.
            + if t.mode == 0 { f64::from(m) / 16. } else { 0. })
        .clamp(0., 178.);
        let pan = (2 * t.pan + if t.mode == 2 { m } else { 0 }).clamp(-128, 127);
        let level = f64::from(t.vol) * f64::from(self.velocity) * f64::from(self.env)
            / (127. * 127. * 255.);
        let level = level
            * if t.mode == 1 {
                f64::from(m + 128) / 128.
            } else {
                1.
            };
        let left = level * f64::from(127 - pan) / 255. * f64::from(128 - self.pan) / 128.;
        let right = level * f64::from(128 + pan) / 255. * f64::from(128 + self.pan) / 128.;
        let rate = f64::from(SAMPLE_RATE);
        let hz = 440. * 2f64.powf((pitch - 69.) / 12.);
        let step = match &self.source {
            Sound::Pcm(s, fixed) => {
                if *fixed {
                    1.
                } else {
                    f64::from(s.pitch_1024) / 1024. * 2f64.powf((pitch - 60.) / 12.) / rate
                }
            }
            Sound::Noise(..) => {
                let n = (pitch as u8).saturating_sub(21).min(59);
                let reg = if n < 56 {
                    ((13 - n / 4) << 4) + (7 - n % 4)
                } else {
                    59 - n
                };
                let div = f64::from(reg & 7);
                524288.
                    / (if div == 0. { 0.5 } else { div })
                    / 2f64.powi(i32::from(reg >> 4) + 1)
                    / rate
            }
            _ => hz / rate,
        };
        for pair in out {
            let value = match &mut self.source {
                Sound::Pcm(s, fixed) => {
                    let len = s.signed_pcm.len();
                    if self.phase >= len as f64 {
                        if let Some(l) = s.loop_start {
                            self.phase = l as f64 + (self.phase - l as f64) % (len - l) as f64;
                        } else {
                            self.state = 0;
                            break;
                        }
                    }
                    let i = self.phase as usize;
                    let a = f64::from(s.signed_pcm[i] as i8);
                    let j = if i + 1 < len {
                        i + 1
                    } else {
                        s.loop_start.unwrap_or(i)
                    };
                    let v = if *fixed {
                        a
                    } else {
                        a + (f64::from(s.signed_pcm[j] as i8) - a) * self.phase.fract()
                    };
                    self.phase += step;
                    v / 128.
                }
                Sound::Square(d, _) => {
                    let duty = [0.125, 0.25, 0.5, 0.75][usize::from(*d)];
                    let v = if self.phase < duty { 1. } else { -1. };
                    self.phase = (self.phase + step).fract();
                    v
                }
                Sound::Wave(w) => {
                    let n = (self.phase * 32.) as usize;
                    let b = w[n / 2];
                    let v = if n.is_multiple_of(2) { b >> 4 } else { b & 15 };
                    self.phase = (self.phase + step).fract();
                    f64::from(v) / 7.5 - 1.
                }
                Sound::Noise(short, reg) => {
                    let v = if *reg & 1 == 0 { 1. } else { -1. };
                    self.phase += step;
                    while self.phase >= 1. {
                        let bit = (*reg ^ (*reg >> 1)) & 1;
                        *reg = (*reg >> 1) | (bit << 14);
                        if *short {
                            *reg = (*reg & !64) | (bit << 6);
                        }
                        self.phase -= 1.;
                    }
                    v
                }
            };
            pair[0] += value * left;
            pair[1] += value * right;
        }
    }
}
/// Render an isolated retail cue. Frame zero is the player's first driver update.
/// One frame produces 176 stereo samples. No battery or emulator state is read.
pub(super) fn render(rom: &[u8], song: usize, frames: usize) -> Result<Render> {
    render_inner(rom, song, frames, false)
}

pub(super) fn render_one_shot(rom: &[u8], song: usize) -> Result<Render> {
    render_inner(rom, song, MAX_FRAMES, true)
}

fn render_inner(
    rom: &[u8],
    song: usize,
    frames: usize,
    stop_when_finished: bool,
) -> Result<Render> {
    if !(1..=MAX_FRAMES).contains(&frames) {
        return Err(Error("render duration outside 1..=10800 frames".into()));
    }
    let header = a2ne_song(rom, song)?;
    let bank = header
        .voicegroup_offset
        .ok_or_else(|| Error("missing bank".into()))?;
    let mut tracks = Vec::new();
    for pc in header.track_offsets {
        tracks.push((
            trace(rom, pc, (frames * 4) as u32)?.into_iter().peekable(),
            Track {
                range: 2,
                speed: 22,
                ..Default::default()
            },
        ));
    }
    let mut voices: Vec<Voice> = Vec::new();
    let mut tick = 0;
    let mut clock = 0;
    let mut tempo = 150;
    let mut result = Render {
        stereo: Vec::with_capacity(frames * SAMPLES_PER_FRAME),
        notes: [0; 5],
        peak_polyphony: 0,
    };
    for _ in 0..frames {
        clock += tempo;
        while clock >= 150 {
            clock -= 150;
            for (idx, (events, t)) in tracks.iter_mut().enumerate() {
                for v in voices
                    .iter_mut()
                    .filter(|v| v.track == idx && v.gate == tick)
                {
                    v.released = true;
                }
                while events.peek().is_some_and(|e| e.tick == tick) {
                    match events.next().unwrap().command {
                        Command::Control { opcode, value } => {
                            t.control(opcode, value, &mut tempo)?
                        }
                        Command::Fine => {
                            t.ended = true;
                            for v in voices.iter_mut().filter(|v| v.track == idx) {
                                v.released = true;
                            }
                        }
                        Command::EndTie { key } => {
                            if let Some(v) = voices
                                .iter_mut()
                                .rev()
                                .find(|v| v.track == idx && v.raw_key == key && !v.released)
                            {
                                v.released = true;
                            }
                        }
                        Command::Extended { opcode, .. } => {
                            return Err(Error(format!(
                                "unsupported extended mixer control {opcode}"
                            )));
                        }
                        Command::Note {
                            key,
                            velocity,
                            gate,
                            voice: inst,
                        } => {
                            let (source, sounding, pan, adsr) = voice(rom, bank, inst, key)?;
                            let kind = source.kind();
                            let priority = header.priority.saturating_add(t.priority);
                            let competing = voices
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| v.source.kind() == kind);
                            if competing.clone().count() >= if kind == 0 { 5 } else { 1 } {
                                let best = competing
                                    .filter(|(_, v)| {
                                        v.released
                                            || (v.priority, std::cmp::Reverse(v.track))
                                                <= (priority, std::cmp::Reverse(idx))
                                    })
                                    .min_by_key(|(i, v)| {
                                        (
                                            !v.released,
                                            v.priority,
                                            std::cmp::Reverse(v.track),
                                            std::cmp::Reverse(*i),
                                        )
                                    })
                                    .map(|(i, _)| i);
                                if let Some(i) = best {
                                    voices.remove(i);
                                } else {
                                    continue;
                                }
                            }
                            t.count = t.delay;
                            if t.delay > 0 {
                                t.phase = 0;
                                t.modulation = 0;
                            }
                            result.notes[kind] += 1;
                            voices.push(Voice {
                                source,
                                track: idx,
                                priority,
                                raw_key: key,
                                key: sounding,
                                velocity,
                                pan,
                                gate: if gate == 0 {
                                    u32::MAX
                                } else {
                                    tick + u32::from(gate)
                                },
                                released: false,
                                phase: 0.,
                                adsr,
                                env: 0,
                                state: 3,
                                age: 0,
                            });
                        }
                    }
                }
                t.lfo();
            }
            tick += 1;
        }
        let mut block = [[0.; 2]; SAMPLES_PER_FRAME];
        result.peak_polyphony = result.peak_polyphony.max(voices.len());
        for v in &mut voices {
            v.envelope();
            if v.state != 0 {
                v.mix(&tracks[v.track].1, &mut block);
            }
        }
        voices.retain(|v| v.state != 0);
        result
            .stereo
            .extend(block.map(|p| p.map(|s| (s * 8192.).round().clamp(-32768., 32767.) as i16)));
        if stop_when_finished && voices.is_empty() && tracks.iter().all(|(_, t)| t.ended) {
            return Ok(result);
        }
    }
    if stop_when_finished {
        return Err(Error("Cue release exceeds bounded driver duration".into()));
    }
    Ok(result)
}
