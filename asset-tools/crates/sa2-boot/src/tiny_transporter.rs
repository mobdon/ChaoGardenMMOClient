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
//!
//! The Drop off screen (.text 0x6218) keeps its title, bars and stats window;
//! its three GBA sockets (.text 0x4854) are not made, and the Move screen's own
//! Chao picker (ChaoMain) stands in their place with one garden panel, the
//! player's side. Read from the generated code and the REL files; no part of
//! it has run yet (unverified live):
//! - Drop off phases (.data +0x15F0): 0 makes the screen, 3 is the socket
//!   input, 0x12..0x14 leave to the top menu;
//! - ChaoMain 0x5287C(mode) makes the picker (work at .bss +0x50C88; a mode
//!   above 4 makes no panels), 0x54464(_, side; f1 x, f2 y) one garden panel;
//! - picker work: +0 mode, +0x28 cursor column (0-3 side 0, 4-7 side 1),
//!   +0x2C row (0, 1 Chao; 2 buttons: column 3 Back, 4 OK), +0x38 and +0x44
//!   the slot chosen on side 0 and 1 (1-based; OK needs both);
//! - picker state at .bss +0x50C8C: 2 cancelled, 4 confirmed; the screen
//!   answers 4 with 2 to close it; 0x52BAC is 0 once its panels are gone;
//! - Chao records: .bss +0x3D08, pages of 8 pointers (0x20 bytes a page).

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
/// .bss: the top menu's five "item can be chosen" flags.
const ENABLED: u32 = 0x828;
const DROP_OFF: u32 = 0x6218;
const SOCKET: u32 = 0x4854;
const PHASE: u32 = 0x3D28;
const SET_PHASE: u32 = 0x3D58;
const SET_DONE: u32 = 0x3DD8;
const PHASE_INPUT: u32 = 3;
const PHASE_LEAVE: u32 = 0x12;
const PHASE_LEAVE_WAIT: u32 = 0x13;

const CHAOMAIN: u32 = 60;
const PICKER_NEW: u32 = 0x5287C;
const PICKER_BUSY: u32 = 0x52BAC;
const PICKER_SET_STATE: u32 = 0x52C5C;
const GARDEN_PANEL: u32 = 0x54464;
const PICKER_WORK: u32 = 0x50C88;
const PICKER_STATE: u32 = 0x50C8C;
const RECORDS: u32 = 0x3D08;
/// The Move screen's panel height (ChaoMain .rodata) and the sockets' x.
const PANEL_Y: u32 = 0x999C;
const PANEL_X: f64 = 480.0;
const MOVE_MODE: u32 = 4;
const CANCELLED: u32 = 2;
const CONFIRMED: u32 = 4;
/// Frames the leave waits for the picker's panels before going on anyway.
const LEAVE_WAIT: u32 = 120;

/// The module 73 .text the overrides were set for (0 = none).
static TEXT: Global<u32> = global!(0);
/// The Drop off screen is being made: its sockets are left out.
static NO_SOCKETS: Global<bool> = global!(false);
static WAITED: Global<u32> = global!(0);

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
    c.rt.set_override(text + DROP_OFF, drop_off);
    c.rt.set_override(text + SOCKET, socket);
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
    // Item 0 opens with no Chao in the player's arms (its native condition
    // is read as "a Chao is held"; unverified live).
    if let Some(bss) = module_section(m, MODULE, 6) {
        m.w32(bss + ENABLED, 1);
    }
    let a = TEXT.get() + TOP_MENU;
    rel_original(c, m, a, top_menu);
    if let Some(before) = before {
        if (1..QUIT).contains(&m.r32(work + 0x28)) {
            m.w32(work + 0x28, if before == 0 { QUIT } else { 0 });
        }
    }
}

fn socket(c: &mut Cpu, m: &mut Mem) {
    if !NO_SOCKETS.get() {
        let a = TEXT.get() + SOCKET;
        rel_original(c, m, a, socket);
    }
}

