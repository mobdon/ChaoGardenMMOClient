//! Chao MMO glue: the pause menu's EXTRAS row and pages, and the game's own
//! option bytes set from the player's server-saved settings.
//!
//! In Chao World the game's pause shows two rows (CONTINUE, EXIT: layout mode 2
//! skips RESTART). Here it uses its own three-row layout (mode 3) and the
//! middle row reads EXTRAS. Picking it turns the same window into the Extras
//! pages: three rows a page, drawn and moved through by the game's own pause
//! code (window, cursor, highlight, sounds). Left/right change the row's value,
//! L/R change the page, B or Start goes back. Values come from the page
//! (`/dev/pad` bytes 12-15, [`set_page_settings`]); a change goes back to it on
//! `/dev/extras`, and the page saves it on the MMO server.
//! In a level (three rows already: CONTINUE, RESTART, EXIT) the same window
//! scrolls through four rows, EXTRAS between RESTART and EXIT.
//! EXIT goes back to the lobby instead of leaving Chao World (see [`exit_stays_in_chao_world`]).
//! Addresses: GSNE8P main.dol, traced in fn_8001E12C / fn_8001B404 /
//! fn_8001F368 (see the pause trace notes).

use crate::{guest_call, original};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

/// fn_8001E12C: moves the pause cursor (wraps at 3 rows, mode 2 skips row 1)
/// and draws the window and rows. Called only by the pause executor.
const ROWS: u32 = 0x8001_E12C;
/// fn_8001B404(text id, row, message): one row; text id 2/3/4 is slot 0/1/2,
/// highlighted when the slot is the cursor.
const ROW_DRAW: u32 = 0x8001_B404;
/// fn_8001F368: the pause executor (calls ROWS, then confirms the cursor's row).
const PAUSE_EXEC: u32 = 0x8001_F368;
/// fn_800DEA3C(text, language): a font message, as fn_80021828 makes the rows.
const MAKE_MESSAGE: u32 = 0x800D_EA3C;
/// main.dol allocator (r3=1, r4=size), as start_stage keeps guest strings.
const ALLOC: u32 = 0x8011_97FC;
/// fn_8006B978(id, 0, 0, 0): menu sounds as the pause plays them: 0x8000
/// cursor move (fn_8001E12C), 0x8002 confirm (fn_8001F368).
const MENU_SE: u32 = 0x8006_B978;
const SE_MOVE: u32 = 0x8000;
const SE_CONFIRM: u32 = 0x8002;
/// BGM_SetVolume(v) (symbols.txt): v <= 0 in the ADX volume scale; -999 is the
/// silent value the function itself uses (0x80011A38).
const BGM_SET_VOLUME: u32 = 0x8001_1A04;
const BGM_PLAY: u32 = 0x8001_2218;
const BGM_SILENT: u32 = -999i32 as u32;
/// fn_8006C944: applies the sound mode byte (OSSetSoundMode/MIXSetSoundMode),
/// as the options menu calls it after a change (advertiseD 0x2861C).
const APPLY_SOUND_MODE: u32 = 0x8006_C944;

/// Game state: +0x10 sound mode (1 mono), +0x11 text language, +0x12 voice
/// language (0 Japanese), +0x14 rumble gate (1 stops the rumble task; which
/// value the options menu calls On is unverified), +0x17 the pausing player.
const GAME: u32 = 0x801C_C168;
const SOUND_MODE: u32 = GAME + 0x10;
const TEXT_LANGUAGE: u32 = GAME + 0x11;
const VOICE_LANGUAGE: u32 = GAME + 0x12;
const RUMBLE_OFF: u32 = GAME + 0x14;
const PAUSE_PLAYER: u32 = GAME + 0x17;
/// Per-controller state pointers (fn_8001F368 0x8001FD08): +0x10 buttons pressed.
const PADS: u32 = 0x801E_55D8;
/// Per-player buttons pressed this frame, as fn_8001E12C reads them to move
/// the cursor (0x8001E218 up, 0x8001E2B0 down); held ones 0x10 further on.
const PRESSED: u32 = 0x801E_5398;
const HELD: u32 = PRESSED + 0x10;
const UP: u32 = 0x10;
const DOWN: u32 = 0x20;

