//! Play walker stages as the Chao Walkers: SA2_CHAO_WALKER=1. The walker
//! follows the player's own Chao in every walker stage: a Dark Chao (record
//! type TYPE_D_*, al_chao_info.h) loads the Dark Chao Walker, any other the
//! Chao Walker. The type is read from the Chao player's record once that Chao
//! is live, else from the server's copy of the record (SA2_OWN_RECORD_IN, the
//! file chao_player reads). With no record, Eggman's stages (Iron Gate) load
//! the Dark Chao Walker and Tails's the Chao Walker. One walker each, through
//! the game's own branch. Tails's and Eggman's walker loaders
//! (0x800B5AC0, 0x800B5D2C) call the Chao Walker loader (0x800B58D4, physics
//! row 13) or the Dark Chao Walker loader (0x800B56E8, row 14) when the 2P flag
//! (+0x1E) and that player's alt-character flag (+0x55 + player) of the game
//! state at 0x801CC168 are set, as in 2P battle. Player 1's walker load runs
//! with those two flags set; both are restored right after, so the stage keeps
//! its 1P rules. Player 2 loads as chosen.
//!
//! Physics: the walker keeps the mech's own row (Tails 6,
//! Eggman 7), so speed and acceleration match the stage. In main.dol rows 13
//! and 14 differ from 6 and 7 at +0x10, +0x3C, +0x40 and +0x60 only.
//!
//! Sounds: the walker code plays the Chao Walkers' own sound (0x201B Chao
//! Walker, 0x201E Dark, via 0x8006B978 at 0x800B4A64/0x800B4A84), which is in
//! the 2P walker bank se_ch_wk_BATTLE.mlt, and its voice sounds (bank 3,
//! sequences 60 and 73 seen on Iron Gate) are in the 2P walker voice bank
//! b03_wk_btl_e/j.mpb; 1P loads se_ch_wk.mlt and b03_ewe/twe.mpb. The game's
//! bank chooser 0x800D960C picks both by the 2P flag (sound tables 0x801CD520
//! / 0x801CD580, voice tables 0x801CD5E0 / 0x801CD680), so it runs with that
//! flag set for the two walker characters.
//!
//! Voices (no Eggman voice lines, Chao sounds instead): while
//! player 1 is a Chao Walker, the voice task (exec 0x800125D8) does not start
//! its queued voice (ADX start at 0x80012760); it plays that walker's own
//! sound instead. Music and other sound effects are unchanged.
//! shortcut: covers every voice line in the stage, not only Eggman's; the
//! speaker of each voice id is not mapped yet.
//!
//! Pilot (prepared, off until the Chao player can draw the player's Chao at a
//! matrix): each walker's model list (obj2 +0x44C, pairs of index and
//! NJS_OBJECT, filled by 0x80050210 in the loaders) has the stock pilot's root
//! as its second entry (index 443 in CWALKMDL, 466 in DWALKMDL; node 14 of
//! the 33-node tree). The small Chao under the dome is the meshes below it
//! (nodes 15, 16 and 31: al_child / al_dn skin, eye and mouth textures); the
//! Chao-shaped body around it is the mech. Setting Ninja's hide bit on those
//! nodes removes only the pilot (checked in headless runs of stages 9 and 11).
//! The walker's own per-node callback 0x800AFA60 (r3 = node, set by its
//! display 0x800AF558 through 0x8011C3A0) is where the game reads node
//! matrices; the seat's is taken there.
//! unverified: that the callback is called for the seat node (it has no mesh).
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
/// Character sound and voice bank chooser (r3 = character, r4/r5 = table outs).
const BANK_CHOOSER: u32 = 0x800D_960C;
/// Play a sound effect (r3 = id, r4-r6 = 0), as the walker code calls it.
const PLAY_SE: u32 = 0x8006_B978;

/// A walker's model list in its obj2: (index, NJS_OBJECT) pairs.
const MODEL_LIST: u32 = 0x44C;
/// The stock pilot's root in CWALKMDL / DWALKMDL, the list's second entry.
const PILOT_INDEX: [u32; 2] = [443, 466];
/// The walker display's per-node callback (r3 = node).
const NODE_CALLBACK: u32 = 0x800A_FA60;
const WALKER_DISPLAY: u32 = 0x800A_F558;
const NJ_CURRENT_MATRIX: u32 = 0x803A_E0C4; // _nj_current_matrix_ptr_
/// NJS_OBJECT: evalflags +0, model +4, child +0x2C, sibling +0x30; NJD_EVAL_HIDE.
const HIDE: u32 = 8;

