//! Chao MMO glue: the player's card box, top left in Chao World, drawn by
//! the game's own code. The box is the Chao Select window (ChaoMain's grid
//! window, display 0x546DC): its nine pieces go through the window's own
//! sprite draw (0x50314) from the window's own sprite records (data
//! 0x38E840, records 10..=18), with the window art's list
//! (AL_TEX_ENT_COMMON_EN, texture 3) loaded by the game's loader. The name
//! and the ring count are the Chao font, drawn by the four ChaoMain calls
//! the Kindergarten chart's text uses (ChaoStgKinder 0xD4C4).
//!
//! The window art is green; the box is white. While the box draws, the
//! sprite draw's GXSetTevOp is followed by a combiner of the glue's own:
//! colour = 1 - green texel * (1 - material colour), alpha = the texel's.
//! The material colour is the sprite draw's own word (data 0x38E7A0).
//!
//! The page feeds the name (SA2_NAME_IN, 7 bytes) and the ring count
//! (SA2_RINGS_IN, 4 bytes, little endian).
//!
//! Unverified until a build run: everything on screen (where the box sits,
//! its shades, the text's place and size), that the art list loads in the
//! lobby, and that the task's display runs after the room's.
//!
//! The portrait is a real Chao from the player's own record, as the Chao
//! Select window's are (CreateChaoExtra, turned -5 degrees): it does not
//! show itself in the world; the box's display draws it at the box's own
//! screen point, the way the chart's portrait is drawn (Kinder 0xC9A8).
//! It lives in the remote Chao heap (chao_player.rs).
//!
//! shortcut: no ring icon yet; the ring art sits on an orange panel in
//! AL_TEX_COMMON and needs cutting out.
use crate::{guest_call as call, original};
use sa2_recomp::rt::{self, Cpu, Mem};
use sa2_recomp::{global, global::Global};

const CHAOMAIN: u32 = 60;
const STAGE_NUM: u32 = 0x803A_D820;
const CREATE_ELEMENTAL_TASK: u32 = 0x8007_1E0C;
/// Texture list by name ("%s.PRS"), as chao_player.rs loads the gear's.
const LOAD_TEX: u32 = 0x8007_2548;
const GX_SET_TEV_OP: u32 = 0x8010_44A8;
const GX_SET_TEV_COLOR_IN: u32 = 0x8010_464C;
const GX_SET_TEV_ALPHA_IN: u32 = 0x8010_46CC;
const GX_SET_TEV_SWAP_MODE: u32 = 0x8010_49A8;
const GX_SET_TEV_SWAP_MODE_TABLE: u32 = 0x8010_4A00;
const GX_SET_Z_MODE: u32 = 0x8010_517C;
const GX_SET_CULL_MODE: u32 = 0x8010_1BBC;
const GX_SET_BLEND_MODE: u32 = 0x8010_4FF8;
// The chart's portrait draw (kinder_chart.rs).
const MODEL_BEGIN: u32 = 0x8012_2B24;
const SET_LIGHT: u32 = 0x8002_4CB8;
const STAGE_LIGHT: u32 = 0x803A_D926;
const NJ_PUSH_MATRIX_EX: u32 = 0x8011_6098;
const NJ_POP_MATRIX_EX: u32 = 0x8011_60E4;
const NJ_UNIT_MATRIX: u32 = 0x8011_53E4;
const NJ_ROTATE_Y: u32 = 0x8011_5828;
const NJ_TRANSLATE: u32 = 0x8011_5BD8;
const NJ_GET_CURRENT_FOV: u32 = 0x8011_26A8;
/// The Chao Select window's turn and depth for its Chao (ChaoMain 0x55A20:
/// -5 degrees, rodata 0x9A8C).
const TURN: u32 = 0xFC72;
/// Deeper than the window's own -31 so the Chao is smaller: in a build
/// shot it stood 63 screen units tall at -31, the box is 64.
const CHAO_DEPTH: f32 = -46.0;
/// Behind the Chao and inside the room's far plane: at -1000 the quad was
/// cut away and a near wall hid the Chao (build shots).
const CLEAR_DEPTH: f32 = -80.0;
/// The language byte the chart's text reads (0 = Japanese font rows).
const LANGUAGE: u32 = 0x801C_C168 + 0x11;

