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
use crate::chao_player::rel_original;
use sa2_recomp::rt::{Cpu, Mem, OS_MODULE_HEAD};
use sa2_recomp::{global, global::Global};

const KARATE: u32 = 72;
const ATTACK_PICK: u32 = 0xFCC8;
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
/// Told to the page: strong moves unlocked (0/1), and the press counter of the
/// last used button whose attack is over (the page takes no new move until
/// its own counter matches).
static TOLD: Global<(u8, u8)> = global!((0, 0));

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
        tell(u8::from(attack > STRONG_ABOVE), USED.get());
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

/// SA2_KARATE_OUT: [strong moves unlocked, press counter that is done].
fn tell(strong: u8, done: u8) {
    if TOLD.get() != (strong, done) {
        TOLD.set((strong, done));
        if let Ok(p) = std::env::var("SA2_KARATE_OUT") {
            let _ = std::fs::write(p, [strong, done]);
        }
    }
}

/// Called every top-level frame: keeps the hook on the Karate module while it
/// is linked (a REL's address is only known then).
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    if std::env::var_os("SA2_KARATE_IN").is_none() {
        return;
    }
    let text = text(m).unwrap_or(0);
    let old = HOOKED.get();
    if text == old {
        return;
    }
    if old != 0 {
        c.rt.overrides.remove(&(old + ATTACK_PICK));
    }
    if text != 0 {
        c.rt.set_override(text + ATTACK_PICK, pick);
        // A press left over from an earlier bout is not carried into this one.
        USED.set(pressed().map_or(0, |p| p[0]));
        tell(0, USED.get());
    }
    c.rt.modules_changed();
    HOOKED.set(text);
}

/// The linked Karate module's .text (section 1), or None.
fn text(m: &Mem) -> Option<u32> {
    std::iter::successors(Some(m.r32(OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .find(|&i| m.r32(i) == KARATE)
        .map(|i| m.r32(m.r32(i + 0x10) + 8) & !1)
}

#[cfg(test)]
mod tests {
    use super::*;

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
