//! The OS context switch without the game: threads made of overrides switch
//! through OSSaveContext/OSLoadContext as SelectThread does, each keeping its
//! own registers, one running at a time.
use sa2_recomp::os::{self, OS_INIT_CONTEXT, OS_LOAD_CONTEXT, OS_SAVE_CONTEXT};
use sa2_recomp::rt::{call, Cpu, Func, Mem};
use std::sync::Mutex;

/// The scheduler is process-wide: one test at a time.
static LOCK: Mutex<()> = Mutex::new(());

const MAIN_CTX: u32 = 0x8000_2000;
const T_CTX: u32 = 0x8000_3000;
const ENTRY: u32 = 0x8100_0000;
const LOG: u32 = 0x8000_1000;
const STACK: u32 = 0x8010_0000;

fn log(m: &mut Mem, v: u32) {
    let n = m.r32(LOG);
    m.w32(LOG + 4 + 4 * n, v);
    m.w32(LOG, n + 1);
}

fn logged(m: &Mem) -> Vec<u32> {
    (0..m.r32(LOG)).map(|i| m.r32(LOG + 4 + 4 * i)).collect()
}

/// SelectThread's switch: save `from`, load `to`.
fn switch(c: &mut Cpu, m: &mut Mem, from: u32, to: u32) {
    c.r[3] = from;
    call(c, m, OS_SAVE_CONTEXT);
    assert_eq!(c.r[3], 0);
    c.r[3] = to;
    call(c, m, OS_LOAD_CONTEXT);
}

fn setup() -> (Cpu, Mem) {
    let mut c = Cpu::default();
    let m = Mem::default();
    // The "recompiled" OSInitContext the OS layer wraps.
    c.rt.generated = |a| (a == OS_INIT_CONTEXT).then_some(init_context as Func);
    os::install(&mut c);
    (c, m)
}

/// OSInitContext(ctx, pc, sp): the parts the switch reads.
fn init_context(c: &mut Cpu, m: &mut Mem) {
    let ctx = c.r[3];
    for o in (0..0x2C8).step_by(4) {
        m.w32(ctx + o, 0);
    }
    m.w32(ctx + 0x198, c.r[4]);
    m.w32(ctx + 4, c.r[5]);
}

/// A new thread for ENTRY with `param`, as OSCreateThread prepares it.
fn create(c: &mut Cpu, m: &mut Mem, param: u32) {
    let saved = c.r;
    c.r[3..6].copy_from_slice(&[T_CTX, ENTRY, STACK]);
    call(c, m, OS_INIT_CONTEXT);
    m.w32(T_CTX + 4 * 3, param);
    c.r = saved;
}

fn worker(c: &mut Cpu, m: &mut Mem) {
    assert_eq!(c.r[1], STACK);
    log(m, c.r[3]); // its param
    c.r[20] = 0x2222;
    switch(c, m, T_CTX, MAIN_CTX);
    assert_eq!(c.r[20], 0x2222, "the worker's own registers survive");
    log(m, 3);
    switch(c, m, T_CTX, MAIN_CTX);
    unreachable!("nothing loads the worker again");
}

#[test]
fn threads_switch_and_keep_their_registers() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut c, mut m) = setup();
    c.rt.set_override(ENTRY, worker);
    create(&mut c, &mut m, 1);
    c.r[20] = 0x1111;
    switch(&mut c, &mut m, MAIN_CTX, T_CTX);
    assert_eq!(c.r[20], 0x1111, "main's registers survive");
    log(&mut m, 2);
    switch(&mut c, &mut m, MAIN_CTX, T_CTX);
    log(&mut m, 4);
    assert_eq!(logged(&m), [1, 2, 3, 4]);
    // Saving stored main's registers in its context.
    assert_eq!(m.r32(MAIN_CTX + 4 * 20), 0x1111);
}

#[test]
fn loading_your_own_context_returns() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut c, mut m) = setup();
    switch(&mut c, &mut m, MAIN_CTX, MAIN_CTX);
    log(&mut m, 9);
    assert_eq!(logged(&m), [9]);
}

#[test]
fn reused_thread_context_starts_fresh() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut c, mut m) = setup();
    c.rt.set_override(ENTRY, worker);
    create(&mut c, &mut m, 1);
    switch(&mut c, &mut m, MAIN_CTX, T_CTX);
    // The worker is parked mid-way; OSCreateThread reuses its OSThread.
    create(&mut c, &mut m, 7);
    switch(&mut c, &mut m, MAIN_CTX, T_CTX);
    assert_eq!(logged(&m), [1, 7]);
}

#[test]
fn a_crash_in_a_thread_reaches_the_host_with_memory() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut c, mut m) = setup();
    c.rt.set_override(ENTRY, |_, m| {
        log(m, 5);
        panic!("worker broke");
    });
    create(&mut c, &mut m, 0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        switch(&mut c, &mut m, MAIN_CTX, T_CTX);
    }));
    let p = r.expect_err("the worker's panic comes back");
    assert_eq!(p.downcast_ref::<&str>().copied(), Some("worker broke"));
    assert_eq!(logged(&m), [5]);
}