/// Small-data globals (r13-relative), from the traced functions.
const STAGE: u32 = 0x7AA0; // i16 stage number; 0x5A is Chao World
const STATE: u32 = 0x7AA6; // i16 game state; 0x11 paused
const CURSOR: u32 = 0x7A89; // u8 pause row
const TEXT_TABLE: u32 = 0x786C; // pointer to the loaded pause text (fn_80021828)
const MODE: u32 = 0x7A88; // u8 pause layout: 2 = no RESTART, 3 = three rows
const CONFIRM_MASK: u32 = 0x7F40; // u32 buttons that confirm
const CHAO_WORLD: u16 = 0x5A;
const PAUSED: u16 = 0x11;
/// Text ids of the three pause rows (fn_8001E12C).
const ROW_TEXT: [u32; 3] = [2, 3, 4];
/// The row messages the stage start made (fn_80021828): CONTINUE, RESTART, EXIT.
const ROW_MESSAGES: [u32; 3] = [0x7A80, 0x7A7C, 0x7A78];
const REPEAT: u32 = 0x7A74; // i16 cursor hold counter; the cursor moves at 0
/// A level's four rows: CONTINUE, RESTART, EXTRAS, EXIT.
const LEVEL_EXTRAS: u8 = 2;

/// GC pad bits (the page's controller 1).
const PAD_LEFT: u16 = 0x0001;
const PAD_RIGHT: u16 = 0x0002;
const PAD_R: u16 = 0x0020;
const PAD_L: u16 = 0x0040;
const PAD_A: u16 = 0x0100;
const PAD_B: u16 = 0x0200;
const PAD_START: u16 = 0x1000;

/// The Extras settings, in page order (three rows a page). Labels and values
/// are MMO text in the game's font; index order matches the page's byte codes.
const SETTINGS: [(&str, &[&str]); 9] = [
    ("VOICE", &["ENGLISH", "JAPANESE"]),
    ("AUDIO", &["STEREO", "MONO"]),
    ("VIBRATION", &["ON", "OFF"]),
    // Steam options.pak opt_theme_{def,sn,sd,tl,kn,rg,am,eg,ma,ch,se}, named from
    // their icons; "CHAO" (ch) and "SECRETARY" (se) are unverified names.
    (
        "MENU THEME",
        &[
            "DEFAULT",
            "SONIC",
            "SHADOW",
            "TAILS",
            "KNUCKLES",
            "ROUGE",
            "AMY",
            "EGGMAN",
            "MARIA",
            "CHAO",
            "SECRETARY",
        ],
    ),
    ("MIC", &["HOLD V", "OPEN"]),
    ("CHAT", &["ON", "OFF"]),
    (
        "VOLUME",
        &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    ),
    ("MUSIC", &["ON", "OFF"]),
    ("FRAME CAP", &["60", "120", "165"]),
];
const PAGES: usize = SETTINGS.len() / 3;
const EXTRAS: &str = "EXTRAS";

/// Current value index of each setting, as the page last sent them.
static VALUES: Global<[u8; 9]> = global!([0, 0, 0, 0, 0, 0, 10, 0, 0]);
/// Whether the page has sent the player's settings yet.
static HAVE: Global<bool> = global!(false);
static APPLIED: Global<Option<[u8; 9]>> = global!(None);
/// Extras page open (Some(page)) in the pause window.
static PAGE: Global<Option<usize>> = global!(None);
/// A level's pause: the first of its four rows the window shows (0 or 1).
static TOP: Global<u8> = global!(0);
/// Controller 1 buttons and stick x last frame, for presses.
static INPUT: Global<(u16, i8)> = global!((0, 0));
static LAST_INPUT: Global<(u16, i8)> = global!((0, 0));
/// Font messages made so far: (text, language, message).
static MESSAGES: std::sync::Mutex<Vec<(Vec<u8>, u8, u32)>> = std::sync::Mutex::new(Vec::new());