// ChaoMain .text
/// Sprite draw (record, 0, x anchor, y anchor; f1 x, f2 y, f3 depth,
/// f4 width scale, f5 height scale). Anchors: 1 = the point is the sprite's
/// right / bottom, 0 = its middle, -1 = its left / top.
const SPRITE: u32 = 0x50314;
/// The window display's first call: normal blending for the sprite draw.
const SPRITE_BLEND: u32 = 0x4F594;
/// The Chao font's set-up (fn_60_1B5D0): counts its users at .bss 0x386B8 and
/// for the first one loads the three font files and clears the glyph cache;
/// fn_60_1B590 frees them with the last user. Only two of the game's own
/// windows use it (their FExec 0x39A8 / 0x480C), so outside the lobby there
/// is often no user at all.
const FONT_INIT: u32 = 0x1_B5D0;
const FONT_USERS: u32 = 0x3_86B8;
const FONT_BEGIN: u32 = 0x1C424;
const FONT_BUILD: u32 = 0x1BD24;
const FONT_PLACE: u32 = 0x1B8F0;
const FONT_DRAW: u32 = 0x1BCB4;
// ChaoMain .data
/// The window's sprite records, 0x24 bytes each: flag, width, height,
/// u0, v0, u1, v1, texture list, texture index.
const RECORDS: u32 = 0x38_E840;
const RECORD: u32 = 0x24;
/// The sprite draw's material colour (RGBA).
const COLOUR: u32 = 0x38_E7A0;

/// The window art: add_select_race_en in al_tex_ent_common_en.
const ART: &[u8] = b"AL_TEX_ENT_COMMON_EN\0";
const ART_INDEX: u32 = 3;
/// The window's depth (ChaoMain rodata 0x9A04).
const DEPTH: f32 = -34.0;
/// Shade for the window pieces: frame 0.61 green -> 0.91, stripes 0.39 -> 0.94
/// (0.81 / 0.88 read as a grey box in a build shot).
const PIECE_SHADE: u32 = 0xD8D8_D8FF;
const WHITE: u32 = 0xFFFF_FFFF;

// Scratch past the chart's (kinder_chart.rs uses 0x8288_1000..0x8288_1300).
const SCRATCH: u32 = 0x8288_2000;
const NAME: u32 = SCRATCH;
const STR: u32 = SCRATCH + 0x20;
const OUT: u32 = SCRATCH + 0x60;
/// Ten records: the window's nine, then a plain opaque texel for the fill.
const COPIES: u32 = SCRATCH + 0x80;
const FILL: u32 = 9;
const ORIGIN: u32 = SCRATCH + 0x200;
const RECORD_COPY: u32 = SCRATCH + 0x800;

const MAIN: u32 = 0x7E00_000C;
const DELETE: u32 = 0x7E00_0010;
const DISPLAY: u32 = 0x7E00_0014;

/// The box in the game's 640x480 screen: left, top, and the width between
/// its two 16-wide corner columns. Rows: 24 (top), 24, 16 (bottom). `s`
/// shrinks the whole box about its top left corner (an MMO choice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub inner: f32,
    pub s: f32,
}

static PLACE: Global<Place> = global!(Place { x: 16.0, y: 14.0, inner: 168.0, s: 0.7 });
static TASK: Global<u32> = global!(0);
/// The page's canvas width / height in thousandths (0 = 4:3).
static ASPECT: Global<u16> = global!(0);
/// The art's texture list; Some(0) = it did not load (text only, no retry
/// until the next stage).
static TEX: Global<Option<u32>> = global!(None);
static WHITENING: Global<bool> = global!(false);
/// The portrait Chao's task (0: none could be made) and its two display functions.
static PORTRAIT: Global<Option<(u32, [u32; 2])>> = global!(None);
static FEED: Global<(String, Option<u32>)> = global!((String::new(), None));
/// The page's count of eggs received (byte 7 of SA2_NAME_IN) as last read,
/// and the frame the notice stays up to.
static EGGS: Global<(u8, u64)> = global!((0, 0));
/// The notice's time on screen (an MMO choice): 5 s at 60 frames a second.
const NOTICE_FRAMES: u64 = 300;
/// The game's sound call and the sound of picking an object up: every
/// character's pick-up action plays it (main.dol 0x80076368, 0x800952C0,
/// 0x800A3AD8), tone 4 of the character bank.
const SE_CALL: u32 = 0x8006_B978;
const SE_NOTICE: u32 = 0x2004;


/// One window piece: record (0..=8 = the window's 10..=18), x anchor,
/// y anchor, x, y, width scale.
type Piece = (u32, i32, i32, f32, f32, f32);

