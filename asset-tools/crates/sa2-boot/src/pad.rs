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
    if std::env::var_os("SA2_TRACE_PAD").is_some() {
        let b = PADS.get()[0].button;
        if b != 0 {
            eprintln!(
                "padread: {b:#06x} at frame {} lr {:#x}",
                sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed),
                c.lr
            );
        }
    }
    for (i, p) in PADS.get().iter().enumerate() {
        to_guest(m, base + 12 * i as u32, p);
    }
    crate::kinder_chart::frame(c, m);
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
///
/// Fixed-frame presses stop once the first FILE-anchored entry's file has
/// opened, so they cannot land on the screens the anchored steps handle.
/// Returns false when SA2_INPUT is unset.
pub fn scripted(frame: u64) -> bool {
    match script(frame) {
        Some(p) => {
            set_port0(p);
            true
        }
        None => false,
    }
}

/// Port 0 as SA2_INPUT has it this frame (None when SA2_INPUT is unset).
/// Values are hex: PAD button bits, plus 0x10000 / 0x20000 / 0x40000 /
/// 0x80000 for the stick held up / down / left / right. `*N` holds a press N
/// frames instead of 3 (e.g. `stg11D.REL+500:10000*300`). `~N` after an
/// anchored delay repeats the press every N frames until a file named by a
/// later entry (on another file) opens (e.g. `title.adx+60~60:1000` presses Start until the
/// next screen's file loads), for screens that ignore an early press.
pub fn script(frame: u64) -> Option<PadStatus> {
    if let Some(p) = replay(frame) {
        return Some(p);
    }
    let script = std::env::var("SA2_INPUT").ok()?;
    let opens = OPENS.lock().unwrap_or_else(|e| e.into_inner());
    Some(script_at(&script, &opens, frame))
}

/// [`script`] for a given script and list of opened files.
fn script_at(script: &str, opens: &[(String, u64)], frame: u64) -> PadStatus {
    let file_of = |item: &str| {
        item.rsplit_once(':')?
            .0
            .split_once('+')
            .map(|(f, _)| f.trim().to_lowercase())
    };
    let items: Vec<&str> = script.split(',').collect();
    let anchored = items
        .iter()
        .find_map(|i| file_of(i))
        .is_some_and(|a| opens.iter().any(|(n, _)| *n == a));
    let mut p = PadStatus::default();
    for (k, item) in items.iter().enumerate() {
        let Some((at, v)) = item.rsplit_once(':') else {
            continue;
        };
        let (v, hold) = match v.split_once('*') {
            Some((v, n)) => (v, n.trim().parse::<u64>().unwrap_or(3)),
            None => (v, 3),
        };
        let Ok(v) = u32::from_str_radix(v.trim().trim_start_matches("0x"), 16) else {
            continue;
        };
        let pressed = match at.trim().split_once('+') {
            Some((file, d)) => {
                let (d, every) = match d.split_once('~') {
                    Some((d, n)) => (d, n.trim().parse::<u64>().ok().filter(|&n| n > 0)),
                    None => (d, None),
                };
                let Ok(d) = d.trim().parse::<u64>() else {
                    continue;
                };
                let file = file.trim().to_lowercase();
                let later: Vec<String> = items[k + 1..]
                    .iter()
                    .filter_map(|i| file_of(i))
                    .filter(|f| *f != file)
                    .collect();
                opens
                    .iter()
                    .filter(|(n, _)| *n == file)
                    .any(|&(_, opened)| {
                        let start = opened + d;
                        let Some(every) = every else {
                            return (start..start + hold).contains(&frame);
                        };
                        let stop = opens
                            .iter()
                            .filter(|(n, f)| *f >= opened && later.contains(n))
                            .map(|(_, f)| *f)
                            .min()
                            .unwrap_or(u64::MAX);
                        frame >= start && frame < stop && (frame - start) % every < hold
                    })
            }
            None => match at.trim().parse::<u64>() {
                Ok(f) if !anchored => (f..f + hold).contains(&frame),
                _ => false,
            },
        };
        if pressed {
            p.button |= v as u16;
            let on = |bit: u32| v & bit != 0;
            if on(0x10000) {
                p.stick[1] = 100;
            }
            if on(0x20000) {
                p.stick[1] = -100;
            }
            if on(0x40000) {
                p.stick[0] = -100;
            }
            if on(0x80000) {
                p.stick[0] = 100;
            }
        }
    }
    p
}

/// Files the game opened, with the frame of each open (SA2_INPUT anchors).
static OPENS: std::sync::Mutex<Vec<(String, u64)>> = std::sync::Mutex::new(Vec::new());

/// Note that the game opened `name` (called by the DVD hook).
pub fn opened(name: &str) {
    let frame = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    OPENS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((name.to_lowercase(), frame));
}

