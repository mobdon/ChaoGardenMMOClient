//! VI without the video interface: VIWaitForRetrace is one frame (the clock
//! moves a field, the retrace count goes up, the post-retrace callback runs),
//! and the setup calls do nothing. VISetNextFrameBuffer is remembered for the
//! host to show.
use crate::{clock, guest_call};
use sa2_recomp::rt::{Cpu, Mem};
use std::cell::Cell;

pub const VI_SET_POST_RETRACE_CALLBACK: u32 = 0x800F_2164;
pub const VI_INIT: u32 = 0x800F_2430;
pub const VI_WAIT_FOR_RETRACE: u32 = 0x800F_28A8;
pub const VI_CONFIGURE: u32 = 0x800F_2D70;
pub const VI_FLUSH: u32 = 0x800F_350C;
pub const VI_SET_NEXT_FRAME_BUFFER: u32 = 0x800F_3628;
pub const VI_SET_BLACK: u32 = 0x800F_3694;
pub const VI_GET_RETRACE_COUNT: u32 = 0x800F_3710;
pub const VI_GET_TV_FORMAT: u32 = 0x800F_3818;
pub const VI_GET_DTV_STATUS: u32 = 0x800F_3894;

thread_local! {
    static RETRACES: Cell<u32> = const { Cell::new(0) };
    static NEXT_FB: Cell<u32> = const { Cell::new(0) };
    static POST_CB: Cell<u32> = const { Cell::new(0) };
    static LAST_POLL: Cell<u64> = const { Cell::new(0) };
}

fn nothing(_c: &mut Cpu, _m: &mut Mem) {}

fn wait_for_retrace(c: &mut Cpu, m: &mut Mem) {
    // A game thread waiting for a retrace yields; only the main thread makes frames.
    if sa2_recomp::os::thread_wait() {
        return;
    }
    clock::advance_frame(c);
    // A retrace is a frame for the OS layer too: due alarms fire, frame hooks run.
    sa2_recomp::os::pump(c, m);
    let n = RETRACES.get().wrapping_add(1);
    RETRACES.set(n);
    let cb = POST_CB.get();
    if cb != 0 {
        guest_call(c, m, cb, &[n]);
    }
}

fn set_post_retrace_callback(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = POST_CB.replace(c.r[3]);
}

fn set_next_frame_buffer(c: &mut Cpu, _m: &mut Mem) {
    NEXT_FB.set(c.r[3]);
}

fn get_retrace_count(c: &mut Cpu, m: &mut Mem) {
    // Polled back to back (no other call in between) by the main thread: the
    // game is waiting for the next retrace, so make one.
    let calls = sa2_recomp::rt::CALLS.load(std::sync::atomic::Ordering::Relaxed);
    if calls.wrapping_sub(LAST_POLL.replace(calls)) <= 2 && !sa2_recomp::os::in_thread() {
        let saved = (c.r, c.lr, c.ctr, c.cr);
        wait_for_retrace(c, m);
        (c.r, c.lr, c.ctr, c.cr) = saved;
    }
    c.r[3] = RETRACES.get();
}

fn zero(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 0;
}

/// The frame buffer the game asked to show next.
pub fn next_frame_buffer() -> u32 {
    NEXT_FB.get()
}

pub fn retrace_count() -> u32 {
    RETRACES.get()
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    for a in [VI_INIT, VI_CONFIGURE, VI_FLUSH, VI_SET_BLACK] {
        c.rt.set_override(a, nothing);
    }
    c.rt.set_override(VI_WAIT_FOR_RETRACE, wait_for_retrace);
    c.rt.set_override(VI_SET_POST_RETRACE_CALLBACK, set_post_retrace_callback);
    c.rt.set_override(VI_SET_NEXT_FRAME_BUFFER, set_next_frame_buffer);
    c.rt.set_override(VI_GET_RETRACE_COUNT, get_retrace_count);
    // NTSC, no component cable.
    c.rt.set_override(VI_GET_TV_FORMAT, zero);
    c.rt.set_override(VI_GET_DTV_STATUS, zero);
}
