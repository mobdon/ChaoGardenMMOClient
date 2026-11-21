//! Play walker stages as the Chao Walkers: SA2_CHAO_WALKER=1. Eggman's stages
//! (Iron Gate) load the Dark Chao Walker, Tails's the Chao Walker: one walker
//! each, through the game's own branch. Tails's and Eggman's walker loaders
//! (0x800B5AC0, 0x800B5D2C) call the Chao Walker loader (0x800B58D4, physics
//! row 13) or the Dark Chao Walker loader (0x800B56E8, row 14) when the 2P flag
//! (+0x1E) and that player's alt-character flag (+0x55 + player) of the game
//! state at 0x801CC168 are set, as in 2P battle. Player 1's walker load runs
//! with those two flags set; both are restored right after, so the stage keeps
//! its 1P rules. Player 2 loads as chosen.
//!
//! Physics (mob, 2026-11-21): the walker keeps the mech's own row (Tails 6,
//! Eggman 7), so speed and acceleration match the stage. In main.dol rows 13
//! and 14 differ from 6 and 7 at +0x10, +0x3C, +0x40 and +0x60 only.
//!
//! Sounds: the walker code plays the Chao Walkers' own sound (0x201B Chao
//! Walker, 0x201E Dark, via 0x8006B978 at 0x800B4A64/0x800B4A84), which is in
//! the 2P walker bank se_ch_wk_BATTLE.mlt; 1P loads se_ch_wk.mlt (table
//! 0x801CD520 vs 0x801CD580 in 0x800D960C). So the 1P walker bank load
//! (0x8006CFEC) gets the 2P bank's name, the game's own string.
//!
//! Voices (mob, 2026-11-21: no Eggman voice lines, Chao sounds instead): while
//! player 1 is a Chao Walker, the voice task (exec 0x800125D8) does not start
//! its queued voice (ADX start at 0x80012760); it plays that walker's own
//! sound instead. Music and other sound effects are unchanged.
//! shortcut: covers every voice line in the stage, not only Eggman's; the
//! speaker of each voice id is not mapped yet.
use crate::{guest_call, original};
use sa2_recomp::rt::{Cpu, Mem};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

const TAILS_WALKER: u32 = 0x800B_5AC0;
const EGGMAN_WALKER: u32 = 0x800B_5D2C;
const TWO_PLAYER: u32 = 0x801C_C168 + 0x1E;
const ALT_CHARACTER: u32 = 0x801C_C168 + 0x55;
/// Character setup: copies physics row (character id at obj2+3) of the table
/// at 0x80196578 (0x84 bytes a row) to obj2+0xC0; obj2 = [r4+0x40].
const PHYSICS_SETUP: u32 = 0x8004_F838;
const PHYSICS_ROWS: u32 = 0x8019_6578;
const ROW: u32 = 0x84;
/// ADX start from an AFS archive; the voice task calls it at 0x80012760.
const ADX_START_AFS: u32 = 0x8013_476C;
const FROM_VOICE_TASK: u32 = 0x8001_2768;
/// Player 1's character work (obj2), set up by 0x8004F838 (table 0x801E5C48 + 0x1AE0).
const PLAYER1_OBJ2: u32 = 0x801E_7728;
/// Sound bank load by name (r3); the walker bank names in main.dol.
const LOAD_BANK: u32 = 0x8006_CFEC;
const WALKER_BANK: u32 = 0x8017_5E78; // "se_ch_wk.mlt"
const WALKER_BANK_2P: u32 = 0x8017_5E9C; // "se_ch_wk_BATTLE.mlt"
/// Play a sound effect (r3 = id, r4-r6 = 0), as the walker code calls it.
const PLAY_SE: u32 = 0x8006_B978;

/// The mech row player 1's Chao Walker takes during its load (0 = none).
static MECH_ROW: AtomicU32 = AtomicU32::new(0);

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_WALKER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    c.rt.set_override(TAILS_WALKER, tails);
    c.rt.set_override(EGGMAN_WALKER, eggman);
    c.rt.set_override(PHYSICS_SETUP, physics);
    c.rt.set_override(ADX_START_AFS, adx_start);
    c.rt.set_override(LOAD_BANK, load_bank);
}

fn load_bank(c: &mut Cpu, m: &mut Mem) {
    if c.r[3] == WALKER_BANK {
        c.r[3] = WALKER_BANK_2P;
    }
    original(c, m, LOAD_BANK);
}

fn tails(c: &mut Cpu, m: &mut Mem) {
    load(c, m, TAILS_WALKER, 6);
}

fn eggman(c: &mut Cpu, m: &mut Mem) {
    load(c, m, EGGMAN_WALKER, 7);
}

fn load(c: &mut Cpu, m: &mut Mem, asked: u32, mech_row: u32) {
    if c.r[3] != 0 {
        return original(c, m, asked);
    }
    let saved = (m.r8(TWO_PLAYER), m.r8(ALT_CHARACTER));
    m.w8(TWO_PLAYER, 1);
    m.w8(ALT_CHARACTER, 1);
    MECH_ROW.store(mech_row, Relaxed);
    original(c, m, asked);
    MECH_ROW.store(0, Relaxed);
    m.w8(TWO_PLAYER, saved.0);
    m.w8(ALT_CHARACTER, saved.1);
}

fn physics(c: &mut Cpu, m: &mut Mem) {
    let obj2 = m.r32(c.r[4].wrapping_add(0x40));
    original(c, m, PHYSICS_SETUP);
    let row = MECH_ROW.load(Relaxed);
    if row != 0 && matches!(m.r8(obj2 + 3), 13 | 14) {
        for i in (0..ROW).step_by(4) {
            let v = m.r32(PHYSICS_ROWS + row * ROW + i);
            m.w32(obj2 + 0xC0 + i, v);
        }
    }
}