/// `/dev/pad` bytes 12-15 from the page: 12 flags (bit 0 settings loaded,
/// 1 mono, 2 Japanese voices, 3 vibration off, 4 music off, 5 chat off,
/// 6 open mic), 13 volume 0-10, 14 frame cap 0/1/2 (60/120/165), 15 theme 0-10.
pub fn set_page_settings(b: [u8; 4]) {
    if b[0] & 1 == 0 {
        return;
    }
    let bit = |n: u8| (b[0] >> n) & 1;
    VALUES.set([
        bit(2),
        bit(1),
        bit(3),
        b[3].min(10),
        bit(6),
        bit(5),
        b[1].min(10),
        bit(4),
        b[2].min(2),
    ]);
    HAVE.set(true);
}

/// Controller 1 as the page sends it this frame.
pub fn set_input(buttons: u16, stick_x: i8) {
    INPUT.set((buttons, stick_x));
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(ROWS, rows);
    c.rt.set_override(ROW_DRAW, row_draw);
    c.rt.set_override(PAUSE_EXEC, pause_exec);
    c.rt.set_override(BGM_SET_VOLUME, bgm_set_volume);
    c.rt.set_override(BGM_PLAY, |c, m| {
        original(c, m, BGM_PLAY);
        if music_off() {
            guest_call(c, m, BGM_SET_VOLUME, &[BGM_SILENT]);
        }
    });
}

fn sda(c: &Cpu, off: u32) -> u32 {
    c.r[13].wrapping_sub(off)
}

fn in_chao_world(c: &Cpu, m: &Mem) -> bool {
    m.r16(sda(c, STAGE)) == CHAO_WORLD
}

fn rows(c: &mut Cpu, m: &mut Mem) {
    if in_chao_world(c, m) && m.r8(sda(c, MODE)) == 2 {
        m.w8(sda(c, MODE), 3);
    }
    original(c, m, ROWS);
}

/// The text a pause row shows: the Extras page's rows, or EXTRAS in
/// RESTART's place in Chao World; None keeps the game's own row.
fn row_text(page: Option<usize>, text_id: u32, values: &[u8; 9]) -> Option<String> {
    let slot = ROW_TEXT.iter().position(|&t| t == text_id)?;
    match page {
        Some(page) => {
            let i = page * 3 + slot;
            let (label, choices) = SETTINGS[i];
            Some(format!(
                "{label}  {}",
                choices[usize::from(values[i]).min(choices.len() - 1)]
            ))
        }
        None => (slot == 1).then(|| EXTRAS.to_owned()),
    }
}

fn row_draw(c: &mut Cpu, m: &mut Mem) {
    if in_chao_world(c, m) {
        let text = match row_text(PAGE.get(), c.r[3], &VALUES.get()) {
            Some(text) => Some(text.into_bytes()),
            // The Chao World pause never shows the rows the stage start made
            // (headless test: only EXTRAS drew), so its CONTINUE/EXIT are made
            // here too, from the game's own pause text.
            None => game_row_text(c, m, c.r[3]),
        };
        if let Some(message) = text.and_then(|text| message(c, m, &text)) {
            c.r[5] = message;
        }
    } else if in_level(c, m) {
        let slot = ROW_TEXT.iter().position(|&t| t == c.r[3]);
        let message = match (PAGE.get(), slot) {
            (_, None) => None,
            (Some(_), _) => row_text(PAGE.get(), c.r[3], &VALUES.get())
                .and_then(|text| message(c, m, text.as_bytes())),
            (None, Some(slot)) => match TOP.get() + slot as u8 {
                LEVEL_EXTRAS => message(c, m, EXTRAS.as_bytes()),
                row => {
                    let game_row = usize::from(row - u8::from(row > LEVEL_EXTRAS));
                    Some(m.r32(sda(c, ROW_MESSAGES[game_row])))
                }
            },
        };
        if let Some(message) = message {
            c.r[5] = message;
        }
    }
    original(c, m, ROW_DRAW);
}

