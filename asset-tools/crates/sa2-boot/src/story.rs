//! Chao MMO glue: the Stage Select shows the stages the server has unlocked
//! for the player, and a cleared stage is reported for the server to record.
//!
//! The game's own Stage Select (advertiseD.rel, module 91) shows a row only
//! once the story save holds a rank for that stage's first mission: 0xEE7C
//! fills one shown flag per row (.bss 0xD3E8 + 0x14, 33 words) from the save
//! records at 0x801F2340 (0xD854 picks the record), and the draw loop 0xD51C
//! reads the flags. Story mode is what leads to an uncleared stage, and the
//! MMO has none, so after 0xEE7C the flags are set from the server's list.
//! The save itself is left as the game wrote it.
//!
//! `SA2_STORY_IN` names a file of (stage, character) byte pairs, the stages
//! the server lets this player start. `SA2_STORY_OUT` names a file that gets
//! [count, stage, character] each time a stage's first mission is cleared.
//! The page (or a desktop launcher) carries both to and from the server.
use crate::chao_player::rel_original;
use crate::original;
use crate::start_stage::module_section;
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

const ADVERTISE: u32 = 91;
/// advertiseD section 1: fills the rows' shown flags, then moves the cursor
/// off a hidden row.
const FILL_SHOWN: u32 = 0xEE7C;
/// advertiseD .data (section 5): the rows, 16 bytes each (stage, character,
/// column, row); 0xEE7C and 0xD51C loop over 33 of them.
const ROWS: u32 = 0x6E64;
const ROW_COUNT: u32 = 33;
/// advertiseD .bss (section 6): the shown flags, one u32 per row.
const SHOWN: u32 = 0xD3E8 + 0x14;
/// advertiseD .bss: the Stage Select's work. +0x68 the cursor's row, +0x20 a
/// byte 0xEE7C sets with it.
const WORK: u32 = 0xD488;
/// City Escape's row and the +0x20 value 0xEE7C sets when it puts the cursor
/// there (0xF0A4). City Escape is unlocked for everyone.
const FIRST_ROW: (u8, u8) = (0x17, 0);
/// Rows left as the game set them: Chao World (0x5A) and Green Hill (0x1F).
const NATIVE_ROWS: [u32; 2] = [0x5A, 0x1F];

/// main.dol: saves a cleared stage's result into the story save (rank + 1 at
/// record + mission, the best scores and times). Returns 1 when it saved,
/// after writing the stage to save + 0x10 and the character to save + 0xF
/// (0x8005AB2C..0x8005AB48).
const SAVE_RESULT: u32 = 0x8005_A650;
const SAVE: u32 = 0x801F_2340;
/// The mission being played (0 = the first), read by 0x8005A650 at 0x8005A6EC.
const MISSION: u32 = 0x801C_C168 + 0x23;

static HOOKED: Global<u32> = global!(0);
static CLEARS: Global<u8> = global!(0);
static SAVE_HOOKED: Global<bool> = global!(false);

fn unlocked() -> Option<Vec<(u8, u8)>> {
    let bytes = std::fs::read(std::env::var("SA2_STORY_IN").ok()?).ok()?;
    Some(bytes.as_chunks::<2>().0.iter().map(|p| (p[0], p[1])).collect())
}

/// Whether a row is shown: None leaves the game's own flag.
fn shown(stage: u32, character: u32, unlocked: &[(u8, u8)]) -> Option<bool> {
    if NATIVE_ROWS.contains(&stage) {
        return None;
    }
    Some(unlocked.iter().any(|&(s, c)| (u32::from(s), u32::from(c)) == (stage, character)))
}

fn fill_shown(c: &mut Cpu, m: &mut Mem) {
    rel_original(c, m, HOOKED.get() + FILL_SHOWN, fill_shown);
    // No list (not signed in yet, file missing): the game's own flags stay.
    let Some(unlocked) = unlocked() else { return };
    let (Some(data), Some(bss)) = (module_section(m, ADVERTISE, 5), module_section(m, ADVERTISE, 6))
    else {
        return;
    };
    for i in 0..ROW_COUNT {
        let row = data + ROWS + i * 16;
        if let Some(show) = shown(m.r32(row), m.r32(row + 4), &unlocked) {
            m.w32(bss + SHOWN + i * 4, u32::from(show));
        }
    }
    // As 0xEE7C does for a cursor on a hidden row.
    let cursor = u32::from(m.r8(bss + WORK + 0x68));
    if cursor < ROW_COUNT && m.r32(bss + SHOWN + cursor * 4) == 0 {
        m.w8(bss + WORK + 0x68, FIRST_ROW.0);
        m.w8(bss + WORK + 0x20, FIRST_ROW.1);
    }
}

fn save_result(c: &mut Cpu, m: &mut Mem) {
    let mission = m.r8(MISSION);
    original(c, m, SAVE_RESULT);
    if c.r[3] != 1 || mission != 0 {
        return;
    }
    if let Ok(path) = std::env::var("SA2_STORY_OUT") {
        CLEARS.set(CLEARS.get().wrapping_add(1));
        let _ = std::fs::write(path, [CLEARS.get(), m.r8(SAVE + 0x10), m.r8(SAVE + 0xF)]);
    }
}

/// Called every top-level frame: keeps the hook on the Stage Select while
/// its module is linked (a REL's address is only known then).
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    if std::env::var_os("SA2_STORY_IN").is_none() {
        return;
    }
    if !SAVE_HOOKED.get() {
        SAVE_HOOKED.set(true);
        c.rt.set_override(SAVE_RESULT, save_result);
    }
    let text = module_section(m, ADVERTISE, 1).unwrap_or(0);
    let old = HOOKED.get();
    if text == old {
        return;
    }
    if old != 0 {
        c.rt.overrides.remove(&(old + FILL_SHOWN));
    }
    if text != 0 {
        c.rt.set_override(text + FILL_SHOWN, fill_shown);
    }
    c.rt.modules_changed();
    HOOKED.set(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_server_list_and_the_two_routes_differ() {
        let unlocked = [(13, 0), (70, 6)];
        assert_eq!(shown(13, 0, &unlocked), Some(true));
        assert_eq!(shown(16, 4, &unlocked), Some(false));
        assert_eq!(shown(70, 6, &unlocked), Some(true)); // Route 101
        assert_eq!(shown(70, 5, &unlocked), Some(false)); // Route 280
    }

    #[test]
    fn chao_world_and_green_hill_stay_the_games() {
        assert_eq!(shown(0x5A, 0, &[]), None);
        assert_eq!(shown(0x1F, 0, &[]), None);
    }
}
