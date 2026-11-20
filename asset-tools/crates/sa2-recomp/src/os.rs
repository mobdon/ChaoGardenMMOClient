//! Host side of the Dolphin OS thread, alarm and interrupt layer, written for
//! this project (behaviour per the GameCube SDK; no emulator code).
//!
//! The recompiled game runs on one host stack, so the game's own context
//! switch (OSLoadContext via __OSReschedule) cannot run. Model: the main thread
//! runs throughout; any wait (OSSleepThread) pumps the host instead: time
//! advances, due alarms fire, and per-frame hooks such as the VI retrace run.
//! Other game threads are created as data but never scheduled yet.

use crate::rt::{self, Cpu, Func, Mem};
use std::cell::RefCell;

/// GSNE8P addresses (decomp symbols.txt).
pub const OS_SET_ALARM: u32 = 0x800E_402C;
pub const OS_SET_PERIODIC_ALARM: u32 = 0x800E_4094;
pub const OS_CANCEL_ALARM: u32 = 0x800E_4110;
pub const OS_RESCHEDULE: u32 = 0x800E_BF68;
pub const OS_SLEEP_THREAD: u32 = 0x800E_C890;
/// Boots the DSP and waits for its mail; there is no DSP here.
pub const OS_INIT_AUDIO_SYSTEM: u32 = 0x800E_4B74;

/// Time base runs at bus clock / 4 (162 MHz / 4).
pub const TB_HZ: u64 = 40_500_000;
pub const TB_PER_FRAME: u64 = TB_HZ / 60;

struct Alarm {
    addr: u32,
    handler: u32,
    fire: u64,
    period: u64,
}

#[derive(Default)]
struct State {
    alarms: Vec<Alarm>,
    frame_hooks: Vec<Func>,
    pumps: u64,
}

thread_local! {
    static S: RefCell<State> = RefCell::new(State::default());
}

/// Register a hook run on every pump (e.g. the VI retrace interrupt).
pub fn add_frame_hook(f: Func) {
    S.with(|s| s.borrow_mut().frame_hooks.push(f));
}

pub fn pumps() -> u64 {
    S.with(|s| s.borrow().pumps)
}

pub fn install(c: &mut Cpu) {
    c.rt.set_override(OS_RESCHEDULE, |_, _| {});
    c.rt.set_override(OS_INIT_AUDIO_SYSTEM, |_, _| {});
    c.rt.set_override(OS_SLEEP_THREAD, |c, m| pump(c, m));
    c.rt.set_override(OS_SET_ALARM, set_alarm);
    c.rt.set_override(OS_SET_PERIODIC_ALARM, set_periodic_alarm);
    c.rt.set_override(OS_CANCEL_ALARM, cancel_alarm);
    c.rt.set_override(OS_CREATE_THREAD, create_thread);
    c.rt.set_override(OS_RESUME_THREAD, resume_thread);
    c.rt.set_override(OS_JOIN_THREAD, join_thread);
    c.rt.set_override(OS_EXIT_THREAD, |c, _| std::panic::panic_any(ThreadExit(c.r[3])));
    c.rt.set_override(OS_SUSPEND_THREAD, suspend_thread);
    c.rt.preempt = Some(preempt);
}

/// The main thread's priority (the SDK's default).
const MAIN_PRIO: u32 = 16;
/// VIWaitForRetrace: a main-thread spin-wait is interrupted with a retrace.
const VI_WAIT_FOR_RETRACE: u32 = 0x800F_28A8;