/// The window's nine pieces as its display (0x546DC) places them: the top
/// row hangs above its line, the others below theirs; the middle column
/// stretches to the inner width.
fn pieces(p: Place) -> [Piece; 9] {
    let (l, r) = (p.x + 16.0 * p.s, p.x + (16.0 + p.inner) * p.s);
    let mid = (l + r) / 2.0;
    let rows = [(p.y + 24.0 * p.s, 1), (p.y + 24.0 * p.s, -1), (p.y + 48.0 * p.s, -1)];
    std::array::from_fn(|i| {
        let (y, v) = rows[i / 3];
        match i % 3 {
            0 => (i as u32, 1, v, l, y, p.s),
            1 => (i as u32, 0, v, mid, y, p.inner * p.s),
            _ => (i as u32, -1, v, r, y, p.s),
        }
    })
}

/// The white under the pieces (x, y, width, height): one bar inside the
/// straight sides, one inside the round corners.
fn fills(p: Place) -> [(f32, f32, f32, f32); 2] {
    let w = p.inner + 32.0;
    [(2.0, 12.0, w - 4.0, 40.0), (10.0, 3.0, w - 20.0, 58.0)].map(|(x, y, w, h)| (p.x + x * p.s, p.y + y * p.s, w * p.s, h * p.s))
}

/// Where the portrait Chao stands (its feet) on the screen.
fn portrait_point(p: Place) -> (f32, f32) {
    (p.x + 34.0 * p.s, p.y + 56.0 * p.s)
}

/// The portrait's depth at a box scale; it stays in front of the white.
fn chao_depth(s: f32) -> f32 {
    (CHAO_DEPTH / s).max(CLEAR_DEPTH + 4.0)
}

/// A screen point at a depth in view space, as ChaoMain 0x4F400 and its
/// sprite draw place things: `tan` is the tangent of half the field of view.
fn view_point((sx, sy): (f32, f32), depth: f32, tan: f32) -> [f32; 3] {
    let k = -depth * tan / 320.0;
    [(sx - 320.0) * k, (240.0 - sy) * k, depth]
}

/// The feed's name: up to 7 printable ASCII bytes, 0 ends.
fn name_of(bytes: &[u8]) -> String {
    bytes.iter().take(7).take_while(|&&b| b != 0).filter(|b| (0x20..0x7F).contains(*b)).map(|&b| b as char).collect()
}

fn rings_of(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(MAIN, |_, _| {});
    c.rt.set_override(DELETE, |_, _| TASK.set(0));
    c.rt.set_override(DISPLAY, display);
    c.rt.set_override(GX_SET_TEV_OP, tev_op);
}

/// A new stage released what the last one loaded (chao_player.rs).
pub(crate) fn drop_stage_loads() {
    TEX.set(None);
    TASK.set(0);
    PORTRAIT.set(None);
}

/// The canvas aspect from the page (pad bytes 20..22).
pub fn set_aspect(thousandths: u16) {
    ASPECT.set(thousandths);
}

/// The game-screen x of the picture's real left edge: 0 at 4:3; a wider
/// canvas shows more of the view to the left of the game's 640 (in a 16:9
/// build shot the box sat 320 px in from the edge).
fn left_edge(aspect: u16) -> f32 {
    let a = f32::from(aspect) / 1000.0;
    if a > 4.0 / 3.0 + 0.01 && a < 4.0 { 320.0 - 240.0 * a } else { 0.0 }
}

/// Move the box (the debug HUD tuner).
pub fn set_place(p: Place) {
    PLACE.set(p);
}

/// GXSetTevOp, then the box's own combiner while its pieces draw.
fn tev_op(c: &mut Cpu, m: &mut Mem) {
    original(c, m, GX_SET_TEV_OP);
    if WHITENING.get() {
        // colour: a ONE, b RASC, c TEXC, d ZERO; alpha: d TEXA.
        call(c, m, GX_SET_TEV_COLOR_IN, &[0, 12, 10, 8, 15]);
        call(c, m, GX_SET_TEV_ALPHA_IN, &[0, 7, 7, 7, 4]);
        // The texel's green as its red, green and blue (swap table 1).
        call(c, m, GX_SET_TEV_SWAP_MODE_TABLE, &[1, 1, 1, 1, 3]);
        call(c, m, GX_SET_TEV_SWAP_MODE, &[0, 0, 1]);
    }
}

