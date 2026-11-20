//! The in-stage frame step (milestone E2; ENGINE-MAP.md section 3):
//! `fn_8001E45C` (play frame), `fn_8001E89C` (one-player frame, G+0x49),
//! `fn_8001E12C` (pause frame, mode 0x11), `fn_80062B38` (frame counter and
//! stage clock) and the display frames `fn_80070C20` / `fn_80070BA8`.
//!
//! No wall clock: one call is one native frame. Engine routines these call
//! that are not ported are logged as [`Event::Engine`] and not run (they fail
//! closed); what they would return is asked of a [`FrameHost`].
//!
//! The tick count: `fn_8001E45C` computes a float from G+0xDC/+0xE0/+0xE4 and
//! the previous tick count, rounds it with `njRoundUp`, then overwrites the
//! count with 1 at 0x8001E518. The result is never used, so the port sets 1 and
//! does not compute it. G+0xE0 is later set from `fn_80035B94`'s f1.
use crate::task::{DisplayEngine, Event, Natives, Runtime, TaskId};

/// G, `lbl_801CC168`.
pub const G: u32 = 0x801C_C168;

// Engine routines the frame calls (logged, not run).
pub const PAD_COPY: u32 = 0x8002_26E0;
pub const PLAY_HOOK: u32 = 0x8006_48C4;
pub const AFTER_EXEC: u32 = 0x8000_6B00;
pub const AFTER_EXEC_2: u32 = 0x800D_0904;
pub const BETWEEN_TICKS: u32 = 0x8002_2EE0;
pub const BEFORE_DISPLAY: u32 = 0x8003_5B68;
pub const MULTI_VIEW_END: u32 = 0x8001_4CD8;
pub const POP_MATRIX: u32 = 0x8011_60E4;
pub const AFTER_DISPLAY: u32 = 0x8006_1CBC;
pub const RING_COUNT: u32 = 0x8006_2C8C;
pub const HUNDRED_RINGS: u32 = 0x8001_C810;
pub const EXIT_REQUEST: u32 = 0x8001_9CA0;
pub const FRAME_END: u32 = 0x8003_5B94;
pub const FRAME_END_2: u32 = 0x8005_7524;
pub const DISABLE_FOG: u32 = 0x8011_C2BC;
pub const ENABLE_FOG: u32 = 0x8011_C2B0;
pub const SET_FOG: u32 = 0x8011_C310;
pub const RENDER_FLAGS: u32 = 0x8011_7C80;
pub const COLOR_BLENDING: u32 = 0x8011_79F8;
pub const SET_TEXTURE: u32 = 0x8011_6E18;
pub const SET_TEXTURE_NUM: u32 = 0x8011_6E34;
pub const DRAW_SPRITE: u32 = 0x8011_E5E0;
pub const DRAW_SPRITE_2: u32 = 0x8011_C508;
pub const SE_CALL: u32 = 0x8006_B978;
pub const PAUSE_TITLE: u32 = 0x8001_B60C;
pub const PAUSE_ITEM: u32 = 0x8001_B404;
pub const DISPLAY_PRE_1: u32 = 0x8011_7D38;
pub const DISPLAY_PRE_2: u32 = 0x8011_7D74;
pub const DISPLAY_PRE_3: u32 = 0x8011_7D14;
/// `fn_80070F5C`, the GX pre-pass; it also runs the +0x2C walk, which is
/// therefore not run here until the pre-pass is ported.
pub const DISPLAY_PREPASS: u32 = 0x8007_0F5C;
pub const GX_FLUSH: u32 = 0x8010_14F0;

