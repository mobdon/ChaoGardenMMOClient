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
//! - entries 0x10 + 2*n, 0x11 + 2*n: large titles, a picture drawn centred
//!   and the words drawn from their left end (0 the bus and "Chao
//!   Transporter", 1 Drop off);
//! - entries 4 + 2*item (+1 greyed): top menu labels (item 0 Drop off),
//!   drawn from their left end; entries 0x1E + item: their pictures;
//! - .rodata +0x19C: a menu bar's full width (184); +0x1D0 and +0x1C4: how
//!   far left of the bar's middle its picture's middle (72) and its label's
//!   left end (44) sit;
//! - .text 0x5CAC: top menu exec(work in r3), cursor u32 at work +0x28 (0..4, 4 = Quit);
//! - .text 0x3990: makes menu item `r3` (f2 = its y; .rodata +0x2D0 = row step).
//!
//! The Drop off screen (.text 0x6218) keeps its title, its "Selected Chao" bar
//! and its stats window; its three GBA sockets (.text 0x4854) and the bar over
//! them (.text 0x24FC with r3 = 0) are not made, and the Move screen's own
//! Chao picker (ChaoMain) stands in their place with one garden panel: side
//! 1, the Move screen's right-hand box. Its Chao are the memory card's server
//! Chao (SA2_CARD_CHAO: their ids, a line each, in the card's order; the page
//! writes their records to SA2_RECORD_IN), not the garden's: a garden Chao
//! has no login. The pick goes to SA2_TINY_PICK_OUT as that Chao's id. The
//! stats window shows the Chao under the cursor: its update (.text 0) makes
//! its Chao from ChaoMain 0xE684 (the Chao in the player's arms) whenever it
//! has none, so that answers with the cursor's record while the screen is up.
//! Read from the generated code and the REL files. First run (2026-11-22):
//! the screen opens and closes; the rest is unverified live:
//! - stats window: its task at .bss +0, work +0x24 its Chao's task (freed
//!   with 0x80071AE0), work +3 set = it makes no Chao;
//! - Drop off phases (.data +0x15F0): 0 makes the screen, 3 is the socket
//!   input, 0x12..0x14 leave to the top menu;
//! - ChaoMain 0x5287C(mode) makes the picker (work at .bss +0x50C88; a mode
//!   above 4 makes no panels), 0x54464(_, side; f1 x, f2 y) one garden panel;
//! - picker work: +0 mode, +0x28 cursor column (0-3 side 0, 4-7 side 1),
//!   +0x2C row (0, 1 Chao; 2 buttons: column 3 Back, 4 OK), +0x38 and +0x44
//!   the slot chosen on side 0 and 1 (1-based; OK needs both);
//! - picker state at .bss +0x50C8C: 2 cancelled, 4 confirmed; the screen
//!   answers 4 with 2 to close it; 0x52BAC is 0 once its panels are gone;
//! - Chao records: .bss +0x3D08 + side * 0x60, three pages of 8 pointers.

//!
//! The machine in the gardens (ChaoMain .text 0x2EE08 makes it, a set object;
//! its update 0x2F0CC) opens the menu when the player's collision touches its
//! first collision shape: 0x80005F88(task, 0) answers non-zero. That shape
//! (ChaoMain .data +0xBB18C) is a cylinder of radius 1.3 at (-0.4, 2, 0) from
//! the machine's spot. The MMO's player is a Chao; so that walking into the
//! machine always opens it, the same question is also answered by reach when
//! the game's own answer is no (MMO glue; whether the game's own hit fires
//! for the Chao is unverified live, the log says which one did).

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
/// Sprite entries: the first item's picture, the first large title's picture.
const ICON: u32 = 0x1E;
const TITLE: u32 = 0x10;
/// .rodata: a menu bar's width, and its picture's and label's place left of
/// its middle. The MMO's bars are wider (an MMO choice, as wide as the
/// designed screen's) to hold "Chao Transporter" at LABEL_H.
const BAR: [(u32, f64); 3] = [(0x19C, 320.0), (0x1D0, 136.0), (0x1C4, 110.0)];
const LABEL_H: f64 = 24.0;
/// .bss: the top menu's five "item can be chosen" flags.
const ENABLED: u32 = 0x828;
const DROP_OFF: u32 = 0x6218;
const SOCKET: u32 = 0x4854;
const CATEGORY_BAR: u32 = 0x24FC;
/// .bss: the stats window's task.
const WINDOW: u32 = 0;
/// DestroyTask (the stats window frees its Chao with it).
const DESTROY_TASK: u32 = 0x8007_1AE0;
const PHASE: u32 = 0x3D28;
const SET_PHASE: u32 = 0x3D58;
const SET_DONE: u32 = 0x3DD8;
const PHASE_INPUT: u32 = 3;
const PHASE_LEAVE: u32 = 0x12;
const PHASE_LEAVE_WAIT: u32 = 0x13;

