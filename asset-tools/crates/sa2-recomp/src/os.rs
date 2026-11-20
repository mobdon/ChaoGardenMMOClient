//! Host side of the Dolphin OS thread, alarm and interrupt layer, written for
//! this project (behaviour per the GameCube SDK; no emulator code).
//!
//! Threads: the game's own OSThread code runs as recompiled (OSCreateThread,
//! OSResumeThread, OSSuspendThread, OSSleepThread/OSWakeupThread,
//! OSYieldThread, mutexes, message queues, SelectThread's priority pick).
//! Only its two context primitives are replaced:
//! - OSSaveContext(ctx) records that `ctx` belongs to the running thread
//!   (and stores its registers there), and returns 0;
//! - OSLoadContext(ctx) switches to the thread owning `ctx`, starting one
//!   for a context OSInitContext prepared.
//!
//! Each OS thread runs on a host thread of its own, holding its own `Cpu`
//! (GPRs, FPRs, LR, CTR, CR, SP: the saved context). One baton (memory, the
//! call table and the time base) moves between them, so exactly one runs at a
//! time and runs are deterministic. A switched-out thread waits inside
//! OSLoadContext; when its context is loaded again, OSLoadContext returns into
//! SelectThread, which then returns as it would from OSSaveContext.
//!
//! Interrupts: a frame (time, due alarms, frame hooks such as the VI retrace)
//! runs with the scheduler disabled, then reschedules, as an interrupt return
//! would. Frames come from VIWaitForRetrace ([`wait_frame`]), from the
//! scheduler idling (every thread waiting) and from a long spin
//! ([`rt::PREEMPT_EVERY`] loop iterations).

use crate::global::Global;
use crate::rt::{self, Cpu, Fpr, Func, Mem, Runtime};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};

/// GSNE8P addresses (decomp symbols.txt).
pub const OS_SET_ALARM: u32 = 0x800E_402C;
pub const OS_SET_PERIODIC_ALARM: u32 = 0x800E_4094;
pub const OS_CANCEL_ALARM: u32 = 0x800E_4110;
/// Boots the DSP and waits for its mail; there is no DSP here.
pub const OS_INIT_AUDIO_SYSTEM: u32 = 0x800E_4B74;
pub const OS_SAVE_CONTEXT: u32 = 0x800E_563C;
pub const OS_LOAD_CONTEXT: u32 = 0x800E_56BC;
pub const OS_INIT_CONTEXT: u32 = 0x800E_57C0;
pub const OS_ENABLE_INTERRUPTS: u32 = 0x800E_78C0;
pub const OS_DISABLE_SCHEDULER: u32 = 0x800E_BA84;
pub const OS_ENABLE_SCHEDULER: u32 = 0x800E_BAC4;
pub const OS_RESCHEDULE: u32 = 0x800E_BF68;
pub const OS_RESUME_THREAD: u32 = 0x800E_C498;
pub const OS_SUSPEND_THREAD: u32 = 0x800E_C720;

/// __OSCurrentThread and the current OSContext pointer (low memory).
pub const CURRENT_THREAD: u32 = 0x8000_00E4;
pub const CURRENT_CONTEXT: u32 = 0x8000_00D4;
/// SelectThread's idle context and ready-queue bitmap (OSThread.c statics).
pub const IDLE_CONTEXT: u32 = 0x802B_B2D8;
pub const RUN_QUEUE_BITS: u32 = 0x803A_DCE0;
/// OSDisableScheduler's count.
pub const RESCHEDULE: u32 = 0x803A_DCE8;

/// Time base runs at bus clock / 4 (162 MHz / 4).
pub const TB_HZ: u64 = 40_500_000;
pub const TB_PER_FRAME: u64 = TB_HZ / 60;
/// Host stack for each game thread but the first (recompiled code nests a
/// Rust frame per guest call). Reserved, not committed.
const THREAD_STACK: usize = 256 << 20;

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
    pumping: bool,
    /// Threads suspended in VIWaitForRetrace until the next frame.
    frame_waiters: Vec<u32>,
}