/// The game state the frame step reads and writes (main.dol addresses).
#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    /// G 0x801CC168, bytes +0x00..+0xF0.
    pub g: [u8; 0xF0],
    /// 0x803AD8AC: ticks to run this frame.
    pub ticks: i32,
    /// 0x803AD8A4: tick being run.
    pub tick_index: i32,
    /// 0x803AD856: five-frame counter (used when G+0xF == 2).
    pub slow: u8,
    /// 0x803AD81A: stage mode (7 start, 0x10 play, 0x11 pause...).
    pub mode: i16,
    /// 0x803AD81C: exit request.
    pub exit_request: i16,
    /// 0x803AD820: stage number.
    pub stage: i16,
    /// 0x803AD854: two-player overlay countdown.
    pub overlay: i16,
    /// 0x803AD839: pause screen hidden (-1) or shown (0).
    pub pause_flag: i8,
    /// 0x803AD837: pause menu cursor.
    pub pause_cursor: u8,
    /// 0x803AD838: pause menu kind.
    pub pause_menu: u8,
    /// 0x803AD84C: pause input timer.
    pub pause_timer: i16,
    /// 0x803AD840, 0x803AD844, 0x803AD848: pause item handles (opaque).
    pub pause_items: [u32; 3],
    /// 0x803ADAD4: viewport count for the multi-view display.
    pub viewports: i32,
    /// 0x803ADAD0: camera/viewport index.
    pub camera: i32,
    /// 0x803AE139: GX byte saved around the demo overlay.
    pub gx_ae139: u8,
    /// `*0x803AD860` +0..+2: minutes, seconds, frames copy of the clock.
    pub clock: [u8; 3],
    /// `playertp` 0x801E7788.
    pub player_tasks: [Option<TaskId>; 4],
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            g: [0; 0xF0],
            ticks: 0,
            tick_index: 0,
            slow: 0,
            mode: 0,
            exit_request: 0,
            stage: 0,
            overlay: 0,
            pause_flag: 0,
            pause_cursor: 0,
            pause_menu: 0,
            pause_timer: 0,
            pause_items: [0; 3],
            viewports: 0,
            camera: 0,
            gx_ae139: 0,
            clock: [0; 3],
            player_tasks: [None; 4],
        }
    }
}

impl GameState {
    /// Signed byte at G+off.
    pub fn gs8(&self, off: usize) -> i8 {
        self.g[off] as i8
    }
    /// Big-endian word at G+off.
    pub fn g32(&self, off: usize) -> u32 {
        u32::from_be_bytes([
            self.g[off],
            self.g[off + 1],
            self.g[off + 2],
            self.g[off + 3],
        ])
    }
    fn set_f32(&mut self, off: usize, v: f32) {
        self.g[off..off + 4].copy_from_slice(&v.to_bits().to_be_bytes());
    }
}

/// What the frame needs from engine parts that are not ported yet.
pub trait FrameHost {
    /// `GetRingNumber(player)`.
    fn ring_count(&mut self, player: u32) -> i16;
    /// Word +0x10 (pressed) of `*0x801E55D8[slot]`.
    fn pad_pressed(&mut self, slot: usize) -> u32;
    /// Word +0x8 (held) of `*0x801E55D8[slot]`.
    fn pad_held(&mut self, slot: usize) -> u32;
    /// Word +0x10 of the pad copy 0x801E5438 (player 0).
    fn pad_copy_pressed(&mut self) -> u32;
    /// Word 0x801E53A8.
    fn word_801e53a8(&mut self) -> u32;
    /// Words 0x801E5398[i].
    fn word_801e5398(&mut self, i: usize) -> u32;
    /// Halfword +0x10 of `playerpwp[i]`, None when the pointer is null.
    fn player_flags(&mut self, i: usize) -> Option<i16>;
    /// f1 returned by `fn_80035B94`.
    fn frame_end_value(&mut self) -> f32;
    /// `fn_80032428()`.
    fn sort_queue_active(&mut self) -> bool;
    /// View-space Z of a twp position.
    fn view_z(&mut self, pos: [f32; 3]) -> f32;
}

struct Display<'a> {
    mode: u8,
    camera: i32,
    host: &'a mut dyn FrameHost,
}

impl DisplayEngine for Display<'_> {
    fn screen_mode(&mut self) -> u8 {
        self.mode
    }
    fn camera_index(&mut self) -> i32 {
        self.camera
    }
    fn sort_queue_active(&mut self) -> bool {
        self.host.sort_queue_active()
    }
    fn view_z(&mut self, pos: [f32; 3]) -> f32 {
        self.host.view_z(pos)
    }
}

fn prelude(rt: &mut Runtime) {
    for a in [DISPLAY_PRE_1, DISPLAY_PRE_2, DISPLAY_PRE_3, DISPLAY_PREPASS] {
        rt.engine(a, vec![]);
    }
}

