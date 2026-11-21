//! Widescreen for the game itself: the window shows a wider view than the
//! GameCube's 4:3, so the game must not skip what lies past the 4:3 edges.
//! Ninja's screen setup (0x801126B0) computes the clip rectangle the game
//! tests objects against; after it runs, the rectangle is widened sideways.
//! Drawing a little more than the window shows costs nothing visible, so one
//! factor covers 16:9 to 21:9. SA2_ASPECT=4:3 keeps the original.
use crate::original;
use sa2_recomp::rt::{Cpu, Mem};

/// Ninja's screen setup (copies an NJS_SCREEN, derives the clip rectangle).
pub const NJ_SET_SCREEN: u32 = 0x8011_26B0;
/// The clip rectangle: +0x18 left, +0x20 right (pixels), +0x08 right - cx,
/// +0x0C left - cx.
const CLIP: u32 = 0x8035_78C0;
/// _nj_screen_ (+0x08 cx).
const SCREEN: u32 = 0x8035_78E8;
/// How much wider than 4:3 the game keeps things (21:9 / 4:3).
const WIDER: f64 = 1.75;

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if std::env::var("SA2_ASPECT").is_ok_and(|a| a.trim() == "4:3") {
        return;
    }
    c.rt.set_override(NJ_SET_SCREEN, set_screen);
}

fn set_screen(c: &mut Cpu, m: &mut Mem) {
    original(c, m, NJ_SET_SCREEN);
    widen_clip(m);
}

/// Widen the clip rectangle about the screen centre.
pub fn widen_clip(m: &mut Mem) {
    let cx = m.rf32(SCREEN + 0x08);
    let left = cx + (m.rf32(CLIP + 0x18) - cx) * WIDER;
    let right = cx + (m.rf32(CLIP + 0x20) - cx) * WIDER;
    m.wf32(CLIP + 0x18, left);
    m.wf32(CLIP + 0x20, right);
    m.wf32(CLIP + 0x08, right - cx);
    m.wf32(CLIP + 0x0C, left - cx);
}
