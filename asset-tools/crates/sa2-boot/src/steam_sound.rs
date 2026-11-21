//! Sound on a Steam install, where the disc's `.mlt`/`.mpb` banks are absent.
//! When the game asks for a bank file the DVD layer cannot serve, Steam's
//! `<name>/<N>.csb` files register as gcax bank N. The game's sound processor
//! (0x800680f4) drives its sequencer with MIDI messages through 0x801403b8:
//! CC32 = bank and program change = sequence (both from its own sound table),
//! CC7 volume, CC10 pan, pitch bend, then note on 0x30 to start and note off
//! to stop. Those messages play Steam cue `<name><bank:03><sequence:03>`
//! through the AX mix. Every message still reaches the game's own sequencer,
//! so the disc path is untouched (with banks present nothing registers here).
//! shortcut: gains are linear (CC/127, SYNTH volume/1000) and pan is linear; pitch bend is ignored (its range in gcax is unproven); fix from gcax's own curves if mob hears a difference.
use crate::audio::{FRAME, RATE};
use sa2_asset_tools::{audio::decode_adx, csb};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};
use sa2_runtime::dvd::GameFiles;
use std::collections::BTreeMap;
use std::sync::Arc;

/// gcax: one MIDI message to a sequence player (handle, ?, status, data1, data2).
pub const SEQ_MESSAGE: u32 = 0x8014_03B8;

struct Bank {
    name: String,
    csb: Vec<u8>,
    cues: csb::Bank,
}

/// A decoded cue: interleaved PCM, and where the looped part starts.
struct Cue {
    pcm: Vec<i16>,
    channels: usize,
    rate: u64,
    loop_at: Option<usize>,
    volume: f32,
}

/// What the game last sent one (player handle, MIDI channel).
#[derive(Default)]
struct Channel {
    bank: u32,
    program: u32,
    volume: Option<u32>,
    pan: Option<u32>,
    voice: Option<Voice>,
}

struct Voice {
    cue: Arc<Cue>,
    pos: f64,
}

static BANKS: Global<BTreeMap<u32, Bank>> = global!(BTreeMap::new());
static CHANNELS: Global<BTreeMap<(u32, u32), Channel>> = global!(BTreeMap::new());

/// The game asked for `request` (e.g. `se_kart.mlt`) and no file answered:
/// register Steam's `.../<stem>/<N>.csb` files under that folder name.
pub fn missing_bank(files: &dyn GameFiles, request: &str) {
    let lower = request.to_ascii_lowercase();
    let Some(stem) = lower
        .strip_suffix(".mlt")
        .or_else(|| lower.strip_suffix(".mpb"))
    else {
        return;
    };
    let stem = stem.rsplit('/').next().unwrap_or(stem);
    for path in files.paths() {
        let p = path.to_ascii_lowercase();
        let mut parts = p.rsplit('/');
        let (Some(file), Some(dir)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(n) = file
            .strip_suffix(".csb")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if dir != stem {
            continue;
        }
        let Some(len) = files.size(&path) else {
            continue;
        };
        let mut bytes = vec![0; len as usize];
        if files.read_at(&path, 0, &mut bytes) != Some(bytes.len()) {
            continue;
        }
        match csb::read(&bytes) {
            Ok(cues) => BANKS.with_borrow_mut(|b| {
                b.insert(
                    n,
                    Bank {
                        name: stem.to_string(),
                        csb: bytes,
                        cues,
                    },
                );
            }),
            Err(e) => eprintln!("steam sound: {path}: {e}"),
        }
    }
}

/// Cue `<name><bank:03><seq:03>` decoded, if Steam has it.
fn cue(bank: &Bank, n: u32, seq: u32) -> Option<Cue> {
    let want = format!("{}{n:03}{seq:03}", bank.name);
    let cue = bank
        .cues
        .cues
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&want))?;
    let mut link = cue.synth.clone();
    let mut volume = 1.0;
    for _ in 0..8 {
        if let Some(e) = bank.cues.elements.iter().find(|e| e.name == link) {
            let (mut pcm, mut loop_at) = (Vec::new(), None);
            for s in e.segments.iter().filter(|s| s.adx && !s.silent) {
                if s.loop_flag != 0 && loop_at.is_none() {
                    loop_at = Some(pcm.len());
                }
                let bytes = bank.csb.get(s.offset..s.offset.checked_add(s.length)?)?;
                pcm.extend(decode_adx(bytes, 1 << 24).ok()?.samples);
            }
            let channels = e.channels.max(1) as usize;
            return Some(Cue {
                loop_at: loop_at.map(|i| i / channels),
                pcm,
                channels,
                rate: e.sample_rate,
                volume,
            });
        }
        let s = bank.cues.synths.iter().find(|s| s.name == link)?;
        volume *= s.volume as f32 / 1000.0;
        link = s.link.clone();
    }
    None
}