/// The Drop off screen: the picker instead of the sockets and their input.
fn drop_off(c: &mut Cpu, m: &mut Mem) {
    let (text, work) = (TEXT.get(), c.r[3]);
    let cm = (module_section(m, CHAOMAIN, 1), module_section(m, CHAOMAIN, 4), module_section(m, CHAOMAIN, 6));
    let ((Some(cm_text), Some(cm_ro), Some(cm_bss)), true) = (cm, work != 0) else {
        return rel_original(c, m, text + DROP_OFF, drop_off);
    };
    let picker = m.r32(cm_bss + PICKER_WORK);
    match crate::guest_call(c, m, text + PHASE, &[]) {
        0 => {
            NO_SOCKETS.set(true);
            rel_original(c, m, text + DROP_OFF, drop_off);
            NO_SOCKETS.set(false);
            // A mode above the Move screen's makes the picker without panels.
            crate::guest_call(c, m, cm_text + PICKER_NEW, &[MOVE_MODE + 1]);
            let picker = m.r32(cm_bss + PICKER_WORK);
            if picker == 0 {
                eprintln!("tiny-transporter: the picker was not made");
                return;
            }
            m.w32(picker, MOVE_MODE);
            c.f[1].ps0 = PANEL_X;
            c.f[2].ps0 = m.rf32(cm_ro + PANEL_Y);
            crate::guest_call(c, m, cm_text + GARDEN_PANEL, &[MOVE_MODE, 0]);
            WAITED.set(0);
            eprintln!("tiny-transporter: picker made");
        }
        PHASE_INPUT => {
            let state = if picker == 0 { CANCELLED } else { m.r32(cm_bss + PICKER_STATE) };
            if picker != 0 {
                let (column, row) = one_side(m.r32(picker + 0x28), m.r32(picker + 0x2C));
                m.w32(picker + 0x28, column);
                m.w32(picker + 0x2C, row);
                m.w32(picker + 0x44, m.r32(picker + 0x38));
            }
            if state == CONFIRMED {
                let slot = m.r32(picker + 0x38);
                let record = record_at(slot).map_or(0, |o| m.r32(cm_bss + RECORDS + o));
                eprintln!("tiny-transporter: picked slot {slot}, record {record:#010x}");
                crate::guest_call(c, m, cm_text + PICKER_SET_STATE, &[CANCELLED]);
            }
            if state == CONFIRMED || state == CANCELLED {
                crate::guest_call(c, m, text + SET_DONE, &[]);
                crate::guest_call(c, m, text + SET_PHASE, &[PHASE_LEAVE]);
            }
        }
        PHASE_LEAVE_WAIT if picker != 0 && crate::guest_call(c, m, cm_text + PICKER_BUSY, &[]) != 0 && WAITED.replace(WAITED.get() + 1) < LEAVE_WAIT => {}
        _ => rel_original(c, m, text + DROP_OFF, drop_off),
    }
}

/// The cursor stays on the player's side: a step onto side 1's Chao lands on OK.
fn one_side(column: u32, row: u32) -> (u32, u32) {
    if row < 2 && column > 3 { (4, 2) } else { (column, row) }
}

/// Offset of slot `slot`'s (1-based) pointer in side 0's record pages.
fn record_at(slot: u32) -> Option<u32> {
    let i = slot.checked_sub(1)?;
    Some((i >> 3) * 0x20 + (i & 7) * 4)
}

/// Run an overridden REL function once without the override.
fn rel_original(c: &mut Cpu, m: &mut Mem, addr: u32, me: rt::Func) {
    c.rt.overrides.remove(&addr);
    c.rt.modules_changed();
    rt::call(c, m, addr);
    c.rt.set_override(addr, me);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keeps_to_side_0_and_the_buttons() {
        assert_eq!(one_side(3, 1), (3, 1));
        assert_eq!(one_side(4, 0), (4, 2)); // onto side 1: OK instead
        assert_eq!(one_side(4, 2), (4, 2)); // OK
        assert_eq!(one_side(3, 2), (3, 2)); // Back
    }

    #[test]
    fn records_are_pages_of_eight_pointers() {
        assert_eq!(record_at(0), None);
        assert_eq!(record_at(1), Some(0));
        assert_eq!(record_at(8), Some(0x1C));
        assert_eq!(record_at(9), Some(0x20));
    }
}