/// Every `rt::PREEMPT_EVERY` loop iterations. A background thread (run at a
/// frame) yields until the next frame, as the real one would be preempted;
/// the main thread spinning on memory gets a retrace, whose callbacks and
/// alarms are what such waits wait for.
fn preempt(c: &mut Cpu, m: &mut Mem) {
    match RUNNING.with_borrow(|r| r.last().copied()) {
        Some((me, true)) => {
            THREADS.with_borrow_mut(|ts| ts.iter_mut().filter(|t| t.addr == me).for_each(|t| t.waiting = true));
            std::panic::panic_any(ThreadYield);
        }
        Some((me, false)) => {
            // shortcut: a thread that spins 16 periods without one call (an
            // idle meter at high priority) is preempted too; restarting a pure
            // spin loses nothing. Real preemption would replace this.
            let n = rt::CALLS.load(std::sync::atomic::Ordering::Relaxed);
            let spun = SPIN.with(|s| {
                let (last, k) = s.get();
                let k = if last == n { k + 1 } else { 0 };
                s.set((n, k));
                k >= 16
            });
            if spun {
                THREADS.with_borrow_mut(|ts| ts.iter_mut().filter(|t| t.addr == me).for_each(|t| t.waiting = true));
                std::panic::panic_any(ThreadYield);
            }
        }
        None => {
            let saved = (c.r, c.f, c.lr, c.ctr, c.cr, c.ca, c.so);
            rt::call(c, m, VI_WAIT_FOR_RETRACE);
            (c.r, c.f, c.lr, c.ctr, c.cr, c.ca, c.so) = saved;
        }
    }
}

thread_local! {
    static SPIN: std::cell::Cell<(u64, u32)> = const { std::cell::Cell::new((0, 0)) };
}

fn current_prio() -> u32 {
    let Some((me, _)) = RUNNING.with_borrow(|r| r.last().copied()) else {
        return MAIN_PRIO;
    };
    THREADS.with_borrow(|ts| ts.iter().find(|t| t.addr == me).map_or(MAIN_PRIO, |t| t.prio))
}

pub const OS_SUSPEND_THREAD: u32 = 0x800E_C720;

// Threads running inline right now (innermost last), and whether each may be
// preempted (run in the background at a frame, not resumed or joined).
thread_local! {
    static RUNNING: RefCell<Vec<(u32, bool)>> = const { RefCell::new(Vec::new()) };
}

struct ThreadYield;

// A thread suspending itself yields back to whoever resumed it; its next
// resume runs the entry again. SA2's worker threads are
// `for (;;) { run queued work; OSSuspendThread(self); }`, whose entry goes
// straight to that loop, so re-entering is the same as continuing.
fn suspend_thread(c: &mut Cpu, m: &mut Mem) {
    if RUNNING.with_borrow(|r| r.last().map(|x| x.0) == Some(c.r[3])) {
        std::panic::panic_any(ThreadYield);
    }
    original(c, m, OS_SUSPEND_THREAD);
}

pub const OS_CREATE_THREAD: u32 = 0x800E_BF98;
pub const OS_EXIT_THREAD: u32 = 0x800E_C0B8;
pub const OS_JOIN_THREAD: u32 = 0x800E_C358;
pub const OS_RESUME_THREAD: u32 = 0x800E_C498;

/// A game thread. Threads run inline on their own stack when resumed, until
/// they return, exit, suspend themselves or wait (a retrace, OSSleepThread):
/// a wait yields back, and each main-thread frame runs the waiting threads
/// again with that first wait counted as done. That is a continuation for
/// SA2's `while (1) { wait; work; }` threads, whose entries go straight to
/// the loop. shortcut: no real context switch (state kept in registers
/// across a wait is lost); switch to real coroutines if a thread needs it.
struct Thread {
    addr: u32,
    entry: u32,
    param: u32,
    stack: u32,
    prio: u32,
    result: Option<u32>,
    /// Runs at the next frame (yielded at a wait, preempted, or a lower
    /// priority than its resumer).
    waiting: bool,
    /// It yielded at a wait, so the next run passes its first wait.
    at_wait: bool,
    skip_wait: bool,
}

/// Called at a wait point. Inside an inline thread: the first wait after a
/// re-run passes, any other yields (unwinds) back. Returns whether the caller
/// is a thread (the main thread's waits are frames; a thread's are not).
/// Whether a game thread (not the main thread) is running.
pub fn in_thread() -> bool {
    RUNNING.with_borrow(|r| !r.is_empty())
}

pub fn thread_wait() -> bool {
    let Some((me, _)) = RUNNING.with_borrow(|r| r.last().copied()) else {
        return false;
    };
    let skip = THREADS.with_borrow_mut(|ts| {
        let t = ts.iter_mut().find(|t| t.addr == me)?;
        let skip = std::mem::take(&mut t.skip_wait);
        t.waiting = !skip;
        t.at_wait = !skip;
        Some(skip)
    });
    if skip == Some(false) {
        std::panic::panic_any(ThreadYield);
    }
    true
}

