//! Chao MMO glue: the gardens' native Chao Transporter (ChaoStgOdekake,
//! module 73) becomes the Tiny Chao Garden menu. Its title reads "Tiny Chao
//! Garden", and its only items are "Chao Transporter" (the native Drop off
//! screen, titled "Tiny Chao Garden") and "Quit". Only the game's own sprites
//! are used: entries of its sprite table are pointed at other native art.
//!
//! Module 73 map (generated ChaoStgOdekake, static reading; unverified live):
//! - sprite table: .data +0xE0, 0x24 bytes per entry (u32 uv mode, f32 w, h,
//!   u0, v0, u1, v1, u32 0, u32 texture);
//! - entry 0x00: "TINY CHAO GARDEN" (add_odekake_menu_en);
//! - entries 0x10 + 2*n: large titles (0 Chao Transporter, 1 Drop off);
//! - entries 4 + 2*item (+1 greyed): top menu labels (item 0 Drop off);
//! - .text 0x5CAC: top menu exec(work in r3), cursor u32 at work +0x28 (0..4, 4 = Quit);
//! - .text 0x3990: makes menu item `r3` (f2 = its y; .rodata +0x2D0 = row step).

use crate::start_stage::module_section;
use sa2_recomp::rt::{self, Cpu, Mem};
use sa2_recomp::{global, global::Global};

const MODULE: u32 = 73;
const TOP_MENU: u32 = 0x5CAC;
const MENU_BAR: u32 = 0x3990;
const TABLE: u32 = 0xE0;
const ENTRY: u32 = 0x24;
const ROW_STEP: u32 = 0x2D0;
const QUIT: u32 = 4;

/// The module 73 .text the overrides were set for (0 = none).
static TEXT: Global<u32> = global!(0);

/// Each frame in Chao World: once module 73 is linked (again), point its
/// sprites at the Tiny Chao Garden art and hook its menu.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    let Some(text) = module_section(m, MODULE, 1) else {
        TEXT.set(0);
        return;
    };
    if TEXT.get() == text {
        return;
    }
    let Some(data) = module_section(m, MODULE, 5) else {
        return;
    };
    TEXT.set(text);
    restyle(m, data);
    c.rt.set_override(text + TOP_MENU, top_menu);
    c.rt.set_override(text + MENU_BAR, menu_bar);
}

fn entry(data: u32, i: u32) -> u32 {
    data + TABLE + i * ENTRY
}

/// Copy sprite `from` into `to`, scaled to `height` (keeping its shape).
fn copy_sprite(m: &mut Mem, from: u32, to: u32, height: f64) {
    for o in (0..ENTRY).step_by(4) {
        m.w32(to + o, m.r32(from + o));
    }
    let (w, h) = (m.rf32(from + 4), m.rf32(from + 8));
    if h > 0.0 {
        m.wf32(to + 4, w * height / h);
        m.wf32(to + 8, height);
    }
}

fn restyle(m: &mut Mem, data: u32) {
    let (tiny, transporter) = (entry(data, 0x00), entry(data, 0x10));
    // The "Chao Transporter" title becomes item 0's label (enabled and greyed),
    // copied before the title itself is changed.
    let label_h = m.rf32(entry(data, 4) + 8);
    copy_sprite(m, transporter, entry(data, 4), label_h);
    copy_sprite(m, transporter, entry(data, 5), label_h);
    // The menu title and the Drop off header read "Tiny Chao Garden"; their
    // second title sprite is hidden (zero size).
    for title in [0x10, 0x12] {
        let h = m.rf32(entry(data, title) + 8);
        copy_sprite(m, tiny, entry(data, title), h);
        m.wf32(entry(data, title + 1) + 4, 0.0);
        m.wf32(entry(data, title + 1) + 8, 0.0);
    }
}

/// Only items 0 (Chao Transporter) and 4 (Quit) are made; Quit takes row 1.
fn menu_bar(c: &mut Cpu, m: &mut Mem) {
    let item = c.r[3];
    if (1..QUIT).contains(&item) {
        return;
    }
    if item == QUIT {
        if let Some(ro) = module_section(m, MODULE, 4) {
            let step = m.rf32(ro + ROW_STEP);
            c.f[2].ps0 -= step * f64::from(QUIT - 1);
        }
    }
    let a = TEXT.get() + MENU_BAR;
    rel_original(c, m, a, menu_bar);
}

/// The cursor skips the hidden items: down from 0 lands on Quit, up from Quit on 0.
fn top_menu(c: &mut Cpu, m: &mut Mem) {
    // r3 is the menu's work (r31 in 0x5CAC; 0 = nothing to do).
    let work = c.r[3];
    let before = (work != 0).then(|| m.r32(work + 0x28));
    let a = TEXT.get() + TOP_MENU;
    rel_original(c, m, a, top_menu);
    if let Some(before) = before {
        if (1..QUIT).contains(&m.r32(work + 0x28)) {
            m.w32(work + 0x28, if before == 0 { QUIT } else { 0 });
        }
    }
}

/// Run an overridden REL function once without the override.
fn rel_original(c: &mut Cpu, m: &mut Mem, addr: u32, me: rt::Func) {
    c.rt.overrides.remove(&addr);
    c.rt.modules_changed();
    rt::call(c, m, addr);
    c.rt.set_override(addr, me);
}
