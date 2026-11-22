//! Chao World chart on Tab: the Kindergarten Doctor's own medical chart,
//! opened for the player's Chao anywhere in Chao World.
//!
//! The chart is the game's own code in ChaoStgKinder (module 74, GSNE8P);
//! this module only loads it and calls it in an order no area does. Trace:
//! /mnt/project-files/native-menus/TAB-CHART-TRACE.md (static reading).
//! - Outside the Kindergarten the module isn't linked: ChaoStgKinder.prs is
//!   read from the player's files, expanded with the game's PRS expander
//!   0x800D2684 and linked with OSLink 0x800E839C into memory chao_player.rs
//!   only uses in stages. Its prolog is not run (it would create the
//!   Kindergarten rooms); its chart art is loaded with its own A10.
//! - The chart state is the Doctor room's 0x68-byte work (B1C8 allocates it
//!   with syCalloc 0x801197FC): +8 the Chao task, +0xC ChaoMain 15E44(chao),
//!   +0x60 the "MsgAlKinderHe" messages (ChaoMain 1CD20, rodata 0x2974).
//!   D208 opens it on the first (blue) page; C3D8 is its own input (L/R page,
//!   B closes); C3C4 says it closed; B244 is its display (task+0x40 = work).
//! - A task (CreateElementalTask 0x80071E0C) carries it: its main is a host
//!   function at a synthetic address (no guest code there), its second
//!   display (+0x1C) is the module's own B244, as the Doctor room sets it.
//! - Closing (or the game dropping the task on an area change) runs the
//!   task's delete (+0x18, also a host function), which frees what the Doctor
//!   room's own delete (BC18) frees: D238 (the
//!   portrait Chao), syFree(work+0x60), then the work; the borrowed module's
//!   textures (9E8) and link (OSUnlink 0x800E87F8) go too, so the
//!   Kindergarten area's own copy of module 74 is never linked twice.
//!
//! The page sends Tab (and the gamepad's Back) as /dev/pad key bit 15
//! (sa2-web game lib.rs -> [`set_tab`]).
//!
//! Five tabs, picked with the mouse. The game's chart has three pages (blue,
//! yellow, pink) turned with L/R. Here a click on a tab picks its page, and
//! two more tabs (green, purple) open two MMO pages:
//! - The tabs are 16-byte quad records (s16 x0, x1, y0, y1, u0, u1, v0, v1;
//!   screen 640x480) at the head of each page's table (rodata 0x1DA4, 0x2134,
//!   0x2464), drawn by F9C4(table, count, RGBA). While the chart is open
//!   their x are rewritten to 70 wide (five fit the 360 wide frame) and the
//!   "L" / "R" labels get no width; closing puts the game's values back.
//! - The two extra tabs and the two extra page frames are the pink page's
//!   own records (tab 8..=10, frame 12..=19) at new x, drawn with F9C4's
//!   colour argument (the art has no green or purple piece).
//! - Paging is no longer the chart's C3D8 (its L/R wraps at three pages);
//!   [`main`] keeps the two other things C3D8 does: B / Back closes through
//!   D1D4 with the game's sound, and work+0x16 advances by 0x10 a frame.
//! - Pages 3 and 4 are never given to C50C (it draws page 0 for them): the
//!   task's display is a host function that runs B244 for the game's pages
//!   and only the backdrop (890) and the frame for the MMO pages.
//!
//! The purple page is breeding: the pink page's own boxes (its records
//! 20..=43) in the purple tint, and strings through the chart's own text
//! draw D4C4(_, cp1252 string, flags, f1 x, f2 y, f3 width, f4 size) at the
//! spots C50C gives the pink page's (title 444,84 size 20 flags 5; headers
//! x 288, y 110 / 276 / 370 size 16; body y 390 size 20). The page feeds it
//! (SA2_BREED_IN: the player's Chao's name, a message, then a line
//! "id<tab>name" for each of the player's other Chao); a click on a name
//! picks it, a second click sends its id to the page (SA2_BREED_OUT), which
//! asks the server. The rules stay the server's.
//!
//! shortcut: the green page is an empty frame (inventory comes next), and
//! the breeding page's top box is empty: the two Chao and the mating in it
//! are not written yet.
//! shortcut: the picked name is marked with ">" (no flat bar among the
//! chart's quads is known); upgrade when the pink table's pieces are named.
//! Unverified until a build run: every text and row position on that page.
//!
//! Unverified until a build run: that a widescreen page moves the tab row
//! the way [`to_game`] undoes (sa2-gx widescreen.rs moves right-hand HUD).
//!
//! shortcut: the player's Chao keeps reading the pad while the chart is open
//! (A/B also reach it), as nothing in the lobby stops it; upgrade by finding
//! the Chao World control lock the Doctor room uses, if any.
use crate::guest_call as call;
use sa2_recomp::rt::{self, Cpu, Mem};
use sa2_recomp::{global, global::Global};

