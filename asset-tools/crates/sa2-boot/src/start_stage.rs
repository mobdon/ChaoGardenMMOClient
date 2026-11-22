//! Chao MMO glue (approved by mob 2026-11-21): SA2_START_STAGE=<stage> starts
//! that stage straight from boot, with no menus and no scripted input. It does
//! what the game's own Stage Select does when a stage is confirmed
//! (advertiseD.rel section 1, 0x101E0..0x1034C): `SetStageNum` 0x8001DD8C,
//! `SetPlayerChar(0, c)` 0x8001DDC0 with the stage's character from the menu's
//! own table, mission/2P/flags cleared in the game state at 0x801CC168, and
//! next global mode 4 (MD_ACTION_INIT). The menu's caller then copies the next
//! mode into the global mode (0x800D3554); here that happens in place of the
//! title (global mode 1, MD_TITLE_INIT, case 1 of 0x800D2DD4).
use crate::{guest_call, original};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

const TOP_LEVEL: u32 = 0x800D_2DD4;
const SET_STAGE_NUM: u32 = 0x8001_DD8C;
const SET_PLAYER_CHAR: u32 = 0x8001_DDC0;
const GAME: u32 = 0x801C_C168;
const GLOBAL_MODE: u32 = GAME + 0x80;
const NEXT_GLOBAL_MODE: u32 = GAME + 0x84;
const MD_TITLE_INIT: u32 = 1;
const MD_ACTION_INIT: u32 = 4;
/// The first global mode after the memory card is read (see top_level).
const MD_AFTER_CARD: u32 = 12;
/// The boot logos (Nintendo, SEGA, Sonic Team), timed against frames with SA2_MODE_LOG.
const MD_LOGOS: u32 = 22;
/// /dev/area values outside the area numbers: cover / uncover the game.
const PAGE_COVER: u32 = 254;
const PAGE_UNCOVER: u32 = 253;
/// The page is covering the logos.
static LOGOS_HIDDEN: Global<bool> = global!(false);

