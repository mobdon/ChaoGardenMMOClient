//! The recompiled SA2 in a browser tab, game side. Built for
//! `wasm32-wasip1-threads`: std works as on the desktop (threads are web
//! workers, files come from the page's local server), and what the window
//! does goes through device files the page serves instead:
//!
//! - `/dev/gx` (write): each finished frame's GX job, the pipe bytes and a
//!   copy of RAM, for the page's renderer (`sa2-web`) to decode and draw.
//! - `/dev/pad` (read): the keyboard and gamepad as the page sees them now.
//! - sound: `SA2_AUDIO_WAV=/dev/audio`, the mix as it is made.
//!
//! The generated `sa2-host` crate calls [`main`] with the recompiled code.
#![forbid(unsafe_code)]

use sa2_asset_tools::input::PadStatus;
use sa2_recomp::rt::{Cpu, Func, Mem, Runtime};
use sa2_recomp::{global, global::Global};
use std::io::{Read, Write};

/// GX job framing on `/dev/gx`: magic, XFB address, pipe length, RAM length.
pub const GX_MAGIC: u32 = 0x4758_4A42; // "GXJB"

/// Run main.dol (paths from the page's args) until the tab closes.
pub fn main(dispatch: fn(u32) -> Option<Func>, register: fn(&mut Runtime)) {
    let a: Vec<String> = std::env::args().collect();
    let (Some(dol), Some(syms)) = (a.get(1), a.get(2)) else {
        eprintln!("usage: sa2-host MAIN.DOL SYMBOLS.TXT");
        return;
    };
    let dol = match std::fs::read(dol) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("can't read main.dol at {dol}: {e}");
            return;
        }
    };
    let syms = std::fs::read_to_string(syms).unwrap_or_default();
    // A day of play; 0 calls = no call budget.
    if let Err(e) = sa2_recomp::host::run(&dol, &syms, dispatch, register, setup, 0, 86_400) {
        eprintln!("host: {e}");
    }
}

/// The desktop host's setup with the window's parts swapped for the page's.
fn setup(c: &mut Cpu, m: &mut Mem) {
    use sa2_runtime::dvd::{DirFiles, FlatDir, GameFiles, Layered};
    let root = std::env::var("SA2_STEAM_DIR").unwrap_or_else(|_| "/steam".into());
    let mut files: Vec<Box<dyn GameFiles>> = vec![Box::new(DirFiles { root: root.into() })];
    if let Some(dir) = std::env::var_os("SA2_DISC_DIR") {
        let disc: Box<dyn GameFiles> = Box::new(FlatDir { dir: dir.into() });
        if std::env::var_os("SA2_DISC_FIRST").is_some() {
            files.insert(0, disc);
        } else {
            files.push(disc);
        }
    }
    if let Some(dir) = std::env::var_os("SA2_REL_DIR") {
        files.insert(0, Box::new(FlatDir { dir: dir.into() }));
    }
    sa2_boot::install_all(c, m, Box::new(Layered(files)));
    // GX: the page decodes; the pipe bytes and RAM go to it whole.
    sa2_boot::io::route_write(m, sa2_boot::gx::PIPE, sa2_boot::gx::PIPE + 8, pipe_write);
    c.rt.set_override(sa2_boot::gx::GX_COPY_DISP, copy_disp);
    sa2_recomp::os::add_frame_hook(frame);
}

/// Pipe bytes of the frame being built.
static QUEUED: Global<Vec<u8>> = global!(Vec::new());

fn pipe_write(_a: u32, size: u32, v: u32) {
    let b = v.to_be_bytes();
    let n = (size as usize).min(4);
    QUEUED.with_borrow_mut(|q| q.extend_from_slice(&b[4 - n..]));
}

/// GXCopyDisp ends a frame: hand its job to the page. The write waits while
/// the page is two frames behind (as the window's queue does).
fn copy_disp(c: &mut Cpu, m: &mut Mem) {
    let dest = c.r[3];
    if let Some(f) = (c.rt.generated)(sa2_boot::gx::GX_COPY_DISP) {
        f(c, m);
    }
    let fifo = QUEUED.with_borrow_mut(std::mem::take);
    let mut head = Vec::with_capacity(16 + fifo.len());
    for v in [GX_MAGIC, dest, fifo.len() as u32, m.ram.len() as u32] {
        head.extend(v.to_le_bytes());
    }
    head.extend_from_slice(&fifo);
    let sent = std::fs::OpenOptions::new()
        .append(true)
        .open("/dev/gx")
        .and_then(|mut f| f.write_all(&head).and_then(|()| f.write_all(&m.ram)));
    if let Err(e) = sent {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| eprintln!("gx: /dev/gx: {e}"));
    }
    sa2_recomp::host::FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Each OS frame: controller 1 from the page, then hold the game to 60.