const CHAOMAIN: u32 = 60;
/// The machine's update, and where it keeps its task (.bss).
const MACHINE_EXEC: u32 = 0x2F0CC;
const MACHINE_TASK: u32 = 0x50AC8;
/// "Which player touches collision shape r4 of task r3" (0 = none).
const HIT: u32 = 0x8000_5F88;
const PLAYERTWP: u32 = 0x801E_7768;
/// The machine's first collision shape: its radius and its offset from the
/// machine's spot (ChaoMain .data +0xBB18C), and its height.
const TRIGGER_R: f32 = 1.3;
const TRIGGER_OFF: f32 = 0.4;
const TRIGGER_H: f32 = 4.0;
/// The player's own width for the reach (an MMO choice; the Chao's collision
/// size is not read).
const PLAYER_R: f32 = 2.0;
const PICKER_NEW: u32 = 0x5287C;
const PICKER_BUSY: u32 = 0x52BAC;
const PICKER_SET_STATE: u32 = 0x52C5C;
const GARDEN_PANEL: u32 = 0x54464;
const PICKER_WORK: u32 = 0x50C88;
const PICKER_STATE: u32 = 0x50C8C;
/// The Chao in the player's arms (its record; 0 = none).
const HELD: u32 = 0xE684;
const RECORDS: u32 = 0x3D08;
/// Side 1's record pointers (three pages of 8).
const SIDE_1: u32 = RECORDS + 0x60;
/// The Move screen's place for side 1's panel (ChaoMain .rodata: 480, 272).
const PANEL_X: u32 = 0x99A0;
const PANEL_Y: u32 = 0x999C;
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
/// The card's Chao in the picker, in slot order.
static CARD: Global<Vec<u64>> = global!(Vec::new());
/// The record the stats window shows (0 = none), and where ChaoMain's HELD
/// is overridden to answer with it (0 = not overridden).
static SHOWN: Global<u32> = global!(0);
static HELD_AT: Global<u32> = global!(0);
/// ChaoMain's (.text + MACHINE_EXEC, .bss + MACHINE_TASK) this frame (0 = not linked).
static MACHINE: Global<(u32, u32)> = global!((0, 0));
static HIT_HOOKED: Global<bool> = global!(false);
/// What the log last said of the machine: its task, and how it was touched (0 no, 1 the game's hit, 2 reach).
static SAID: Global<(u32, u8)> = global!((0, 0));

/// Each frame in Chao World: once module 73 is linked (again), point its
/// sprites at the Tiny Chao Garden art and hook its menu.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    MACHINE.set(match (module_section(m, CHAOMAIN, 1), module_section(m, CHAOMAIN, 6)) {
        (Some(text), Some(bss)) => (text + MACHINE_EXEC, bss + MACHINE_TASK),
        _ => (0, 0),
    });
    if !HIT_HOOKED.replace(true) {
        c.rt.set_override(HIT, hit);
    }
    let Some(text) = module_section(m, MODULE, 1) else {
        TEXT.set(0);
        unhold(c);
        return;
    };
    if TEXT.get() == text {
        return;
    }
    let (Some(ro), Some(data)) = (module_section(m, MODULE, 4), module_section(m, MODULE, 5)) else {
        return;
    };
    TEXT.set(text);
    restyle(m, ro, data);
    c.rt.set_override(text + TOP_MENU, top_menu);
    c.rt.set_override(text + MENU_BAR, menu_bar);
    c.rt.set_override(text + DROP_OFF, drop_off);
    c.rt.set_override(text + SOCKET, socket);
    c.rt.set_override(text + CATEGORY_BAR, category_bar);
}