/// The boot logos are running behind the page's cover before an MMO start:
/// the host may run those frames as fast as it can (their own timer still
/// counts every frame, so the game's own path out of them is unchanged).
pub fn logos_hidden() -> bool {
    LOGOS_HIDDEN.get()
}
/// The Stage Select's start code reads its stage/character table through the
/// relocation at advertiseD section 1 + 0x10246 (`lis r3` at 0x10244).
const TABLE_RELOC: (u8, u32) = (1, 0x10246);
/// Chao World. Not a row of the Stage Select table: its own branch
/// (advertiseD section 1, 0x103E8..0x104F0) does `SetStageNum(0x5A)`,
/// `SetPlayerChar(0, c)`, clears +0x20/+0x1E/+0, next global mode 4.
const CHAO_WORLD: u16 = 0x5A;
/// ChaoMain (module 0x3C) .text: `AL_GetStageNumber` 0x27FA0 (current Chao
/// area) and the area change 0x27E00 (FINDINGS, chao-competition-recomp).
const CHAOMAIN: u32 = 0x3C;
const AL_GET_STAGE_NUMBER: u32 = 0x2_7FA0;
const AL_CHANGE_AREA: u32 = 0x2_7E00;
/// ChaoMain area numbers 1..=12 (1 Neutral, 2 Hero, 3 Dark, 5 Entrance/race,
/// 9 Stadium, 10 Karate); 0 is the lobby Chao World starts in.
const LAST_AREA: u32 = 12;
/// The Chao Lobby's stage number (al_stage.h: 7 lobby; 0 is none, while loading).
const LOBBY: u32 = 7;
/// ChaoMain .bss (section 5) +0x972E8: the previous stage, returned by
/// `AL_GetPreStageNumber` 0x27F90 (symbols.txt). Unverified that the lobby
/// reads it only at its start; the headless arrival check is the test.
const PRE_STAGE: u32 = 0x9_72E8;
/// A new player's egg, laid the way ChaoMain's `AL_ItemCreateManagerExecutor`
/// (0x18F7C, its call at 0x19208) makes a bought egg: a save slot from
/// `fn_60_2C30` 0x2C30, `AL_GeneCreate` 0x1218C into a 0xC0-byte gene,
/// then `CreateEgg(gene, slot, 0, &pos, 3)` 0x8F20. ChaoMain symbols.txt (GSNE8P).
const SAVE_SLOT: u32 = 0x2C30;
const AL_GENE_CREATE: u32 = 0x1_218C;
const CREATE_EGG: u32 = 0x8F20;
const GENE_SIZE: u32 = 0xC0;
/// AL_GENE (al_chao_info.h, 0xA4 bytes; CHAO_PARAM_GC +0x438..+0x4DC).
const GENE_BYTES: usize = 0xA4;
/// The AL_GENE byte ranges the server's egg carries (EggColor; Abl for its 7
/// stats; LifeTime; Personality through Multi). The rest keeps AL_GeneCreate's.
/// The layout is the SAMT (PC) header's; unverified on GC until a live hatch.
const SERVER_GENE: [(u32, u32); 4] = [(0x01, 0x02), (0x5C, 0x6A), (0x6C, 0x6E), (0x74, 0x9C)];
/// MMO hub residents: the game's own character Chao (al_chao_info.h TYPE_TAILS
/// 0x17, TYPE_KNUCKLES 0x18, TYPE_AMY 0x19), made with `CreateChaoExtra`
/// (see CREATE_CHAO_EXTRA). Offsets from the player in the lobby are
/// the old web hub's NPC spots (src/scene.js addNPC), an MMO choice.
const RESIDENTS: [(u8, f32, f32); 3] = [(0x18, 7.0, 2.0), (0x17, 3.0, 3.0), (0x19, -4.0, 8.0)];
/// Tails (RESIDENTS[1]) stands still by the arrival teleporter as the guide:
/// no Chao AI, the native stand motion (ChaoMain AL_SetMotionLink 0x1B504,
/// motion 1 as chao_player uses it). Its offset from the arrival point is an
/// MMO placement choice.
const STANDING: usize = 1;
const AL_BEHAVIOR_CONTROL: u32 = 0xF8;
const AL_SET_MOTION_LINK: u32 = 0x1_B504;
const STAND_MOTION: u32 = 1;
/// ChaoMain `CreateChaoExtra` 0x3148C (chao.h): (param, IsParamCopy, element,
/// &pos, angy). IsParamCopy=1 keeps them out of every save.
const CREATE_CHAO_EXTRA: u32 = 0x3_148C;
/// CHAO_PARAM_GC is 0x800 bytes; `type` at +0x80 (al_chao_info.h).
const PARAM_SIZE: u32 = 0x800;
/// New-player tutorial (MMO glue): the lobby's Tails Chao resident is the
/// guide (ChaoMain never creates TYPE_OMOCHAO, so Omochao can't be used).
/// Reaching it ends the tutorial: the new Chao's growth (CHAO_PARAM_GC +0xA8
/// body, +0x18 `growth` = +0xC0) is set to the value ChaoMain 0x1531C
/// compares it with; that function then starts the game's own cocoon
/// (`ALBHV_Mayu` 0x3AE44) for a child (type 2) Chao.
const GUIDE: usize = 1;
/// ChaoMain 0x1531C loads the full-growth value with `lis r3` of section 5
/// + 0x2A50 and `lfs f1, 0x48(r3)` (recompiled p2.rs).
const FULL_GROWTH: (u32, u32) = (0x2A50, 0x48);
const GROWTH: u32 = 0xC0;
/// How close (world units, squared) the player must come to the guide.
const REACH_SQ: f32 = 2.0 * 2.0;
/// main.dol `playertwp` .bss 0x801E7768 (taskwk*[]); taskwk +0x14 is pos.
const PLAYERTWP: u32 = 0x801E_7768;
/// main.dol positional sound calls ChaoMain makes for a Chao's voice, each
/// given that Chao's taskwk +0x14 (task +0x34 is the taskwk): `fn_8006B7EC`
/// (id, 0, 0, vol, pos) and `SE_CallV2_Timer` 0x8006AFFC (id, task, 1, vol,
/// timer, pos). Their callers in ChaoMain ignore the return value.
const SE_AT_POS: u32 = 0x8006_B7EC;
const SE_AT_POS_TIMER: u32 = 0x8006_AFFC;