/// The mech row player 1's Chao Walker takes during its load (0 = none).
static MECH_ROW: AtomicU32 = AtomicU32::new(0);
/// Ninja's current matrix (3x4, camera included) at the seat node in this display.
static SEAT: std::sync::Mutex<Option<[[f32; 4]; 3]>> = std::sync::Mutex::new(None);

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_WALKER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    c.rt.set_override(TAILS_WALKER, tails);
    c.rt.set_override(EGGMAN_WALKER, eggman);
    c.rt.set_override(PHYSICS_SETUP, physics);
    c.rt.set_override(ADX_START_AFS, adx_start);
    c.rt.set_override(BANK_CHOOSER, banks);
    c.rt.set_override(NODE_CALLBACK, node_callback);
    c.rt.set_override(WALKER_DISPLAY, display);
}

/// The stock pilot's root node of player 1's Chao Walker, the seat.
fn seat_node(m: &Mem) -> Option<u32> {
    let obj2 = m.r32(PLAYER1_OBJ2);
    if obj2 == 0 || !matches!(m.r8(obj2 + 3), 13 | 14) {
        return None;
    }
    let list = m.r32(obj2 + MODEL_LIST);
    (list != 0 && PILOT_INDEX.contains(&m.r32(list + 8))).then(|| m.r32(list + 12))
}

/// Ninja's hide bit on every mesh node below the seat: the stock pilot.
fn hide_pilot(m: &mut Mem, seat: u32) {
    let mut todo = vec![m.r32(seat + 0x2C)];
    while let Some(node) = todo.pop() {
        if node == 0 {
            continue;
        }
        if m.r32(node + 4) != 0 {
            let flags = m.r32(node);
            m.w32(node, flags | HIDE);
        }
        todo.extend([m.r32(node + 0x2C), m.r32(node + 0x30)]);
    }
}

fn node_callback(c: &mut Cpu, m: &mut Mem) {
    let node = c.r[3];
    original(c, m, NODE_CALLBACK);
    if seat_node(m) == Some(node) {
        let at = m.r32(NJ_CURRENT_MATRIX);
        let word = |r: usize, k: usize| f32::from_bits(m.r32(at + 4 * (4 * r + k) as u32));
        *SEAT.lock().unwrap() = Some(std::array::from_fn(|r| std::array::from_fn(|k| word(r, k))));
    }
}

/// Draw the player's own Chao at the seat (Ninja's current matrix there).
/// shortcut: draws nothing until chao_player has its draw-at-a-matrix function (being added by that file's owner); call it here and return what it returns.
/// shortcut: the seat matrix is passed at scale 1.0; the Chao takes its size from it, so scale it here to the stock pilot's size once that ratio is read from the two models.
fn own_pilot(_c: &mut Cpu, _m: &mut Mem, _seat: &[[f32; 4]; 3]) -> bool {
    false
}

fn display(c: &mut Cpu, m: &mut Mem) {
    *SEAT.lock().unwrap() = None;
    original(c, m, WALKER_DISPLAY);
    let seat = SEAT.lock().unwrap().take();
    if let (Some(mtx), Some(node)) = (seat, seat_node(m)) {
        // The stock pilot leaves only once the player's Chao is drawn in its place.
        if own_pilot(c, m, &mtx) {
            hide_pilot(m, node);
        }
    }
}

fn banks(c: &mut Cpu, m: &mut Mem) {
    if !matches!(c.r[3], 6 | 7) {
        return original(c, m, BANK_CHOOSER);
    }
    let saved = m.r8(TWO_PLAYER);
    m.w8(TWO_PLAYER, 1);
    original(c, m, BANK_CHOOSER);
    m.w8(TWO_PLAYER, saved);
}

fn tails(c: &mut Cpu, m: &mut Mem) {
    load(c, m, TAILS_WALKER, 6);
}

fn eggman(c: &mut Cpu, m: &mut Mem) {
    load(c, m, EGGMAN_WALKER, 7);
}

/// A Dark Chao by its record's type byte (CHAO_PARAM_GC +0x80): TYPE_D_NORMAL
/// 7, then every third type up to TYPE_D_CHAOS 0x16 (al_chao_info.h).
fn dark_type(t: u8) -> bool {
    (5..=0x16).contains(&t) && (t - 5) % 3 == 2
}

/// The player's own Chao's type, if a hatched Chao's record is at hand.
fn own_type(m: &Mem) -> Option<u8> {
    let file = || {
        let b = std::fs::read(std::env::var_os("SA2_OWN_RECORD_IN")?).ok()?;
        (b.len() == 0x800).then_some(b)
    };
    let t = crate::chao_player::player_record(m).or_else(file)?[0x80];
    (t >= 2).then_some(t)
}

/// The stage's loader, or the one for the player's own Chao when it is known.
fn pick(asked: u32, own: Option<u8>) -> u32 {
    match own {
        Some(t) if dark_type(t) => EGGMAN_WALKER,
        Some(_) => TAILS_WALKER,
        None => asked,
    }
}

