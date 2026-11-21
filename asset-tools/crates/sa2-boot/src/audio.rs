//! Audio with the host as the DSP. The game's own AX library runs (AXInit,
//! voices, MIX), except `__AXOutInit`, which would boot the DSP and the AI.
//! ARAM is real memory, so ARStartDMA and ARQPostRequest move bytes both
//! ways and run their callbacks at once.
//!
//! Every 5 ms of game time ([`FRAME`] samples at [`RATE`]) a frame hook does
//! what `__AXOutNewFrame` does: `__AXSyncPBs` copies the game's voice changes
//! into the parameter blocks, the host mixes every running PB from ARAM
//! (DSP-ADPCM, PCM16, PCM8) to stereo and writes back what the DSP updates
//! (state, position, ADPCM history, sample-rate state, volumes), then the
//! voice callbacks and the game's AX frame callback run.
//!
//! The mix goes to `SA2_AUDIO_WAV` (a .wav file) and, in a window
//! (`SA2_WINDOW`) unless `SA2_MUTE` is set, to the speakers ([`play`]).
//! shortcut: main L/R only (no aux A/B effects, no surround), linear
//! resampling in place of the DSP's 4-tap filter, no per-ms PB updates; add
//! them if something sounds wrong.
use crate::{guest_call, io};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};
use std::collections::VecDeque;
use std::io::{Seek, SeekFrom, Write};
use std::sync::Mutex;

pub const AI_REGISTER_DMA_CALLBACK: u32 = 0x800F_56E4;
pub const AI_INIT_DMA: u32 = 0x800F_5728;
pub const AI_START_DMA: u32 = 0x800F_57B0;
pub const AI_INIT: u32 = 0x800F_5AF8;
pub const AR_START_DMA: u32 = 0x800F_5FE8;
pub const AR_ALLOC: u32 = 0x800F_60D8;
pub const AR_INIT: u32 = 0x800F_61B4;
pub const ARQ_INIT: u32 = 0x800F_73BC;
pub const ARQ_POST_REQUEST: u32 = 0x800F_7424;
pub const AX_INIT: u32 = 0x800F_7580;
pub const AX_QUIT: u32 = 0x800F_75B4;
pub const AX_SERVICE_CALLBACK_STACK: u32 = 0x800F_7600;
pub const AX_OUT_INIT: u32 = 0x800F_859C;
pub const AX_REGISTER_CALLBACK: u32 = 0x800F_892C;
pub const AX_SYNC_PBS: u32 = 0x800F_94E0;
pub const AX_GET_PBS: u32 = 0x800F_9728;
pub const DSP_CHECK_MAIL_FROM_DSP: u32 = 0x800F_E0F4;
pub const DSP_SEND_MAIL_TO_DSP: u32 = 0x800F_E11C;
pub const DSP_INIT: u32 = 0x800F_E130;
pub const DSP_ADD_TASK: u32 = 0x800F_E288;

pub const DSP_REGS: u32 = 0xCC00_5000;
pub const AI_REGS: u32 = 0xCC00_6C00;
/// 16 MB of ARAM; the first 16 KB belong to the audio library.
const ARAM_SIZE: usize = 16 << 20;
const ARAM_USER_BASE: u32 = 0x4000;
/// The AX DSP mixes 32 kHz stereo in 5 ms frames.
pub const RATE: u32 = 32_000;
pub const FRAME: usize = 160;
/// AX_MAX_VOICES: the PB list is never longer.
const MAX_VOICES: usize = 64;

static ARAM: Global<Vec<u8>> = global!(vec![0; ARAM_SIZE]);
static AR_NEXT: Global<u32> = global!(ARAM_USER_BASE);
/// Set once the game's AXInit has run.
static AX_ON: Global<bool> = global!(false);
static USER_CB: Global<u32> = global!(0);
static OS_FRAMES: Global<u64> = global!(0);
static AX_FRAMES: Global<u64> = global!(0);
static WAV: Global<Option<Wav>> = global!(None);
/// (callback, request) of finished ARQ transfers not yet reported.
static ARQ_DONE: Global<Vec<(u32, u32)>> = global!(Vec::new());

fn nothing(_c: &mut Cpu, _m: &mut Mem) {}
fn zero(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 0;
}