const KINDER: u32 = 74;
const CHAOMAIN: u32 = 60;
const FILE: &str = "ChaoStgKinder.prs";

const PRS_EXPAND: u32 = 0x800D_2684;
const OS_LINK: u32 = 0x800E_839C;
const OS_UNLINK: u32 = 0x800E_87F8;
const SY_CALLOC: u32 = 0x8011_97FC;
const SY_FREE: u32 = 0x8011_9828;
const CREATE_ELEMENTAL_TASK: u32 = 0x8007_1E0C;
const FREE_TASK: u32 = 0x8007_1AE0;
const STAGE_NUM: u32 = 0x803A_D820;

/// ChaoStgKinder .text offsets (section 1).
const TEX_LOAD: u32 = 0xA10;
const TEX_RELEASE: u32 = 0x9E8;
const DOCTOR_DISPLAY: u32 = 0xB244;
const BACKDROP: u32 = 0x890;
const SET_TEXLIST: u32 = 0x9C0;
const CLOSE: u32 = 0xD1D4;
const QUADS: u32 = 0xF9C4;
const QUAD_STATE: u32 = 0xFC98;
const TEXT: u32 = 0xD4C4;
/// ChaoStgKinder rodata: the three pages' quad tables.
const TABLE: [u32; 3] = [0x1DA4, 0x2134, 0x2464];
/// What C50C runs before its quads: 0x80122AB4, then the chart atlas
/// (texture 7 of the module's list) through 0x8011C03C(7, 0, 0, 0).
const DRAW_BEGIN: u32 = 0x8012_2AB4;
const SET_TEXTURE: u32 = 0x8011_C03C;
const ATLAS: u32 = 7;
const SE_CALL: u32 = 0x8006_B978;
const SE_PAGE: u32 = 0x8000;
const SE_CLOSE: u32 = 0x100A;
/// The pad record C3D8 reads: *PAD + 0x10 = pressed; 0x402 closes.
const PAD: u32 = 0x801E_55D8;
const PAD_CLOSE: u32 = 0x402;
const CLOSED: u32 = 0xC3C4;
const OPEN: u32 = 0xD208;
const FREE_PORTRAIT: u32 = 0xD238;
/// ChaoStgKinder rodata (section 4): "MsgAlKinderHe".
const MSG_NAME: u32 = 0x2974;
/// ChaoMain .text: the message file load and the chart's Chao info.
const MSG_LOAD: u32 = 0x1CD20;
const CHAO_INFO: u32 = 0x15E44;

const WORK_SIZE: u32 = 0x68;

/// Where a borrowed ChaoStgKinder goes: chao_player.rs's ChaoMain area,
/// free in Chao World (the game loads ChaoMain itself there).
const BUF: u32 = 0x8200_0000;
const BASE: u32 = 0x8240_0000;
const BSS: u32 = 0x8288_0000;

/// Host functions at addresses no guest code has (outside RAM).
const MAIN: u32 = 0x7E00_0000;
const DELETE: u32 = 0x7E00_0004;
const DISPLAY: u32 = 0x7E00_0008;
/// The task's name, "MMOChart", after the borrowed bss.
const NAME: u32 = BSS + 0x1000;
/// Quad records built here: the two extra tabs (6), an MMO page's frame (8).
const EXTRA: u32 = BSS + 0x1100;
const FRAME: u32 = EXTRA + 6 * 16;
/// The string [`text`] hands to the chart's text draw.
const STR: u32 = BSS + 0x1200;
const STR_MAX: usize = 63;
/// The breeding page's list: two columns (the pink page's own x 288 and
/// 452) of four names in the pink page's list box.
const LIST_X: [f32; 2] = [288.0, 452.0];
const LIST_Y: f32 = 298.0;
const LIST_PITCH: f32 = 18.0;
const LIST_ROWS: usize = 4;
const LIST_BOX: (f32, f32) = (281.0, 607.0);