/// (stage, character) to start, until it has started.
static PENDING: Global<Option<(u16, u32)>> = global!(None);
/// SA2_MODE_LOG: the last global mode logged while a start is pending.
static MODE_SEEN: Global<u32> = global!(u32::MAX);
/// SA2_CHAO_AREA: the Chao World area to change to once the lobby is up.
static AREA: Global<Option<u32>> = global!(None);
/// Started in Chao World: the last area told to the page (u32::MAX before the first).
static SHOWN: Global<Option<u32>> = global!(None);
/// SA2_TP_ARRIVE=1: enter Chao World in the Neutral Garden, then go to the lobby
/// the game's own way, so it arrives as from a garden teleporter.
static TP_ARRIVE: Global<bool> = global!(false);
/// SA2_HATCH_EGG=1: lay a new player's egg where they arrive in the lobby.
static HATCH: Global<bool> = global!(false);
/// The lobby's residents are out (cleared when the game leaves the lobby).
static RESIDENTS_OUT: Global<bool> = global!(false);
/// The residents' taskwks, so their Chao sounds stay silent.
static RESIDENT_TWP: Global<[u32; 3]> = global!([0; 3]);
/// The new player's Chao save slot while the tutorial runs (0 = no tutorial).
static TUTORIAL_SLOT: Global<u32> = global!(0);

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    let Some(stage) = std::env::var("SA2_START_STAGE")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
    else {
        return;
    };
    if stage == CHAO_WORLD {
        // Character 0 (Sonic): the Chao World branch takes the cursor's
        // character; any character enters the same Chao World.
        // MMO glue: Shadow (character 1) rides, so a Chao in Shadow's shoes walks
        // and skates with Shadow's own motions (SA2_CHAO_RIDER=n overrides; debug).
        // shortcut: character number 1 = Shadow is unverified against the
        // character table; the headless run checks the rider's model.
        let rider = std::env::var("SA2_CHAO_RIDER").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(1);
        PENDING.set(Some((CHAO_WORLD, rider)));
        SHOWN.set(Some(u32::MAX));
        AREA.set(
            std::env::var("SA2_CHAO_AREA")
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|a| (1..=LAST_AREA).contains(a)),
        );
        if AREA.get().is_none() && std::env::var("SA2_TP_ARRIVE").is_ok_and(|v| v.trim() == "1") {
            TP_ARRIVE.set(true);
        }
        HATCH.set(std::env::var("SA2_HATCH_EGG").is_ok_and(|v| v.trim() == "1"));
        c.rt.set_override(TOP_LEVEL, top_level);
        c.rt.set_override(SE_AT_POS, |c, m| quiet_residents(c, m, SE_AT_POS, 7));
        c.rt.set_override(SE_AT_POS_TIMER, |c, m| {
            quiet_residents(c, m, SE_AT_POS_TIMER, 8)
        });
        return;
    }
    if crate::combat::wanted(stage) {
        // The combat area (combat.rs): Big Foot's stage, not looked up in the
        // Stage Select table. Character 0 (Sonic), whose boss it is.
        PENDING.set(Some((stage, 0)));
        c.rt.set_override(TOP_LEVEL, top_level);
        return;
    }
    let Some(rel) = crate::dvd::read_file("advertiseD.rel") else {
        eprintln!("start-stage: advertiseD.rel not found; booting to the title");
        return;
    };
    match stage_select_table(&rel).and_then(|t| t.into_iter().find(|&(s, _)| s == i32::from(stage)))
    {
        // advertiseD 0x10258: Cannon's Core (0x22) starts at its Tails part (0x24), character 6.
        Some(_) if stage == 0x22 => {
            PENDING.set(Some((0x24, 6)));
            c.rt.set_override(TOP_LEVEL, top_level);
        }
        Some((_, ch)) => {
            PENDING.set(Some((stage, ch as u32)));
            c.rt.set_override(TOP_LEVEL, top_level);
        }
        None => eprintln!(
            "start-stage: stage {stage} is not in the game's Stage Select; booting to the title"
        ),
    }
}