fn ar_init(c: &mut Cpu, _m: &mut Mem) {
    AR_NEXT.set(ARAM_USER_BASE);
    c.r[3] = ARAM_USER_BASE;
}

fn ar_alloc(c: &mut Cpu, _m: &mut Mem) {
    let at = AR_NEXT.get();
    AR_NEXT.set(at.saturating_add(c.r[3]));
    c.r[3] = at;
}

/// `to_aram`: main memory -> ARAM, else back.
fn dma(m: &mut Mem, to_aram: bool, main: u32, aram: u32, len: u32) {
    ARAM.with_borrow_mut(|ar| {
        let a = aram as usize;
        let Some(slot) = ar.get_mut(a..a.saturating_add(len as usize)) else {
            return;
        };
        if to_aram {
            for (i, b) in slot.iter_mut().enumerate() {
                *b = m.r8(main.wrapping_add(i as u32));
            }
        } else {
            m.load(main, slot);
        }
    });
}

fn ar_start_dma(c: &mut Cpu, m: &mut Mem) {
    // ARStartDMA(type, mainmem, aram, length): type 0 is MRAM to ARAM.
    let [ty, main, aram, len] = [c.r[3], c.r[4], c.r[5], c.r[6]];
    dma(m, ty == 0, main, aram, len);
}

fn arq_post_request(c: &mut Cpu, m: &mut Mem) {
    // ARQPostRequest(req, owner, type, prio, source, dest, length, callback).
    let [req, ty, src, dst, len, cb] = [c.r[3], c.r[5], c.r[7], c.r[8], c.r[9], c.r[10]];
    // Fill the ARQRequest as the SDK does: callbacks read it back (the ADX
    // output's callback finds its stream and channel from `owner`).
    let fields = [0, c.r[4], ty, c.r[6], src, dst, len, cb];
    for (i, v) in fields.into_iter().enumerate() {
        m.w32(req + 4 * i as u32, v);
    }
    if ty == 0 {
        dma(m, true, src, dst, len);
    } else {
        dma(m, false, dst, src, len);
    }
    // The transfer is done, but its callback comes later, as the ARQ interrupt
    // does: callers record that they wait only after posting.
    if cb != 0 {
        ARQ_DONE.with_borrow_mut(|q| q.push((cb, req)));
    }
}

/// Frame hook: completion callbacks of the ARQ requests posted since the last one.
fn arq_done(c: &mut Cpu, m: &mut Mem) {
    for (cb, req) in ARQ_DONE.take() {
        guest_call(c, m, cb, &[req]);
    }
}

fn dsp_add_task(_c: &mut Cpu, _m: &mut Mem) {
    // Returns the task it was given (still in r3).
}

/// In place of `__AXOutInit` (end of AXInit): no DSP task, no AI DMA.
fn ax_out_init(_c: &mut Cpu, _m: &mut Mem) {
    USER_CB.set(0);
    AX_ON.set(true);
}

fn ax_register_callback(c: &mut Cpu, _m: &mut Mem) {
    USER_CB.set(c.r[3]);
}

/// Frame hook: run the AX frames that fit in the game time so far.
fn tick(c: &mut Cpu, m: &mut Mem) {
    let f = OS_FRAMES.get() + 1;
    OS_FRAMES.set(f);
    if !AX_ON.get() {
        return;
    }
    // One NTSC field is 1001/60000 s.
    let due = f * u64::from(RATE) * 1001 / 60_000 / FRAME as u64;
    while AX_FRAMES.get() < due {
        AX_FRAMES.set(AX_FRAMES.get() + 1);
        ax_frame(c, m);
    }
}