/// Tabs: five across the frame's 360 (x 264..624), in the game's tab row.
const PAGES: u8 = 5;
const TAB_X: i16 = 264;
const TAB_W: i16 = 70;
const TAB_Y: (f32, f32) = (40.0, 56.0);
/// The frame's top edge between its corners.
const EDGE: (i16, i16) = (272, 616);
/// F9C4 colours (RGBA) over the pink art for the green and purple pages.
const TINT: [u32; 2] = [0xAAFF_AAFF, 0xE0C4_FFFF];
const WHITE: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Closed,
    /// task, work, whether the module was borrowed (linked by us)
    Open {
        task: u32,
        work: u32,
        borrowed: bool,
    },
}

// Shared by every game thread (each runs on a host thread of its own): the
// page's keys arrive on the frame hook's thread, the chart runs on the game's.
static TAB: Global<bool> = global!(false);
static TAB_WAS: Global<bool> = global!(false);
static STATE: Global<State> = global!(State::Closed);
static PAGE: Global<u8> = global!(0);
/// The page's last left click: canvas x, y (0..=65535), the canvas
/// aspect in thousandths (0 = 4:3), a count that changes per click.
static CLICK: Global<[u16; 4]> = global!([0; 4]);
static CLICK_SEEN: Global<u16> = global!(0);
/// The game's x values of the records [`retab`] rewrote.
static SAVED: Global<Vec<(u32, u16)>> = global!(Vec::new());

/// The breeding page's feed and the picked Chao's id.
#[derive(Debug, PartialEq)]
struct Breed {
    me: String,
    message: String,
    /// id, name
    chao: Vec<(String, String)>,
    picked: String,
}
static BREED: Global<Breed> = global!(Breed {
    me: String::new(),
    message: String::new(),
    chao: Vec::new(),
    picked: String::new(),
});

/// SA2_BREED_IN's text into the feed (the pick is kept by the caller).
fn breed_feed(text: &str) -> Breed {
    let mut lines = text.lines();
    let mut next = || lines.next().unwrap_or("").trim().to_string();
    let (me, message) = (next(), next());
    let chao = lines
        .filter_map(|l| l.split_once('\t'))
        .map(|(id, name)| (id.trim().to_string(), name.trim().to_string()))
        .filter(|(id, _)| !id.is_empty())
        .take(LIST_X.len() * LIST_ROWS)
        .collect();
    Breed { me, message, chao, picked: String::new() }
}

/// The list entry at a game screen position.
fn row_at(x: f32, y: f32) -> Option<usize> {
    let row = ((y - LIST_Y) / LIST_PITCH).floor();
    let inside = x >= LIST_BOX.0 && x < LIST_BOX.1 && row >= 0.0 && row < LIST_ROWS as f32;
    inside.then(|| usize::from(x >= LIST_X[1] - 6.0) * LIST_ROWS + row as usize)
}

/// Read the page's feed; a picked Chao that left the list is unpicked.
fn breed_read() {
    let Some(text) = std::env::var("SA2_BREED_IN").ok().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return;
    };
    let mut feed = breed_feed(&text);
    BREED.with_borrow_mut(|b| {
        if feed.chao.iter().any(|(id, _)| *id == b.picked) {
            feed.picked = std::mem::take(&mut b.picked);
        }
        *b = feed;
    });
}

/// A click on the breeding page: a name is picked; the picked name again
/// goes to the page as the partner to breed with. True when it was a name.
fn breed_click(x: f32, y: f32) -> bool {
    let Some(i) = row_at(x, y) else { return false };
    BREED.with_borrow_mut(|b| {
        let Some((id, _)) = b.chao.get(i) else { return false };
        if *id == b.picked {
            eprintln!("kinder-chart: breed with {id}");
            if let Ok(path) = std::env::var("SA2_BREED_OUT") {
                let _ = std::fs::write(path, id);
            }
        } else {
            b.picked = id.clone();
        }
        true
    })
}

/// One string through the chart's own text draw (flags 5 centres it).
fn text(c: &mut Cpu, m: &mut Mem, s: &str, flags: u32, x: f32, y: f32, size: f32) {
    let mut bytes: Vec<u8> = s.bytes().filter(|b| (0x20..0x7F).contains(b)).take(STR_MAX).collect();
    bytes.push(0);
    m.load(STR, &bytes);
    let saved = [c.f[1], c.f[2], c.f[3], c.f[4]];
    for (i, v) in [x, y, 999.0, size].into_iter().enumerate() {
        c.f[i + 1].ps0 = f64::from(v);
    }
    call(c, m, rt::sec(m, KINDER, 1) + TEXT, &[0, STR, flags]);
    for (i, f) in saved.into_iter().enumerate() {
        c.f[i + 1] = f;
    }
}