fn top_level(c: &mut Cpu, m: &mut Mem) {
    if PENDING.get().is_some() && std::env::var("SA2_MODE_LOG").is_ok() {
        let mode = m.r32(GLOBAL_MODE);
        if MODE_SEEN.get() != mode {
            MODE_SEEN.set(mode);
            eprintln!("start-stage: global mode {mode} next {}", m.r32(NEXT_GLOBAL_MODE));
        }
    }
    // Without a memory card the boot reaches MD_TITLE_INIT (1); with one (the
    // MMO server's card) it goes 0, 2, 13, 21, 22, 13, 14, then 12 and on into
    // the intro movies without ever reaching 1 (seen with SA2_MODE_LOG). Mode 12
    // comes after the card is read, so the start happens there too. Unverified:
    // what mode 12 is in the game's own terms.
    let mode = m.r32(GLOBAL_MODE);
    // MMO glue: the boot logos (mode 22) can't be cut short (the game finishes
    // work there the start needs), so the page covers them while a start is
    // pending: /dev/area 254 = covering on, 253 = off.
    if PENDING.get().is_some() && (mode == MD_LOGOS) != LOGOS_HIDDEN.get() {
        LOGOS_HIDDEN.set(mode == MD_LOGOS);
        tell_page(if mode == MD_LOGOS { PAGE_COVER } else { PAGE_UNCOVER });
    }
    if mode == MD_TITLE_INIT || mode == MD_AFTER_CARD {
        if let Some((stage, ch)) = PENDING.take() {
            start(c, m, stage, ch);
        }
    }
    if SHOWN.get().is_some() {
        crate::tiny_transporter::frame(c, m);
    }
    if let (Some(shown), Some(text)) = (SHOWN.get(), chaomain_text(m)) {
        let now = guest_call(c, m, text + AL_GET_STAGE_NUMBER, &[]);
        // Once the lobby is running, change area the game's own way.
        if let (Some(area), LOBBY) = (AREA.get(), now) {
            guest_call(c, m, text + AL_CHANGE_AREA, &[area]);
            AREA.set(None);
        }
        // Arrived in the lobby (the player task is up): lay the egg once.
        // Until the player stands in the lobby, its previous stage reads as the
        // Neutral Garden, so the lobby places the player as arriving from it.
        if TP_ARRIVE.get() {
            if now == LOBBY && m.r32(PLAYERTWP) != 0 {
                TP_ARRIVE.set(false);
            } else if let Some(bss) = chaomain_section(m, 5) {
                m.w32(bss + PRE_STAGE, 1);
            }
        }
        if HATCH.get() && !TP_ARRIVE.get() && now == LOBBY && m.r32(PLAYERTWP) != 0 {
            HATCH.set(false);
            lay_egg_with(c, m, text, None);
        }
        // Eggs the player picked and named on the page hatch here too.
        if !TP_ARRIVE.get() && now == LOBBY && m.r32(PLAYERTWP) != 0 {
            for gene in picked_eggs() {
                lay_egg_with(c, m, text, gene.as_ref());
            }
        }
        if now != shown {
            RESIDENTS_OUT.set(false);
            RESIDENT_TWP.set([0; 3]);
            STANDING_TASK.set(0);
        }
        if !RESIDENTS_OUT.get() && m.r32(PLAYERTWP) != 0 {
            RESIDENTS_OUT.set(true);
            residents(c, m, text, now);
        }
        if now == LOBBY && TUTORIAL_SLOT.get() != 0 {
            tutorial(m, text);
        }
        if now != shown {
            SHOWN.set(Some(now));
            // The page's rooms know the lobby as area 0.
            tell_page(if now == LOBBY { 0 } else { now });
        }
    }
    crate::race_probe::frame(c, m);
    crate::karate::frame(c, m);
    original(c, m, TOP_LEVEL);
}

