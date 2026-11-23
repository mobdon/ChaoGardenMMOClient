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
//! - The pick's right side is the other Memory Card's garden. ChaoMain (module
//!   60): GardenInfoList (.bss 0x3E80) is two gardens of 0xCA6C bytes;
//!   AL_GetAnotherGardenInfo (.text 0xDF60) is the one of the other slot. Its
//!   24 Chao records are at +0xA64 + i * 0x800; AL_SortChaoSaveInfo (0x2E9C,
//!   run once when the pick task starts, 0x52D98) lists a record whose type
//!   byte +0x80 is 1..0x19 under its garden number +0x81. The pick shows a red
//!   X while gIsAnotherGardenLoaded (.bss 0x1D358) is 0 (0x55218). Karate's
//!   main task fn_72_78 only fills that garden from a card while lbl_72_bss_0
//!   is not 0.
//! - State 6 stores the two picked records at *lbl_72_bss_8 +0x18/+0x1C
//!   (0x4C88, 0x4C94); fn_72_5020 (state 9 step 0, 0x5174) makes fighter 0
//!   from +0x18 and fighter 1 from +0x1C with fn_72_11038(record, .., 1).
//! - State 7 (fn_72_4D7C) goes on to state 8 on either pad's A, or when its
//!   message window moves on (fn_72_8B9C below 0, 0x4E98). State 8
//!   (fn_72_4EE8) step 2 counts work +6 down and enters state 9 at 0.
//! - fn_72_10E0C(task) (.text 0x10E0C) is a fighter's frame: its state is the
//!   byte at *(task +0x34) (jump table .data 0x17348 for 0..=0xB; 2 ready,
//!   3 approach, 4 attack, 5 dodge, 6 hurt, 7 stagger), then for every state
//!   it copies position (+0x14) and angle (+8) to the Chao, runs the action's
//!   motion (fn_72_E868, action byte at *(work +0xC)), movement and the
//!   ring-out test (fn_72_F5B0). Health is work +0x3C; flag 0x1000 at
//!   *(task +0x34) +4 is knocked out, as the server's port names it.
//!
//! MMO queue (not the game's own): the 2-player entry is the PvP queue.
//! - A on "Interleague Game" is held back while the page queues; once the
//!   server has paired a player, that player's Chao record is written as the
//!   only Chao of the other garden and the A goes through, so the game's own
//!   pick opens with the opponent on the right.
//! - Ready is sent once the game's own confirm has gone on to state 8, which
//!   is held (work +6 kept at 2) until the server starts the bout.
//! - The server's bout is the one shown: while it runs, a fighter's own state
//!   handler is skipped (state byte parked at 0xC for the call, above the jump
//!   table) and position, angle, health, state and action are the server's;
//!   the game still plays the motions, moves the Chao and judges the end.
//! Unverified: that nothing outside ChaoStgKarate saves the other garden
//! (only AL_ConfirmSaveStart_WithoutAnother is called in the modules read).
use crate::chao_player::rel_original;
use sa2_recomp::rt::{Cpu, Mem, OS_MODULE_HEAD};
use sa2_recomp::{global, global::Global};

const KARATE: u32 = 72;
const ATTACK_PICK: u32 = 0xFCC8;
const MENU: u32 = 0x60E4;
const FIGHTER: u32 = 0x10E0C;
const CHAO_MAIN: u32 = 60;
const OTHER_GARDEN: u32 = 0xDF60;
const GARDEN_LOADED: u32 = 0x1D358;
const GARDEN_CHAO: u32 = 0xA64;
const KNOCKED_OUT: u16 = 0x1000;
/// chao_player.rs INFO: the 0x800 record the player's own Chao is made from.
const OWN_INFO: u32 = 0x82F0_1000;
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
/// Told to the page with them: 0 not queueing, 1 queueing (the page joins the
/// queue), 2 confirmed (the page marks ready).
static QUEUE: Global<u8> = global!(0);
/// A on the 2-player entry is held back until an opponent is in the garden.
static WAITING: Global<bool> = global!(false);
/// The pick has been given its two Chao (once per time it opens).
static PICKED: Global<bool> = global!(false);
/// The opponent whose record is in the other garden, and where (0 = none).
static GUEST: Global<(u64, u32)> = global!((0, 0));
/// The page's file SA2_PVP_IN, read once a frame.
static PVP: Global<Pvp> = global!(Pvp { flags: 0, slot: 0, winner: 2, count: 0, opponent: 0, fighters: [Shown::NONE; 2] });
/// Per fighter: the snapshot count last applied.
static APPLIED: Global<[u8; 2]> = global!([0; 2]);

const MATCHED: u8 = 1;
const STARTED: u8 = 2;
const RESULT: u8 = 4;
/// With RESULT: it ended by knock-out or forfeit, not on time.
const KO: u8 = 8;