/// The breeding page's strings.
fn breed_text(c: &mut Cpu, m: &mut Mem) {
    // Taken out while the game draws: a guest call may switch game threads.
    let (me, message, names, picked) = BREED.with_borrow(|b| {
        let picked = b.chao.iter().position(|(id, _)| *id == b.picked);
        (b.me.clone(), b.message.clone(), b.chao.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>(), picked)
    });
    text(c, m, "Breeding", 5, 444.0, 84.0, 20.0);
    let pair = match picked {
        Some(i) => format!("{me} and {}", names[i]),
        None => "Pick a partner".to_string(),
    };
    text(c, m, &pair, 0, 288.0, 110.0, 16.0);
    text(c, m, "Your Chao", 0, 288.0, 276.0, 16.0);
    for (i, name) in names.iter().enumerate() {
        let mark = if picked == Some(i) { ">" } else { " " };
        let y = LIST_Y + LIST_PITCH * (i % LIST_ROWS) as f32;
        text(c, m, &format!("{mark}{name}"), 0, LIST_X[i / LIST_ROWS], y, 16.0);
    }
    text(c, m, "Message", 0, 288.0, 370.0, 16.0);
    let line = match (message.is_empty(), names.is_empty(), picked) {
        (false, ..) => message,
        (_, true, _) => "You have no other Chao yet.".to_string(),
        (_, _, Some(i)) => format!("Click {} again to breed.", names[i]),
        _ => "Click a Chao.".to_string(),
    };
    text(c, m, &line, 0, 288.0, 390.0, 20.0);
}

/// The page's last left click (/dev/pad bytes 16..24).
pub fn set_click(x: u16, y: u16, aspect: u16, count: u16) {
    CLICK.set([x, y, aspect, count]);
}

/// Whether the chart is open (the page's left mouse is not B then).
pub fn is_open() -> bool {
    STATE.get() != State::Closed
}

/// A click's game screen position (640x480) from its canvas position: a
/// wider canvas shows the chart 4:3 in the middle, the game's x scaled by
/// k = (4/3) / aspect (seen in a 16:9 build run: with the HUD's move to the
/// edge undone as well, every tab's click landed one tab left).
fn to_game(x: u16, y: u16, aspect: u16) -> (f32, f32) {
    let mut nx = f32::from(x) / 65535.0 * 2.0 - 1.0;
    let k = (4.0 / 3.0) / (f32::from(aspect) / 1000.0);
    if k > 0.1 && k < 0.999 {
        nx /= k;
    }
    ((nx + 1.0) * 320.0, f32::from(y) / 65535.0 * 480.0)
}

/// The tab at a game screen position.
fn tab_at(x: f32, y: f32) -> Option<u8> {
    let i = ((x - f32::from(TAB_X)) / f32::from(TAB_W)).floor();
    (y >= TAB_Y.0 && y < TAB_Y.1 && i >= 0.0 && i < f32::from(PAGES)).then_some(i as u8)
}

/// A tab's three pieces (cap, body, cap) as x spans.
fn tab_spans(tab: u8) -> [(i16, i16); 3] {
    let x = TAB_X + TAB_W * i16::from(tab);
    [
        (x, x + 8),
        (x + 8, x + TAB_W - 8),
        (x + TAB_W - 8, x + TAB_W),
    ]
}

/// The frame's top edge for a page: left of its tab, the opening under the
/// tab, right of it.
fn edge_spans(page: u8) -> [(i16, i16); 3] {
    let x = TAB_X + TAB_W * i16::from(page);
    let (a, b) = (x.max(EDGE.0), (x + TAB_W).min(EDGE.1));
    [(EDGE.0, a), (a, b), (b, EDGE.1)]
}

/// Which records of a page's table are what: its three tabs' first piece,
/// its two labels, and its edge pieces (page 0 has no piece left of its tab).
fn layout(page: usize) -> ([u32; 3], [u32; 2], [Option<u32>; 3]) {
    if page == 0 {
        ([0, 4, 7], [3, 10], [None, Some(12), Some(13)])
    } else {
        ([0, 5, 8], [3, 11], [Some(13), Some(14), Some(15)])
    }
}

fn set_x(m: &mut Mem, rec: u32, (x0, x1): (i16, i16), saved: &mut Vec<(u32, u16)>) {
    for (a, v) in [(rec, x0), (rec + 2, x1)] {
        saved.push((a, m.r16(a)));
        m.w16(a, v as u16);
    }
}