/// The game's hit question; for the machine's first shape, also yes when the
/// player stands within reach of it.
fn hit(c: &mut Cpu, m: &mut Mem) {
    let (task, shape) = (c.r[3], c.r[4]);
    crate::original(c, m, HIT);
    let (exec, slot) = MACHINE.get();
    if shape != 0 || exec == 0 || m.r32(slot) != task || m.r32(task + 0x10) != exec {
        return;
    }
    let pos = |twp: u32| [0x14, 0x18, 0x1C].map(|o| m.rf32(twp + o) as f32);
    let (machine, player) = (pos(m.r32(task + 0x34)), m.r32(PLAYERTWP));
    let how = if c.r[3] != 0 {
        1
    } else if player != 0 && in_reach(pos(player), machine) {
        c.r[3] = player;
        2
    } else {
        0
    };
    if SAID.replace((task, how)) != (task, how) {
        eprintln!("tiny-transporter: machine at {machine:?}, touched: {}", ["no", "the game's own hit", "by reach"][how as usize]);
    }
}

/// The player at `player` stands on the machine's trigger shape.
fn in_reach(player: [f32; 3], machine: [f32; 3]) -> bool {
    let flat = (player[0] - machine[0]).hypot(player[2] - machine[2]);
    flat <= TRIGGER_R + TRIGGER_OFF + PLAYER_R && (player[1] - machine[1]).abs() <= TRIGGER_H
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

fn restyle(m: &mut Mem, ro: u32, data: u32) {
    let (tiny, bus, words) = (entry(data, 0x00), entry(data, TITLE), entry(data, TITLE + 1));
    // Item 0 reads as the "Chao Transporter" title does: the bus as its
    // picture, the words as its label (enabled and greyed). Copied before
    // the titles change.
    copy_sprite(m, bus, entry(data, ICON), LABEL_H);
    copy_sprite(m, words, entry(data, 4), LABEL_H);
    copy_sprite(m, words, entry(data, 5), LABEL_H);
    for (at, v) in BAR {
        m.wf32(ro + at, v);
    }
    // The menu title and the Drop off header read "Tiny Chao Garden" at its
    // own size where their words were; their picture is hidden (zero size).
    let tiny_h = m.rf32(tiny + 8);
    for title in [TITLE, TITLE + 2] {
        copy_sprite(m, tiny, entry(data, title + 1), tiny_h);
        m.wf32(entry(data, title) + 4, 0.0);
        m.wf32(entry(data, title) + 8, 0.0);
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
    unhold(c);
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

/// The bar over the sockets ("TINY CHAO GARDEN", r3 = 0) is left out with
/// them: the picker's panel has its own header there.
fn category_bar(c: &mut Cpu, m: &mut Mem) {
    if !(NO_SOCKETS.get() && c.r[3] == 0) {
        let a = TEXT.get() + CATEGORY_BAR;
        rel_original(c, m, a, category_bar);
    }
}

/// ChaoMain's "Chao in the player's arms" while the Drop off screen is up:
/// the record its stats window is to show.
fn held(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = SHOWN.get();
}

fn unhold(c: &mut Cpu) {
    let at = HELD_AT.replace(0);
    if at != 0 {
        c.rt.overrides.remove(&at);
        c.rt.modules_changed();
    }
}

/// The stats window shows `record` (0: no Chao): its Chao is freed, and its
/// update makes the next one from `held`.
fn show(c: &mut Cpu, m: &mut Mem, record: u32) {
    SHOWN.set(record);
    let window = module_section(m, MODULE, 6).map_or(0, |bss| m.r32(bss + WINDOW));
    if window == 0 {
        return;
    }
    let work = m.r32(window + 0x34);
    let chao = m.r32(work + 0x24);
    if chao != 0 {
        crate::guest_call(c, m, DESTROY_TASK, &[chao]);
        m.w32(work + 0x24, 0);
    }
    m.w8(work + 3, u8::from(record == 0));
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
            m.w32(picker + 0x28, 4); // the cursor starts on side 1's first Chao
            let card = card_chao(m);
            for i in 0..24 {
                let info = card.get(i as usize).and_then(|&id| crate::chao_cache::info_addr(id));
                m.w32(cm_bss + SIDE_1 + i * 4, info.unwrap_or(0));
            }
            c.f[1].ps0 = m.rf32(cm_ro + PANEL_X);
            c.f[2].ps0 = m.rf32(cm_ro + PANEL_Y);
            crate::guest_call(c, m, cm_text + GARDEN_PANEL, &[MOVE_MODE, 1]);
            WAITED.set(0);
            c.rt.set_override(cm_text + HELD, held);
            HELD_AT.set(cm_text + HELD);
            let first = m.r32(cm_bss + SIDE_1);
            show(c, m, first);
            eprintln!("tiny-transporter: picker made with {} card Chao", card.len());
            CARD.set(card);
        }
        PHASE_INPUT => {
            let state = if picker == 0 { CANCELLED } else { m.r32(cm_bss + PICKER_STATE) };
            // Their records stay cached while they are on show.
            CARD.with_borrow(|c| c.iter().for_each(|&id| crate::chao_cache::set_online(id, true)));
            if picker != 0 {
                let (column, row) = one_side(m.r32(picker + 0x28), m.r32(picker + 0x2C));
                m.w32(picker + 0x28, column);
                m.w32(picker + 0x2C, row);
                m.w32(picker + 0x38, m.r32(picker + 0x44));
                // The window follows the cursor over the Chao; on the buttons
                // it keeps the last one.
                if let Some(i) = cursor_slot(column, row) {
                    let record = m.r32(cm_bss + SIDE_1 + i * 4);
                    if record != SHOWN.get() {
                        show(c, m, record);
                    }
                }
            }
            if state == CONFIRMED {
                let slot = m.r32(picker + 0x44);
                let id = CARD.with_borrow(|c| slot.checked_sub(1).and_then(|i| c.get(i as usize).copied()));
                eprintln!("tiny-transporter: picked slot {slot}, Chao {id:?}");
                if let (Some(id), Ok(path)) = (id, std::env::var("SA2_TINY_PICK_OUT")) {
                    let _ = std::fs::write(path, id.to_string());
                }
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

/// The cursor stays on side 1: a step onto side 0's Chao lands on OK.
fn one_side(column: u32, row: u32) -> (u32, u32) {
    if row < 2 && column < 4 { (4, 2) } else { (column, row) }
}

/// The cursor's place in side 1's first page of Chao (None: on the buttons).
// shortcut: the first page only (the card has at most 8 Chao); read the
// panel's page too if the list ever grows past a page.
fn cursor_slot(column: u32, row: u32) -> Option<u32> {
    (row < 2 && (4..8).contains(&column)).then(|| row * 4 + column - 4)
}

/// The card's Chao (at most a page of 8), each with its record in the cache.
fn card_chao(m: &mut Mem) -> Vec<u64> {
    let text = std::env::var("SA2_CARD_CHAO").ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let ids = card_ids(&text);
    if let Ok(dir) = std::env::var("SA2_RECORD_IN") {
        for &id in &ids {
            if let Ok(bytes) = std::fs::read(std::path::Path::new(&dir).join(id.to_string())) {
                crate::chao_cache::replace(m, id, &bytes);
            }
        }
    }
    ids
}

fn card_ids(text: &str) -> Vec<u64> {
    text.lines().filter_map(|l| l.trim().parse().ok()).take(8).collect()
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
        assert_eq!(one_side(4, 1), (4, 1));
        assert_eq!(one_side(3, 0), (4, 2)); // onto side 0: OK instead
        assert_eq!(one_side(4, 2), (4, 2)); // OK
        assert_eq!(one_side(3, 2), (3, 2)); // Back
    }

    #[test]
    fn reach_is_the_trigger_shape_and_the_player_width() {
        assert!(in_reach([3.0, 1.0, 0.0], [0.0, 0.0, 0.0]));
        assert!(!in_reach([4.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
        assert!(!in_reach([0.0, 5.0, 0.0], [0.0, 0.0, 0.0])); // a floor above
    }

    #[test]
    fn cursor_slot_counts_side_1_row_by_row() {
        assert_eq!(cursor_slot(4, 0), Some(0));
        assert_eq!(cursor_slot(7, 1), Some(7));
        assert_eq!(cursor_slot(4, 2), None); // OK button
        assert_eq!(cursor_slot(3, 0), None); // side 0
    }

    #[test]
    fn card_ids_are_a_page_of_numbers() {
        assert_eq!(card_ids("12\nx\n 7 \n"), vec![12, 7]);
        assert_eq!(card_ids(&"5\n".repeat(9)).len(), 8);
    }
}