/// Named eggs the page queued since the last read (/dev/egg: a u32 count,
/// then per egg a u32 "has gene" flag and an AL_GENE; absent outside the
/// browser). Each is the server egg's gene, or None for the game's own.
fn picked_eggs() -> Vec<Option<Vec<u8>>> {
    use std::io::Read;
    const SLOT: usize = 4 + GENE_BYTES;
    let mut b = vec![0u8; 4 + 8 * SLOT];
    let Ok(n) = std::fs::File::open("/dev/egg").and_then(|mut f| f.read(&mut b)) else {
        return Vec::new();
    };
    if n < 4 {
        return Vec::new();
    }
    let count = (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).min((n - 4) / SLOT);
    (0..count)
        .map(|i| {
            let s = &b[4 + i * SLOT..4 + (i + 1) * SLOT];
            (s[..4] != [0; 4]).then(|| s[4..].to_vec())
        })
        .collect()
}

/// The game's own egg at the player's feet, with guest scratch for the gene
/// and position on a lowered stack (as the native caller keeps them).
fn lay_egg_with(c: &mut Cpu, m: &mut Mem, text: u32, from_server: Option<&Vec<u8>>) {
    let slot = guest_call(c, m, text + SAVE_SLOT, &[]);
    if slot == 0 {
        eprintln!("start-stage: no free Chao save slot; no egg");
        return;
    }
    let sp = c.r[1];
    c.r[1] = sp - 0x100 - GENE_SIZE;
    let (gene, pos) = (c.r[1] + 0x10, c.r[1] + 0x10 + GENE_SIZE);
    let player = m.r32(PLAYERTWP) + 0x14;
    for i in 0..3 {
        m.w32(pos + i * 4, m.r32(player + i * 4));
    }
    guest_call(c, m, text + AL_GENE_CREATE, &[gene]);
    if let Some(bytes) = from_server {
        for (from, to) in SERVER_GENE {
            for o in from..to {
                m.w8(gene + o, bytes[o as usize]);
            }
        }
    }
    guest_call(c, m, text + CREATE_EGG, &[gene, slot, 0, pos, 3]);
    c.r[1] = sp;
    // The tutorial is off until it is checked (SA2_TUTORIAL=1 turns it on).
    if std::env::var("SA2_TUTORIAL").is_ok_and(|v| v.trim() == "1") {
        TUTORIAL_SLOT.set(slot);
    }
}

/// The hub's character Chao, with normal Chao AI, near where the player arrives.
/// The residents' round: a garden, back to the lobby, then the next garden.
const ROAM: [u32; 6] = [1, LOBBY, 2, LOBBY, 3, LOBBY];
/// Where resident `n` is now. Each stays a random 2 to 5 minutes, then moves on
/// along ROAM. The waits come from a hash of the day, the Chao and the hop, so
/// every client reads the same schedule from the wall clock.
fn roam_area(n: usize) -> u32 {
    // The guide stays in the lobby by the arrival teleporter.
    if n == STANDING {
        return LOBBY;
    }
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    ROAM[roam_hop(n as u64, t) % ROAM.len()]
}

/// The hop resident `n` is on at unix time `t` (restarts each UTC day).
fn roam_hop(n: u64, t: u64) -> usize {
    let (day, mut left) = (t / 86_400, t % 86_400);
    let mut hop = 0u64;
    loop {
        // splitmix64 of (day, n, hop): 120..=300 s.
        let mut z = (day << 20 ^ n << 16 ^ hop).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ z >> 30).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ z >> 27).wrapping_mul(0x94D0_49BB_1331_11EB);
        let wait = 120 + (z ^ z >> 31) % 181;
        if left < wait {
            return (hop + n * 2 + day) as usize;
        }
        left -= wait;
        hop += 1;
    }
}

#[test]
fn roam_waits_two_to_five_minutes() {
    let mut last = roam_hop(1, 0);
    let mut since = 0;
    for t in 1..86_400 {
        let h = roam_hop(1, t);
        since += 1;
        if h != last {
            assert!((120..=300).contains(&since), "{since}");
            (last, since) = (h, 0);
        }
    }
}