/// One `__AXOutNewFrame`, with the host mixing where the DSP would.
fn ax_frame(c: &mut Cpu, m: &mut Mem) {
    arq_done(c, m);
    guest_call(c, m, AX_SYNC_PBS, &[0]);
    let pbs = guest_call(c, m, AX_GET_PBS, &[]);
    let mut mix = mix_voices(m, pbs);
    crate::steam_sound::mix(&mut mix);
    guest_call(c, m, AX_SERVICE_CALLBACK_STACK, &[]);
    let cb = USER_CB.get();
    if cb != 0 {
        guest_call(c, m, cb, &[]);
    }
    let pcm: Vec<i16> = mix
        .iter()
        .flatten()
        .map(|s| (*s).clamp(-32768, 32767) as i16)
        .collect();
    WAV.with_borrow_mut(|w| {
        if let Some(wav) = w {
            if wav.push(&pcm).is_err() {
                *w = None;
            }
        }
    });
    if let Ok(mut q) = LIVE.lock() {
        if q.cap > 0 {
            q.buf.extend(pcm);
            // Behind the speakers by more than the cap: drop the oldest.
            let over = q.buf.len().saturating_sub(q.cap);
            q.buf.drain(..over);
        }
    }
}

/// Mixed samples waiting for the speakers; `cap` 0 while nothing plays.
struct Live {
    buf: VecDeque<i16>,
    cap: usize,
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    buf: VecDeque::new(),
    cap: 0,
});

/// Play the mix on the default output device, on a thread of its own.
/// The browser build plays through SA2_AUDIO_WAV instead (sa2-web).
#[cfg(not(target_os = "wasi"))]
pub fn play() {
    std::thread::spawn(|| {
        if let Err(e) = run_output() {
            eprintln!("audio: {e}");
        }
    });
}

#[cfg(not(target_os = "wasi"))]
fn run_output() -> Result<(), Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let dev = cpal::default_host()
        .default_output_device()
        .ok_or("no output device")?;
    let cfg = dev.default_output_config()?;
    let (rate, chans) = (cfg.sample_rate().0, usize::from(cfg.channels()));
    if let Ok(mut q) = LIVE.lock() {
        // A quarter second of 32 kHz stereo.
        q.cap = RATE as usize / 2;
    }
    // Device frames per mixed frame, as 16.16, stepped with a linear blend.
    let step = (u64::from(RATE) << 16) / u64::from(rate.max(1));
    let (mut pos, mut prev, mut next) = (0u64, [0i16; 2], [0i16; 2]);
    let stream = dev.build_output_stream(
        &cfg.config(),
        move |out: &mut [f32], _| {
            let Ok(mut q) = LIVE.lock() else { return };
            for frame in out.chunks_mut(chans.max(1)) {
                pos += step;
                while pos >= 1 << 16 {
                    pos -= 1 << 16;
                    prev = next;
                    if q.buf.len() >= 2 {
                        next = [
                            q.buf.pop_front().unwrap_or(0),
                            q.buf.pop_front().unwrap_or(0),
                        ];
                    }
                }
                for (ch, s) in frame.iter_mut().enumerate() {
                    let k = ch.min(1);
                    let (a, b) = (f32::from(prev[k]), f32::from(next[k]));
                    *s = (a + (b - a) * pos as f32 / 65536.0) / 32768.0;
                }
            }
        },
        |e| eprintln!("audio: {e}"),
        None,
    )?;
    stream.play()?;
    // The stream lives as long as the process.
    loop {
        std::thread::park();
    }
}

/// Mix one frame of every running PB in the list that starts at `first`.
pub fn mix_voices(m: &mut Mem, first: u32) -> [[i32; 2]; FRAME] {
    let mut out = [[0; 2]; FRAME];
    ARAM.with_borrow(|ar| {
        let mut pb = first;
        for _ in 0..MAX_VOICES {
            if pb == 0 {
                break;
            }
            if m.r16(pb + 0x0E) == 1 {
                voice(m, pb, ar, &mut out);
            }
            pb = m.r32(pb);
        }
    });
    out
}

/// A voice's sample source (AXPBADDR, AXPBADPCM, AXPBADPCMLOOP).
struct Source {
    running: bool,
    looped: bool,
    streaming: bool,
    format: u16,
    loop_at: u32,
    end: u32,
    cur: u32,
    coefs: [i16; 16],
    ps: u16,
    yn1: i16,
    yn2: i16,
    loop_ps: u16,
    loop_yn1: i16,
    loop_yn2: i16,
}

