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
    c.rt.set_override(OS_SLEEP_THREAD, |c, m| pump(c, m));
    c.rt.set_override(OS_SET_ALARM, set_alarm);
    c.rt.set_override(OS_SET_PERIODIC_ALARM, set_periodic_alarm);
    c.rt.set_override(OS_CANCEL_ALARM, cancel_alarm);
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
    let hooks: Vec<Func> = S.with(|s| s.borrow().frame_hooks.clone());
    for h in hooks {
        h(c, m);
    }
    (c.r, c.lr, c.ctr, c.cr) = saved;
}
