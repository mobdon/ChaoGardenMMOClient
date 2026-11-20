//! GX: stores to the write-gather pipe (0xCC008000) feed an `sa2_gx`
//! [`GxFifo`]. The pipe's store hook can't see memory, so bytes queue up and
//! run at GXFlush, GXDrawDone and GXCopyDisp, where vertex arrays, textures
//! and called display lists are read from RAM. GXCopyDisp also ends a frame.
use crate::{io, original};
use sa2_gx::api::{Frame, Ram};
use sa2_gx::fifo::GxFifo;
use sa2_recomp::rt::{Cpu, Mem};
use std::cell::RefCell;

pub const GX_FLUSH: u32 = 0x8010_14F0;
pub const GX_DRAW_DONE: u32 = 0x8010_154C;
pub const GX_COPY_DISP: u32 = 0x8010_24D8;
/// The write-gather pipe.
pub const PIPE: u32 = 0xCC00_8000;

#[derive(Default)]
struct State {
    fifo: GxFifo,
    queued: Vec<u8>,
    frames: Vec<(u32, Frame)>,
}

thread_local! {
    static GX: RefCell<State> = RefCell::new(State::default());
}

fn pipe_write(_a: u32, size: u32, v: u32) {
    GX.with_borrow_mut(|s| s.queued.extend(io::bytes(size, v)));
}

/// Run the queued pipe bytes against memory.
pub fn drain(m: &Mem) {
    GX.with_borrow_mut(|s| {
        let q = std::mem::take(&mut s.queued);
        s.fifo.write(&q, &Ram(&m.ram));
    });
}

fn flush(_c: &mut Cpu, m: &mut Mem) {
    drain(m);
}

fn copy_disp(c: &mut Cpu, m: &mut Mem) {
    let dest = c.r[3];
    original(c, m, GX_COPY_DISP);
    drain(m);
    GX.with_borrow_mut(|s| {
        let f = s.fifo.gx.take_frame();
        s.frames.push((dest, f));
    });
}

/// Frames finished since the last call, with the XFB each was copied to.
pub fn take_frames() -> Vec<(u32, Frame)> {
    GX.with_borrow_mut(|s| std::mem::take(&mut s.frames))
}

pub fn install(c: &mut Cpu, m: &mut Mem) {
    io::route_write(m, PIPE, PIPE + 8, pipe_write);
    c.rt.set_override(GX_FLUSH, flush);
    c.rt.set_override(GX_DRAW_DONE, flush);
    c.rt.set_override(GX_COPY_DISP, copy_disp);
}
