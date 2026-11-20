//! Audio without the DSP: AI/AR/AX/DSP setup returns success, and the DSP
//! (0xCC005000) and AI (0xCC006C00) registers read as idle. ARAM is real
//! memory here, so ARStartDMA and ARQPostRequest move bytes both ways and
//! run their callbacks at once. No sound is mixed yet.
use crate::{guest_call, io};
use sa2_recomp::rt::{Cpu, Mem};
use std::cell::{Cell, RefCell};

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
pub const AX_REGISTER_CALLBACK: u32 = 0x800F_892C;
pub const DSP_CHECK_MAIL_FROM_DSP: u32 = 0x800F_E0F4;
pub const DSP_SEND_MAIL_TO_DSP: u32 = 0x800F_E11C;
pub const DSP_INIT: u32 = 0x800F_E130;
pub const DSP_ADD_TASK: u32 = 0x800F_E288;

pub const DSP_REGS: u32 = 0xCC00_5000;
pub const AI_REGS: u32 = 0xCC00_6C00;
/// 16 MB of ARAM; the first 16 KB belong to the audio library.
const ARAM_SIZE: usize = 16 << 20;
const ARAM_USER_BASE: u32 = 0x4000;

thread_local! {
    static ARAM: RefCell<Vec<u8>> = RefCell::new(vec![0; ARAM_SIZE]);
    static AR_NEXT: Cell<u32> = const { Cell::new(ARAM_USER_BASE) };
}

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
    if ty == 0 {
        dma(m, true, src, dst, len);
    } else {
        dma(m, false, dst, src, len);
    }
    if cb != 0 {
        guest_call(c, m, cb, &[req]);
    }
}

fn dsp_add_task(_c: &mut Cpu, _m: &mut Mem) {
    // Returns the task it was given (still in r3).
}

pub fn install(c: &mut Cpu, m: &mut Mem) {
    for a in [
        AI_INIT_DMA,
        AI_START_DMA,
        AI_INIT,
        ARQ_INIT,
        AX_INIT,
        AX_QUIT,
        DSP_SEND_MAIL_TO_DSP,
        DSP_INIT,
    ] {
        c.rt.set_override(a, nothing);
    }
    for a in [
        AI_REGISTER_DMA_CALLBACK,
        AX_REGISTER_CALLBACK,
        DSP_CHECK_MAIL_FROM_DSP,
    ] {
        c.rt.set_override(a, zero);
    }
    c.rt.set_override(AR_INIT, ar_init);
    c.rt.set_override(AR_ALLOC, ar_alloc);
    c.rt.set_override(AR_START_DMA, ar_start_dma);
    c.rt.set_override(ARQ_POST_REQUEST, arq_post_request);
    c.rt.set_override(DSP_ADD_TASK, dsp_add_task);
    for (base, len) in [(DSP_REGS, 0x200), (AI_REGS, 0x20)] {
        io::route_read(m, base, base + len, |_, _| 0);
        io::route_write(m, base, base + len, |_, _, _| {});
    }
}