static S: Global<State> = crate::global!(State::default());

/// What a host thread waits for.
enum Msg {
    /// Run: the baton.
    Run(Baton),
    /// Your OSThread was reused; unwind and end.
    Exit,
    /// (To the first thread) another thread panicked with the baton.
    Died(Baton, Box<dyn Any + Send>),
}

struct Baton {
    mem: Mem,
    rt: Runtime,
    tb: u64,
}

#[derive(Default)]
struct Sched {
    /// Host thread index -> its inbox. Index 0 is the thread that booted.
    inbox: Vec<Sender<Msg>>,
    /// OSContext address -> host thread index.
    owner: HashMap<u32, usize>,
    running: usize,
}

static SCHED: Global<Sched> = crate::global!(Sched::default());

thread_local! {
    /// This host thread's index and inbox.
    static ME: RefCell<Option<(usize, Receiver<Msg>)>> = const { RefCell::new(None) };
}

/// Unwinds a host thread whose OS thread is gone.
struct ThreadGone;

/// Register a hook run on every frame (e.g. the VI retrace interrupt). The
/// same hook twice runs once.
pub fn add_frame_hook(f: Func) {
    S.with_borrow_mut(|s| {
        if !s.frame_hooks.iter().any(|h| std::ptr::fn_addr_eq(*h, f)) {
            s.frame_hooks.push(f);
        }
    });
}

pub fn pumps() -> u64 {
    S.with_borrow(|s| s.pumps)
}

/// Install on the host thread that will run the game's entry point.
pub fn install(c: &mut Cpu) {
    S.set(State::default());
    let (tx, rx) = channel();
    SCHED.set(Sched {
        inbox: vec![tx],
        ..Sched::default()
    });
    ME.set(Some((0, rx)));
    c.rt.set_override(OS_INIT_AUDIO_SYSTEM, |_, _| {});
    c.rt.set_override(OS_SET_ALARM, set_alarm);
    c.rt.set_override(OS_SET_PERIODIC_ALARM, set_periodic_alarm);
    c.rt.set_override(OS_CANCEL_ALARM, cancel_alarm);
    c.rt.set_override(OS_SAVE_CONTEXT, save_context);
    c.rt.set_override(OS_LOAD_CONTEXT, load_context);
    c.rt.set_override(OS_INIT_CONTEXT, init_context);
    c.rt.set_override(OS_ENABLE_INTERRUPTS, enable_interrupts);
    c.rt.preempt = Some(preempt);
}

fn original(c: &mut Cpu, m: &mut Mem, addr: u32) {
    match (c.rt.generated)(addr) {
        Some(f) => f(c, m),
        None => panic!("sa2-recomp: no recompiled function at {addr:#010x}"),
    }
}

/// Call a game function if there is one (tests run without the game).
fn guest(c: &mut Cpu, m: &mut Mem, addr: u32, r3: u32) {
    if c.rt.resolve(m, addr).is_some() {
        c.r[3] = r3;
        rt::call(c, m, addr);
    }
}

// ---- context switch ------------------------------------------------------

// OSContext layout.
const CTX_CR: u32 = 0x80;
const CTX_LR: u32 = 0x84;
const CTX_CTR: u32 = 0x88;
const CTX_FPR: u32 = 0x90;
const CTX_FPSCR: u32 = 0x194;
const CTX_SRR0: u32 = 0x198;
const CTX_SRR1: u32 = 0x19C;
const CTX_GQR: u32 = 0x1A4;
const CTX_PSF: u32 = 0x1C8;