/// One fighter of the server's bout (its queue-bout event).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Shown {
    pos: [f32; 3],
    hp: f32,
    yaw: u16,
    state: u8,
    step: u8,
    action: u8,
}

impl Shown {
    const NONE: Self = Self { pos: [0.0; 3], hp: 0.0, yaw: 0, state: 0, step: 0, action: 0 };
}

/// The page's side of a queued match: flags (MATCHED, STARTED, RESULT, KO),
/// the player's fighter slot, the winner's slot (2 = draw), a count that goes up
/// with every server snapshot, the opponent's id and the two fighters.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Pvp {
    flags: u8,
    slot: u8,
    winner: u8,
    count: u8,
    opponent: u64,
    fighters: [Shown; 2],
}

impl Pvp {
    /// 16 bytes, then 28 per fighter, little-endian:
    /// [flags, slot, winner, count, 4 unused, opponent u64] and
    /// [pos 3 x f32, hp f32, 4 unused, yaw u16, state, step, action, 3 unused].
    fn parse(b: &[u8]) -> Option<Self> {
        let f32_at = |at: usize| Some(f32::from_le_bytes(*b.get(at..)?.first_chunk()?));
        let shown = |at: usize| {
            let t = b.get(at + 20..at + 28)?;
            Some(Shown {
                pos: [f32_at(at)?, f32_at(at + 4)?, f32_at(at + 8)?],
                hp: f32_at(at + 12)?,
                yaw: u16::from_le_bytes([t[0], t[1]]),
                state: t[2],
                step: t[3],
                action: t[4],
            })
        };
        Some(Self {
            flags: *b.first()?,
            slot: b.get(1)? & 1,
            winner: *b.get(2)?,
            count: *b.get(3)?,
            opponent: u64::from_le_bytes(*b.get(8..)?.first_chunk()?),
            fighters: [shown(16)?, shown(44)?],
        })
    }
}

/// The queue mark for the menu task's state, the mode byte (+5) and whether
/// the 2-player entry's A is being held back on the mode select.
fn queue(state: u8, mode: u8, waiting: bool) -> u8 {
    match state {
        3 => u8::from(waiting),
        _ if mode != 1 => 0,
        6 | 7 => 1,
        8..=14 => 2,
        _ => 0,
    }
}

/// Writes player `id`'s Chao record (SA2_RECORD_IN/<id>, 0x800 bytes) as the
/// only Chao of the other Memory Card's garden and marks that garden loaded.
fn invite(c: &mut Cpu, m: &mut Mem, bss: u32, id: u64) -> bool {
    if GUEST.get().0 == id {
        return true;
    }
    let (text60, bss60) = (sa2_recomp::rt::sec(m, CHAO_MAIN, 1), sa2_recomp::rt::sec(m, CHAO_MAIN, 6));
    // fn_72_78 still owns the garden while the card read runs.
    if text60 == 0 || bss60 == 0 || m.r32(bss) != 0 {
        return false;
    }
    let Some(record) = std::env::var("SA2_RECORD_IN").ok().and_then(|d| std::fs::read(std::path::Path::new(&d).join(id.to_string())).ok()) else {
        return false;
    };
    if record.len() < 0x800 || !(1..0x1A).contains(&record[0x80]) {
        eprintln!("karate-queue: record of {id} unusable ({} bytes, type {:?})", record.len(), record.get(0x80));
        return false;
    }
    let at = crate::guest_call(c, m, text60 + OTHER_GARDEN, &[]) + GARDEN_CHAO;
    for (i, b) in record[..0x800].iter().enumerate() {
        m.w8(at + i as u32, *b);
    }
    // AL_SortChaoSaveInfo lists it under garden +0x81 (1..=3).
    if !(1..=3).contains(&record[0x81]) {
        m.w8(at + 0x81, 1);
    }
    m.w32(bss60 + GARDEN_LOADED, 1);
    GUEST.set((id, at));
    eprintln!("karate-queue: opponent {id} in the other garden at {at:#x}");
    true
}

/// The 2-player pick (ChaoMain fn_60_534BC, work *lbl_60_bss_50C88, kind 1):
/// each pad's A on a listed Chao (cursor column +0x28, row +0x2C, lists at
/// lbl_60_bss_3D08 by card side and garden) stores that record at +0x48 for
/// pad 1 and +0x50 for pad 2 (0x537FC); once both are set it hands them to
/// AL_SetSelectedChaoSaveInfo and ends with result 4 (0x53928). The cursor
/// starts on the first place of the left list.
/// MMO glue: pad 2's choice is the opponent's Chao, already made; the MMO
/// card's garden has no grown Chao to list, so the first place of the left
/// list is the player's current Chao (the record chao_player.rs keeps at
/// OWN_INFO). The player's own A on it then ends the pick as the game does.
fn choose_both(m: &mut Mem) -> bool {
    let bss60 = sa2_recomp::rt::sec(m, CHAO_MAIN, 6);
    let pick = if bss60 != 0 { m.r32(bss60 + 0x50C88) } else { 0 };
    if pick == 0 {
        return false;
    }
    m.w32(bss60 + 0x3D08, OWN_INFO);
    m.w32(pick + 0x50, GUEST.get().1);
    m.w32(pick + 0x54, 1);
    true
}

