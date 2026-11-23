//! Chao MMO glue (browser build only: off unless SA2_KARATE_IN is set): the
//! player's Chao in Chao Karate attacks with the button pressed on the page
//! instead of the game's random pick. Dodges and everything else stay native.
//!
//! From GSNE8P ChaoStgKarate.rel (module 72), static reading:
//! - fn_72_FCC8(task) (.text 0xFCC8) is the fighter's approach phase. When the
//!   approach motion ends it picks an attack: rand < 0.5 -> punch family, else
//!   kick family; fighter work (task +0x40) +0x2C > 1600.0 (lbl_72_data_16874)
//!   -> the strong one. It passes 7 punch / 8 windup punch / 4 hard kick /
//!   5 roundhouse to fn_72_B620(task, id), which stores the id in the action
//!   byte at *(work +0xC) and clears the two step bytes after it.
//! - work +0x2C = 1200.0 + Power (fn_72_11038, AL_ParameterGetSkill index 3),
//!   so "> 1600" is Power above 400.
//! - work +2 is the fighter's index; fn_72_5020 always creates index 0 from the
//!   player's own Chao (kind 1, *lbl_72_bss_8 +0x10).
//! - Action 6 (Beating Up, motion 0x245) has its own handler fn_72_BDBC in the
//!   action jump table (sec5 0x17290) but no caller in this module passes 6 to
//!   fn_72_B620. Unverified: that the game ever plays it in a bout.
//! - A bout's two fighter tasks are at *lbl_72_bss_8 +0x10 and +0x14
//!   (fn_72_5020 creates them; fn_72_13258 tests both the same way). The page
//!   shows its buttons only while both are set. Unverified: that the game
//!   clears them when it returns to the mode select.
//! - fn_72_60E4(task) (.text 0x60E4) is the menu task. Its state is the byte
//!   at *(task +0x34), jump table at .data 0x1928: 3 fn_72_40D8 mode select,
//!   4 fn_72_4840 and 5 fn_72_4454 (1-player), 6 fn_72_4AC0, 7 fn_72_4D7C,
//!   8 fn_72_4EE8, 9 fn_72_5020 (makes the fighters), 15 leaves.
//! - fn_72_40D8: the cursor byte is *lbl_72_bss_8 +0xA; A stores it at +5 and
//!   goes to state 4 for entry 0, state 6 for entry 1. fn_72_4D7C (state 7,
//!   the game's own confirm) also takes pad 2's A only when +5 == 1, so entry
//!   1 is the 2-player entry (on screen: "Interleague Game").
//! - fn_72_4AC0 (state 6) is that entry's Chao pick; at 0x4C48 it keeps
//!   fn_60_52B78's answer in the menu work flags (+4 & 0x200), seen set
//!   exactly while GO BACK is lit.
//!
//! MMO queue (not the game's own): the 2-player entry is the PvP queue. The
//! game's confirm (state 7) cannot be reached without a second Memory Card's
//! garden, so A on the pick screen with the cursor off GO BACK is the ready
//! confirm.
use crate::chao_player::rel_original;
use sa2_recomp::rt::{Cpu, Mem, OS_MODULE_HEAD};
use sa2_recomp::{global, global::Global};

const KARATE: u32 = 72;
const ATTACK_PICK: u32 = 0xFCC8;
const MENU: u32 = 0x60E4;
/// main.dol lbl_801E55D8: the pads; *(+0) +0x10 is pad 1's presses, 4 is A.
const PADS: u32 = 0x801E_55D8;
const STRONG_ABOVE: f32 = 1600.0;
const HARD_KICK: u8 = 4;
const ROUNDHOUSE: u8 = 5;
const BEATING_UP: u8 = 6;
const PUNCH: u8 = 7;
const WINDUP_PUNCH: u8 = 8;

/// .text of the linked module the hook is on (0 = none).
static HOOKED: Global<u32> = global!(0);
/// Press counter of the last button used, so one press is one attack.
static USED: Global<u8> = global!(0);
/// Told to the page: strong moves unlocked (0/1), the press counter of the
/// last used button whose attack is over (the page takes no new move until
/// its own counter matches), and whether a bout is on (0/1).
static TOLD: Global<(u8, u8, u8, u8)> = global!((0, 0, 0, 0));
/// Told to the page with them: 0 not in the 2-player entry, 1 in it (the page
/// joins the queue), 2 confirmed (the page marks ready).
static QUEUE: Global<u8> = global!(0);

/// The queue mark for the menu task's state, the mode byte (+5) and whether A
/// was pressed on the pick.
fn queue(state: u8, mode: u8, confirmed: bool) -> u8 {
    match state {
        _ if mode != 1 => 0,
        6 if confirmed => 2,
        6 | 7 => 1,
        8..=14 => 2,
        _ => 0,
    }
}

fn menu(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    let work = m.r32(task + 0x34);
    // The pick is open (state 6, step 3) with its cursor off GO BACK.
    let picking = m.r8(work) == 6 && m.r8(work + 1) == 3 && m.r16(work + 4) & 0x200 == 0;
    rel_original(c, m, HOOKED.get() + MENU, menu);
    let Some((_, bss)) = sections(m) else { return };
    let state = m.r32(bss + 8);
    if state != 0 {
        let a = picking && m.r32(m.r32(PADS) + 0x10) & 4 != 0;
        let now = queue(m.r8(work), m.r8(state + 5), a || QUEUE.get() == 2);
        if now != QUEUE.get() {
            eprintln!("karate-queue: mark {now}");
            QUEUE.set(now);
        }
    }
}