/// Rewrite the three pages' tab and edge records for five 70 wide tabs.
fn retab(m: &mut Mem, s4: u32) {
    let mut saved = Vec::new();
    for (page, table) in TABLE.iter().enumerate() {
        let t = s4 + table;
        let (tabs, labels, edge) = layout(page);
        for (tab, first) in tabs.iter().enumerate() {
            for (i, span) in tab_spans(tab as u8).into_iter().enumerate() {
                set_x(m, t + (first + i as u32) * 16, span, &mut saved);
            }
        }
        for l in labels {
            set_x(m, t + l * 16, (0, 0), &mut saved);
        }
        for (rec, span) in edge.into_iter().zip(edge_spans(page as u8)) {
            if let Some(rec) = rec {
                set_x(m, t + rec * 16, span, &mut saved);
            }
        }
    }
    SAVED.set(saved);
}

/// Put the game's own values back (the Kindergarten's Doctor uses them).
fn untab(m: &mut Mem) {
    let saved = SAVED.take();
    // An area change may have unlinked the module before the task goes.
    if linked(m, KINDER) {
        for (a, v) in saved.into_iter().rev() {
            m.w16(a, v);
        }
    }
}

/// Copy `n` records and give record `i` the span `x(i)` (None keeps it).
fn copy_records(m: &mut Mem, from: u32, to: u32, n: u32, x: impl Fn(u32) -> Option<(i16, i16)>) {
    for i in 0..n {
        for o in (0..16).step_by(2) {
            let v = m.r16(from + i * 16 + o);
            m.w16(to + i * 16 + o, v);
        }
        if let Some((x0, x1)) = x(i) {
            m.w16(to + i * 16, x0 as u16);
            m.w16(to + i * 16 + 2, x1 as u16);
        }
    }
}

/// The chart task's display: the game's own for its three pages, the
/// backdrop and a tinted frame for the MMO pages; then the two extra tabs.
fn display(c: &mut Cpu, m: &mut Mem) {
    let State::Open { task, work, .. } = STATE.get() else {
        return;
    };
    if m.r8(work + 0x14) == 0 {
        return;
    }
    let s1 = rt::sec(m, KINDER, 1);
    let pink = rt::sec(m, KINDER, 4) + TABLE[2];
    let page = PAGE.get();
    if page < 3 {
        call(c, m, s1 + DOCTOR_DISPLAY, &[task]);
    } else {
        call(c, m, s1 + BACKDROP, &[]);
    }
    call(c, m, DRAW_BEGIN, &[]);
    call(c, m, s1 + QUAD_STATE, &[]);
    call(c, m, s1 + SET_TEXLIST, &[]);
    call(c, m, SET_TEXTURE, &[ATLAS, 0, 0, 0]);
    if page >= 3 {
        // The game's three tabs (the pink table's first 12 records), then
        // the pink frame (12..=19) with its opening under this page's tab.
        call(c, m, s1 + QUADS, &[pink, 12, WHITE]);
        let edge = edge_spans(page);
        copy_records(m, pink + 12 * 16, FRAME, 8, |i| {
            (1..=3).contains(&i).then(|| edge[i as usize - 1])
        });
        call(c, m, s1 + QUADS, &[FRAME, 8, TINT[page as usize - 3]]);
    }
    if page == 4 {
        call(c, m, s1 + QUADS, &[pink + 20 * 16, 24, TINT[1]]);
    }
    for (n, tint) in TINT.into_iter().enumerate() {
        let spans = tab_spans(3 + n as u8);
        let to = EXTRA + n as u32 * 48;
        copy_records(m, pink + 8 * 16, to, 3, |i| Some(spans[i as usize]));
        call(c, m, s1 + QUADS, &[to, 3, tint]);
    }
    if page == 4 {
        breed_text(c, m);
    }
}

/// This frame's Tab key (page /dev/pad key bit 15).
pub fn set_tab(down: bool) {
    TAB.set(down);
}

/// A press: down now, up last frame.
fn pressed(down: bool, was: bool) -> bool {
    down && !was
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(MAIN, main);
    c.rt.set_override(DELETE, delete);
    c.rt.set_override(DISPLAY, display);
}

