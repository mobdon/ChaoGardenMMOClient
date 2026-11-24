//! Chao MMO glue: SA2_START_STAGE=<stage> starts
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
/// Set when the game leaves a stage for the title: the stage's exec returning
/// -1 (exit) or -2 (0x800D2EF0, 0x800D2F68), after the stage is torn down the
/// same way as before a restart's mode 4. Its handler only goes on to mode 13
/// and the title (0x800D2E8C). At boot it comes before the start.
const MD_LEFT_STAGE: u32 = 2;
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
/// ChaoMain area numbers 1..=12 (1 Neutral, 2 Hero, 3 Dark, 4 the race,
/// 5 the race entrance, 6 Kindergarten,
/// 9 Stadium, 10 Karate); 0 is the lobby Chao World starts in.
const LAST_AREA: u32 = 12;
/// The Chao Lobby's stage number (al_stage.h: 7 lobby; 0 is none, while loading).
pub(crate) const LOBBY: u32 = 7;
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
/// The egg's frames left before it starts to hatch, in its work at task +0x40.
const EGG_WAIT: u32 = 0x14;
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
/// New-player tutorial (MMO glue): the lobby's Tails Chao (a resident, see
/// RESIDENTS) is the guide. Reaching it ends the tutorial: the new Chao's
/// growth (CHAO_PARAM_GC +0xA8 body, +0x18 `growth` = +0xC0) is set to the
/// value ChaoMain 0x1531C compares it with; that function then starts the
/// game's own cocoon (`ALBHV_Mayu` 0x3AE44) for a child (type 2) Chao.
/// ChaoMain never creates TYPE_OMOCHAO itself, so Omochao can't be used here.
const GUIDE: usize = 1;
/// CHAO_PARAM_GC `type` (+0x80) of a child Chao (al_chao_info.h TYPE_CHILD).
const CHILD: u8 = 2;
/// ChaoMain 0x1531C loads the full-growth value with `lis r3` of section 5
/// + 0x2A50 and `lfs f1, 0x48(r3)` (recompiled p2.rs).
const FULL_GROWTH: (u32, u32) = (0x2A50, 0x48);
const GROWTH: u32 = 0xC0;
/// How close (world units, squared) the player must come to the guide (an MMO choice).
/// Above 5.2: the two Chao's collision keeps the player that far from the standing guide (build 18 run).
const REACH_SQ: f32 = 8.0 * 8.0;
/// What the guide says, one line at a time, while the player stays near it.
/// The first and last are the game's own lobby hint lines (Steam
/// gd_PC/Message/mhChaoLobbye.prs, s2392 and s2393); the middle two are MMO text.
const GUIDE_LINES: [&str; 4] = [
    "Welcome to Chao world!",
    "Press Tab to see your\nChao's card.",
    "Your items are in the\ninventory on that card.",
    "Get to the Chao garden from here.",
];
/// Frames each line stays up before the next one.
/// shortcut: timed, not closed by the player; switch to the window's own close once it is traced.
const LINE_FRAMES: u32 = 240;
/// ChaoMain's message window (the one its own warnings use, fn 0x1CE28):
/// 0x1E030(text) opens it with mode 5. Unverified beyond that native use.
const AL_MESSAGE: u32 = 0x1_E030;
/// main.dol allocator as the message window task calls it (0x800DF630:
/// r3=1, r4=size). Unverified name; used to keep the lines in guest memory.
const ALLOC: u32 = 0x8011_97FC;
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
/// The Chao World area the player was last in (0: none, or already back in the lobby).
static CAME_FROM: Global<u32> = global!(0);
/// SA2_HATCH_EGG=1: lay a new player's egg where they arrive in the lobby.
static HATCH: Global<bool> = global!(false);
/// The pending start has happened; the title after it means the game came back.
static STARTED: Global<bool> = global!(false);
/// The pause menu's EXIT asked for this area ([`go_area`]; 0 = none).
static TO_AREA: Global<u32> = global!(0);
/// The Stadium (the menu between Race and Karate).
pub(crate) const STADIUM: u32 = 9;
/// ChaoMain .text: sets garden flag bits in the Chao save (word at +0xC).
/// The lobby makes its Kindergarten door only with bit 4 (ChaoStgLobby .text
/// 0x28C), which the game sets in the Neutral Garden's start (ChaoStgNeut
/// .text 0x60C). Read from the generated code; unverified live.
const GARDEN_FLAG_ON: u32 = 0xCC1C;
const KINDER_DOOR: u32 = 4;
/// ChaoMain .text: tests garden flag bits. The lobby makes its Hero door
/// (to area 2) on bit 0x10 and its Dark door (to area 3) on bit 0x40
/// (ChaoStgLobby .text 0x1DC, 0x234; the lobby model at 0x2E4).
const GARDEN_FLAG: u32 = 0xCC54;
const LOBBY_MODULE: u32 = 65;
/// ChaoStgLobby's .text is under this size (its last function starts at 0x1C30).
const LOBBY_TEXT: u32 = 0x2000;
/// The ChaoMain .text the garden locks were hooked for (0 = none).
static LOCKS_AT: Global<u32> = global!(0);
/// The lobby exit was walked into (chao_player): leave for the game's own
/// Stage Select instead of starting Chao World again.
static TO_STAGE_SELECT: Global<bool> = global!(false);
/// advertiseD (module 91): .bss 0x62054 is the menu screen its dispatcher
/// fn_91_30818 switches on (jump table .data 0x9DE60); screen 3 runs
/// fn_91_2BC44, the Stage Select (its confirm is fn_91_101E0, as `start`).
/// The menu's own transitions write the next screen at +4.
const ADVERTISE: u32 = 91;
const MENU_SCREEN: u32 = 0x6_2054;
const SCREEN_STAGE_SELECT: u32 = 3;
/// advertiseD .text fn_91_2BB38: the Stage Select's setup, called before screen 3.
const STAGE_SELECT_SETUP: u32 = 0x2BB38;
/// Game state byte advertiseD reads to open its menu instead of the title.
const GO_TO_MENU: u32 = 0x5B;
static MENU_SEEN: Global<(u32, u32, u32)> = global!((u32::MAX, u32::MAX, u32::MAX));