/// Takes the opponent's record out of the other garden again.
fn uninvite(m: &mut Mem) {
    let (id, at) = GUEST.replace((0, 0));
    let bss60 = sa2_recomp::rt::sec(m, CHAO_MAIN, 6);
    if id == 0 || bss60 == 0 {
        return;
    }
    for i in 0..0x800 {
        m.w8(at + i, 0);
    }
    m.w32(bss60 + GARDEN_LOADED, 0);
    eprintln!("karate-queue: opponent {id} out of the other garden");
}

fn menu(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    let work = m.r32(task + 0x34);
    let Some((_, bss)) = sections(m) else { return rel_original(c, m, HOOKED.get() + MENU, menu) };
    let state = m.r32(bss + 8);
    if state != 0 {
        let pvp = PVP.get();
        let presses = m.r32(PADS) + 0x10;
        let a = m.r32(presses) & 4 != 0;
        match (m.r8(work), m.r8(work + 1)) {
            // Mode select with the cursor on the 2-player entry.
            (3, _) if m.r8(state + 0xA) == 1 && (a || WAITING.get()) => {
                WAITING.set(true);
                let here = pvp.flags & MATCHED != 0 && invite(c, m, bss, pvp.opponent);
                let held = m.r32(presses) & !4;
                m.w32(presses, held | if here { 4 } else { 0 });
            }
            (3, _) => WAITING.set(false),
            // The confirm is done: wait here until the server starts the bout.
            (8, 2) if m.r8(state + 5) == 1 && pvp.flags & STARTED == 0 => m.w16(work + 6, 2),
            // The pick opens with the opponent's Chao chosen and the player's listed.
            (6, 3) if m.r8(state + 5) == 1 && GUEST.get().0 != 0 && !PICKED.get() => PICKED.set(choose_both(m)),
            // The server's fighter 0 is the game's left one on both screens:
            // the player's current Chao and the opponent's, by the player's slot.
            (9, 0) if m.r8(state + 5) == 1 && GUEST.get().0 != 0 => {
                let sides = [OWN_INFO, GUEST.get().1];
                m.w32(state + 0x18, sides[usize::from(pvp.slot)]);
                m.w32(state + 0x1C, sides[usize::from(pvp.slot ^ 1)]);
            }
            _ => {}
        }
    }
    rel_original(c, m, HOOKED.get() + MENU, menu);
    if state != 0 {
        if m.r8(work) != 3 {
            WAITING.set(false);
        }
        if m.r8(work) != 6 {
            PICKED.set(false);
        }
        let now = queue(m.r8(work), m.r8(state + 5), WAITING.get());
        if now != QUEUE.get() {
            eprintln!("karate-queue: mark {now}");
            QUEUE.set(now);
            if now == 0 {
                uninvite(m);
            }
        }
    }
}

