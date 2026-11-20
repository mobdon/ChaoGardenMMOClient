//! `itemBoxManager`: the HUD queue that slides the icons of the items a
//! player took onto the screen, holds them and shrinks them away.
//!
//! stg42D.rel section 1: the item callback 0x9FB8 ([`item_callback`]; the
//! boxes call it directly, and the stage setup installs it at the GSNE8P
//! address 0x801A2640, an install this module leaves to the stage setup's
//! port), the spawn 0x9EA0, the exec 0xA0CC with its
//! modes 0xA138 (slide in), 0xA2B8 (hold) and 0xA35C (shrink), the dest
//! 0x9F48 and the display 0xA40C. One task per player (s5 +0xD9CC holds
//! the two task pointers); its 0xFC-byte work (syCalloc) holds a count
//! (+4), a timer (+8) and 20 entries of 12 bytes from +0xC (item kind, x,
//! size). The callback never checks the count: a 21st item would write past
//! the work block. Here the entries are a `Vec`, which simply grows.
//!
//! Constants: stg42D section 4 +0x3C0..+0x404.
use super::{game, Draw, Global, ItemBoxHost, ItemEvent, World, KIND_EMPTY, STG42D};

/// s4 +0x3CC: where a new icon starts.
const START_X: f32 = -60.0;
/// s4 +0x3D0: an icon's full size.
const SIZE: f32 = 64.0;
/// s4 +0x3D4: slide speed per frame.
const SLIDE: f32 = 30.0;
/// s4 +0x3D8: the queue's centre x (and a screen's width in the split
/// display).
const CENTER_X: f32 = 320.0;
/// s4 +0x3DC: the spacing between icons.
const SPACING: f32 = 80.0;
/// s4 +0x3C0.
const HALF: f32 = 0.5;
/// s4 +0x3E0: growth per frame (0.2).
const GROW: f32 = f32::from_bits(0x3e4c_cccd);
/// s4 +0x3F0: 1/60.
const SIXTIETH: f32 = f32::from_bits(0x3c88_8889);
/// The hold time in frames (0xA2B8's `cmplwi 0x3C`).
const HOLD: u32 = 60;
/// s4 +0x400, +0x404: the icons' y and z.
const ICON_Y: f32 = 370.0;
const ICON_Z: f32 = f32::from_bits(0x3f73_cf3e);

/// One icon (12 bytes from work +0xC).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// +0: the item kind.
    pub kind: i32,
    /// +4: x on the screen.
    pub x: f32,
    /// +8: the size.
    pub size: f32,
}

/// One player's queue task.
#[derive(Debug, Clone, PartialEq)]
pub struct Queue {
    /// twp +0 (signed byte): 0 = done (the task goes), 1 slide in, 2 hold,
    /// 3 shrink.
    pub mode: i8,
    /// twp +3: the player it belongs to.
    pub player: u8,
    /// work +4 (compared signed).
    pub count: i32,
    /// work +8 (compared unsigned).
    pub timer: u32,
    /// work +0xC.., the first `count` are live.
    pub entries: Vec<Entry>,
}

/// The queue's exec (0xA0CC), one frame.
pub fn exec(q: &mut Queue, host: &mut impl ItemBoxHost) {
    match q.mode {
        0 => host.item_event(ItemEvent::DestroyTask),
        1 => slide_in(q),
        2 => hold(q),
        3 => shrink(q),
        _ => {}
    }
}

fn live(q: &mut Queue) -> impl Iterator<Item = (usize, &mut Entry)> {
    let n = usize::try_from(q.count).unwrap_or(0);
    q.entries.iter_mut().take(n).enumerate()
}

fn grow(e: &mut Entry) {
    e.size += GROW;
    if e.size > SIZE {
        e.size = SIZE;
    }
}

/// 0xA138: each icon slides right to its place; when all are there the
/// queue holds.
fn slide_in(q: &mut Queue) {
    let count = q.count;
    let mut placed = 0;
    for (i, e) in live(q) {
        e.x += SLIDE;
        let target = CENTER_X + SPACING * (count.wrapping_sub(1) as f32) * HALF;
        let target = target - SPACING * (i as i32 as f32);
        if e.x >= target {
            e.x = target;
            placed += 1;
        }
        grow(e);
    }
    if placed == q.count {
        q.mode = 2;
        q.timer = 0;
    }
}

/// 0xA2B8.
fn hold(q: &mut Queue) {
    for (_, e) in live(q) {
        grow(e);
    }
    q.timer = q.timer.wrapping_add(1);
    if q.timer > HOLD {
        q.timer = HOLD;
        q.mode = 3;
    }
}