/// `fn_80070C20`: one viewport.
pub fn display_single(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &GameState,
    host: &mut dyn FrameHost,
) {
    prelude(rt);
    let mut d = Display {
        mode: st.g[0x3D],
        camera: st.camera,
        host,
    };
    rt.display_pass(n, &mut d);
    rt.engine(GX_FLUSH, vec![]);
}

/// `fn_80070BA8(count)`: the display pass per viewport while G+0x22 is 0.
pub fn display_multi(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &GameState,
    host: &mut dyn FrameHost,
    count: i32,
) {
    prelude(rt);
    let mut i = 0;
    while i < count {
        let mut d = Display {
            mode: st.g[0x3D],
            camera: st.camera,
            host: &mut *host,
        };
        rt.display_pass(n, &mut d);
        if st.gs8(0x22) != 0 {
            break;
        }
        i += 1;
    }
    rt.engine(GX_FLUSH, vec![]);
}

/// `fn_80062B38`: frame counter G+0x7C and the stage clock.
pub fn frame_counter(st: &mut GameState) {
    if st.gs8(0x37) != 0 {
        return;
    }
    let c = st.g32(0x7C).wrapping_add(1);
    st.g[0x7C..0x80].copy_from_slice(&c.to_be_bytes());
    if st.g[0x1A] == 1 || st.g[0x39] == 1 {
        return;
    }
    st.g[0x1D] = st.g[0x1D].wrapping_add(1);
    if st.gs8(0x1D) >= 60 {
        st.g[0x1D] = 0;
        st.g[0x1C] = st.g[0x1C].wrapping_add(1);
        if st.gs8(0x1C) >= 60 {
            st.g[0x1C] = 0;
            st.g[0x1B] = st.g[0x1B].wrapping_add(1);
            if st.gs8(0x1B) >= 100 {
                st.g[0x1B] = 99;
                st.g[0x1C] = 59;
                st.g[0x1D] = 59;
            }
        }
    }
    st.clock = [st.g[0x1B], st.g[0x1C], st.g[0x1D]];
    if st.gs8(0x33) > 0 {
        st.g[0x33] = st.g[0x33].wrapping_sub(1);
    }
    if st.gs8(0x34) > 0 {
        st.g[0x34] = st.g[0x34].wrapping_sub(1);
    }
}

fn tick_count(st: &mut GameState) {
    st.ticks = 1;
    st.tick_index = 0;
    if st.g[0x0F] == 2 {
        st.slow = st.slow.wrapping_add(1);
        if st.slow == 5 {
            st.ticks += 1;
        }
        if st.slow as i8 >= 5 {
            st.slow = 0;
        }
    }
}

fn between_ticks(rt: &mut Runtime, st: &mut GameState) {
    st.tick_index += 1;
    if st.tick_index < st.ticks {
        rt.engine(BETWEEN_TICKS, vec![(st.ticks - st.tick_index) as u32]);
    }
}

/// The exit check both frames share (player-0 start or a player flag 0x4000).
fn demo_exit(host: &mut dyn FrameHost, start0: bool) -> bool {
    if start0 || host.pad_copy_pressed() & 8 != 0 {
        return true;
    }
    host.player_flags(0).is_some_and(|f| f & 0x4000 != 0)
        || host.player_flags(1).is_some_and(|f| f & 0x4000 != 0)
}

fn demo_overlay(rt: &mut Runtime) {
    rt.engine(DISABLE_FOG, vec![]);
    rt.engine(SET_FOG, vec![]);
    rt.engine(SET_TEXTURE, vec![0x803A_D368]);
    rt.engine(DRAW_SPRITE_2, vec![0x8019_492C, 4, 0x05F5_E236, 1]);
    rt.engine(SET_FOG, vec![]);
}