fn adx_start(c: &mut Cpu, m: &mut Mem) {
    let obj2 = m.r32(PLAYER1_OBJ2);
    if c.lr == FROM_VOICE_TASK && obj2 != 0 {
        let se = match m.r8(obj2 + 3) {
            13 => 0x201B,
            14 => 0x201E,
            _ => return original(c, m, ADX_START_AFS),
        };
        guest_call(c, m, PLAY_SE, &[se, 0, 0, 0]);
        return;
    }
    original(c, m, ADX_START_AFS);
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJ: u32 = 0x8000_4000;
    const OBJ2: u32 = 0x8000_5000;

    // Stand-ins: each loader records r4 = which ran, r5 = the flags it saw, then
    // runs the physics setup for a Dark Chao Walker (id 14, row 14 = 0x14141414).
    fn fake(c: &mut Cpu, m: &mut Mem, id: u32) {
        c.r[4] = id;
        c.r[5] = u32::from(m.r8(TWO_PLAYER)) << 8 | u32::from(m.r8(ALT_CHARACTER));
        let r = (c.r[4], c.r[5]);
        m.w8(OBJ2 + 3, 14);
        c.r[4] = OBJ;
        sa2_recomp::rt::call(c, m, PHYSICS_SETUP);
        (c.r[4], c.r[5]) = r;
    }
    fn setup(_c: &mut Cpu, m: &mut Mem) {
        for i in (0..ROW).step_by(4) {
            m.w32(OBJ2 + 0xC0 + i, 0x1414_1414);
        }
    }
    fn generated(a: u32) -> Option<sa2_recomp::rt::Func> {
        match a {
            TAILS_WALKER => Some(|c, m| fake(c, m, 6)),
            EGGMAN_WALKER => Some(|c, m| fake(c, m, 7)),
            PHYSICS_SETUP => Some(setup),
            _ => None,
        }
    }

    #[test]
    fn player_one_takes_the_chao_branch_with_the_mech_physics() {
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = generated;
        c.rt.set_override(PHYSICS_SETUP, physics);
        m.w32(OBJ + 0x40, OBJ2);
        for i in (0..ROW).step_by(4) {
            m.w32(PHYSICS_ROWS + 7 * ROW + i, 0x0707_0707);
        }
        eggman(&mut c, &mut m);
        assert_eq!(
            (c.r[4], c.r[5]),
            (7, 0x0101),
            "Eggman's own loader, 2P + alt set"
        );
        assert_eq!((m.r8(TWO_PLAYER), m.r8(ALT_CHARACTER)), (0, 0), "restored");
        assert_eq!(m.r32(OBJ2 + 0xC0), 0x0707_0707, "Eggman's row");
        assert_eq!(m.r32(OBJ2 + 0xC0 + ROW - 4), 0x0707_0707, "whole row");

        c.r[3] = 1;
        eggman(&mut c, &mut m);
        assert_eq!((c.r[4], c.r[5]), (7, 0), "player 2 untouched");
        assert_eq!(m.r32(OBJ2 + 0xC0), 0x1414_1414, "player 2 keeps row 14");
    }

    #[test]
    fn the_walker_bank_is_the_2p_one() {
        fn bank(c: &mut Cpu, m: &mut Mem) {
            m.w32(0x8000_6000, c.r[3]);
        }
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = |a| (a == LOAD_BANK).then_some(bank as sa2_recomp::rt::Func);
        c.r[3] = WALKER_BANK;
        load_bank(&mut c, &mut m);
        assert_eq!(m.r32(0x8000_6000), WALKER_BANK_2P);
        c.r[3] = 0x8017_5E68;
        load_bank(&mut c, &mut m);
        assert_eq!(m.r32(0x8000_6000), 0x8017_5E68, "other banks unchanged");
    }

    #[test]
    fn voice_task_plays_the_walker_sound_only_for_a_chao_walker() {
        fn started(c: &mut Cpu, _m: &mut Mem) {
            c.r[9] = 1;
        }
        fn se(c: &mut Cpu, m: &mut Mem) {
            m.w32(0x8000_6000, c.r[3]);
        }
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = |a| match a {
            ADX_START_AFS => Some(started as sa2_recomp::rt::Func),
            PLAY_SE => Some(se),
            _ => None,
        };
        m.w32(PLAYER1_OBJ2, OBJ2);
        let run = |c: &mut Cpu, m: &mut Mem, id: u8, lr: u32| {
            m.w8(OBJ2 + 3, id);
            (c.r[9], c.lr) = (0, lr);
            adx_start(c, m);
            c.r[9]
        };
        assert_eq!(
            run(&mut c, &mut m, 14, FROM_VOICE_TASK),
            0,
            "Dark Chao Walker: no voice"
        );
        assert_eq!(m.r32(0x8000_6000), 0x201E, "its own sound");
        assert_eq!(
            run(&mut c, &mut m, 13, FROM_VOICE_TASK),
            0,
            "Chao Walker: no voice"
        );
        assert_eq!(m.r32(0x8000_6000), 0x201B, "its own sound");
        assert_eq!(
            run(&mut c, &mut m, 7, FROM_VOICE_TASK),
            1,
            "Eggman: voice plays"
        );
        assert_eq!(
            run(&mut c, &mut m, 14, 0x8000_1000),
            1,
            "other ADX starts play"
        );
    }
}