/// Each frame from the player's exec (chao_player.rs, the thread that knows
/// the player's Chao): a Tab press in Chao World opens the chart; a second
/// press closes it.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    let down = TAB.get();
    let press = pressed(down, TAB_WAS.replace(down));
    let in_world = m.r16(STAGE_NUM) as i16 >= 90 && linked(m, CHAOMAIN);
    if press {
        let chao = crate::chao_player::player_chao();
        eprintln!("kinder-chart: Tab (Chao World {in_world}, Chao {chao:x?}, {:?})", STATE.get());
    }
    match STATE.get() {
        State::Closed if press && in_world => {
            if let Some(chao) = crate::chao_player::player_chao() {
                STATE.set(open(c, m, chao));
            }
        }
        State::Open { work, .. } if press => {
            // The chart's own close (D1D4 via its B input) is D258: flag off.
            m.w8(work + 0x14, 0);
        }
        State::Open { work, .. } => {
            if PAGE.get() == 4 && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed).is_multiple_of(15) {
                breed_read();
            }
            click(c, m, work);
        }
        _ => {}
    }
    CLICK_SEEN.set(CLICK.get()[3]);
}

/// A new click on a tab picks its page, with the game's page sound.
fn click(c: &mut Cpu, m: &mut Mem, work: u32) {
    let [x, y, aspect, count] = CLICK.get();
    if count == CLICK_SEEN.get() {
        return;
    }
    let (gx, gy) = to_game(x, y, aspect);
    let Some(page) = tab_at(gx, gy) else {
        if PAGE.get() == 4 && breed_click(gx, gy) {
            call(c, m, SE_CALL, &[SE_PAGE, 0, 0, 0]);
        }
        return;
    };
    if page != PAGE.replace(page) {
        if page == 4 {
            breed_read();
        }
        if page < 3 {
            m.w8(work + 0x15, page);
        }
        call(c, m, SE_CALL, &[SE_PAGE, 0, 0, 0]);
    }
}

/// A name as the Chao's record holds it (7 bytes at +0x12, 0 ends):
/// ChaoMain 1D58C shows code 0x5F as a space and a code below it as the
/// ASCII character 0x20 above.
/// shortcut: ASCII only (other characters are left out); the game's table
/// has more, add them when names need them.
fn name_codes(name: &[u8]) -> [u8; 7] {
    let mut out = [0; 7];
    let codes = name.iter().take_while(|&&b| b != 0).filter_map(|&b| match b {
        b' ' => Some(0x5F),
        0x21..=0x7E => Some(b - 0x20),
        _ => None,
    });
    for (o, code) in out.iter_mut().zip(codes) {
        *o = code;
    }
    out
}

/// The chart's portrait is a whole Chao out of the game heap.
/// Unverified: a build run crashed inside the portrait's creation once in
/// two opens; too little heap is the cause that fits, not a proven one, so
/// the free heap is logged at every open.
use crate::chao_player::REMOTE_CHAO_BYTES as HEAP_NEEDED;

fn open(c: &mut Cpu, m: &mut Mem, chao: u32) -> State {
    let free = crate::chao_player::os_heap_free(m, m.r32(c.r[13].wrapping_sub(0x71B4)));
    eprintln!("kinder-chart: {} KB free in the game heap", free / 1024);
    if free < HEAP_NEEDED {
        eprintln!("kinder-chart: not opened, the portrait Chao needs {} KB", HEAP_NEEDED / 1024);
        return State::Closed;
    }
    let borrowed = !linked(m, KINDER);
    if borrowed && !borrow(c, m) {
        return State::Closed;
    }
    let s1 = rt::sec(m, KINDER, 1);
    let s4 = rt::sec(m, KINDER, 4);
    let c1 = rt::sec(m, CHAOMAIN, 1);
    if borrowed {
        call(c, m, s1 + TEX_LOAD, &[]);
    }
    let work = call(c, m, SY_CALLOC, &[1, WORK_SIZE]);
    if work == 0 {
        eprintln!("kinder-chart: no memory for the chart");
        if borrowed {
            give_back(c, m);
        }
        return State::Closed;
    }
    m.w32(work + 8, chao);
    // The server's name for the Chao (the page's SA2_NAME_IN), into its record.
    if let Some(name) = std::env::var("SA2_NAME_IN").ok().and_then(|p| std::fs::read(p).ok()).filter(|n| n.first().is_some_and(|&b| b != 0)) {
        m.load(m.r32(m.r32(chao + 0x34) + 0x5C) + 0x12, &name_codes(&name));
    }
    let info = call(c, m, c1 + CHAO_INFO, &[chao]);
    m.w32(work + 0xC, info);
    let msg = call(c, m, c1 + MSG_LOAD, &[s4 + MSG_NAME]);
    if msg == 0 {
        // The chart reads its strings through this pointer without a check.
        eprintln!("kinder-chart: the chart's messages (MsgAlKinderHe) didn't load");
        call(c, m, SY_FREE, &[work]);
        if borrowed {
            give_back(c, m);
        }
        return State::Closed;
    }
    m.w32(work + 0x60, msg);
    m.load(NAME, b"MMOChart\0");
    let task = call(c, m, CREATE_ELEMENTAL_TASK, &[2, 2, MAIN, NAME]);
    if task == 0 {
        eprintln!("kinder-chart: no task for the chart");
        call(c, m, SY_FREE, &[msg]);
        call(c, m, SY_FREE, &[work]);
        if borrowed {
            give_back(c, m);
        }
        return State::Closed;
    }
    m.w32(task + 0x40, work);
    m.w32(task + 0x18, DELETE);
    m.w32(task + 0x1C, DISPLAY);
    call(c, m, s1 + OPEN, &[work]);
    PAGE.set(0);
    retab(m, s4);
    eprintln!(
        "kinder-chart: open (module {})",
        if borrowed { "borrowed" } else { "the area's" }
    );
    State::Open {
        task,
        work,
        borrowed,
    }
}