/// Run the threads that yielded at a wait (main thread, once per frame).
fn run_waiting(c: &mut Cpu, m: &mut Mem) {
    let ready: Vec<u32> = THREADS.with_borrow_mut(|ts| {
        ts.iter_mut()
            .filter(|t| t.waiting && t.result.is_none())
            .map(|t| {
                t.waiting = false;
                t.skip_wait = std::mem::take(&mut t.at_wait);
                t.addr
            })
            .collect()
    });
    for t in ready {
        run_inline(c, m, t, true);
    }
}

thread_local! {
    static THREADS: RefCell<Vec<Thread>> = const { RefCell::new(Vec::new()) };
}

struct ThreadExit(u32);

fn original(c: &mut Cpu, m: &mut Mem, addr: u32) {
    match (c.rt.generated)(addr) {
        Some(f) => f(c, m),
        None => panic!("sa2-recomp: no recompiled function at {addr:#010x}"),
    }
}

// OSCreateThread(thread, func, param, stack top, size, prio, attr)
fn create_thread(c: &mut Cpu, m: &mut Mem) {
    let t = Thread {
        addr: c.r[3],
        entry: c.r[4],
        param: c.r[5],
        stack: c.r[6],
        prio: c.r[8],
        result: None,
        waiting: false,
        at_wait: false,
        skip_wait: false,
    };
    if std::env::var_os("SA2_TRACE_THREADS").is_some() {
        eprintln!("thread: create {:#010x} entry {:#010x} prio {} (current {})", t.addr, t.entry, t.prio, current_prio());
    }
    original(c, m, OS_CREATE_THREAD);
    THREADS.with_borrow_mut(|ts| {
        ts.retain(|x| x.addr != t.addr);
        ts.push(t);
    });
}

fn resume_thread(c: &mut Cpu, m: &mut Mem) {
    let me = c.r[3];
    if !THREADS.with_borrow(|ts| ts.iter().any(|t| t.addr == me && t.result.is_none())) {
        return original(c, m, OS_RESUME_THREAD);
    }
    let prio = THREADS.with_borrow(|ts| ts.iter().find(|t| t.addr == me).map_or(MAIN_PRIO, |t| t.prio));
    if prio > current_prio() {
        // Lower priority: it runs when the resumer waits (a frame) or joins it.
        THREADS.with_borrow_mut(|ts| ts.iter_mut().filter(|t| t.addr == me).for_each(|t| t.waiting = true));
    } else {
        run_inline(c, m, me, false);
    }
    c.r[3] = 1; // previous suspend count of a new thread
}

fn run_inline(c: &mut Cpu, m: &mut Mem, me: u32, preemptible: bool) {
    let Some((entry, param, stack)) = THREADS.with_borrow(|ts| ts.iter().find(|t| t.addr == me).map(|t| (t.entry, t.param, t.stack)))
    else {
        return;
    };
    let saved = (c.r, c.f, c.lr, c.ctr, c.cr);
    c.r[1] = (stack & !7).wrapping_sub(8);
    c.r[3] = param;
    RUNNING.with_borrow_mut(|r| r.push((me, preemptible)));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt::call(c, m, entry)));
    RUNNING.with_borrow_mut(|r| r.pop());
    let result = match r {
        Ok(()) => Some(c.r[3]),
        Err(p) => match p.downcast::<ThreadExit>() {
            Ok(e) => Some(e.0),
            Err(p) => match p.downcast::<ThreadYield>() {
                Ok(_) => None, // suspended itself: resumable again
                Err(p) => std::panic::resume_unwind(p),
            },
        },
    };
    THREADS.with_borrow_mut(|ts| ts.iter_mut().filter(|t| t.addr == me).for_each(|t| t.result = result));
    (c.r, c.f, c.lr, c.ctr, c.cr) = saved;
}