fn start(bank: u32, seq: u32) -> Option<Voice> {
    BANKS.with_borrow(|b| {
        let k = b.get(&bank)?;
        let c = cue(k, bank, seq);
        if c.is_none() {
            eprintln!("steam sound: no cue {}{bank:03}{seq:03}", k.name);
        }
        let c = c.filter(|c| c.rate > 0 && !c.pcm.is_empty())?;
        if std::env::var_os("SA2_TRACE_SOUND").is_some() {
            eprintln!("steam sound: play {}{bank:03}{seq:03}", k.name);
        }
        Some(Voice {
            cue: Arc::new(c),
            pos: 0.0,
        })
    })
}

/// One message as the game sends it to the sequencer.
fn message(handle: u32, status: u32, d1: u32, d2: u32) {
    let key = (handle, status & 0x0F);
    CHANNELS.with_borrow_mut(|chs| {
        let ch = chs.entry(key).or_default();
        match (status & 0xF0, d1) {
            (0xB0, 0x20) => ch.bank = d2,
            (0xB0, 0x07) => ch.volume = Some(d2),
            (0xB0, 0x0A) => ch.pan = Some(d2),
            (0xC0, _) => ch.program = d1,
            (0x90, _) if d2 > 0 => ch.voice = start(ch.bank, ch.program),
            (0x80 | 0x90, _) => ch.voice = None,
            _ => {}
        }
    });
}

/// In place of 0x801403b8: follow the message, then let the game's own
/// sequencer have it.
fn seq_message(c: &mut Cpu, m: &mut Mem) {
    if std::env::var_os("SA2_TRACE_SOUND").is_some() {
        eprintln!(
            "steam sound: msg {:#x} {:02x} {:02x} {:02x}",
            c.r[3],
            c.r[5] & 0xFF,
            c.r[6] & 0xFF,
            c.r[7] & 0xFF
        );
    }
    message(c.r[3], c.r[5] & 0xFF, c.r[6] & 0xFF, c.r[7] & 0xFF);
    if let Some(f) = (c.rt.generated)(SEQ_MESSAGE) {
        f(c, m);
    }
}

/// Add the playing Steam voices to one AX frame.
pub fn mix(out: &mut [[i32; 2]; FRAME]) {
    CHANNELS.with_borrow_mut(|chs| {
        for ch in chs.values_mut() {
            let Some(v) = &mut ch.voice else { continue };
            let gain = v.cue.volume * ch.volume.map_or(1.0, |x| x as f32 / 127.0);
            let pan = ch.pan.map_or(0.5, |p| p as f32 / 127.0);
            let (gl, gr) = (gain * (1.0 - pan).min(0.5) * 2.0, gain * pan.min(0.5) * 2.0);
            let frames = v.cue.pcm.len() / v.cue.channels;
            let step = v.cue.rate as f64 / f64::from(RATE);
            let mut done = false;
            for o in out.iter_mut() {
                let mut i = v.pos as usize;
                if i >= frames {
                    match v.cue.loop_at {
                        Some(l) if l < frames => {
                            v.pos = l as f64 + (v.pos - frames as f64);
                            i = v.pos as usize;
                        }
                        _ => {
                            done = true;
                            break;
                        }
                    }
                }
                let n = v.cue.channels;
                o[0] += (f32::from(v.cue.pcm[i * n]) * gl) as i32;
                o[1] += (f32::from(v.cue.pcm[i * n + n - 1]) * gr) as i32;
                v.pos += step;
            }
            if done {
                ch.voice = None;
            }
        }
    });
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(SEQ_MESSAGE, seq_message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_messages_drive_a_steam_voice() {
        // Synthetic cue, no game data: 2 mono samples at half the mix rate.
        let cue = Cue {
            pcm: vec![100, -50],
            channels: 1,
            rate: u64::from(RATE) / 2,
            loop_at: None,
            volume: 1.0,
        };
        message(7, 0xB0, 0x0A, 127); // pan hard right
        CHANNELS.with_borrow_mut(|c| {
            c.get_mut(&(7, 0)).unwrap().voice = Some(Voice {
                cue: Arc::new(cue),
                pos: 0.0,
            })
        });
        let mut out = [[0; 2]; FRAME];
        mix(&mut out);
        assert_eq!(&out[..5], &[[0, 100], [0, 100], [0, -50], [0, -50], [0, 0]]);
        assert!(
            CHANNELS.with_borrow(|c| c[&(7, 0)].voice.is_none()),
            "ended"
        );
        // Note on for a bank nobody registered starts nothing; note off clears.
        message(7, 0x90, 0x30, 0x7F);
        message(7, 0x80, 0x30, 0);
        assert!(CHANNELS.with_borrow(|c| c[&(7, 0)].voice.is_none()));
    }
}