// shortcut: a resident only appears or leaves when the player enters an area,
// not at the 5-minute mark itself; despawn live once KillChao's address is known.
fn residents(c: &mut Cpu, m: &mut Mem, text: u32, now: u32) {
    let sp = c.r[1];
    c.r[1] = sp - 0x100 - PARAM_SIZE;
    let (param, pos) = (c.r[1] + 0x10, c.r[1] + 0x10 + PARAM_SIZE);
    // AL_SHAPE_ELEMENT (0x34 bytes, chao.h): CreateChaoExtra runs AL_GeneAnalyze
    // on a fresh gene, which resets param type to child, then fn_60_209C8 copies
    // the element's type (+0) over it; that is the type the shape init sees.
    let elem = pos + 0x10;
    let player = m.r32(PLAYERTWP) + 0x14;
    let mut twp = [0; 3];
    let mut standing_task = 0;
    for (n, (ty, dx, dz)) in RESIDENTS.into_iter().enumerate() {
        if roam_area(n) != now {
            continue;
        }
        for i in (0..PARAM_SIZE).step_by(4) {
            m.w32(param + i, 0);
        }
        m.w8(param + 0x80, ty);
        for i in (0..0x34).step_by(4) {
            m.w32(elem + i, 0);
        }
        m.w8(elem, ty);
        m.w32(pos, (f32::from_bits(m.r32(player)) + dx).to_bits());
        m.w32(pos + 4, m.r32(player + 4));
        m.w32(pos + 8, (f32::from_bits(m.r32(player + 8)) + dz).to_bits());
        let task = guest_call(c, m, text + CREATE_CHAO_EXTRA, &[param, 1, elem, pos, 0]);
        if task != 0 {
            twp[n] = m.r32(task + 0x34);
            if n == STANDING {
                standing_task = task;
            }
        }
    }
    RESIDENT_TWP.set(twp);
    c.r[1] = sp;
    // Tails stands as the guide only by the lobby's arrival teleporter.
    if twp[STANDING] != 0 && now == LOBBY {
        // chao_player's AI hook skips the standing resident; install it if
        // the player-Chao has not.
        let a = text + AL_BEHAVIOR_CONTROL;
        if !c.rt.overrides.contains_key(&a) {
            c.rt.set_override(a, crate::chao_player::behavior);
        }
        STANDING_TASK.set(standing_task);
        let link = text + AL_SET_MOTION_LINK;
        guest_call(c, m, link, &[standing_task, STAND_MOTION]);
    }
}

/// A hub resident's al_chao_info.h type (0x17 Tails, 0x18 Knuckles, 0x19 Amy), for its name tag.
pub(crate) fn resident_type(m: &Mem, task: u32) -> Option<u8> {
    let twp = m.r32(task + 0x34);
    let n = RESIDENT_TWP.get().iter().position(|&t| t != 0 && t == twp)?;
    Some(RESIDENTS[n].0)
}

/// The standing resident's task (0 = none).
static STANDING_TASK: Global<u32> = global!(0);

/// True for the standing resident's task or its taskwk (AL_BehaviorControl's r3).
pub(crate) fn standing_resident(m: &Mem, r3: u32) -> bool {
    let task = STANDING_TASK.get();
    task != 0 && (r3 == task || r3 == m.r32(task + 0x34))
}

/// The tutorial ends when the player reaches the Tails Chao guide: the new
/// Chao's growth goes to full, and ChaoMain's own check evolves it.
fn tutorial(m: &mut Mem, text: u32) {
    let guide = RESIDENT_TWP.get()[GUIDE];
    if guide == 0 {
        return;
    }
    let (p, o) = (m.r32(PLAYERTWP) + 0x14, guide + 0x14);
    let d = |i| f32::from_bits(m.r32(p + i)) - f32::from_bits(m.r32(o + i));
    if d(0) * d(0) + d(8) * d(8) > REACH_SQ {
        return;
    }
    let (lo, off) = FULL_GROWTH;
    let hi = (section(m, text, 5) + lo).wrapping_add(0x8000) & 0xFFFF_0000;
    m.w32(TUTORIAL_SLOT.get() + GROWTH, m.r32(hi + off));
    TUTORIAL_SLOT.set(0);
}

/// ChaoMain's section `n` address, from the same module entry as `text`.
fn section(m: &Mem, text: u32, n: u32) -> u32 {
    let mut info = m.r32(0x8000_30C8);
    while info != 0 {
        let table = m.r32(info + 0x10);
        if m.r32(table + 8) & !1 == text {
            return m.r32(table + n * 8) & !1;
        }
        info = m.r32(info + 4);
    }
    0
}