pub(crate) fn to_stage_select() {
    TO_STAGE_SELECT.set(true);
}

/// Leaving for the Stage Select: the player's Chao is not made again meanwhile.
pub(crate) fn to_stage_select_pending() -> bool {
    TO_STAGE_SELECT.get()
}

/// While going to the Stage Select: once the menu module is up, ask its
/// screen dispatcher for the Stage Select, and log its top state and screen.
/// Unverified: that the request field is honoured from the title state; the
/// log shows which top state the menu is in when the request lands.
fn stage_select_frame(c: &mut Cpu, m: &mut Mem) {
    let Some(bss) = module_section(m, ADVERTISE, 6) else { return };
    let now = (m.r32(bss), m.r32(bss + MENU_SCREEN), m.r32(bss + MENU_SCREEN + 4));
    if MENU_SEEN.replace(now) != now {
        eprintln!("start-stage: menu state {} screen {} next {}", now.0, now.1, now.2);
    }
    // The menu's own transitions do not take the next field from just any
    // screen (build 11: "screen 7 next 3" stayed on 7, the screen menu init
    // fn_91_30A0C leads to). Set the current screen instead, once, while the
    // screen struct is fresh from that init, so the Stage Select's exec
    // (fn_91_2BC44) starts from its own first step.
    // Both of the menu's own ways to screen 3 (advertiseD .text 0x27448 and
    // 0x2C2EC) first call fn_91_2BB38, the Stage Select's setup: skipped, its
    // pictures drew as grey noise (headless, with and without the gear).
    if now.1 == SCREEN_STAGE_SELECT {
        TO_STAGE_SELECT.set(false);
    } else if let Some(text) = module_section(m, ADVERTISE, 1) {
        // SA2_SS_SETUP=0 (debug): without the setup call, as before.
        if !std::env::var("SA2_SS_SETUP").is_ok_and(|v| v == "0") {
            guest_call(c, m, text + STAGE_SELECT_SETUP, &[]);
        }
        m.w32(bss + MENU_SCREEN, SCREEN_STAGE_SELECT);
        m.w32(bss + MENU_SCREEN + 4, 0);
    }
}
/// The lobby's residents are out (cleared when the game leaves the lobby).
static RESIDENTS_OUT: Global<bool> = global!(false);
/// The residents' taskwks, so their Chao sounds stay silent.
static RESIDENT_TWP: Global<[u32; 3]> = global!([0; 3]);
/// The residents' tasks, and how many frames each has been walking out (0: staying).
static RESIDENT_TASK: Global<[u32; 3]> = global!([0; 3]);
static LEAVING: Global<[u32; 3]> = global!([0; 3]);
/// Where the player arrived in this area: residents come in and go out there.
static ENTRY: Global<[f32; 3]> = global!([0.0; 3]);
/// The new player's Chao save slot while the tutorial runs (0 = no tutorial).
static TUTORIAL_SLOT: Global<u32> = global!(0);
/// Cocoon timers (section 5 + 0x2A50, read by `ALBHV_Mayu` 0x3AE44) and its
/// native frames 200/1800/900/900/100 (about 65 s); phases 2-4 cut to 300 frames each
/// (1200 frames, about 20 s) for the tutorial's evolution only. Phases 1 and 5 stay native.
const COCOON_FRAMES: [(u32, u32); 3] = [(0x88, 300), (0x8C, 300), (0x90, 300)];
/// Frames to keep the short timers before putting the native ones back.
/// shortcut: shared table, so a garden Chao evolving in this window is also fast; a per-Chao hook would fix that.
const COCOON_HOLD: u32 = 1300;
static COCOON_RESTORE: Global<(u32, [u32; 3], u32)> = global!((0, [0; 3], 0));
/// The new Chao's (save slot, type, growth bits) last logged, and frames the tutorial has run.
static SLOT_SEEN: Global<(u32, u8, u32)> = global!((0, 0, 0));
/// The save slot of the egg laid for a new player (0 = none laid).
static EGG_SLOT: Global<u32> = global!(0);
static LOG_FRAMES: Global<u32> = global!(0);
/// Next guide line to show and frames until it shows.
static LINE: Global<(usize, u32)> = global!((0, 0));

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    let Some(stage) = std::env::var("SA2_START_STAGE")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
    else {
        return;
    };
    // Every MMO start: Chao that must stay silent (see quiet_residents).
    c.rt.set_override(SE_AT_POS, |c, m| quiet_residents(c, m, SE_AT_POS, 7));
    c.rt.set_override(SE_AT_POS_TIMER, |c, m| {
        quiet_residents(c, m, SE_AT_POS_TIMER, 8)
    });
    if stage == CHAO_WORLD {
        // The Chao World branch takes the cursor's character; any character
        // enters the same Chao World. The MMO's is the rider (see rider()).
        PENDING.set(Some((CHAO_WORLD, rider())));
        SHOWN.set(Some(u32::MAX));
        c.rt.set_override(TOP_LEVEL, top_level);
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
    // Every change of global mode, so a run shows which way the game went.
    {
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
            STARTED.set(true);
            start(c, m, stage, ch);
        }
    }
    if SHOWN.get().is_some() {
        crate::tiny_transporter::frame(c, m);
    }
    if TO_STAGE_SELECT.get() {
        stage_select_frame(c, m);
    }
    if mode == MD_LEFT_STAGE && STARTED.get() && TO_STAGE_SELECT.get() {
        // The game's own way on from here is its title and menus. advertiseD's
        // start (.text 0x1DD4..0x1E04) skips the title for its menu (init
        // fn_91_30A0C, exec fn_91_30AE4) while game state +0x5B is non-zero;
        // with it 0 it picks by +0x45, and the title demo ran (test build 9).
        if m.r8(GAME + GO_TO_MENU) == 0 {
            eprintln!("start-stage: left Chao World by the lobby exit; on to the game's menu");
            m.w8(GAME + GO_TO_MENU, 1);
        }
    } else if mode == MD_LEFT_STAGE && STARTED.get() {
        // MMO glue: the title never shows. The game left the stage it was
        // started in (a level's EXIT, Chao World's own way out): Chao World
        // again, as at boot. Unverified: an exit that skips this mode (the
        // exec's -1 path can go to mode 21 then 13, 0x800D2FB0).
        eprintln!("start-stage: the game left its stage; starting Chao World");
        if SHOWN.get().is_none() {
            SHOWN.set(Some(u32::MAX));
        }
        crate::chao_player::unlink_copies(c, m);
        // The same rider as at boot.
        start(c, m, CHAO_WORLD, rider());
    }
    if let (Some(shown), Some(text)) = (SHOWN.get(), chaomain_text(m)) {
        let now = guest_call(c, m, text + AL_GET_STAGE_NUMBER, &[]);
        if LOCKS_AT.replace(text) != text {
            c.rt.set_override(text + AL_CHANGE_AREA, change_area);
            c.rt.set_override(text + GARDEN_FLAG, garden_flag);
        }
        // MMO glue: the Kindergarten door is there from the first visit (the
        // game opens it after the Neutral Garden). Each frame: the card's save
        // may load after ChaoMain links.
        guest_call(c, m, text + GARDEN_FLAG_ON, &[KINDER_DOOR]);
        let to = TO_AREA.replace(0);
        if to != 0 && now != 0 && now != to {
            // The call ChaoMain's own way back to the lobby makes (0xB4F0).
            eprintln!("start-stage: area {now} to area {to} (pause EXIT)");
            guest_call(c, m, text + AL_CHANGE_AREA, &[to]);
        }
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
        // Not before the lobby has put the player at its arrival point: laid on the
        // player task's first frame the egg sat at the start spot inside the centre
        // exit's dish, 113 away from the player and the camera (build 18 run).
        if HATCH.get() && now == LOBBY && m.r32(PLAYERTWP) != 0 && player_placed(m) {
            HATCH.set(false);
            // Only a new player's egg: an egg an existing player picked is not their own Chao.
            EGG_SLOT.set(lay_egg_with(c, m, text, None, true));
        }
        // Eggs the player picked and named on the page hatch here too.
        if !TP_ARRIVE.get() && now == LOBBY && m.r32(PLAYERTWP) != 0 {
            for gene in picked_eggs() {
                lay_egg_with(c, m, text, gene.as_ref(), false);
            }
        }
        // Back to the lobby from an area: the lobby places the player by its
        // previous stage (ChaoMain .text 0x27988). A return that passes
        // through area 13 (0x27DA0, from a garden) leaves 13 there, and the
        // lobby then uses its default spot (the centre) instead of that
        // area's door: the area left is put back until the player stands in
        // the lobby. SA2_DOOR_RETURN=0 (debug): off.
        if now != LOBBY && now != 13 && (1..=LAST_AREA).contains(&now) {
            CAME_FROM.set(now);
        } else if CAME_FROM.get() != 0 && !TP_ARRIVE.get() && !std::env::var("SA2_DOOR_RETURN").is_ok_and(|v| v == "0") {
            if now == LOBBY && shown == LOBBY && m.r32(PLAYERTWP) != 0 {
                CAME_FROM.set(0);
            } else if let Some(bss) = chaomain_section(m, 5) {
                if m.r32(bss + PRE_STAGE) != CAME_FROM.get() {
                    eprintln!("start-stage: lobby return from area {} (previous stage read {})", CAME_FROM.get(), m.r32(bss + PRE_STAGE));
                    m.w32(bss + PRE_STAGE, CAME_FROM.get());
                }
            }
        }
        if now != shown {
            RESIDENTS_OUT.set(false);
            RESIDENT_TWP.set([0; 3]);
            RESIDENT_TASK.set([0; 3]);
            LEAVING.set([0; 3]);
            SHARED.set([None; 3]);
            MOTION.set([0; 3]);
            PINNED.set([false; 3]);
            STANDING_TASK.set(0);
        }
        // As the egg: ENTRY is the arrival point only once the area has placed the player (mob 2026-11-23).
        if !RESIDENTS_OUT.get() && m.r32(PLAYERTWP) != 0 && player_placed(m) {
            RESIDENTS_OUT.set(true);
            let at = m.r32(PLAYERTWP) + 0x14;
            ENTRY.set([0, 4, 8].map(|o| f32::from_bits(m.r32(at + o))));
            residents(c, m, text, now, &[0, 1, 2]);
        } else if RESIDENTS_OUT.get() {
            roam_live(c, m, text, now);
        }
        share_residents(c, m, text);
        hold_residents(m);
        restore_cocoon(m);
        watch_slot(m);
        if now == LOBBY && TUTORIAL_SLOT.get() != 0 {
            tutorial(c, m, text);
        }
        if now != shown {
            SHOWN.set(Some(now));
            // The page's rooms know the lobby as area 0.
            tell_page(if now == LOBBY { 0 } else { now });
        }
    }
    crate::race_control::frame(c, m);
    crate::race_probe::frame(c, m);
    crate::karate::frame(c, m);
    crate::story::frame(c, m);
    original(c, m, TOP_LEVEL);
}