fn save_context(c: &mut Cpu, m: &mut Mem) {
    let ctx = c.r[3];
    for i in 0..32 {
        m.w32(ctx + 4 * i, c.r[i as usize]);
        m.wf64(ctx + CTX_FPR + 8 * i, c.f[i as usize].ps0);
        m.wf64(ctx + CTX_PSF + 8 * i, c.f[i as usize].ps1);
    }
    for i in 0..8 {
        m.w32(ctx + CTX_GQR + 4 * i, c.gqr[i as usize]);
    }
    m.w32(ctx + CTX_CR, c.cr);
    m.w32(ctx + CTX_LR, c.lr);
    m.w32(ctx + CTX_CTR, c.ctr);
    m.w32(ctx + CTX_FPSCR, c.fpscr);
    m.w32(ctx + CTX_SRR0, c.lr);
    m.w32(ctx + CTX_SRR1, c.msr);
    let me = my_index();
    SCHED.with_borrow_mut(|s| s.owner.insert(ctx, me));
    c.r[3] = 0;
}

fn load_context(c: &mut Cpu, m: &mut Mem) {
    let ctx = c.r[3];
    let me = my_index();
    let owner = SCHED.with_borrow(|s| s.owner.get(&ctx).copied());
    let to = match owner {
        Some(i) if i == me => return, // picked again: back into SelectThread
        Some(i) => i,
        None => start_thread(c, m, ctx),
    };
    if std::env::var_os("SA2_TRACE_THREADS").is_some() {
        eprintln!("thread: switch {me} -> {to} (context {ctx:#010x})");
    }
    handoff(c, m, to);
}

// OSInitContext(ctx, pc, sp): a context reused for a new thread; the host
// thread that owned it (an exited OS thread) ends.
fn init_context(c: &mut Cpu, m: &mut Mem) {
    let ctx = c.r[3];
    let old = SCHED.with_borrow_mut(|s| {
        let i = s.owner.remove(&ctx)?;
        (i != s.running).then(|| s.inbox[i].clone())
    });
    if let Some(tx) = old {
        let _ = tx.send(Msg::Exit);
    }
    original(c, m, OS_INIT_CONTEXT);
}

fn my_index() -> usize {
    ME.with_borrow(|me| me.as_ref().map_or(0, |m| m.0))
}

/// Registers a new thread starts with, read from its OSContext.
struct Start {
    r: [u32; 32],
    f: [Fpr; 32],
    cr: u32,
    lr: u32,
    ctr: u32,
    fpscr: u32,
    msr: u32,
    gqr: [u32; 8],
    spr: Box<[u32; 1024]>,
    pc: u32,
}

/// A host thread for the context OSInitContext prepared at `ctx`.
fn start_thread(c: &Cpu, m: &Mem, ctx: u32) -> usize {
    let s = Start {
        r: std::array::from_fn(|i| m.r32(ctx + 4 * i as u32)),
        f: std::array::from_fn(|i| Fpr {
            ps0: m.rf64(ctx + CTX_FPR + 8 * i as u32),
            ps1: m.rf64(ctx + CTX_PSF + 8 * i as u32),
        }),
        cr: m.r32(ctx + CTX_CR),
        lr: m.r32(ctx + CTX_LR),
        ctr: m.r32(ctx + CTX_CTR),
        fpscr: m.r32(ctx + CTX_FPSCR),
        msr: m.r32(ctx + CTX_SRR1),
        gqr: std::array::from_fn(|i| m.r32(ctx + CTX_GQR + 4 * i as u32)),
        spr: c.spr.clone(),
        pc: m.r32(ctx + CTX_SRR0),
    };
    let (tx, rx) = channel();
    let i = SCHED.with_borrow_mut(|sc| {
        sc.inbox.push(tx);
        sc.owner.insert(ctx, sc.inbox.len() - 1);
        sc.inbox.len() - 1
    });
    if std::env::var_os("SA2_TRACE_THREADS").is_some() {
        eprintln!("thread: start {i} at {:#010x} (context {ctx:#010x})", s.pc);
    }
    let spawned = std::thread::Builder::new()
        .name(format!("game thread {i}"))
        .stack_size(THREAD_STACK)
        .spawn(move || thread_main(i, rx, s));
    if let Err(e) = spawned {
        panic!("sa2-recomp: can't start a host thread for context {ctx:#010x}: {e}");
    }
    i
}