/// The hub's character Chao make no Chao sounds: a positional sound whose
/// position is a resident's taskwk +0x14 is dropped; every other one plays.
fn quiet_residents(c: &mut Cpu, m: &mut Mem, addr: u32, pos_reg: usize) {
    let twp = c.r[pos_reg].wrapping_sub(0x14);
    if twp != 0 && (RESIDENT_TWP.get().contains(&twp) || twp == crate::chao_player::own_twp(m)) {
        return;
    }
    original(c, m, addr);
}

/// advertiseD 0x1028C..0x1034C (the one-player branch), in order.
fn start(c: &mut Cpu, m: &mut Mem, stage: u16, ch: u32) {
    guest_call(c, m, SET_STAGE_NUM, &[u32::from(stage)]);
    guest_call(c, m, SET_PLAYER_CHAR, &[0, ch]);
    if stage != CHAO_WORLD {
        m.w8(GAME + 0x23, 0); // mission: the menu's cursor, mission 1 here (and 0 for stage 0x1F)
    }
    m.w8(GAME + 0x20, 0);
    m.w8(GAME + 0x1E, 0);
    m.w8(GAME, 0);
    m.w32(NEXT_GLOBAL_MODE, MD_ACTION_INIT);
    m.w32(GLOBAL_MODE, MD_ACTION_INIT);
}

/// The browser page's `/dev/area` (sa2-web worker.js): the Chao World area,
/// so the page keeps the player in that area's MMO room. Absent on desktop.
fn tell_page(area: u32) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/area") {
        let _ = f.write_all(&[area as u8]);
    }
}

/// ChaoMain's linked .text (section 1), through the game's module list
/// (OSModuleInfo: id +0, next +4, section table +0x10). None until linked.
fn chaomain_text(m: &Mem) -> Option<u32> {
    chaomain_section(m, 1)
}

/// A linked ChaoMain section's address (section table entries are 8 bytes:
/// offset, size; OSLink stores the .bss address there too).
fn chaomain_section(m: &Mem, n: u32) -> Option<u32> {
    module_section(m, CHAOMAIN, n)
}

/// A linked REL module's section address, None while it is not linked.
pub(crate) fn module_section(m: &Mem, module: u32, n: u32) -> Option<u32> {
    let mut info = m.r32(0x8000_30C8);
    while info != 0 {
        if m.r32(info) == module {
            let at = m.r32(m.r32(info + 0x10) + n * 8) & !1;
            return (at != 0).then_some(at);
        }
        info = m.r32(info + 4);
    }
    None
}