/// Controller port 0 only (the others stay disconnected).
pub fn set_port0(p: PadStatus) {
    record(p);
    let mut pads = default_pads();
    pads[0] = p;
    set(pads);
}

/// One recorded frame hook: `hook button sx sy cx cy l r a b` (decimal).
fn line(frame: u64, p: &PadStatus) -> String {
    format!(
        "{frame} {} {} {} {} {} {} {} {} {}",
        p.button,
        p.stick[0],
        p.stick[1],
        p.substick[0],
        p.substick[1],
        p.triggers[0],
        p.triggers[1],
        p.analog_a,
        p.analog_b
    )
}

fn parse(l: &str) -> Option<(u64, PadStatus)> {
    let v: Vec<i64> = l
        .split_whitespace()
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let [f, b, sx, sy, cx, cy, tl, tr, aa, ab] = v.as_slice().try_into().ok()?;
    let i8_ = |x: i64| i8::try_from(x).ok();
    let u8_ = |x: i64| u8::try_from(x).ok();
    Some((
        u64::try_from(f).ok()?,
        PadStatus {
            button: u16::try_from(b).ok()?,
            stick: [i8_(sx)?, i8_(sy)?],
            substick: [i8_(cx)?, i8_(cy)?],
            triggers: [u8_(tl)?, u8_(tr)?],
            analog_a: u8_(aa)?,
            analog_b: u8_(ab)?,
            err: 0,
        },
    ))
}

static RECORDER: Global<Option<std::io::BufWriter<std::fs::File>>> = global!(None);
/// Frame hooks recorded / replayed so far (the line key).
static RECORDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REPLAYED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// SA2_RECORD=file: write controller 1 as the game reads it, a line a frame,
/// so a run can be replayed exactly (SA2_REPLAY).
fn record(p: PadStatus) {
    let Some(path) = std::env::var_os("SA2_RECORD") else {
        return;
    };
    use std::io::Write;
    // The frame hook sets the pad once a hook; replay counts the same way.
    let frame = RECORDED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    RECORDER.with_borrow_mut(|r| {
        if r.is_none() {
            *r = std::fs::File::create(&path)
                .ok()
                .map(std::io::BufWriter::new);
        }
        if let Some(w) = r {
            let _ = writeln!(w, "{}", line(frame, &p));
            let _ = w.flush();
        }
    });
}

static REPLAY: Global<Option<std::collections::HashMap<u64, PadStatus>>> = global!(None);

/// SA2_REPLAY=file: controller 1 as recorded with SA2_RECORD (frames not in
/// the file: nothing pressed).
fn replay(_frame: u64) -> Option<PadStatus> {
    let path = std::env::var_os("SA2_REPLAY")?;
    let frame = REPLAYED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    REPLAY.with_borrow_mut(|r| {
        let map = r.get_or_insert_with(|| {
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .filter_map(parse)
                .collect()
        });
        Some(map.get(&frame).copied().unwrap_or_default())
    })
}

#[cfg(test)]
mod tests {
    use super::script_at;

    #[test]
    fn recorded_lines_read_back() {
        let p = super::PadStatus {
            button: 0x1100,
            stick: [-100, 71],
            substick: [3, -4],
            triggers: [200, 0],
            analog_a: 9,
            analog_b: 0,
            err: 0,
        };
        assert_eq!(super::parse(&super::line(4321, &p)), Some((4321, p)));
        assert_eq!(super::parse("12 not numbers"), None);
    }

    #[test]
    fn repeat_presses_until_the_next_screen_loads() {
        let script = "movie.m1v+60~60:1000,title.adx+30:0100";
        let mut opens = vec![("movie.m1v".to_string(), 100)];
        let at = |opens: &[(String, u64)], f| script_at(script, opens, f).button;
        assert_eq!(at(&opens, 159), 0);
        assert_eq!(at(&opens, 160), 0x1000);
        assert_eq!(at(&opens, 163), 0, "held 3 frames");
        assert_eq!(at(&opens, 400), 0x1000, "repeats every 60");
        opens.push(("title.adx".to_string(), 450));
        assert_eq!(at(&opens, 460), 0, "stopped once the title loaded");
        assert_eq!(at(&opens, 480), 0x0100, "the title's own press");
    }

    #[test]
    fn fixed_frames_stop_once_an_anchor_opens() {
        let script = "0:1100,500:1100,a.prs+10:0002";
        assert_eq!(script_at(script, &[], 0).button, 0x1100);
        let opens = [("a.prs".to_string(), 100)];
        assert_eq!(script_at(script, &opens, 500).button, 0);
        assert_eq!(script_at(script, &opens, 111).button, 0x0002);
    }
}