fn fcall(c: &mut Cpu, m: &mut Mem, addr: u32, ints: &[u32], floats: &[f32]) {
    let saved: Vec<_> = (1..=floats.len()).map(|i| c.f[i]).collect();
    for (i, &v) in floats.iter().enumerate() {
        c.f[i + 1].ps0 = f64::from(v);
    }
    call(c, m, addr, ints);
    for (i, f) in saved.into_iter().enumerate() {
        c.f[i + 1] = f;
    }
}

/// One string in the Chao font, as the chart's text (Kinder 0xD4C4) does.
fn text(c: &mut Cpu, m: &mut Mem, s: &str, x: f32, y: f32, size: f32) {
    let mut bytes: Vec<u8> = s.bytes().filter(|b| (0x20..0x7F).contains(b)).take(31).collect();
    bytes.push(0);
    m.load(STR, &bytes);
    let c1 = rt::sec(m, CHAOMAIN, 1);
    let scale = size / 24.0;
    let japanese = u32::from(m.r8(LANGUAGE) == 0);
    call(c, m, c1 + FONT_BEGIN, &[0, 0xFF]);
    call(c, m, c1 + FONT_BUILD, &[OUT, STR, japanese, (999.0 / scale) as u32]);
    fcall(c, m, c1 + FONT_PLACE, &[OUT], &[x, y, -1.0, scale]);
    call(c, m, c1 + FONT_DRAW, &[OUT]);
}

/// The art list, into game heap 1 as the gear's (the lobby's heap 0 is
/// nearly full), and the glue's copies of the window's sprite records.
fn load(c: &mut Cpu, m: &mut Mem) -> u32 {
    m.load(NAME, ART);
    let heap_var = c.r[13].wrapping_sub(crate::chao_player::SY_HEAP_SDA);
    let keep = m.r32(heap_var);
    m.w32(heap_var, crate::chao_player::GEAR_HEAP);
    let tex = call(c, m, LOAD_TEX, &[NAME]);
    m.w32(heap_var, keep);
    let count = if tex != 0 { m.r32(tex + 4) } else { 0 };
    eprintln!("card-box: window art {tex:#x} ({count} textures)");
    if count <= ART_INDEX {
        return 0;
    }
    let from = rt::sec(m, CHAOMAIN, 5) + RECORDS + 10 * RECORD;
    for i in 0..9 * RECORD / 4 {
        m.w32(COPIES + i * 4, m.r32(from + i * 4));
    }
    // The fill: a piece of the top edge's solid band.
    let fill = COPIES + FILL * RECORD;
    m.w32(fill, 1);
    for (i, v) in [1.0f32, 1.0, 0.479, 0.08, 0.482, 0.12].into_iter().enumerate() {
        m.w32(fill + 4 + i as u32 * 4, v.to_bits());
    }
    for i in 0..=FILL {
        m.w32(COPIES + i * RECORD + 0x1C, tex);
        m.w32(COPIES + i * RECORD + 0x20, ART_INDEX);
    }
    tex
}