/// `fn_8001E45C`: one in-stage frame. Returns the pause request (0 none,
/// 1 player 1, 2 player 2).
pub fn stage_frame(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &mut GameState,
    host: &mut dyn FrameHost,
) -> u32 {
    if st.gs8(0x49) != 0 {
        solo_frame(rt, n, st, host);
        return 0;
    }
    let mut pause = 0;
    let mut exit = false;
    tick_count(st);
    if st.g[0x23] == 1 && st.gs8(0x1E) == 0 && (st.stage <= 0x3C || st.stage == 0x46) {
        rt.engine(RING_COUNT, vec![0]);
        if host.ring_count(0) >= 100 && st.gs8(0x42) == 0 {
            rt.engine(HUNDRED_RINGS, vec![0]);
        }
    }
    while st.tick_index < st.ticks {
        rt.engine(PAD_COPY, vec![]);
        if st.mode == 0x10 {
            rt.engine(PLAY_HOOK, vec![]);
        }
        rt.exec_pass(n);
        rt.engine(AFTER_EXEC, vec![]);
        rt.engine(AFTER_EXEC_2, vec![]);
        frame_counter(st);
        let start0 = host.pad_pressed(0) & 8 != 0;
        let start1 = !start0 && host.pad_pressed(1) & 8 != 0 && st.gs8(0x1E) != 0;
        if (start0 || start1) && st.gs8(0x16) == 0 && st.gs8(0x39) == 0 && st.gs8(0x42) == 0 {
            pause = if start0 { 1 } else { 2 };
        }
        if st.gs8(0x00) != 0 {
            if start0 || host.pad_copy_pressed() & 8 != 0 {
                exit = true;
                if start0 {
                    st.g[0x59] = 0;
                }
            } else if demo_exit(host, false) {
                exit = true;
            }
        }
        between_ticks(rt, st);
    }
    rt.engine(BEFORE_DISPLAY, vec![]);
    if st.gs8(0x20) != 0 {
        display_multi(rt, n, st, host, st.viewports);
        rt.engine(MULTI_VIEW_END, vec![]);
    } else {
        display_single(rt, n, st, host);
        rt.engine(POP_MATRIX, vec![]);
    }
    rt.engine(AFTER_DISPLAY, vec![]);
    if st.gs8(0x1E) != 0 && st.overlay > 0 {
        rt.engine(DISABLE_FOG, vec![]);
        rt.engine(SET_FOG, vec![]);
        rt.engine(RENDER_FLAGS, vec![0]);
        rt.engine(COLOR_BLENDING, vec![0, 8]);
        rt.engine(COLOR_BLENDING, vec![1, 6]);
        rt.engine(SET_TEXTURE, vec![0x803A_D338]);
        rt.engine(SET_TEXTURE_NUM, vec![0]);
        rt.engine(DRAW_SPRITE, vec![0x8019_4E74, 4, 1]);
        rt.engine(RENDER_FLAGS, vec![0x2000]);
        rt.engine(ENABLE_FOG, vec![]);
        rt.engine(SET_FOG, vec![]);
        st.overlay -= 1;
    }
    if st.gs8(0x00) != 0 && st.mode == 0x10 {
        if st.g32(0x78) & 0x40 != 0 && st.g[0x0B] == 1 {
            demo_overlay(rt);
        }
        if exit {
            st.exit_request = 1;
            rt.engine(EXIT_REQUEST, vec![]);
        }
        pause = 0;
    }
    rt.engine(FRAME_END, vec![]);
    let v = host.frame_end_value();
    st.set_f32(0xE0, v);
    rt.engine(FRAME_END_2, vec![]);
    pause
}

/// `fn_8001E89C`: G+0x49 ≠ 0, level 0 and one player's exec only.
pub fn solo_frame(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &mut GameState,
    host: &mut dyn FrameHost,
) {
    let player = st.gs8(0x49) as i32 - 1;
    let mut exit = false;
    tick_count(st);
    while st.tick_index < st.ticks {
        rt.engine(PAD_COPY, vec![]);
        if st.mode == 0x10 {
            rt.engine(PLAY_HOOK, vec![]);
        }
        let task = usize::try_from(player)
            .ok()
            .and_then(|i| st.player_tasks.get(i).copied().flatten());
        rt.exec_level(n, 0);
        match task {
            Some(t) => rt.run_exec(n, t),
            None => rt
                .events
                .push(Event::Anomaly("one-player frame: playertp entry is null")),
        }
        rt.engine(AFTER_EXEC_2, vec![]);
        if st.gs8(0x00) != 0 {
            let start0 = host_start0(host);
            exit |= demo_exit(host, start0);
        }
        between_ticks(rt, st);
    }
    rt.engine(BEFORE_DISPLAY, vec![]);
    display_single(rt, n, st, host);
    rt.engine(POP_MATRIX, vec![]);
    if st.gs8(0x20) != 0 {
        rt.engine(MULTI_VIEW_END, vec![]);
    }
    if st.gs8(0x00) != 0 {
        if st.g32(0x78) & 0x40 != 0 && st.g[0x0B] == 1 {
            demo_overlay(rt);
        }
        if exit {
            st.exit_request = 1;
        }
    }
    rt.engine(FRAME_END, vec![]);
    let v = host.frame_end_value();
    st.set_f32(0xE0, v);
}

