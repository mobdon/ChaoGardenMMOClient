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

/// (stage, character) to start, until it has started.
static PENDING: Global<Option<(u16, u32)>> = global!(None);
/// SA2_CHAO_AREA: the Chao World area to change to once the lobby is up.
static AREA: Global<Option<u32>> = global!(None);

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
        PENDING.set(Some((CHAO_WORLD, 0)));
        AREA.set(
            std::env::var("SA2_CHAO_AREA")
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|a| (1..=LAST_AREA).contains(a)),
        );
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
    if m.r32(GLOBAL_MODE) == MD_TITLE_INIT {
        if let Some((stage, ch)) = PENDING.take() {
            start(c, m, stage, ch);
        }
    }
    if let Some(area) = AREA.get() {
        if let Some(text) = chaomain_text(m) {
            // Once the lobby (area 0) is running, change area the game's own way.
            if guest_call(c, m, text + AL_GET_STAGE_NUMBER, &[]) == 0 {
                guest_call(c, m, text + AL_CHANGE_AREA, &[area]);
                AREA.set(None);
            }
        }
    }
    original(c, m, TOP_LEVEL);
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

/// ChaoMain's linked .text (section 1), through the game's module list
/// (OSModuleInfo: id +0, next +4, section table +0x10). None until linked.
fn chaomain_text(m: &Mem) -> Option<u32> {
    let mut info = m.r32(0x8000_30C8);
    while info != 0 {
        if m.r32(info) == CHAOMAIN {
            return Some(m.r32(m.r32(info + 0x10) + 8) & !1);
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