/// 0xA35C: the icons shrink with the timer; at 0 the queue empties.
fn shrink(q: &mut Queue) {
    let timer = q.timer;
    for (_, e) in live(q) {
        e.size = SIXTIETH * (SIZE * timer as f32);
    }
    q.timer = timer.wrapping_sub(1);
    if q.timer == 0 {
        q.count = 0;
        q.mode = 0;
    }
}

/// The item callback (0x9FB8): `player` took an item of `kind`. Kind 7
/// (empty), players other than 0 and 1, or lbl_801CC168 +0xB clear: no
/// icon. The player's queue task is made when missing (0x9EA0); the item
/// goes at the end and the queue slides in again.
///
/// When the task or its work can't be allocated, 0x9EA0 returns null and
/// the callback reads through it; here nothing more happens.
pub fn item_callback(world: &mut World, player: i32, kind: i32, host: &mut impl ItemBoxHost) {
    if kind == KIND_EMPTY || !(0..2).contains(&player) {
        return;
    }
    if host.global(Global::Game(game::QUEUE_ON)) as i8 == 0 {
        return;
    }
    let slot = player as usize;
    if world.queues[slot].is_none() {
        world.queues[slot] = spawn(host);
    }
    let Some(q) = world.queues[slot].as_mut() else {
        return;
    };
    q.player = player as u8;
    let e = Entry {
        kind,
        x: START_X,
        size: SIZE,
    };
    match usize::try_from(q.count)
        .ok()
        .filter(|&n| n < q.entries.len())
    {
        Some(n) => q.entries[n] = e,
        None => q.entries.push(e),
    }
    q.count = q.count.wrapping_add(1);
    q.mode = 1;
}

/// 0x9EA0: CreateElementalTask(2, 3, 0xA0CC, "itemBoxManager"), then
/// syCalloc(1, 0xFC) for its work (when that fails, DestroyTask and null).
fn spawn(host: &mut impl ItemBoxHost) -> Option<Queue> {
    if !host.create_elemental_task(2, 3, EXEC, STG42D.queue_name) {
        return None;
    }
    if !host.sy_calloc(1, 0xfc) {
        host.item_event(ItemEvent::DestroyTask);
        return None;
    }
    Some(Queue {
        mode: 0,
        player: 0,
        count: 0,
        timer: 0,
        entries: Vec::new(),
    })
}

/// The queue's exec callback (stg42D section 1).
pub const EXEC: u32 = 0xa0cc;

/// The queue's dest (0x9F48): the slot that holds it is cleared and its
/// work freed.
pub fn dest(world: &mut World, slot: usize, host: &mut impl ItemBoxHost) {
    if let Some(s) = world.queues.get_mut(slot) {
        *s = None;
    }
    host.item_event(ItemEvent::SyFree);
}

/// The queue's display (0xA40C): only on its player's screen, or on every
/// screen when lbl_801CC168 +0x1E is set and +0x20 is clear. With +0x1E set
/// (and +0x20 clear) the icons are laid out per player at half size;
/// otherwise each screen's share of the width.
pub fn display(q: &Queue, host: &mut impl ItemBoxHost) -> Vec<Draw> {
    let mut d = Vec::new();
    let screens = host.global(Global::Screens);
    if screens <= 0 {
        return d;
    }
    let screen = host.global(Global::Screen);
    let player = i32::from(q.player);
    let split = host.global(Global::Game(game::SPLIT)) as i8;
    let flag20 = host.global(Global::Game(game::FLAG_20)) as i8;
    if player != screen && (flag20 != 0 || split == 0) {
        return d;
    }
    if split != 0 {
        d.push(Draw::Fn800148FC);
        d.push(Draw::Fn80014CD8);
    }
    let n = usize::try_from(q.count).unwrap_or(0);
    for e in q.entries.iter().take(n) {
        // The loop reads both bytes again for every icon.
        let split = host.global(Global::Game(game::SPLIT)) as i8;
        let (x, size) = if split != 0 && host.global(Global::Game(game::FLAG_20)) as i8 == 0 {
            (HALF * e.x + CENTER_X * f32::from(q.player), HALF * e.size)
        } else {
            let screen = host.global(Global::Screen);
            let screens = host.global(Global::Screens);
            (
                CENTER_X * screen as f32 + e.x / screens as f32,
                e.size / screens as f32,
            )
        };
        d.push(Draw::HudIcon {
            kind: e.kind,
            pos: [x, ICON_Y, ICON_Z],
            size,
        });
    }
    if host.global(Global::Game(game::SPLIT)) as i8 != 0 {
        d.push(Draw::Fn8001475C);
    }
    d
}