impl Source {
    fn read(m: &Mem, pb: u32) -> Self {
        let h = |o: u32| m.r16(pb + o);
        let mut coefs = [0; 16];
        for (i, k) in coefs.iter_mut().enumerate() {
            *k = h(0x7E + 2 * i as u32) as i16;
        }
        Source {
            running: true,
            looped: h(0x6E) != 0,
            streaming: h(0x10) != 0,
            format: h(0x70),
            loop_at: m.r32(pb + 0x72),
            end: m.r32(pb + 0x76),
            cur: m.r32(pb + 0x7A),
            coefs,
            ps: h(0xA0),
            yn1: h(0xA2) as i16,
            yn2: h(0xA4) as i16,
            loop_ps: h(0xB4),
            loop_yn1: h(0xB6) as i16,
            loop_yn2: h(0xB8) as i16,
        }
    }

    fn write(&self, m: &mut Mem, pb: u32) {
        if !self.running {
            m.w16(pb + 0x0E, 0);
        }
        m.w32(pb + 0x7A, self.cur);
        m.w16(pb + 0xA0, self.ps);
        m.w16(pb + 0xA2, self.yn1 as u16);
        m.w16(pb + 0xA4, self.yn2 as u16);
    }

    /// The sample at the current address, then step (loop or stop at the end).
    fn next(&mut self, ar: &[u8]) -> i16 {
        if !self.running {
            return 0;
        }
        let byte = |a: u32| ar.get(a as usize).copied().unwrap_or(0);
        let s = match self.format {
            // DSP-ADPCM: addresses count nibbles; each 8-byte frame starts
            // with a predictor/scale byte.
            0 => {
                if self.cur & 0xF == 0 {
                    self.ps = byte(self.cur >> 1).into();
                    self.cur = self.cur.wrapping_add(2);
                }
                let b = byte(self.cur >> 1);
                let nib = if self.cur & 1 == 0 { b >> 4 } else { b & 0xF };
                let nib = i64::from(((nib << 4) as i8) >> 4);
                let k = usize::from((self.ps >> 4) & 7) * 2;
                let (c1, c2) = (i64::from(self.coefs[k]), i64::from(self.coefs[k + 1]));
                let v = ((nib << (self.ps & 0xF)) << 11)
                    + c1 * i64::from(self.yn1)
                    + c2 * i64::from(self.yn2);
                let v = ((v + 1024) >> 11).clamp(-32768, 32767) as i16;
                self.yn2 = self.yn1;
                self.yn1 = v;
                v
            }
            // PCM16: addresses count samples.
            10 => {
                let a = self.cur.wrapping_mul(2);
                i16::from_be_bytes([byte(a), byte(a.wrapping_add(1))])
            }
            // PCM8: addresses count bytes.
            25 => i16::from(byte(self.cur) as i8) << 8,
            _ => 0,
        };
        if self.cur == self.end {
            if self.looped {
                self.cur = self.loop_at;
                if self.format == 0 {
                    self.ps = self.loop_ps;
                    if !self.streaming {
                        self.yn1 = self.loop_yn1;
                        self.yn2 = self.loop_yn2;
                    }
                }
            } else {
                self.running = false;
            }
        } else {
            self.cur = self.cur.wrapping_add(1);
        }
        s
    }
}

/// Mix one running PB into `out` and write back what the DSP changes.
fn voice(m: &mut Mem, pb: u32, ar: &[u8], out: &mut [[i32; 2]; FRAME]) {
    let h = |o: u32| m.r16(pb + o);
    let src_select = h(0x08);
    let ramp = h(0x0C) & 8 != 0;
    let (mut vl, dl) = (i32::from(h(0x12)), i32::from(h(0x14) as i16));
    let (mut vr, dr) = (i32::from(h(0x16)), i32::from(h(0x18) as i16));
    let (mut vol, dvol) = (i32::from(h(0x64)), i32::from(h(0x66) as i16));
    // srcSelect 2 is no conversion; ratioHi stays below 5 (__AXSrcCycles).
    let ratio = if src_select == 2 {
        0x1_0000
    } else {
        m.r32(pb + 0xA6).min(0x4_FFFF)
    };
    let mut frac = u32::from(h(0xAA));
    let mut last = [0i16; 4];
    for (i, s) in last.iter_mut().enumerate() {
        *s = h(0xAC + 2 * i as u32) as i16;
    }
    let mut src = Source::read(m, pb);
    for o in out.iter_mut() {
        let (a, b) = (i64::from(last[2]), i64::from(last[3]));
        let s = (a + (((b - a) * i64::from(frac)) >> 16)) as i32;
        frac += ratio;
        while frac >= 0x1_0000 {
            frac -= 0x1_0000;
            last.rotate_left(1);
            last[3] = src.next(ar);
        }
        let s = (s * vol) >> 15;
        vol = (vol + dvol).clamp(0, 0xFFFF);
        o[0] += (s * vl) >> 15;
        o[1] += (s * vr) >> 15;
        if ramp {
            vl = (vl + dl).clamp(0, 0xFFFF);
            vr = (vr + dr).clamp(0, 0xFFFF);
        }
    }
    src.write(m, pb);
    m.w16(pb + 0x12, vl as u16);
    m.w16(pb + 0x16, vr as u16);
    m.w16(pb + 0x64, vol as u16);
    m.w16(pb + 0xAA, frac as u16);
    for (i, s) in last.iter().enumerate() {
        m.w16(pb + 0xAC + 2 * i as u32, *s as u16);
    }
}