/// A fighter's frame while the server runs the bout: the server's fighter is
/// put in place and the game's own state handler is skipped for the call.
fn fighter(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    let (twp, work) = (m.r32(task + 0x34), m.r32(task + 0x40));
    let pvp = PVP.get();
    if pvp.flags & STARTED == 0 || pvp.count == 0 {
        return rel_original(c, m, HOOKED.get() + FIGHTER, fighter);
    }
    let i = usize::from(m.r16(work + 2) & 1);
    let f = pvp.fighters[i];
    // The fighter stands where the server has it. Seen headless 2026-11-23:
    // left to MOV_DetectCollision the fighters find no floor in this build
    // and fall from their first frame, which fn_72_F5B0 calls a ring-out.
    let place = |m: &mut Mem| {
        for (k, p) in f.pos.iter().enumerate() {
            m.w32(twp + 0x14 + 4 * k as u32, p.to_bits());
        }
        let moving = m.r32(task + 0x38);
        if moving != 0 {
            m.w32(moving + 4, 0);
        }
    };
    // States 0/1 are the game's entrance and 8.. its ending: its own, on the
    // server's spot.
    if !(2..=7).contains(&m.r8(twp)) {
        place(m);
        rel_original(c, m, HOOKED.get() + FIGHTER, fighter);
        return place(m);
    }
    if APPLIED.get()[i] != pvp.count {
        APPLIED.with_borrow_mut(|a| a[i] = pvp.count);
        m.w32(twp + 0xC, u32::from(f.yaw));
        let action = m.r32(work + 0xC);
        // A new action: what fn_72_B620 does.
        if action != 0 && m.r8(action) != f.action {
            if std::env::var_os("SA2_PVP_LOG").is_some() {
                eprintln!("karate-pvp: {} f{i} action {} -> {} state {}.{} hp {}", pvp.count, m.r8(action), f.action, f.state, f.step, f.hp);
            }
            m.w16(twp + 4, m.r16(twp + 4) & !0x8000);
            m.w8(action + 5, m.r8(action + 5) & !0x18);
            m.w8(action, f.action);
            m.w8(action + 1, 0);
            m.w8(action + 2, 0);
        }
    }
    // The server's bout is over while the game's round is still on
    // (*lbl_72_bss_8 +4 == 2, phase +3 == 0): the game's own ending. A loser
    // is knocked out as fn_72_F5B0 0xF83C does it (phase 2, flag 0x1000);
    // otherwise time is up (phase 3) and fn_72_F4CC judges the server's health.
    let state = sections(m).map_or(0, |(_, bss)| m.r32(bss + 8));
    if pvp.flags & RESULT != 0 && state != 0 && m.r8(state + 4) == 2 && m.r8(state + 3) == 0 {
        if pvp.flags & KO != 0 && usize::from(pvp.winner) == (i ^ 1) {
            m.w16(twp + 4, m.r16(twp + 4) | KNOCKED_OUT);
            m.w8(state + 3, 2);
        } else if pvp.flags & KO == 0 {
            m.w8(state + 3, 3);
        }
    }
    m.w32(work + 0x3C, f.hp.to_bits());
    place(m);
    m.w8(twp, 0xC);
    rel_original(c, m, HOOKED.get() + FIGHTER, fighter);
    if m.r8(twp) == 0xC {
        m.w8(twp, f.state.clamp(2, 7));
        m.w8(twp + 1, f.step);
    }
    place(m);
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
    if text != 0 {
        let page = std::env::var("SA2_PVP_IN").ok().and_then(|p| std::fs::read(p).ok());
        if let Some(pvp) = page.as_deref().and_then(Pvp::parse) {
            PVP.set(pvp);
        }
    }
    if text == old {
        let (strong, done, ..) = TOLD.get();
        tell(strong, done, u8::from(text != 0 && bout_on(m.r32(bss + 8), |a| m.r32(a))));
        return;
    }
    if old != 0 {
        c.rt.overrides.remove(&(old + ATTACK_PICK));
        c.rt.overrides.remove(&(old + MENU));
        c.rt.overrides.remove(&(old + FIGHTER));
    }
    // Left the Karate area: out of the queue (the next frame's tell says so).
    QUEUE.set(0);
    WAITING.set(false);
    uninvite(m);
    if text != 0 {
        c.rt.set_override(text + ATTACK_PICK, pick);
        c.rt.set_override(text + MENU, menu);
        c.rt.set_override(text + FIGHTER, fighter);
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
        // 1-player never queues; the mode select only while the A is held back.
        assert!((4..=16).all(|s| queue(s, 0, false) == 0));
        assert_eq!([queue(3, 0, true), queue(3, 1, false)], [1, 0]);
        assert!([0, 1, 2, 4, 5, 15, 16].iter().all(|&s| queue(s, 1, false) == 0));
        assert_eq!([queue(6, 1, false), queue(7, 1, false)], [1, 1]);
        assert!((8..=14).all(|s| queue(s, 1, false) == 2));
    }

    #[test]
    fn the_pages_match_file_is_read_as_written() {
        let mut b = vec![0u8; 72];
        b[..4].copy_from_slice(&[MATCHED | STARTED, 1, 2, 9]);
        b[8..16].copy_from_slice(&0x1234_5678_9ABCu64.to_le_bytes());
        for (k, v) in [5.0f32, 8.0, 19.0, 1234.5].iter().enumerate() {
            b[44 + 4 * k..48 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        b[64..69].copy_from_slice(&[0x00, 0xC0, 4, 2, 7]);
        let p = Pvp::parse(&b).unwrap();
        assert_eq!((p.flags, p.slot, p.winner, p.count, p.opponent), (3, 1, 2, 9, 0x1234_5678_9ABC));
        assert_eq!(p.fighters[0], Shown::NONE);
        let right = Shown { pos: [5.0, 8.0, 19.0], hp: 1234.5, yaw: 0xC000, state: 4, step: 2, action: 7 };
        assert_eq!(p.fighters[1], right);
        assert_eq!(Pvp::parse(&b[..71]), None);
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