/// A level's pause with all three of the game's rows (layout mode 3).
fn in_level(c: &Cpu, m: &Mem) -> bool {
    !in_chao_world(c, m) && m.r16(sda(c, STATE)) == PAUSED && m.r8(sda(c, MODE)) == 3
}

/// One cursor step through a level's four rows in the three-row window:
/// the new first row, and whether the step only scrolls (the cursor stays).
/// Past either end the game's own wrap moves the cursor and the window follows.
fn level_step(top: u8, cursor: u8, down: bool) -> (u8, bool) {
    match (top, cursor, down) {
        (0, 2, true) => (1, true),
        (1, 0, false) => (0, true),
        (1, 2, true) => (0, false),
        (0, 0, false) => (1, false),
        _ => (top, false),
    }
}

/// The loaded pause text (r13-0x786C, offsets at +4*id, as fn_80021828 reads
/// text ids 2/3/4); the English rows when it isn't loaded.
fn game_row_text(c: &Cpu, m: &Mem, text_id: u32) -> Option<Vec<u8>> {
    let fallback = match text_id {
        2 => "CONTINUE",
        4 => "EXIT",
        _ => return None,
    };
    let table = m.r32(sda(c, TEXT_TABLE));
    if table >> 24 == 0x80 {
        let start = table.wrapping_add(m.r32(table + 4 * text_id));
        let text: Vec<u8> = (start..start + 64)
            .map(|a| m.r8(a))
            .take_while(|&b| b != 0)
            .collect();
        if start >> 24 == 0x80 && !text.is_empty() && text.len() < 64 {
            return Some(text);
        }
    }
    Some(fallback.as_bytes().to_vec())
}

fn message(c: &mut Cpu, m: &mut Mem, bytes: &[u8]) -> Option<u32> {
    let language = m.r8(TEXT_LANGUAGE);
    let mut made = MESSAGES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(_, _, message)) = made.iter().find(|(t, l, _)| t == bytes && *l == language) {
        return Some(message);
    }
    let guest = guest_call(c, m, ALLOC, &[1, bytes.len() as u32 + 1]);
    if guest == 0 {
        return None;
    }
    for (i, b) in bytes.iter().chain(&[0]).enumerate() {
        m.w8(guest + i as u32, *b);
    }
    let message = guest_call(c, m, MAKE_MESSAGE, &[guest, u32::from(language)]);
    if message == 0 {
        return None;
    }
    // shortcut: messages are kept for the session (a few dozen short strings), never freed.
    made.push((bytes.to_vec(), language, message));
    Some(message)
}