/// A 16-bit stereo .wav whose sizes are rewritten after every append.
struct Wav {
    file: std::fs::File,
    bytes: u32,
}

impl Wav {
    fn create(path: &std::path::Path) -> std::io::Result<Self> {
        let mut w = Wav {
            file: std::fs::File::create(path)?,
            bytes: 0,
        };
        w.header()?;
        Ok(w)
    }

    fn header(&mut self) -> std::io::Result<()> {
        let mut h = Vec::with_capacity(44);
        h.extend(b"RIFF");
        h.extend((36 + self.bytes).to_le_bytes());
        h.extend(b"WAVEfmt ");
        h.extend(16u32.to_le_bytes());
        h.extend(1u16.to_le_bytes());
        h.extend(2u16.to_le_bytes());
        h.extend(RATE.to_le_bytes());
        h.extend((RATE * 4).to_le_bytes());
        h.extend(4u16.to_le_bytes());
        h.extend(16u16.to_le_bytes());
        h.extend(b"data");
        h.extend(self.bytes.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&h)
    }

    fn push(&mut self, pcm: &[i16]) -> std::io::Result<()> {
        let data: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&data)?;
        self.bytes = self.bytes.saturating_add(data.len() as u32);
        self.header()
    }
}

pub fn install(c: &mut Cpu, m: &mut Mem) {
    for a in [
        AI_INIT_DMA,
        AI_START_DMA,
        AI_INIT,
        ARQ_INIT,
        AX_QUIT,
        DSP_SEND_MAIL_TO_DSP,
        DSP_INIT,
    ] {
        c.rt.set_override(a, nothing);
    }
    for a in [AI_REGISTER_DMA_CALLBACK, DSP_CHECK_MAIL_FROM_DSP] {
        c.rt.set_override(a, zero);
    }
    c.rt.set_override(AR_INIT, ar_init);
    c.rt.set_override(AR_ALLOC, ar_alloc);
    c.rt.set_override(AR_START_DMA, ar_start_dma);
    c.rt.set_override(ARQ_POST_REQUEST, arq_post_request);
    c.rt.set_override(DSP_ADD_TASK, dsp_add_task);
    c.rt.set_override(AX_OUT_INIT, ax_out_init);
    c.rt.set_override(AX_REGISTER_CALLBACK, ax_register_callback);
    sa2_recomp::os::add_frame_hook(arq_done);
    sa2_recomp::os::add_frame_hook(tick);
    if let Some(path) = std::env::var_os("SA2_AUDIO_WAV") {
        match Wav::create(path.as_ref()) {
            Ok(w) => WAV.set(Some(w)),
            Err(e) => eprintln!("audio: SA2_AUDIO_WAV: {e}"),
        }
    }
    if std::env::var_os("SA2_WINDOW").is_some() && std::env::var_os("SA2_MUTE").is_none() {
        #[cfg(not(target_os = "wasi"))]
        play();
    }
    for (base, len) in [(DSP_REGS, 0x200), (AI_REGS, 0x20)] {
        io::route_read(m, base, base + len, |_, _| 0);
        io::route_write(m, base, base + len, |_, _, _| {});
    }
}