fn thread_main(i: usize, rx: Receiver<Msg>, s: Start) {
    let b = match rx.recv() {
        Ok(Msg::Run(b)) => b,
        _ => return,
    };
    ME.set(Some((i, rx)));
    let mut c = Cpu {
        r: s.r,
        f: s.f,
        cr: s.cr,
        lr: s.lr,
        ctr: s.ctr,
        fpscr: s.fpscr,
        msr: s.msr,
        gqr: s.gqr,
        spr: s.spr,
        tb: b.tb,
        rt: b.rt,
        ..Cpu::default()
    };
    let mut m = b.mem;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // The entry returns into OSExitThread (the context's LR), which
        // switches away for good.
        let exit = c.lr;
        rt::call(&mut c, &mut m, s.pc);
        if exit != 0 {
            rt::call(&mut c, &mut m, exit);
        }
        panic!("sa2-recomp: game thread {i} returned past OSExitThread");
    }));
    let Err(p) = r;
    if p.is::<ThreadGone>() {
        return;
    }
    // The run is over: hand the baton and the panic to the first thread.
    let b = Baton {
        mem: m,
        rt: std::mem::take(&mut c.rt),
        tb: c.tb,
    };
    let first = SCHED.with_borrow(|s| s.inbox.first().cloned());
    if let Some(tx) = first {
        let _ = tx.send(Msg::Died(b, p));
    }
}

/// Give the baton to host thread `to` and wait to get it back.
fn handoff(c: &mut Cpu, m: &mut Mem, to: usize) {
    let empty = Mem {
        ram: Vec::new(),
        lc: Vec::new(),
        io_read: m.io_read,
        io_write: m.io_write,
    };
    let b = Baton {
        mem: std::mem::replace(m, empty),
        rt: std::mem::take(&mut c.rt),
        tb: c.tb,
    };
    let tx = SCHED.with_borrow_mut(|s| {
        s.running = to;
        s.inbox.get(to).cloned()
    });
    if !tx.is_some_and(|tx| tx.send(Msg::Run(b)).is_ok()) {
        panic!("sa2-recomp: game thread {to} is gone");
    }
    let msg = ME.with_borrow(|me| me.as_ref().map(|(_, rx)| rx.recv()));
    match msg {
        Some(Ok(Msg::Run(b))) => {
            *m = b.mem;
            c.rt = b.rt;
            c.tb = b.tb;
        }
        Some(Ok(Msg::Died(b, p))) => {
            *m = b.mem;
            c.rt = b.rt;
            c.tb = b.tb;
            std::panic::resume_unwind(p);
        }
        _ => std::panic::panic_any(ThreadGone),
    }
}

// ---- interrupts ----------------------------------------------------------

fn in_idle(m: &Mem) -> bool {
    m.r32(CURRENT_CONTEXT) == IDLE_CONTEXT && m.r32(RUN_QUEUE_BITS) == 0
}

/// The next loop iteration preempts again (SelectThread's idle spin: each
/// pass is a frame until a thread is ready).
fn preempt_soon(c: &mut Cpu, m: &Mem) {
    if in_idle(m) {
        c.rt.ticks |= rt::PREEMPT_EVERY - 1;
    }
}

// SelectThread's idle loop enables interrupts, then spins until a thread is
// ready: that is when the next interrupt comes.
fn enable_interrupts(c: &mut Cpu, m: &mut Mem) {
    original(c, m, OS_ENABLE_INTERRUPTS);
    if in_idle(m) {
        let r3 = c.r[3];
        pump(c, m);
        c.r[3] = r3;
        preempt_soon(c, m);
    }
}