/// The Stage Select's rows (stage, character; 16 bytes each, ended by stage
/// -1), found through the menu code's own relocation.
fn stage_select_table(rel: &[u8]) -> Option<Vec<(i32, i32)>> {
    let u = |o: usize| {
        rel.get(o..o + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    };
    let module = u(0)?;
    let sections = u(0x10)? as usize;
    let (imp, imp_size) = (u(0x28)? as usize, u(0x2C)? as usize);
    let mut target = None;
    for k in 0..imp_size / 8 {
        if u(imp + k * 8)? != module {
            continue;
        }
        let (mut p, mut sec, mut off) = (u(imp + k * 8 + 4)? as usize, 0u8, 0u32);
        loop {
            let r = rel.get(p..p + 8)?;
            p += 8;
            off += u32::from(u16::from_be_bytes([r[0], r[1]]));
            match r[2] {
                203 => break,
                202 => (sec, off) = (r[3], 0),
                _ if (sec, off) == TABLE_RELOC => {
                    target = Some((r[3], u32::from_be_bytes(r[4..8].try_into().unwrap())))
                }
                _ => {}
            }
        }
    }
    let (sec, addend) = target?;
    let at = (u(sections + usize::from(sec) * 8)? & !1) as usize + addend as usize;
    let mut rows = Vec::new();
    loop {
        let row = rel.get(at + rows.len() * 16..)?;
        let stage = i32::from_be_bytes(row.get(..4)?.try_into().unwrap());
        if stage == -1 {
            return Some(rows);
        }
        rows.push((
            stage,
            i32::from_be_bytes(row.get(4..8)?.try_into().unwrap()),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic REL (test input only, not game data): module 7, section 1
    /// code with one relocation at 0x10246 to section 2 + 8, and two rows.
    fn rel() -> Vec<u8> {
        let mut r = vec![0u8; 0x100];
        r[0..4].copy_from_slice(&7u32.to_be_bytes());
        r[0x10..0x14].copy_from_slice(&0x40u32.to_be_bytes());
        // Section table: 0 empty, 1 (code, unused here), 2 data at 0x80.
        r[0x48..0x4C].copy_from_slice(&0x61u32.to_be_bytes());
        r[0x50..0x54].copy_from_slice(&0x80u32.to_be_bytes());
        for (i, v) in [13, 0, 0, 0, 34, 1, 0, 0, -1, 0, 0, 0].iter().enumerate() {
            r[0x88 + i * 4..0x8C + i * 4].copy_from_slice(&(*v as i32).to_be_bytes());
        }
        // Import table at 0xC0: module 7's relocations at 0xD0.
        r[0x28..0x2C].copy_from_slice(&0xC0u32.to_be_bytes());
        r[0x2C..0x30].copy_from_slice(&8u32.to_be_bytes());
        r[0xC0..0xC4].copy_from_slice(&7u32.to_be_bytes());
        r[0xC4..0xC8].copy_from_slice(&0xD0u32.to_be_bytes());
        let relocs: [(u16, u8, u8, u32); 4] = [
            (0, 202, 1, 0),
            (0xFFFF, 201, 0, 0),
            (0x0247, 6, 2, 8),
            (0, 203, 0, 0),
        ];
        let mut p = 0xD0;
        for (o, t, s, a) in relocs {
            r[p..p + 2].copy_from_slice(&o.to_be_bytes());
            (r[p + 2], r[p + 3]) = (t, s);
            r[p + 4..p + 8].copy_from_slice(&a.to_be_bytes());
            p += 8;
        }
        r
    }

    #[test]
    fn reads_the_rows_the_menu_code_points_at() {
        assert_eq!(stage_select_table(&rel()), Some(vec![(13, 0), (34, 1)]));
    }

    #[test]
    fn start_does_the_stage_selects_writes() {
        fn set_stage(c: &mut Cpu, m: &mut Mem) {
            m.w32(0x8000_0000, c.r[3]);
        }
        fn set_char(c: &mut Cpu, m: &mut Mem) {
            m.w32(0x8000_0004 + 4 * c.r[3], c.r[4]);
        }
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.set_override(SET_STAGE_NUM, set_stage);
        c.rt.set_override(SET_PLAYER_CHAR, set_char);
        m.w8(GAME + 0x23, 2);
        m.w8(GAME + 0x1E, 1);
        start(&mut c, &mut m, 13, 0);
        assert_eq!((m.r32(0x8000_0000), m.r32(0x8000_0004)), (13, 0));
        assert_eq!(
            (m.r8(GAME + 0x23), m.r8(GAME + 0x1E), m.r32(GLOBAL_MODE)),
            (0, 0, MD_ACTION_INIT)
        );
    }

    #[test]
    fn finds_chaomain_text_in_the_module_list() {
        let mut m = Mem::default();
        assert_eq!(chaomain_text(&m), None);
        // Two modules: 0x10 then ChaoMain; ChaoMain's section 1 at 0x8100_0001 (exec bit).
        m.w32(0x8000_30C8, 0x8040_0000);
        m.w32(0x8040_0000, 0x10);
        m.w32(0x8040_0004, 0x8040_0100);
        m.w32(0x8040_0100, CHAOMAIN);
        m.w32(0x8040_0110, 0x8040_0200);
        m.w32(0x8040_0208, 0x8100_0001);
        assert_eq!(chaomain_text(&m), Some(0x8100_0000));
    }

    /// The player's own advertiseD.rel (kept local): SA2_ADVERTISE_REL=path.
    #[test]
    #[ignore]
    fn the_real_stage_select_table() {
        let rel = std::fs::read(std::env::var("SA2_ADVERTISE_REL").unwrap()).unwrap();
        let t = stage_select_table(&rel).unwrap();
        assert!(t.contains(&(13, 0)), "City Escape is Sonic's: {t:?}");
        eprintln!("{} rows: {t:?}", t.len());
    }
}