/// Each frame from the player's exec (chao_player.rs): in Chao World the
/// box's task is there, with the page's name and ring count.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    if (m.r16(STAGE_NUM) as i16) < 90 || !crate::chao_player::module_linked(m, CHAOMAIN) || crate::chao_player::player_chao().is_none() {
        return;
    }
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed).is_multiple_of(30) {
        let read = |var: &str| std::env::var(var).ok().and_then(|p| std::fs::read(p).ok());
        let name = read("SA2_NAME_IN");
        // An egg came (the server's event, counted by the page): the sound
        // now, the notice in the box for a while.
        let (seen, until) = EGGS.get();
        let eggs = name.as_ref().and_then(|b| b.get(7).copied()).unwrap_or(seen);
        if eggs != seen {
            let now = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
            EGGS.set((eggs, now + NOTICE_FRAMES));
            eprintln!("card-box: egg received, notice up");
            call(c, m, SE_CALL, &[SE_NOTICE, 0, 0, 0]);
        } else {
            EGGS.set((seen, until));
        }
        FEED.set((name.map(|b| name_of(&b)).unwrap_or_default(), read("SA2_RINGS_IN").and_then(|b| rings_of(&b))));
    }
    // An area change frees the task without a word: its display slot tells.
    let task = TASK.get();
    if task != 0 && m.r32(task + 0x1C) == DISPLAY {
        return;
    }
    if TEX.get().is_none() {
        TEX.set(Some(load(c, m)));
    }
    // The box's text is a user of the font too. Without one, the game freed
    // the font with the lobby's window and the box drew its glyphs from, and
    // into, freed memory (mob 2026-11-24: the name and rings as stripes and
    // noise in the garden). This user is never taken back, so the font stays
    // for the whole visit to Chao World, for the Tab chart's text as well.
    let users = m.r32(rt::sec(m, CHAOMAIN, 6) + FONT_USERS);
    eprintln!("card-box: the Chao font has {users} users");
    if users == 0 {
        let c1 = rt::sec(m, CHAOMAIN, 1);
        call(c, m, c1 + FONT_INIT, &[]);
        eprintln!("card-box: font set up for the box; game heap 0 free {} KB", crate::chao_player::os_heap_free(m, 0) / 1024);
    }
    m.load(NAME, b"MMOCard\0");
    let task = call(c, m, CREATE_ELEMENTAL_TASK, &[2, 3, MAIN, NAME]);
    if task != 0 {
        m.w32(task + 0x18, DELETE);
        m.w32(task + 0x1C, DISPLAY);
        eprintln!("card-box: task {task:#x}");
    }
    TASK.set(task);
    // The area change that took the old task took the old portrait too.
    let mut portrait = (0, [0; 2]);
    if let Some(record) = crate::chao_player::player_record(m) {
        m.load(RECORD_COPY, &record);
        m.load(ORIGIN, &[0; 12]);
        let chao = crate::chao_player::extra_chao(c, m, RECORD_COPY, ORIGIN);
        eprintln!("card-box: portrait Chao {chao:#x}");
        if chao != 0 {
            let wk = m.r32(chao + 0x34);
            m.w32(wk + 0xA0, crate::kinder_chart::portrait_flags(m.r32(wk + 0xA0)));
            // It never shows itself: the box draws it.
            portrait = (chao, [m.r32(chao + 0x14), m.r32(chao + 0x20)]);
            m.w32(chao + 0x14, 0);
            m.w32(chao + 0x20, 0);
        }
    }
    PORTRAIT.set(Some(portrait));
}