/// Every `rt::PREEMPT_EVERY` loop iterations: an interrupt (a frame), after
/// which a higher-priority thread it readied runs.
fn preempt(c: &mut Cpu, m: &mut Mem) {
    pump(c, m);
    preempt_soon(c, m);
}

/// VIWaitForRetrace: the calling thread sleeps until the next frame while
/// other ready threads run. With none ready, or before threads exist, the
/// frame comes at once.
pub fn wait_frame(c: &mut Cpu, m: &mut Mem) {
    let me = m.r32(CURRENT_THREAD);
    let can_suspend = c.rt.resolve(m, OS_SUSPEND_THREAD).is_some();
    let none_ready = m.r32(RUN_QUEUE_BITS) == 0;
    if me == 0 || none_ready || m.r32(RESCHEDULE) != 0 || !can_suspend {
        pump(c, m);
        return;
    }
    S.with_borrow_mut(|s| s.frame_waiters.push(me));
    let saved = (c.r, c.f, c.lr, c.ctr, c.cr);
    guest(c, m, OS_SUSPEND_THREAD, me);
    (c.r, c.f, c.lr, c.ctr, c.cr) = saved;
}

fn tick(hi: u32, lo: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

// OSSetAlarm(OSAlarm* alarm, OSTime tick, handler): r3, r5:r6, r7
fn set_alarm(c: &mut Cpu, _m: &mut Mem) {
    let a = Alarm {
        addr: c.r[3],
        handler: c.r[7],
        fire: c.tb + tick(c.r[5], c.r[6]),
        period: 0,
    };
    S.with_borrow_mut(|s| {
        s.alarms.retain(|x| x.addr != a.addr);
        s.alarms.push(a);
    });
}

// OSSetPeriodicAlarm(alarm, OSTime start, OSTime period, handler): r3, r5:r6, r7:r8, r9
fn set_periodic_alarm(c: &mut Cpu, _m: &mut Mem) {
    let start = tick(c.r[5], c.r[6]);
    let period = tick(c.r[7], c.r[8]).max(1);
    let a = Alarm {
        addr: c.r[3],
        handler: c.r[9],
        fire: start.max(c.tb),
        period,
    };
    S.with_borrow_mut(|s| {
        s.alarms.retain(|x| x.addr != a.addr);
        s.alarms.push(a);
    });
}

fn cancel_alarm(c: &mut Cpu, _m: &mut Mem) {
    let addr = c.r[3];
    S.with_borrow_mut(|s| s.alarms.retain(|x| x.addr != addr));
}

/// One frame as an interrupt: time advances, due alarms fire, frame hooks
/// run and frame waiters resume, all with the scheduler disabled; then the
/// highest-priority ready thread runs (maybe not the caller). Registers the
/// interrupted code had are kept.
pub fn pump(c: &mut Cpu, m: &mut Mem) {
    if S.with_borrow_mut(|s| std::mem::replace(&mut s.pumping, true)) {
        return; // a frame inside a frame's handlers
    }
    let saved = (c.r, c.f, c.lr, c.ctr, c.cr);
    guest(c, m, OS_DISABLE_SCHEDULER, 0);
    c.tb += TB_PER_FRAME;
    let now = c.tb;
    let due: Vec<(u32, u32)> = S.with_borrow_mut(|s| {
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
    let at = std::env::var("SA2_STACK")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    if at.is_some() && at == Some(pumps()) {
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
    let hooks: Vec<Func> = S.with_borrow(|s| s.frame_hooks.clone());
    for h in hooks {
        h(c, m);
    }
    let waiters = S.with_borrow_mut(|s| std::mem::take(&mut s.frame_waiters));
    for t in waiters {
        guest(c, m, OS_RESUME_THREAD, t);
    }
    guest(c, m, OS_ENABLE_SCHEDULER, 0);
    S.with_borrow_mut(|s| s.pumping = false);
    guest(c, m, OS_RESCHEDULE, 0);
    (c.r, c.f, c.lr, c.ctr, c.cr) = saved;
}