fn host_start0(host: &mut dyn FrameHost) -> bool {
    host.pad_pressed(0) & 8 != 0
}

/// `fn_8001E12C`: the pause frame (mode 0x11): pads, display and the pause
/// menu, no exec pass and no frame counter. Returns 0.
pub fn pause_frame(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &mut GameState,
    host: &mut dyn FrameHost,
) -> u32 {
    rt.engine(PAD_COPY, vec![]);
    if st.mode == 0x11 {
        rt.engine(PLAY_HOOK, vec![]);
    }
    st.tick_index = 0;
    if st.gs8(0x20) != 0 {
        display_multi(rt, n, st, host, st.viewports);
        rt.engine(MULTI_VIEW_END, vec![]);
    } else {
        display_single(rt, n, st, host);
        rt.engine(POP_MATRIX, vec![]);
    }
    rt.engine(AFTER_DISPLAY, vec![]);
    st.pause_flag = if host.pad_held(0) & 0x600 == 0x600 {
        -1
    } else {
        0
    };
    if host.word_801e53a8() & 0xF0 == 0 {
        st.pause_timer = 0;
    }
    if st.pause_timer >= 1 {
        st.pause_timer = st.pause_timer.wrapping_add(1);
        if st.pause_timer > 0xF0 {
            st.pause_timer = 0;
        }
    }
    if st.pause_timer == 0 {
        let kind = st.g[0x17];
        let two = st.gs8(0x1E) != 0;
        let w0 = host.word_801e5398(0);
        let w1 = host.word_801e5398(1);
        let up = (kind == 1 && w0 & 0x10 != 0) || (kind == 2 && w1 & 0x10 != 0 && two);
        let down = !up && ((kind == 1 && w0 & 0x20 != 0) || (kind == 2 && w1 & 0x20 != 0 && two));
        if up || down {
            let step = |c: u8| {
                if up {
                    c.wrapping_sub(1)
                } else {
                    c.wrapping_add(1)
                }
            };
            st.pause_cursor = step(st.pause_cursor);
            if st.pause_menu == 2 && st.pause_cursor == 1 {
                st.pause_cursor = step(st.pause_cursor);
            }
            rt.engine(SE_CALL, vec![0x8000, 0, 0, 0]);
        }
    }
    let c = st.pause_cursor as i8;
    if c >= 3 {
        st.pause_cursor = 0;
    } else if c < 0 {
        st.pause_cursor = 2;
    }
    if st.pause_flag >= 0 {
        rt.engine(DISABLE_FOG, vec![]);
        rt.engine(SET_FOG, vec![]);
        rt.engine(COLOR_BLENDING, vec![0, 8]);
        rt.engine(COLOR_BLENDING, vec![1, 6]);
        rt.engine(PAUSE_TITLE, vec![st.pause_menu as i8 as i32 as u32]);
        rt.engine(PAUSE_ITEM, vec![2, 0, st.pause_items[0]]);
        let mut line = 1;
        if st.pause_menu == 3 {
            rt.engine(PAUSE_ITEM, vec![3, 1, st.pause_items[1]]);
            line = 2;
        }
        rt.engine(PAUSE_ITEM, vec![4, line, st.pause_items[2]]);
        rt.engine(ENABLE_FOG, vec![]);
        rt.engine(SET_FOG, vec![]);
    }
    0
}
