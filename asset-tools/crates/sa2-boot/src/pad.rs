//! PAD from the host: [`set`] gives this frame's controllers (the runtime's
//! `PadStatus`), PADRead copies them into the game's PADStatus[4] and PADClamp
//! is the runtime's port. Port 0 is connected until told otherwise.
use sa2_asset_tools::input::{pad_clamp, PadStatus};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

pub const PAD_CLAMP: u32 = 0x800F_3A00;
pub const PAD_RESET: u32 = 0x800F_4368;
pub const PAD_RECALIBRATE: u32 = 0x800F_4468;
pub const PAD_INIT: u32 = 0x800F_456C;
pub const PAD_READ: u32 = 0x800F_4780;
pub const PAD_SET_SAMPLING_RATE: u32 = 0x800F_4B2C;
pub const PAD_CONTROL_MOTOR: u32 = 0x800F_4C3C;
pub const PAD_SET_SPEC: u32 = 0x800F_4CE0;

/// PAD_ERR_NO_CONTROLLER.
const NO_CONTROLLER: i8 = -1;

fn default_pads() -> [PadStatus; 4] {
    let mut p = [PadStatus::default(); 4];
    for s in &mut p[1..] {
        s.err = NO_CONTROLLER;
    }
    p
}

static PADS: Global<[PadStatus; 4]> = global!(default_pads());

/// This frame's controllers.
pub fn set(pads: [PadStatus; 4]) {
    PADS.set(pads);
}

fn to_guest(m: &mut Mem, a: u32, p: &PadStatus) {
    m.w16(a, p.button);
    let b = [
        p.stick[0] as u8,
        p.stick[1] as u8,
        p.substick[0] as u8,
        p.substick[1] as u8,
        p.triggers[0],
        p.triggers[1],
        p.analog_a,
        p.analog_b,
        p.err as u8,
    ];
    for (i, v) in b.into_iter().enumerate() {
        m.w8(a + 2 + i as u32, v);
    }
}

fn from_guest(m: &Mem, a: u32) -> PadStatus {
    let b = |o: u32| m.r8(a + o);
    PadStatus {
        button: m.r16(a),
        stick: [b(2) as i8, b(3) as i8],
        substick: [b(4) as i8, b(5) as i8],
        triggers: [b(6), b(7)],
        analog_a: b(8),
        analog_b: b(9),
        err: b(10) as i8,
    }
}

fn read(c: &mut Cpu, m: &mut Mem) {
    let base = c.r[3];
    for (i, p) in PADS.get().iter().enumerate() {
        to_guest(m, base + 12 * i as u32, p);
    }
    c.r[3] = 0;
}

fn clamp(c: &mut Cpu, m: &mut Mem) {
    let base = c.r[3];
    let mut s: [PadStatus; 4] = std::array::from_fn(|i| from_guest(m, base + 12 * i as u32));
    pad_clamp(&mut s);
    for (i, p) in s.iter().enumerate() {
        to_guest(m, base + 12 * i as u32, p);
    }
}

fn ok(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 1;
}

fn nothing(_c: &mut Cpu, _m: &mut Mem) {}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(PAD_INIT, ok);
    c.rt.set_override(PAD_RESET, ok);
    c.rt.set_override(PAD_RECALIBRATE, ok);
    c.rt.set_override(PAD_READ, read);
    c.rt.set_override(PAD_CLAMP, clamp);
    for a in [PAD_SET_SAMPLING_RATE, PAD_CONTROL_MOTOR, PAD_SET_SPEC] {
        c.rt.set_override(a, nothing);
    }
}

/// Scripted input for headless runs: port 0 holds A and Start for 3 frames
/// out of every 60, enough to click through warning and title screens.
pub fn autopress(frame: u64) {
    let mut pads = default_pads();
    if frame % 60 < 3 {
        pads[0].button = 0x0100 | 0x1000; // A | Start
    }
    set(pads);
}

/// Scripted input from SA2_INPUT, comma separated. Each press holds 3 frames;
/// buttons are hex PAD bits (0x0001 left, 0x0002 right, 0x0004 down, 0x0008
/// up, 0x0100 A, 0x0200 B, 0x1000 Start).
/// - `frame:buttons` presses at a fixed frame (warning and title screens).
/// - `FILE+delay:buttons` presses `delay` frames after each time the game
///   opens FILE (a screen's own file, so the timing follows the screen).
/// Fixed-frame presses stop once the first FILE-anchored entry's file has
/// opened, so they cannot land on the screens the anchored steps handle.
/// Returns false when SA2_INPUT is unset.
pub fn scripted(frame: u64) -> bool {
    let Ok(script) = std::env::var("SA2_INPUT") else { return false };
    let opens = OPENS.lock().unwrap_or_else(|e| e.into_inner());
    let anchor = script
        .split(',')
        .find_map(|i| i.rsplit_once(':')?.0.split_once('+').map(|(f, _)| f.trim().to_lowercase()));
    let anchored = anchor.is_some_and(|a| opens.iter().any(|(n, _)| *n == a));
    let mut pads = default_pads();
    for item in script.split(',') {
        let Some((at, b)) = item.rsplit_once(':') else { continue };
        let Ok(b) = u16::from_str_radix(b.trim().trim_start_matches("0x"), 16) else { continue };
        let starts: Vec<u64> = match at.trim().split_once('+') {
            Some((file, d)) => {
                let Ok(d) = d.trim().parse::<u64>() else { continue };
                let file = file.trim().to_lowercase();
                opens.iter().filter(|(n, _)| *n == file).map(|(_, f)| f + d).collect()
            }
            None => match at.trim().parse::<u64>() {
                Ok(f) if !anchored => vec![f],
                _ => continue,
            },
        };
        if starts.iter().any(|s| (*s..*s + 3).contains(&frame)) {
            pads[0].button |= b;
        }
    }
    drop(opens);
    set(pads);
    true
}

/// Files the game opened, with the frame of each open (SA2_INPUT anchors).
static OPENS: std::sync::Mutex<Vec<(String, u64)>> = std::sync::Mutex::new(Vec::new());

/// Note that the game opened `name` (called by the DVD hook).
pub fn opened(name: &str) {
    let frame = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    OPENS.lock().unwrap_or_else(|e| e.into_inner()).push((name.to_lowercase(), frame));
}

/// Controller port 0 only (the others stay disconnected).
pub fn set_port0(p: PadStatus) {
    let mut pads = default_pads();
    pads[0] = p;
    set(pads);
}