/// The chart task's main: B / Back closes as the chart's own input (C3D8)
/// does, without its L/R paging; once the chart says it closed, the task
/// goes (FreeTask runs [`delete`]).
fn main(c: &mut Cpu, m: &mut Mem) {
    let State::Open { task, work, .. } = STATE.get() else {
        return;
    };
    let s1 = rt::sec(m, KINDER, 1);
    if m.r8(work + 0x14) != 0 {
        if m.r32(m.r32(PAD) + 0x10) & PAD_CLOSE != 0 {
            call(c, m, s1 + CLOSE, &[work]);
            call(c, m, SE_CALL, &[SE_CLOSE, 0, 0, 0]);
        }
        let t = m.r16(work + 0x16).wrapping_add(0x10);
        m.w16(work + 0x16, t);
    }
    if call(c, m, s1 + CLOSED, &[work]) != 0 {
        call(c, m, FREE_TASK, &[task]);
    }
}

/// The chart task's delete: free the chart as the Doctor room's BC18 does.
fn delete(c: &mut Cpu, m: &mut Mem) {
    let State::Open {
        task,
        work,
        borrowed,
    } = STATE.replace(State::Closed)
    else {
        return;
    };
    let s1 = rt::sec(m, KINDER, 1);
    untab(m);
    call(c, m, s1 + FREE_PORTRAIT, &[work]);
    call(c, m, SY_FREE, &[m.r32(work + 0x60)]);
    m.w32(task + 0x40, 0);
    m.w32(task + 0x1C, 0);
    call(c, m, SY_FREE, &[work]);
    if borrowed {
        give_back(c, m);
    }
    eprintln!("kinder-chart: closed");
}

/// Read, expand and link ChaoStgKinder from the player's files.
fn borrow(c: &mut Cpu, m: &mut Mem) -> bool {
    let Some(prs) = crate::dvd::read_file(FILE) else {
        eprintln!("kinder-chart: {FILE} not found");
        return false;
    };
    if prs.len() as u32 > BASE - BUF {
        eprintln!(
            "kinder-chart: {FILE} is {} bytes, too big for its buffer",
            prs.len()
        );
        return false;
    }
    m.load(BUF, &prs);
    call(c, m, PRS_EXPAND, &[BUF, BASE]);
    if m.r32(BASE) != KINDER {
        eprintln!("kinder-chart: {FILE} did not expand (id {})", m.r32(BASE));
        return false;
    }
    m.load(BSS, &vec![0; m.r32(BASE + 0x20) as usize]);
    if call(c, m, OS_LINK, &[BASE, BSS]) == 0 {
        eprintln!("kinder-chart: OSLink {FILE} failed");
        return false;
    }
    c.rt.modules_changed();
    true
}

fn give_back(c: &mut Cpu, m: &mut Mem) {
    call(c, m, rt::sec(m, KINDER, 1) + TEX_RELEASE, &[]);
    call(c, m, OS_UNLINK, &[BASE]);
    c.rt.modules_changed();
}