fn load(c: &mut Cpu, m: &mut Mem, asked: u32, mech_row: u32) {
    if c.r[3] != 0 {
        return original(c, m, asked);
    }
    let own = own_type(m);
    let asked = pick(asked, own);
    eprintln!("chao-walker: own Chao type {own:x?}, loader {asked:#x}, mech row {mech_row}");
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
    fn the_seat_is_found_and_only_the_pilot_below_it_is_hidden() {
        // Stand-in tree: seat (no mesh) -> body (mesh) -> head (mesh), and a
        // mech node (mesh) as the seat's sibling.
        const LIST: u32 = 0x8000_7000;
        const SEAT_NODE: u32 = 0x8000_7100;
        const BODY: u32 = 0x8000_7200;
        const HEAD: u32 = 0x8000_7300;
        const MECH: u32 = 0x8000_7400;
        fn nothing(_c: &mut Cpu, _m: &mut Mem) {}
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = |a| (a == NODE_CALLBACK).then_some(nothing as sa2_recomp::rt::Func);
        m.w32(PLAYER1_OBJ2, OBJ2);
        m.w8(OBJ2 + 3, 14);
        m.w32(OBJ2 + MODEL_LIST, LIST);
        for (i, v) in [465, 0x8000_7500, 466, SEAT_NODE, u32::MAX, 0]
            .into_iter()
            .enumerate()
        {
            m.w32(LIST + 4 * i as u32, v);
        }
        for (node, flags, model, child, sibling) in [
            (SEAT_NODE, 0xA, 0, BODY, MECH),
            (BODY, 6, 0x8000_7600, HEAD, 0),
            (HEAD, 0x17, 0x8000_7700, 0, 0),
            (MECH, 0x12, 0x8000_7800, 0, 0),
        ] {
            m.w32(node, flags);
            m.w32(node + 4, model);
            m.w32(node + 0x2C, child);
            m.w32(node + 0x30, sibling);
        }
        assert_eq!(seat_node(&m), Some(SEAT_NODE));
        hide_pilot(&mut m, SEAT_NODE);
        assert_eq!((m.r32(BODY), m.r32(HEAD)), (0xE, 0x1F), "pilot hidden");
        assert_eq!(
            (m.r32(SEAT_NODE), m.r32(MECH)),
            (0xA, 0x12),
            "seat and mech untouched"
        );

        // The callback keeps the current matrix at the seat node only.
        m.w32(NJ_CURRENT_MATRIX, 0x8000_7900);
        m.w32(0x8000_7900 + 12, 0x4120_0000);
        c.r[3] = MECH;
        node_callback(&mut c, &mut m);
        assert!(SEAT.lock().unwrap().is_none());
        c.r[3] = SEAT_NODE;
        node_callback(&mut c, &mut m);
        assert_eq!(SEAT.lock().unwrap().take().map(|s| s[0][3]), Some(10.0));
        m.w8(OBJ2 + 3, 7);
        assert_eq!(seat_node(&m), None, "Eggman's own walker has no seat");
    }

    #[test]
    fn the_walker_follows_the_own_chaos_type() {
        // al_chao_info.h: child 2, N/H/D normal 5/6/7 .. N/H/D chaos 0x14/0x15/0x16, Tails 0x17.
        for t in [7, 0xA, 0xD, 0x10, 0x13, 0x16] {
            assert_eq!(
                pick(TAILS_WALKER, Some(t)),
                EGGMAN_WALKER,
                "dark type {t:#x}"
            );
        }
        for t in [2, 5, 6, 8, 9, 0x14, 0x15, 0x17, 0x19] {
            assert_eq!(pick(EGGMAN_WALKER, Some(t)), TAILS_WALKER, "type {t:#x}");
        }
        assert_eq!(
            pick(EGGMAN_WALKER, None),
            EGGMAN_WALKER,
            "no record: the stage's"
        );
        assert_eq!(pick(TAILS_WALKER, None), TAILS_WALKER);
    }

    #[test]
    fn the_walkers_banks_are_chosen_as_in_2p() {
        // Stand-in chooser: records the 2P flag it saw.
        fn chooser(_c: &mut Cpu, m: &mut Mem) {
            let seen = m.r8(TWO_PLAYER);
            m.w8(0x8000_6000, seen);
        }
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = |a| (a == BANK_CHOOSER).then_some(chooser as sa2_recomp::rt::Func);
        for (character, flag) in [(7, 1), (6, 1), (0, 0), (4, 0)] {
            c.r[3] = character;
            banks(&mut c, &mut m);
            assert_eq!(m.r8(0x8000_6000), flag, "character {character}");
            assert_eq!(m.r8(TWO_PLAYER), 0, "restored");
        }
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
