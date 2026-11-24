//! Chao MMO glue: the Stage Select shows the stages the server has unlocked
//! for the player, and a cleared stage is reported for the server to record.
//!
//! The game's own Stage Select (advertiseD.rel, module 91) shows a row only
//! once the story save holds a rank for that stage's first mission: 0xEE7C
//! fills one shown flag per row (.bss 0xD3E8 + 0x14, 33 words) from the save
//! records at 0x801F2340 (0xD854 picks the record), and the draw loop 0xD51C
//! reads the flags. Story mode is what leads to an uncleared stage, and the
//! MMO has none, so 0xEE7C's record lookups are answered from the server's
//! list. The save itself is left as the game wrote it.
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
/// advertiseD section 1: picks a row's save record; the row fill (0xEE7C, which
/// then moves the cursor off a hidden row) calls it at 0xEF38.
const PICK_RECORD: u32 = 0xD854;
const FILL_RETURN: u32 = 0xEF3C;
/// advertiseD .data (section 5): the rows, 16 bytes each (stage, character,
/// column, row); 0xEE7C and 0xD51C loop over 33 of them.
const ROWS: u32 = 0x6E64;
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

/// The server's list as read for this pass of the row fill (None: no list).
static LIST: Global<Option<Vec<(u8, u8)>>> = global!(None);

/// advertiseD 0xD854(out, stage, row): picks a row's save record. For the row
/// fill (its call at 0xEF38) a row's flag is the first byte at out[1] (0xEF54);
/// the answer is pointed at a byte that says what the server's list says, so
/// the game's own pass fills the flags, keeps the cursor on a shown row and
/// moves the map itself. Writing the flags after the pass (as before) left the
/// cursor check to the save: the MMO's save has no clears, so every frame the
/// cursor went back to Iron Gate (0xF0C8) and no other stage could be picked.
fn pick_record(c: &mut Cpu, m: &mut Mem) {
    let text = HOOKED.get();
    let (out, stage, row, fill) = (c.r[3], c.r[4], c.r[5], c.lr == text + FILL_RETURN);
    rel_original(c, m, text + PICK_RECORD, pick_record);
    if !fill {
        return;
    }
    if row == 0 {
        // No list (not signed in yet, file missing): the game's own flags stay.
        LIST.set(unlocked());
    }
    let Some(data) = module_section(m, ADVERTISE, 5) else { return };
    let character = m.r32(data + ROWS + row * 16 + 4);
    if let Some(show) = LIST.with_borrow(|l| l.as_ref().and_then(|l| shown(stage, character, l))) {
        // out[0] is the record kind, 0 or 1: its first byte is 0. out[1] is
        // an address in RAM: its first byte is 0x80.
        m.w32(out + 4, if show { out + 4 } else { out });
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
        c.rt.overrides.remove(&(old + PICK_RECORD));
    }
    if text != 0 {
        c.rt.set_override(text + PICK_RECORD, pick_record);
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