fn pause_exec(c: &mut Cpu, m: &mut Mem) {
    apply(c, m);
    let (now, stick) = INPUT.get();
    let (before, stick_before) = LAST_INPUT.get();
    LAST_INPUT.set((now, stick));
    let pressed = now & !before;
    let left = pressed & PAD_LEFT != 0 || (stick < -64 && stick_before >= -64);
    let right = pressed & PAD_RIGHT != 0 || (stick > 64 && stick_before <= 64);
    let level = in_level(c, m);
    let paused = level || (in_chao_world(c, m) && m.r16(sda(c, STATE)) == PAUSED);
    if !paused {
        PAGE.set(None);
        TOP.set(0);
        original(c, m, PAUSE_EXEC);
        return;
    }
    if let Some(page) = PAGE.get() {
        if pressed & (PAD_B | PAD_START) != 0 {
            PAGE.set(None);
            // Back on EXTRAS: the middle row (a level's window scrolled down).
            TOP.set(1);
            m.w8(sda(c, CURSOR), 1);
            guest_call(c, m, MENU_SE, &[SE_MOVE, 0, 0, 0]);
        } else if pressed & (PAD_L | PAD_R) != 0 {
            let next = if pressed & PAD_R != 0 {
                page + 1
            } else {
                page + PAGES - 1
            };
            PAGE.set(Some(next % PAGES));
            guest_call(c, m, MENU_SE, &[SE_MOVE, 0, 0, 0]);
        } else if left || right || pressed & PAD_A != 0 {
            let i = page * 3 + usize::from(m.r8(sda(c, CURSOR)).min(2));
            let n = SETTINGS[i].1.len() as u8;
            let mut values = VALUES.get();
            values[i] = if left {
                (values[i] + n - 1) % n
            } else {
                (values[i] + 1) % n
            };
            VALUES.set(values);
            tell_page(i as u8, values[i]);
            guest_call(c, m, MENU_SE, &[SE_MOVE, 0, 0, 0]);
        }
        // The game's own window, cursor and highlight; its confirm/cancel stay out.
        original(c, m, ROWS);
        return;
    }
    let player = u32::from(m.r8(PAUSE_PLAYER).clamp(1, 2)) - 1;
    let pad = m.r32(PADS + 4 * player);
    let mask = m.r32(sda(c, CONFIRM_MASK));
    let cursor = m.r8(sda(c, CURSOR));
    if level {
        // The cursor moves as fn_8001E12C will move it (0x8001E1B4..0x8001E1FC).
        let pressed = PRESSED + 4 * player;
        let held = m.r32(HELD + 4 * player) & 0xF0 != 0;
        let repeat = m.r16(sda(c, REPEAT)) as i16;
        let moves = !held || !(1..0xF0).contains(&repeat);
        let (up, down) = (m.r32(pressed) & UP != 0, m.r32(pressed) & DOWN != 0);
        if moves && (up || down) {
            // Up wins, as in the game's own order.
            let (top, scrolls) = level_step(TOP.get(), cursor, !up);
            TOP.set(top);
            if scrolls {
                m.w32(pressed, m.r32(pressed) & !(UP | DOWN));
                guest_call(c, m, MENU_SE, &[SE_MOVE, 0, 0, 0]);
            }
        }
    }
    let extras_row = if level { LEVEL_EXTRAS - TOP.get() } else { 1 };
    if pad != 0 && m.r32(pad + 0x10) & mask != 0 {
        if level && TOP.get() == 1 && cursor == 0 {
            // RESTART scrolled to the top row: the game's own row 1.
            m.w8(sda(c, CURSOR), 1);
        } else if cursor == extras_row {
            // Used up here, so the game's restart never sees it.
            m.w32(pad + 0x10, m.r32(pad + 0x10) & !mask);
            guest_call(c, m, MENU_SE, &[SE_CONFIRM, 0, 0, 0]);
            PAGE.set(Some(0));
            m.w8(sda(c, CURSOR), 0);
            original(c, m, ROWS);
            return;
        }
        let area = crate::start_stage::chao_area(c, m);
        if cursor == 2 && exit_stays_in_chao_world(area) {
            // The game's own CONTINUE closes the pause; the lobby follows.
            if area != Some(crate::start_stage::LOBBY) {
                crate::start_stage::go_lobby();
            }
            m.w8(sda(c, CURSOR), 0);
        }
    }
    original(c, m, PAUSE_EXEC);
}

/// EXIT in Chao World (MMO): from an area it goes back to the lobby, in the
/// lobby it only closes the pause. The game's own EXIT stays for the race
/// (area 4: back to the race entrance, fn_8001F368 0x8001FE58..0x8001FE88)
/// and when the game was not started in Chao World (area unknown).
fn exit_stays_in_chao_world(area: Option<u32>) -> bool {
    // 0: Chao World is still loading.
    area.is_some_and(|a| a != 4 && a != 0)
}

/// The page's `/dev/extras` (sa2-web worker.js): setting `i` changed to `v`.
fn tell_page(i: u8, v: u8) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/extras") {
        let _ = f.write_all(&[i, v]);
    }
}

fn music_off() -> bool {
    HAVE.get() && VALUES.get()[7] == 1
}