// OSJoinThread(thread, void** val)
fn join_thread(c: &mut Cpu, m: &mut Mem) {
    let me = c.r[3];
    let get = || THREADS.with_borrow(|ts| ts.iter().find(|t| t.addr == me).map(|t| t.result));
    if get() == Some(None) {
        // Joining waits for it: run it now.
        let saved = (c.r[3], c.r[4]);
        THREADS.with_borrow_mut(|ts| ts.iter_mut().filter(|t| t.addr == me).for_each(|t| t.waiting = false));
        run_inline(c, m, me, false);
        (c.r[3], c.r[4]) = saved;
    }
    match get().flatten() {
        Some(v) => {
            if c.r[4] != 0 {
                m.w32(c.r[4], v);
            }
            c.r[3] = 1;
        }
        None => original(c, m, OS_JOIN_THREAD),
    }
}

fn tick(c: &Cpu, ticks: u32, ticks_lo: u32) -> u64 {
    let _ = c;
    (ticks as u64) << 32 | ticks_lo as u64
}

// OSSetAlarm(OSAlarm* alarm, OSTime tick, handler): r3, r5:r6, r7
fn set_alarm(c: &mut Cpu, _m: &mut Mem) {
    let a = Alarm { addr: c.r[3], handler: c.r[7], fire: c.tb + tick(c, c.r[5], c.r[6]), period: 0 };
    S.with(|s| {
        let mut s = s.borrow_mut();
        s.alarms.retain(|x| x.addr != a.addr);
        s.alarms.push(a);
    });
}

// OSSetPeriodicAlarm(alarm, OSTime start, OSTime period, handler): r3, r5:r6, r7:r8, r9
fn set_periodic_alarm(c: &mut Cpu, _m: &mut Mem) {
    let start = tick(c, c.r[5], c.r[6]);
    let period = tick(c, c.r[7], c.r[8]).max(1);
    let a = Alarm { addr: c.r[3], handler: c.r[9], fire: start.max(c.tb), period };
    S.with(|s| {
        let mut s = s.borrow_mut();
        s.alarms.retain(|x| x.addr != a.addr);
        s.alarms.push(a);
    });
}

fn cancel_alarm(c: &mut Cpu, _m: &mut Mem) {
    let addr = c.r[3];
    S.with(|s| s.borrow_mut().alarms.retain(|x| x.addr != addr));
}

/// Advance one frame of time, fire due alarms and run frame hooks.
/// Registers the game expects preserved across the wait are saved.
pub fn pump(c: &mut Cpu, m: &mut Mem) {
    if thread_wait() {
        return; // a thread waiting: it yielded, or its wait is already over
    }
    let saved = (c.r, c.lr, c.ctr, c.cr);
    c.tb += TB_PER_FRAME;
    let now = c.tb;
    let due: Vec<(u32, u32)> = S.with(|s| {
        let mut s = s.borrow_mut();
        s.pumps += 1;
        let mut due = Vec::new();
        s.alarms.retain_mut(|a| {
            if a.fire > now {
                return true;
            }
            due.push((a.addr, a.handler));
            if a.period > 0 {
                while a.fire <= now {
                    a.fire += a.period;
                }
                true
            } else {
                false
            }
        });
        due
    });
    // Handler(OSAlarm*, OSContext*): context pointer is unused by game handlers.
    for (addr, handler) in due {
        c.r[3] = addr;
        c.r[4] = 0;
        rt::call(c, m, handler);
    }
    // Debug: SA2_STACK=N prints the guest call stack (saved LRs along the
    // back chain) at pump N, to find what a wait loop is waiting in.
    let at = std::env::var("SA2_STACK").ok().and_then(|s| s.parse::<u64>().ok());
    if at == Some(S.with(|s| s.borrow().pumps)) {
        let mut sp = saved.0[1];
        eprint!("stack at pump {}:", at.unwrap_or(0));
        for _ in 0..24 {
            sp = m.r32(sp);
            if sp == 0 || sp == u32::MAX {
                break;
            }
            eprint!(" {:#010x}", m.r32(sp + 4));
        }
        eprintln!();
    }
    let hooks: Vec<Func> = S.with(|s| s.borrow().frame_hooks.clone());
    for h in hooks {
        h(c, m);
    }
    run_waiting(c, m);
    (c.r, c.lr, c.ctr, c.cr) = saved;
}