/// SA2_INPUT presses are added, as in the window (the keyboard works
/// throughout and has the pad to itself once the script is done).
fn frame(_c: &mut Cpu, _m: &mut Mem) {
    let mut p = pad();
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if let Some(s) = sa2_boot::pad::script(f) {
        p.button |= s.button;
        if s.stick != [0, 0] {
            p.stick = s.stick;
        }
    }
    sa2_boot::pad::set_port0(p);
    let t = std::time::Instant::now();
    pace();
    // SA2_PROFILE: where each second's frame time went (game, limiter sleep).
    if sa2_boot::prof::on() {
        sa2_boot::prof::add(&sa2_boot::prof::SLEEP, t);
        sa2_boot::prof::frames(1);
    }
}

/// `/dev/pad`: 16 bytes. Keys held (u16, bits as [`KEYS`]); then the
/// gamepad: connected (u8), unused (u8), buttons (u16, GC bits), stick
/// x/y, C-stick x/y (i8), L/R (u8).
fn read_pad() -> [u8; 16] {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/pad") {
        let _ = f.read_exact(&mut b);
    }
    b
}

/// GC button bits of the keys after the four stick directions (A D S W), in
/// the page's bit order: Space = A, E and Left Shift = B, Q = X, R = Y,
/// Enter and Esc = Start, 1 = L, 3 = R; then left mouse = B, right mouse = A.
const KEYS: [u16; 15] = [
    0, 0, 0, 0, 0x0100, 0x0200, 0x0200, 0x0400, 0x0800, 0x1000, 0x1000, 0x0040, 0x0020, 0x0200,
    0x0100,
];

/// Controller 1: the gamepad and the keyboard/mouse together, as the
/// desktop window combines them.
fn pad() -> PadStatus {
    let b = read_pad();
    let h = u16::from_le_bytes([b[0], b[1]]);
    let on = |i: usize| h >> i & 1 != 0;
    let mut p = PadStatus::default();
    for (i, bit) in KEYS.iter().enumerate() {
        if *bit != 0 && on(i) {
            p.button |= bit;
        }
    }
    p.stick = key_stick(on(0), on(1), on(2), on(3));
    if b[2] != 0 {
        let s = |i: usize| b[i] as i8;
        p.button |= u16::from_le_bytes([b[4], b[5]]);
        let stick = [s(6), s(7)];
        if stick[0].unsigned_abs() > 12 || stick[1].unsigned_abs() > 12 {
            p.stick = stick;
        }
        p.substick = [s(8), s(9)];
        p.triggers = [b[10], b[11]];
    }
    p
}

/// The keyboard stick as (angle in radians, magnitude 0..=1).
static EASED: std::sync::Mutex<(f32, f32)> = std::sync::Mutex::new((0.0, 0.0));

/// W/A/S/D as a stick, as the desktop window does it
/// (`sa2_boot::window::key_stick`): full deflection while any key is held,
/// the angle turning about 46° a frame toward the keys, so a turn never dips
/// through the centre. shortcut: a copy, since that module needs winit; move
/// it somewhere both builds share when the window changes it again.
pub fn key_stick(left: bool, right: bool, down: bool, up: bool) -> [i8; 2] {
    const TURN: f32 = 0.8;
    const RAMP: f32 = 1.0;
    let dx = f32::from(i8::from(right) - i8::from(left));
    let dy = f32::from(i8::from(up) - i8::from(down));
    let mut e = EASED.lock().unwrap_or_else(|e| e.into_inner());
    let (angle, mag) = &mut *e;
    if dx != 0.0 || dy != 0.0 {
        let want = dy.atan2(dx);
        if *mag == 0.0 {
            *angle = want;
        } else {
            let pi = std::f32::consts::PI;
            let diff = (want - *angle + 3.0 * pi).rem_euclid(2.0 * pi) - pi;
            *angle += diff.clamp(-TURN, TURN);
        }
        *mag = (*mag + RAMP).min(1.0);
    } else {
        *mag = (*mag - RAMP).max(0.0);
    }
    [
        (angle.cos() * *mag * 100.0).round() as i8,
        (angle.sin() * *mag * 100.0).round() as i8,
    ]
}

/// Hold the game to 60 frames a second of real time.
fn pace() {
    use std::time::{Duration, Instant};
    static NEXT: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
    let frame = Duration::from_nanos(1_000_000_000 / 60);
    let Ok(mut next) = NEXT.lock() else { return };
    let now = Instant::now();
    let due = next.map_or(now, |t| t.max(now - frame));
    if due > now {
        std::thread::sleep(due - now);
    }
    *next = Some(due + frame);
}

#[cfg(test)]
mod tests {
    use super::key_stick;

    #[test]
    fn keys_turn_at_full_deflection() {
        key_stick(false, false, false, false);
        assert_eq!(key_stick(false, false, false, true), [0, 100]);
        assert_eq!(key_stick(false, true, false, true), [71, 71]);
        assert_eq!(key_stick(false, false, false, false), [0, 0]);
    }
}