fn linked(m: &Mem, id: u32) -> bool {
    std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .any(|i| m.r32(i) == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_the_records_codes() {
        assert_eq!(name_codes(b"Tester\0"), [0x34, 0x45, 0x53, 0x54, 0x45, 0x52, 0]);
        assert_eq!(name_codes(b"A b\0zz"), [0x21, 0x5F, 0x42, 0, 0, 0, 0]);
        assert_eq!(name_codes(b"LongerName"), [0x2C, 0x4F, 0x4E, 0x47, 0x45, 0x52, 0x2E]);
    }

    #[test]
    fn the_breeding_feed_is_name_message_then_chao() {
        let b = breed_feed("Tester\nNot now.\n12\tMika\n\tnoid\nbad line\n34\tTails2\n");
        assert_eq!((b.me.as_str(), b.message.as_str()), ("Tester", "Not now."));
        assert_eq!(b.chao, [("12".to_string(), "Mika".to_string()), ("34".to_string(), "Tails2".to_string())]);
        assert_eq!(breed_feed("").chao.len(), 0);
        let many: String = (0..20).map(|i| format!("{i}\tC{i}\n")).collect();
        assert_eq!(breed_feed(&format!("a\n\n{many}")).chao.len(), 8, "what the list box holds");
    }

    #[test]
    fn a_click_in_the_list_is_its_entry() {
        for i in 0..8usize {
            let (x, y) = (LIST_X[i / 4] + 30.0, LIST_Y + LIST_PITCH * (i % 4) as f32 + 9.0);
            assert_eq!(row_at(x, y), Some(i));
        }
        assert_eq!(row_at(300.0, 290.0), None, "above the list");
        assert_eq!(row_at(300.0, 371.0), None, "below the list");
        assert_eq!(row_at(270.0, 300.0), None, "left of the box");
        assert_eq!(row_at(610.0, 300.0), None, "right of the box");
    }

    #[test]
    fn tab_opens_once_per_press() {
        assert!(pressed(true, false));
        assert!(!pressed(true, true));
        assert!(!pressed(false, true));
        assert!(!pressed(false, false));
    }

    #[test]
    fn borrowed_module_fits_between_chao_player_areas() {
        // GC ChaoStgKinder.rel is 0xDE248 bytes expanded; bss 0x1A8.
        const {
            assert!(BASE + 0xDE248 <= BSS);
            assert!(BSS + 0x1A8 <= 0x82A0_0000, "chao_player MOT");
            assert!(FRAME + 8 * 16 <= STR);
            assert!(STR + STR_MAX as u32 + 1 <= 0x82A0_0000);
        }
    }

    #[test]
    fn click_on_a_tab_picks_its_page() {
        // 4:3 canvas: the canvas is the game's 640x480.
        let at = |x: f32, y: f32| {
            let (gx, gy) = to_game(
                (x / 640.0 * 65535.0) as u16,
                (y / 480.0 * 65535.0) as u16,
                0,
            );
            tab_at(gx, gy)
        };
        for page in 0..5u8 {
            assert_eq!(at(264.0 + 70.0 * f32::from(page) + 35.0, 48.0), Some(page));
        }
        assert_eq!(at(260.0, 48.0), None, "left of the tabs");
        assert_eq!(at(616.0, 48.0), None, "right of the fifth tab");
        assert_eq!(at(300.0, 60.0), None, "below the tab row");
        assert_eq!(at(300.0, 38.0), None, "above the tab row");
    }

    #[test]
    fn click_on_a_wide_canvas_undoes_the_narrowing() {
        // 16:9: game x shows at clip k*x.
        let k = 0.75f32;
        let shown = |gx: f32| ((gx / 320.0 - 1.0) * k + 1.0) / 2.0;
        for page in 0..5u8 {
            let gx = 264.0 + 70.0 * f32::from(page) + 35.0;
            let (x, y) = to_game((shown(gx) * 65535.0) as u16, 6553, 1778);
            assert!((x - gx).abs() < 1.0, "page {page}: {x} vs {gx}");
            assert_eq!(tab_at(x, y), Some(page));
        }
    }

    #[test]
    fn click_tabs_fit_the_frame_and_edges_join() {
        assert_eq!(tab_spans(0)[0].0, 264);
        assert!(
            tab_spans(4)[2].1 <= 616,
            "the fifth tab ends before the frame's corner"
        );
        for page in 0..5u8 {
            let [l, o, r] = edge_spans(page);
            assert_eq!((l.0, l.1, o.1, r.1), (272, o.0, r.0, 616));
            assert!(o.0 < o.1);
        }
        assert_eq!(edge_spans(0)[1], (272, 334));
        assert_eq!(edge_spans(4)[1], (544, 614));
    }
}