fn display(c: &mut Cpu, m: &mut Mem) {
    if !crate::chao_player::module_linked(m, CHAOMAIN) {
        return;
    }
    let mut p = PLACE.get();
    p.x += left_edge(ASPECT.get());
    let c1 = rt::sec(m, CHAOMAIN, 1);
    if TEX.get().is_some_and(|t| t != 0) {
        let colour = rt::sec(m, CHAOMAIN, 5) + COLOUR;
        let keep = m.r32(colour);
        call(c, m, GX_SET_CULL_MODE, &[0]);
        call(c, m, c1 + SPRITE_BLEND, &[]);
        WHITENING.set(true);
        m.w32(colour, WHITE);
        // The white goes behind the Chao and into the depth buffer, so
        // what the room drew there does not hide the Chao.
        call(c, m, GX_SET_Z_MODE, &[1, 7, 1]);
        for (x, y, w, h) in fills(p) {
            fcall(c, m, c1 + SPRITE, &[COPIES + FILL * RECORD, 0, -1i32 as u32, -1i32 as u32], &[x, y, CLEAR_DEPTH, w, h]);
        }
        call(c, m, GX_SET_Z_MODE, &[1, 7, 0]);
        m.w32(colour, PIECE_SHADE);
        for (i, h, v, x, y, w) in pieces(p) {
            fcall(c, m, c1 + SPRITE, &[COPIES + i * RECORD, 0, h as u32, v as u32], &[x, y, DEPTH, w, p.s]);
        }
        WHITENING.set(false);
        m.w32(colour, keep);
        call(c, m, GX_SET_TEV_SWAP_MODE, &[0, 0, 0]);
        call(c, m, GX_SET_TEV_SWAP_MODE_TABLE, &[1, 0, 0, 0, 3]);
    }
    let (name, rings) = FEED.with_borrow(|f| f.clone());
    let at = |x: f32, y: f32| (p.x + x * p.s, p.y + y * p.s);
    let (tx, ty) = at(66.0, 10.0);
    text(c, m, &name, tx, ty, 20.0 * p.s);
    let (tx, ty) = at(66.0, 34.0);
    // The ring line gives way to the egg notice while it is up.
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) < EGGS.get().1 {
        text(c, m, "Egg received!", tx, ty, 14.0 * p.s);
    } else if let Some(rings) = rings {
        text(c, m, &format!("Rings {rings}"), tx, ty, 16.0 * p.s);
    }
    let Some((chao, shows)) = PORTRAIT.get().filter(|p| p.0 != 0) else { return };
    let fov = call(c, m, NJ_GET_CURRENT_FOV, &[]) as i32;
    let tan = (fov as f32 / 2.0 * std::f32::consts::TAU / 65536.0).tan();
    // Further away is smaller: the Chao shrinks with the box.
    let [x, y, z] = view_point(portrait_point(p), chao_depth(p.s), tan);
    call(c, m, GX_SET_BLEND_MODE, &[1, 4, 5, 0]);
    call(c, m, GX_SET_Z_MODE, &[1, 3, 1]);
    call(c, m, MODEL_BEGIN, &[]);
    call(c, m, SET_LIGHT, &[10]);
    call(c, m, NJ_PUSH_MATRIX_EX, &[]);
    call(c, m, NJ_UNIT_MATRIX, &[0]);
    fcall(c, m, NJ_TRANSLATE, &[0], &[x, y, z]);
    call(c, m, NJ_ROTATE_Y, &[0, TURN]);
    for show in shows {
        if show != 0 {
            call(c, m, show, &[chao]);
        }
    }
    call(c, m, NJ_POP_MATRIX_EX, &[]);
    call(c, m, SET_LIGHT, &[m.r8(STAGE_LIGHT) as i8 as u32]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Place = Place { x: 16.0, y: 14.0, inner: 128.0, s: 1.0 };

    #[test]
    fn the_portrait_stands_inside_its_part_of_the_box() {
        let (sx, sy) = portrait_point(P);
        assert!(fills(P).iter().any(|&(x, y, w, h)| sx > x && sx < x + w && sy > y && sy <= y + h));
        assert!(CLEAR_DEPTH < CHAO_DEPTH);
        // A smaller box puts the Chao further back, still before the white.
        assert!(chao_depth(0.7) < CHAO_DEPTH && chao_depth(0.1) > CLEAR_DEPTH);
        // Screen centre is straight ahead; left of it is -x, above it +y (90 degree view).
        assert_eq!(view_point((320.0, 240.0), -31.0, 1.0), [0.0, 0.0, -31.0]);
        let [vx, vy, _] = view_point((0.0, 0.0), -32.0, 1.0);
        assert_eq!((vx, vy), (-32.0, 24.0));
    }

    #[test]
    fn the_pieces_tile_the_box_without_gaps() {
        let ps = pieces(P);
        // Columns: corners end / start at the inner edges, the edge sits between.
        assert_eq!((ps[0].1, ps[0].3), (1, 32.0));
        assert_eq!((ps[1].1, ps[1].3, ps[1].5), (0, 96.0, 128.0));
        assert_eq!((ps[2].1, ps[2].3), (-1, 160.0));
        // Rows: the top hangs above y+24, the middle starts there, the bottom at y+48.
        assert_eq!([ps[0].2, ps[3].2, ps[6].2], [1, -1, -1]);
        assert_eq!([ps[0].4, ps[3].4, ps[6].4], [38.0, 38.0, 62.0]);
        assert_eq!(ps.map(|p| p.0), [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn the_fill_stays_inside_the_box() {
        for (x, y, w, h) in fills(P) {
            assert!(x > P.x && y > P.y && x + w < P.x + 160.0 && y + h < P.y + 64.0);
        }
    }

    #[test]
    fn a_smaller_box_shrinks_about_its_top_left_corner() {
        let h = Place { s: 0.5, ..P };
        let (full, half) = (pieces(P), pieces(h));
        for (a, b) in full.iter().zip(half) {
            assert_eq!(((a.3 - P.x) / 2.0, (a.4 - P.y) / 2.0, a.5 / 2.0), (b.3 - P.x, b.4 - P.y, b.5));
        }
        assert_eq!(fills(h)[0], (17.0, 20.0, 78.0, 20.0));
        assert_eq!(portrait_point(h), (33.0, 42.0));
    }

    #[test]
    fn a_wide_canvas_moves_the_box_to_its_real_left_edge() {
        assert_eq!(left_edge(0), 0.0);
        assert_eq!(left_edge(1333), 0.0);
        // 16:9: the 4:3 picture starts 106.7 game units in from the left.
        assert!((left_edge(1778) + 106.72).abs() < 0.1);
    }

    #[test]
    fn the_feed_is_a_short_name_and_a_ring_count() {
        assert_eq!(name_of(b"Tester\0"), "Tester");
        assert_eq!(name_of(b"LongerName"), "LongerN");
        assert_eq!(name_of(b"\0junk"), "");
        assert_eq!(rings_of(&154u32.to_le_bytes()), Some(154));
        assert_eq!(rings_of(&[1, 2]), None);
    }
}