fn bgm_set_volume(c: &mut Cpu, m: &mut Mem) {
    if music_off() {
        c.r[3] = BGM_SILENT;
    }
    original(c, m, BGM_SET_VOLUME);
}

/// Each frame (sa2-web calls this): write the settings into the game's own
/// option bytes when they change, and let the game apply them its own way.
pub fn apply(c: &mut Cpu, m: &mut Mem) {
    if !HAVE.get() {
        return;
    }
    let values = VALUES.get();
    let was = APPLIED.get();
    if was == Some(values) {
        return;
    }
    APPLIED.set(Some(values));
    let mono = values[1];
    if m.r8(SOUND_MODE) != mono {
        m.w8(SOUND_MODE, mono);
        guest_call(c, m, APPLY_SOUND_MODE, &[]);
    }
    // fn_80012A7C reloads the event voices when this byte changes (0 Japanese).
    m.w8(VOICE_LANGUAGE, u8::from(values[0] == 0));
    m.w8(RUMBLE_OFF, values[2]);
    if was.is_none_or(|w| w[7] != values[7]) {
        // Music off: the game's silent value; back on: full volume (0).
        let v = if music_off() { BGM_SILENT } else { 0 };
        guest_call(c, m, BGM_SET_VOLUME, &[v]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_bytes_become_setting_values() {
        set_page_settings([0b111_1111, 7, 2, 4]);
        assert_eq!(VALUES.get(), [1, 1, 1, 4, 1, 1, 7, 1, 2]);
        assert!(music_off());
        set_page_settings([0b000_0001, 99, 9, 99]);
        assert_eq!(VALUES.get(), [0, 0, 0, 10, 0, 0, 10, 0, 2]);
        assert!(!music_off());
    }

    #[test]
    fn exit_goes_to_the_lobby_except_from_the_race() {
        assert!(exit_stays_in_chao_world(Some(1)), "garden: to the lobby");
        assert!(exit_stays_in_chao_world(Some(7)), "lobby: close the pause");
        assert!(
            !exit_stays_in_chao_world(Some(0)),
            "loading: the game's own"
        );
        assert!(!exit_stays_in_chao_world(Some(4)), "race: the game's own");
        assert!(!exit_stays_in_chao_world(None), "not started in Chao World");
    }

    #[test]
    fn a_levels_four_rows_scroll_through_the_window() {
        // Down from CONTINUE to EXIT and round: (top, cursor) after each step.
        let (mut top, mut cursor) = (0u8, 0u8);
        let mut seen = vec![];
        for _ in 0..4 {
            let (t, scrolls) = level_step(top, cursor, true);
            top = t;
            if !scrolls {
                cursor = (cursor + 1) % 3;
            }
            seen.push(top + cursor);
        }
        assert_eq!(seen, [1, 2, 3, 0], "RESTART, EXTRAS, EXIT, CONTINUE");
        for _ in 0..4 {
            let (t, scrolls) = level_step(top, cursor, false);
            top = t;
            if !scrolls {
                cursor = (cursor + 2) % 3;
            }
            seen.push(top + cursor);
        }
        assert_eq!(seen[4..], [3, 2, 1, 0], "and back up");
    }

    #[test]
    fn pause_rows_in_chao_world_and_on_extras_pages() {
        let values = [0, 1, 0, 0, 1, 0, 10, 0, 2];
        assert_eq!(row_text(None, 3, &values).as_deref(), Some("EXTRAS"));
        assert_eq!(
            row_text(None, 2, &values),
            None,
            "CONTINUE stays the game's"
        );
        assert_eq!(
            row_text(Some(0), 3, &values).as_deref(),
            Some("AUDIO  MONO")
        );
        assert_eq!(
            row_text(Some(2), 4, &values).as_deref(),
            Some("FRAME CAP  165")
        );
        assert_eq!(row_text(Some(1), 3, &values).as_deref(), Some("MIC  OPEN"));
        assert_eq!(row_text(Some(0), 9, &values), None, "not a pause row");
    }
}