/// The attack the player's Chao does: the pressed button when the game allows
/// it (the two strong moves need attack above 1600), else the game's own pick.
fn choose(native: u8, button: u8, attack: f32) -> u8 {
    match button {
        HARD_KICK | BEATING_UP | PUNCH => button,
        ROUNDHOUSE | WINDUP_PUNCH if attack > STRONG_ABOVE => button,
        _ => native,
    }
}

fn is_attack(a: u8) -> bool {
    (HARD_KICK..=WINDUP_PUNCH).contains(&a)
}

fn pick(c: &mut Cpu, m: &mut Mem) {
    let work = m.r32(c.r[3] + 0x40);
    let action = m.r32(work + 0xC);
    let before = m.r8(action);
    let player = m.r16(work + 2) == 0;
    let attack = f32::from_bits(m.r32(work + 0x2C));
    // The player's fighter is approaching again: its last attack is over.
    if player && !is_attack(before) {
        tell(u8::from(attack > STRONG_ABOVE), USED.get(), TOLD.get().2);
    }
    rel_original(c, m, HOOKED.get() + ATTACK_PICK, pick);
    let native = m.r8(action);
    if !player || is_attack(before) || !is_attack(native) {
        return;
    }
    let Some([seq, button]) = pressed() else { return };
    if seq == USED.get() {
        return;
    }
    USED.set(seq);
    // fn_72_B620 has just run for the native pick; only the id differs.
    m.w8(action, choose(native, button, attack));
}

/// The page's file SA2_KARATE_IN: [press counter, button].
fn pressed() -> Option<[u8; 2]> {
    std::fs::read(std::env::var("SA2_KARATE_IN").ok()?).ok()?.first_chunk().copied()
}

/// SA2_KARATE_OUT: [strong moves unlocked, press counter that is done, bout on,
/// queue mark].
fn tell(strong: u8, done: u8, bout: u8) {
    let now = (strong, done, bout, QUEUE.get());
    if TOLD.get() != now {
        TOLD.set(now);
        if let Ok(p) = std::env::var("SA2_KARATE_OUT") {
            let _ = std::fs::write(p, [now.0, now.1, now.2, now.3]);
        }
    }
}

/// Called every top-level frame: keeps the hook on the Karate module while it
/// is linked (a REL's address is only known then).
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    if std::env::var_os("SA2_KARATE_IN").is_none() {
        return;
    }
    let (text, bss) = sections(m).unwrap_or((0, 0));
    let old = HOOKED.get();
    if text == old {
        let (strong, done, ..) = TOLD.get();
        tell(strong, done, u8::from(text != 0 && bout_on(m.r32(bss + 8), |a| m.r32(a))));
        return;
    }
    if old != 0 {
        c.rt.overrides.remove(&(old + ATTACK_PICK));
        c.rt.overrides.remove(&(old + MENU));
    }
    // Left the Karate area: out of the queue (the next frame's tell says so).
    QUEUE.set(0);
    if text != 0 {
        c.rt.set_override(text + ATTACK_PICK, pick);
        c.rt.set_override(text + MENU, menu);
        // A press left over from an earlier bout is not carried into this one.
        USED.set(pressed().map_or(0, |p| p[0]));
        tell(0, USED.get(), 0);
    }
    c.rt.modules_changed();
    HOOKED.set(text);
}

/// Both fighter tasks of the bout state at `state` (*lbl_72_bss_8) exist.
fn bout_on(state: u32, r32: impl Fn(u32) -> u32) -> bool {
    state != 0 && r32(state + 0x10) != 0 && r32(state + 0x14) != 0
}

/// The linked Karate module's .text and .bss (sections 1 and 6), or None.
fn sections(m: &Mem) -> Option<(u32, u32)> {
    std::iter::successors(Some(m.r32(OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .find(|&i| m.r32(i) == KARATE)
        .map(|i| m.r32(i + 0x10))
        .map(|t| (m.r32(t + 8) & !1, m.r32(t + 48) & !1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bout_is_on_only_with_both_fighters() {
        let mem = |f0: u32, f1: u32| move |a: u32| [f0, f1][usize::from(a == 0x114)];
        assert!(!bout_on(0, mem(1, 1)));
        assert!(!bout_on(0x100, mem(1, 0)));
        assert!(!bout_on(0x100, mem(0, 1)));
        assert!(bout_on(0x100, mem(1, 1)));
    }

    #[test]
    fn only_the_two_player_entry_queues() {
        assert!((0..=16).all(|s| queue(s, 0, true) == 0));
        assert!([0, 1, 2, 3, 4, 5, 15, 16].iter().all(|&s| queue(s, 1, true) == 0));
        assert_eq!([queue(6, 1, false), queue(7, 1, false)], [1, 1]);
        assert_eq!(queue(6, 1, true), 2);
        assert!((8..=14).all(|s| queue(s, 1, false) == 2));
    }

    #[test]
    fn karate_button_keeps_the_power_gate() {
        for b in [HARD_KICK, BEATING_UP, PUNCH] {
            assert_eq!(choose(PUNCH, b, 1200.0), b);
        }
        // At the threshold the game itself picks the plain move (ble).
        for b in [ROUNDHOUSE, WINDUP_PUNCH] {
            assert_eq!(choose(HARD_KICK, b, 1600.0), HARD_KICK);
            assert_eq!(choose(HARD_KICK, b, 1600.5), b);
        }
        // No button, or a value that is not an attack: the game's own pick.
        assert_eq!(choose(ROUNDHOUSE, 0, 2000.0), ROUNDHOUSE);
        assert_eq!(choose(PUNCH, 9, 2000.0), PUNCH);
    }
}