/// ChaoMain .bss 0x41F6C (section 6): the task made by fn_60_27908 whose exec
/// fn_60_27988 (.text 0x27988) writes the player's arrival point for the area
/// and previous stage (lobby from the Neutral Garden: 80, 0, 80) while its
/// taskwk mode is 0, 1, 2, then stays at 3. Read from the disassembly; unverified live.
const PLACER_TASK: u32 = 0x4_1F6C;

/// The area's own placing of the player is over (or there is no such task).
fn player_placed(m: &Mem) -> bool {
    let task = chaomain_section(m, 6).map_or(0, |bss| m.r32(bss + PLACER_TASK));
    task == 0 || !(0..3).contains(&(m.r8(m.r32(task + 0x34)) as i8))
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

/// The Chao World area the player is in ([`LOBBY`] = lobby, 0 while loading),
/// when the game was started in Chao World and ChaoMain is loaded.
pub(crate) fn chao_area(c: &mut Cpu, m: &mut Mem) -> Option<u32> {
    SHOWN.get()?;
    let text = chaomain_text(m)?;
    Some(guest_call(c, m, text + AL_GET_STAGE_NUMBER, &[]))
}

/// MMO glue: SA2_GARDENS (the page, from the server's alignment rule) names
/// the gardens this player may enter: 'h' Hero (area 2), 'd' Dark (area 3).
/// Unset (no MMO): nothing is locked beyond the game's own rule.
fn garden_locked(area: u32, allowed: Option<&str>) -> bool {
    let key = match area {
        2 => 'h',
        3 => 'd',
        _ => return false,
    };
    allowed.is_some_and(|v| !v.contains(key))
}

fn gardens() -> Option<String> {
    std::env::var("SA2_GARDENS").ok()
}

/// ChaoMain's area change: never into a locked garden, whoever asks (a door,
/// SA2_CHAO_AREA, a go-to).
fn change_area(c: &mut Cpu, m: &mut Mem) {
    if garden_locked(c.r[3], gardens().as_deref()) {
        eprintln!("start-stage: area {} is locked for this Chao; staying", c.r[3]);
        return;
    }
    let at = LOCKS_AT.get() + AL_CHANGE_AREA;
    crate::chao_player::rel_original(c, m, at, change_area);
}

/// ChaoMain's garden flag test, for the lobby alone: a locked garden has no
/// door (and the lobby's model without it), as before the game unlocks it.
fn garden_flag(c: &mut Cpu, m: &mut Mem) {
    let area = match c.r[3] {
        0x10 => 2,
        0x40 => 3,
        _ => 0,
    };
    let lobby = module_section(m, LOBBY_MODULE, 1).is_some_and(|t| (t..t + LOBBY_TEXT).contains(&c.lr));
    if lobby && garden_locked(area, gardens().as_deref()) {
        c.r[3] = 0;
        return;
    }
    let at = LOCKS_AT.get() + GARDEN_FLAG;
    crate::chao_player::rel_original(c, m, at, garden_flag);
}

/// Change to `area` on the next frame, outside the pause.
pub(crate) fn go_area(area: u32) {
    TO_AREA.set(area);
}

/// The character under the Chao in Chao World. MMO glue: Sonic (0), as in his
/// own levels: the Chao runs like him, or skates like Shadow while it wears
/// Shadow's Air Shoes (chao_player shadow_moves swaps his action table live).
/// Until 2026-11-23 Shadow himself (character 1) rode here.
/// SA2_CHAO_RIDER=n overrides (debug; 1 = Shadow).
fn rider() -> u32 {
    let Ok(v) = std::env::var("SA2_CHAO_RIDER") else { return 0 };
    v.trim().parse().unwrap_or_else(|_| {
        eprintln!("start-stage: SA2_CHAO_RIDER={v:?} is not a number; riding Sonic (0)");
        0
    })
}

/// The game's own egg at the player's feet, with guest scratch for the gene
/// and position on a lowered stack (as the native caller keeps them).
/// Returns the egg's save slot (0 = none was free).
fn lay_egg_with(c: &mut Cpu, m: &mut Mem, text: u32, from_server: Option<&Vec<u8>>, hatch_now: bool) -> u32 {
    let slot = guest_call(c, m, text + SAVE_SLOT, &[]);
    if slot == 0 {
        eprintln!("start-stage: no free Chao save slot; no egg");
        return 0;
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
    let egg = guest_call(c, m, text + CREATE_EGG, &[gene, slot, 0, pos, 3]);
    c.r[1] = sp;
    // MMO glue: a new player's own egg (`hatch_now`) hatches on arrival; other eggs keep the game's wait. CreateEgg (0x9168) sets the wait at
    // the egg's work (task +0x40) +0x14 to (100 + a random part) * 60 frames;
    // AL_EggStop (0xA728) counts it down and, once it is below zero, sets the
    // egg's state to 3. Read from the disassembly. Inferred, unverified until a
    // run: that state 3 (AL_EggJump by the exec's call order) leads to the
    // hatch (CreateChaoFromEgg is called from AL_EggCrash, 0x9914).
    // No work block: nothing to set (a write there would land at guest address 0x14).
    if hatch_now && egg != 0 && m.r32(egg + 0x40) != 0 {
        m.w32(m.r32(egg + 0x40) + EGG_WAIT, 0);
        eprintln!("tutorial: egg task {egg:#x}, its wait set to 0");
    }
    eprintln!("tutorial: egg laid, save slot {slot:#x}");
    // The tutorial runs only with SA2_TUTORIAL=1: a start with the egg crashed the lobby on arrival.
    if std::env::var("SA2_TUTORIAL").is_ok_and(|v| v.trim() == "1") {
        SLOT_SEEN.set((slot, 0, 0));
        TUTORIAL_SLOT.set(slot);
    }
    slot
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
fn walking_out_steps_toward_the_way_out_and_stops_there() {
    let (x, z, yaw) = walk_step([0.0, 5.0, 0.0], [0.0, 0.0, 10.0]).unwrap();
    assert_eq!((x, z, yaw), (0.0, WALK_OUT_SPEED, 0));
    let (x, _, yaw) = walk_step([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]).unwrap();
    assert_eq!((x, yaw), (WALK_OUT_SPEED, 0x4000));
    assert_eq!(walk_step([0.0, 0.0, 9.5], [0.0, 3.0, 10.0]), None);
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

/// A walking-out resident's step (an MMO choice, under the 1.0 a frame where
/// chao_player's gait turns to a run), the reach that counts as the way out,
/// and the frames after which it leaves wherever it is.
const WALK_OUT_SPEED: f32 = 0.5;
const WALK_OUT_REACH: f32 = 1.0;
const WALK_OUT_FRAMES: u32 = 20 * 60;
/// AL_motion_table 578 alm_aruki_ncz (walk), as chao_player's gait uses it.
const WALK_MOTION: u32 = 578;

/// One step from `pos` toward `to` on the ground plane: the new x and z and
/// the yaw facing the way (None: arrived).
fn walk_step(pos: [f32; 3], to: [f32; 3]) -> Option<(f32, f32, u32)> {
    let (dx, dz) = (to[0] - pos[0], to[2] - pos[2]);
    let d = dx.hypot(dz);
    if d <= WALK_OUT_REACH {
        return None;
    }
    let yaw = (dx.atan2(dz) * 65536.0 / std::f32::consts::TAU) as i32 as u32 & 0xFFFF;
    Some((pos[0] + dx / d * WALK_OUT_SPEED, pos[2] + dz / d * WALK_OUT_SPEED, yaw))
}

/// While the player stays in an area: a roaming resident whose time here is up
/// walks to where the player came in and leaves there, and one whose time
/// here starts comes in there. Unverified live. There is no fade yet: the
/// Chao's draw has no known alpha control.
fn roam_live(c: &mut Cpu, m: &mut Mem, text: u32, now: u32) {
    let (mut twp, mut task, mut leaving) = (RESIDENT_TWP.get(), RESIDENT_TASK.get(), LEAVING.get());
    let entry = ENTRY.get();
    for n in (0..RESIDENTS.len()).filter(|&n| n != STANDING) {
        let here = roam_area(n) == now;
        if task[n] == 0 {
            if here {
                residents(c, m, text, now, &[n]);
                (twp[n], task[n]) = (RESIDENT_TWP.get()[n], RESIDENT_TASK.get()[n]);
            }
            continue;
        }
        if leaving[n] == 0 {
            if here {
                continue;
            }
            guest_call(c, m, text + AL_SET_MOTION_LINK, &[task[n], WALK_MOTION]);
            set_motion(n, WALK_MOTION);
        }
        leaving[n] += 1;
        let at = twp[n] + 0x14;
        let pos = [0, 4, 8].map(|o| f32::from_bits(m.r32(at + o)));
        match walk_step(pos, entry).filter(|_| leaving[n] < WALK_OUT_FRAMES) {
            Some((x, z, yaw)) => {
                m.w32(at, x.to_bits());
                m.w32(at + 8, z.to_bits());
                m.w32(twp[n] + 0x0C, yaw);
            }
            None => {
                // FreeTask, not KillChao: KillChao zeroes the Chao's save
                // record (AL_ClearChaoParam), and a resident who comes back
                // later is made again from that record.
                guest_call(c, m, crate::chao_player::FREE_TASK, &[task[n]]);
                (twp[n], task[n], leaving[n]) = (0, 0, 0);
            }
        }
    }
    RESIDENT_TWP.set(twp);
    RESIDENT_TASK.set(task);
    LEAVING.set(leaving);
}

/// Make the residents `which` that are in area `now`, by the entry point.
fn residents(c: &mut Cpu, m: &mut Mem, text: u32, now: u32, which: &[usize]) {
    let sp = c.r[1];
    c.r[1] = sp - 0x100 - PARAM_SIZE;
    let (param, pos) = (c.r[1] + 0x10, c.r[1] + 0x10 + PARAM_SIZE);
    // AL_SHAPE_ELEMENT (0x34 bytes, chao.h): CreateChaoExtra runs AL_GeneAnalyze
    // on a fresh gene, which resets param type to child, then fn_60_209C8 copies
    // the element's type (+0) over it; that is the type the shape init sees.
    let elem = pos + 0x10;
    let entry = ENTRY.get();
    let (mut twp, mut tasks) = (RESIDENT_TWP.get(), RESIDENT_TASK.get());
    let mut standing_task = 0;
    for (n, (ty, dx, dz)) in RESIDENTS.into_iter().enumerate() {
        if !which.contains(&n) || roam_area(n) != now {
            continue;
        }
        for i in (0..PARAM_SIZE).step_by(4) {
            m.w32(param + i, 0);
        }
        m.w8(param + 0x80, ty);
        // As chao_player's own Chao (Dark Chaos 0x16, drawn adult): marked
        // analysed (+0x438) with +0x84 = 0xFF, so the gene analysis does not
        // reset it to a child (the residents crawled as babies, mob).
        // Unverified beyond that: the meaning of +0x84.
        m.w8(param + 0x438, 1);
        m.w8(param + 0x84, 0xFF);
        for i in (0..0x34).step_by(4) {
            m.w32(elem + i, 0);
        }
        m.w8(elem, ty);
        m.w32(pos, (entry[0] + dx).to_bits());
        m.w32(pos + 4, entry[1].to_bits());
        m.w32(pos + 8, (entry[2] + dz).to_bits());
        let task = guest_call(c, m, text + CREATE_CHAO_EXTRA, &[param, 1, elem, pos, 0]);
        if task != 0 {
            twp[n] = m.r32(task + 0x34);
            tasks[n] = task;
            set_motion(n, 0);
            let mut pinned = PINNED.get();
            pinned[n] = false;
            PINNED.set(pinned);
            if n == STANDING {
                standing_task = task;
            }
        }
    }
    eprintln!("tutorial: residents' taskwks {twp:x?} (guide is [{GUIDE}])");
    RESIDENT_TWP.set(twp);
    RESIDENT_TASK.set(tasks);
    c.r[1] = sp;
    // Shared residents: the AI hook to hold them still on a game that only
    // draws them, and the motion hook to send what the leading game's AI plays.
    for (a, f) in [(AL_BEHAVIOR_CONTROL, crate::chao_player::behavior as sa2_recomp::rt::Func), (AL_SET_MOTION_LINK, resident_motion)] {
        if !c.rt.overrides.contains_key(&(text + a)) {
            c.rt.set_override(text + a, f);
        }
    }
    // Tails stands as the guide only by the lobby's arrival teleporter.
    if standing_task != 0 && now == LOBBY {
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

/// The residents never age or die (mob 2026-11-22), every frame. Their types
/// (0x17..0x19) are already outside the game's own life loss (AL_CalcParameter
/// 0x2149C: the LifeMax decay only for types under 0x14, as for Chaos Chao;
/// `old` still counts up) and evolution (0x1531C: a child, type 2, only). But the death
/// check (0x15060: a non-child with `life` 0 on the ground starts ALBHV_Dead)
/// has no type test, and a resident's record is made zeroed and never
/// analysed, so its life starts at 0. Held here: life and LifeMax (+0x8A,
/// +0x8C) at ChaoGlobal's InitBaseLife (+0x16; what AL_GeneAnalyzeCommon gives
/// a Chao whose LifeTime genes are 0, as theirs are), age and old (+0x86,
/// +0x88) at 0. Offsets: al_chao_info.h, al_global.h. Unverified live.
fn hold_residents(m: &mut Mem) {
    let Some(data) = chaomain_section(m, 5) else { return };
    let life = m.r16(data + FULL_GROWTH.0 + 0x16);
    for (n, twp) in RESIDENT_TWP.get().into_iter().enumerate() {
        // chaowk +0x5C is the record; only while it is still this resident's.
        let param = if twp == 0 { 0 } else { m.r32(twp + 0x5C) };
        if !(0x8000_0000..0x8C00_0000).contains(&param) || m.r8(param + 0x80) != RESIDENTS[n].0 {
            continue;
        }
        m.w16(param + 0x86, 0);
        m.w16(param + 0x88, 0);
        m.w16(param + 0x8A, life);
        m.w16(param + 0x8C, life);
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

/// True for the task or taskwk (AL_BehaviorControl's r3) of a resident that
/// runs no AI: the standing one, one walking out, and every one while
/// another player's game runs them (see share_residents).
pub(crate) fn standing_resident(m: &Mem, r3: u32) -> bool {
    let (tasks, leaving, drawn) = (RESIDENT_TASK.get(), LEAVING.get(), !LEAD.get());
    let out = (0..3).filter(|&n| drawn || leaving[n] != 0).map(|n| tasks[n]);
    std::iter::once(STANDING_TASK.get()).chain(out).any(|task| task != 0 && (r3 == task || r3 == m.r32(task + 0x34)))
}

// ---- Residents shared between players (MMO glue). ----
// The server names one player's game per area as the leader. That game runs
// the roaming residents with the game's own Chao AI and sends their poses;
// every other game holds their AI and places them from those poses.

/// This game runs the residents' AI. True with no server (the PC window).
static LEAD: Global<bool> = global!(true);
/// The leader's last pose of each resident: position, yaw, motion.
static SHARED: Global<[Option<([f32; 3], u32, u32)>; 3]> = global!([None; 3]);
/// The motion each resident last had set (0: none seen yet).
static MOTION: Global<[u32; 3]> = global!([0; 3]);
/// A drawn resident has been put on the leader's spot once (later it glides).
static PINNED: Global<[bool; 3]> = global!([false; 3]);
/// Frames between pose exchanges, as chao_player's own pose (about 10 a second).
const SHARE_EVERY: u64 = 6;
/// The part of the way to the leader's spot a drawn resident covers each
/// frame (an MMO choice: smooths the 10-a-second poses).
const GLIDE: f32 = 0.25;

fn set_motion(n: usize, motion: u32) {
    let mut all = MOTION.get();
    all[n] = motion;
    MOTION.set(all);
}

/// AL_SetMotionLink(task, motion): a resident's motion is kept for its pose.
/// shortcut: only AL_SetMotionLink is seen, so a motion the AI sets another way (AL_SetMotionLinkStep 0x1B4DC and others) is not sent; read the motion from the Chao's work once its offset is traced.
fn resident_motion(c: &mut Cpu, m: &mut Mem) {
    if let Some(n) = RESIDENT_TASK.get().iter().position(|&t| t != 0 && t == c.r[3]) {
        set_motion(n, c.r[4]);
    }
    let a = sa2_recomp::rt::sec(m, 60, 1) + AL_SET_MOTION_LINK;
    crate::chao_player::rel_original(c, m, a, resident_motion);
}

/// SA2_RESIDENT_IN: "lead 0|1", then one "n x y z yaw motion" line per
/// resident the leader has out. No file: this game leads.
fn parse_shared(text: Option<&str>) -> (bool, [Option<([f32; 3], u32, u32)>; 3]) {
    let mut poses = [None; 3];
    let Some(text) = text else { return (true, poses) };
    for line in text.lines() {
        let v: Vec<f32> = line.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        if let [n, x, y, z, yaw, motion] = v[..] {
            // Whole and not negative: a cast would turn -1 or 0.5 into resident 0.
            if let Some(p) = Some(n).filter(|n| *n >= 0.0 && n.fract() == 0.0).and_then(|n| poses.get_mut(n as usize)) {
                *p = Some(([x, y, z], yaw as u32 & 0xFFFF, motion as u32));
            }
        }
    }
    (text.lines().next() != Some("lead 0"), poses)
}

#[test]
fn shared_residents_lead_without_a_file_and_read_the_leaders_poses() {
    assert_eq!(parse_shared(None), (true, [None; 3]));
    assert_eq!(parse_shared(Some("lead 1\n")), (true, [None; 3]));
    let (lead, poses) = parse_shared(Some("lead 0\n2 1.5 0 -2 16384 578\n7 0 0 0 0 0\n-1 9 9 9 0 0\n0.5 9 9 9 0 0\nbad\n"));
    assert!(!lead);
    assert_eq!(poses, [None, None, Some(([1.5, 0.0, -2.0], 16384, 578))]);
}

/// Every frame: exchange the residents' poses with the page (SA2_RESIDENT_IN
/// and SA2_RESIDENT_OUT, as chao_player's pose files), and on a game that
/// does not lead, put each roaming resident where the leader has it.
/// Unverified live. The guide stands at the same spot in every game and is not sent.
fn share_residents(c: &mut Cpu, m: &mut Mem, text: u32) {
    let (twp, task) = (RESIDENT_TWP.get(), RESIDENT_TASK.get());
    let roaming = || (0..RESIDENTS.len()).filter(|&n| n != STANDING && task[n] != 0);
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % SHARE_EVERY == 0 {
        let path = std::env::var("SA2_RESIDENT_IN").ok();
        let file = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        // A server file that cannot be read this time (or has no word yet) changes nothing.
        let (lead, poses) = if path.is_some() && file.is_none() { (LEAD.get(), SHARED.get()) } else { parse_shared(file.as_deref()) };
        if lead != LEAD.get() {
            eprintln!("residents: this game {}", if lead { "leads" } else { "draws the leader's" });
            PINNED.set([false; 3]);
        }
        LEAD.set(lead);
        SHARED.set(poses);
        if let Ok(path) = std::env::var("SA2_RESIDENT_OUT") {
            let motion = MOTION.get();
            let lines: String = roaming()
                .map(|n| {
                    let g = |o: u32| f32::from_bits(m.r32(twp[n] + o));
                    format!("{n} {} {} {} {} {}\n", g(0x14), g(0x18), g(0x1C), m.r32(twp[n] + 0x0C) & 0xFFFF, motion[n])
                })
                .collect();
            let _ = std::fs::write(path, lines);
        }
    }
    if LEAD.get() {
        return;
    }
    let (shared, mut pinned) = (SHARED.get(), PINNED.get());
    for n in roaming() {
        let Some((to, yaw, motion)) = shared[n] else { continue };
        let k = if pinned[n] { GLIDE } else { 1.0 };
        pinned[n] = true;
        for (i, to) in to.into_iter().enumerate() {
            let at = twp[n] + 0x14 + 4 * i as u32;
            let from = f32::from_bits(m.r32(at));
            m.w32(at, (from + (to - from) * k).to_bits());
        }
        m.w32(twp[n] + 0x0C, yaw);
        if motion != 0 && motion != MOTION.get()[n] {
            guest_call(c, m, text + AL_SET_MOTION_LINK, &[task[n], motion]);
            set_motion(n, motion);
        }
    }
    PINNED.set(pinned);
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

/// A new player's run: `Some(0)` until the egg is laid, then its save slot.
/// The player has no Chao of their own before that egg hatches (chao_player).
pub(crate) fn hatch_slot() -> Option<u32> {
    match EGG_SLOT.get() {
        0 => HATCH.get().then_some(0),
        slot => Some(slot),
    }
}

/// The tutorial's evolution is under way (growth written, cocoon timers short).
pub(crate) fn evolving() -> bool {
    COCOON_RESTORE.get().0 != 0
}

/// Logs the new Chao's type (param +0x80) and growth when either changes:
/// egg, hatch, cocoon, adult.
fn watch_slot(m: &Mem) {
    let (slot, ty, growth) = SLOT_SEEN.get();
    if slot == 0 {
        return;
    }
    let now = (slot, m.r8(slot + 0x80), m.r32(slot + GROWTH));
    if now != (slot, ty, growth) {
        SLOT_SEEN.set(now);
        eprintln!("tutorial: slot type {} growth {}", now.1, f32::from_bits(now.2));
    }
}

/// Puts the cocoon's native phase timers back once the tutorial's evolution is over.
fn restore_cocoon(m: &mut Mem) {
    let (table, saved, left) = COCOON_RESTORE.get();
    if table == 0 {
        return;
    }
    if left > 0 {
        COCOON_RESTORE.set((table, saved, left - 1));
        return;
    }
    for (i, (o, _)) in COCOON_FRAMES.into_iter().enumerate() {
        m.w32(table + o, saved[i]);
    }
    eprintln!("tutorial: cocoon timers back to {saved:?}");
    COCOON_RESTORE.set((0, [0; 3], 0));
}

/// The tutorial ends when the player reaches the Tails Chao guide: the new
/// Chao's growth goes to full, and ChaoMain's own check evolves it.
fn tutorial(c: &mut Cpu, m: &mut Mem, text: u32) {
    let guide = RESIDENT_TWP.get()[GUIDE];
    // Nothing starts until the egg has hatched: the slot's type is 1 for the
    // egg (seen in a run's log) and 2 for the child the cocoon check wants.
    if guide == 0 || m.r8(TUTORIAL_SLOT.get() + 0x80) != CHILD {
        return;
    }
    // The player is the hidden character or, as its own Chao, that Chao: the nearer one counts.
    let o = guide + 0x14;
    let dist_sq = |twp: u32| {
        let d = |i| f32::from_bits(m.r32(twp + 0x14 + i)) - f32::from_bits(m.r32(o + i));
        d(0) * d(0) + d(8) * d(8)
    };
    let own = crate::chao_player::own_chao_twp(m);
    let mut near = dist_sq(m.r32(PLAYERTWP));
    if own != 0 {
        near = near.min(dist_sq(own));
    }
    let frames = LOG_FRAMES.get();
    LOG_FRAMES.set(frames + 1);
    if frames % 120 == 0 {
        eprintln!("tutorial: {:.1} from the guide (reach {:.1})", near.sqrt(), REACH_SQ.sqrt());
    }
    if near > REACH_SQ {
        return;
    }
    let (line, wait) = LINE.get();
    if wait > 0 {
        LINE.set((line, wait - 1));
        return;
    }
    if let Some(words) = GUIDE_LINES.get(line) {
        // shortcut: the window call crashed a run (bad address inside ChaoMain) and is untraced,
        // so the lines only show with SA2_TUTORIAL_TEXT=1; make it the default once the call is proven.
        if !std::env::var("SA2_TUTORIAL_TEXT").is_ok_and(|v| v.trim() == "1") {
            eprintln!("tutorial: line {line} skipped (SA2_TUTORIAL_TEXT is off)");
            LINE.set((line + 1, LINE_FRAMES));
            return;
        }
        let buf = guest_call(c, m, ALLOC, &[1, words.len() as u32 + 1]);
        eprintln!("tutorial: line {line} buffer {buf:#x}, calling the window");
        if buf != 0 {
            for (i, b) in words.bytes().chain([0]).enumerate() {
                m.w8(buf + i as u32, b);
            }
            let r = guest_call(c, m, text + AL_MESSAGE, &[buf]);
            eprintln!("tutorial: line {line} shown, window call returned {r:#x}");
        } else {
            eprintln!("tutorial: line {line} not shown, no memory for it");
        }
        LINE.set((line + 1, LINE_FRAMES));
        return;
    }
    let (lo, off) = FULL_GROWTH;
    let Some(bss) = chaomain_section(m, 5) else {
        return;
    };
    let table = bss + lo;
    m.w32(TUTORIAL_SLOT.get() + GROWTH, m.r32(table + off));
    // Shorten the cocoon's phase timers for this evolution, put back afterwards.
    let mut saved = [0u32; 3];
    for (i, (o, f)) in COCOON_FRAMES.into_iter().enumerate() {
        saved[i] = m.r32(table + o);
        m.w32(table + o, f);
    }
    eprintln!(
        "tutorial: growth set to {} (bits {:#x}); cocoon timers {saved:?} now 300 each",
        f32::from_bits(m.r32(table + off)),
        m.r32(table + off)
    );
    COCOON_RESTORE.set((table, saved, COCOON_HOLD));
    TUTORIAL_SLOT.set(0);
    LINE.set((0, 0));
}

/// The hub's character Chao make no Chao sounds: a positional sound whose
/// position is a resident's taskwk +0x14 is dropped; every other one plays.
fn quiet_residents(c: &mut Cpu, m: &mut Mem, addr: u32, pos_reg: usize) {
    let twp = c.r[pos_reg].wrapping_sub(0x14);
    // SA2_SE_LOG=1 (debug): every positional sound with its id, caller and
    // the task work it is placed at, to name a sound heard in a run.
    if std::env::var("SA2_SE_LOG").is_ok_and(|v| v == "1") {
        eprintln!("se: {addr:#x} id {:#x} lr {:#x} at twp {twp:#x} pmove {}", c.r[3], c.lr, crate::chao_player::character_sounds_muted());
    }
    if twp != 0 && (RESIDENT_TWP.get().contains(&twp) || twp == crate::chao_player::own_twp(m)) {
        return;
    }
    // The hidden character's own movement sounds (footsteps, skate jets)
    // while its exec moves the player's Chao.
    if crate::chao_player::character_sounds_muted() {
        return;
    }
    // Chao voices (the 0x60xx Chao bank, e.g. ChaoMain fn_60_4BA6C's random
    // 0x6014/0x6015/0x601C) are not played: Chao in the MMO are silent (mob).
    if crate::chao_player::chao_voice(c.r[3]) {
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
    fn only_hero_and_dark_lock_and_only_under_the_mmo() {
        assert!(garden_locked(2, Some("")) && garden_locked(3, Some("h")));
        assert!(!garden_locked(2, Some("h")) && !garden_locked(3, Some("hd")));
        assert!(!garden_locked(1, Some("")) && !garden_locked(6, Some("")));
        assert!(!garden_locked(2, None));
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
