//! Knuckles's stages (and Rouge's, which run Knuckles) on the Chao's own rig:
//! the Chao's own motions with the skate.rs overlay from his skeleton, as in
//! Sonic's stages (chao_player LEVEL_RIG). A child module of chao_player.
//!
//! His skeleton (KNUCKMDL body, 62 nodes) has Sonic's tree: same parents and
//! the same node numbers for body 4, head 44, arms 37/40 and 56/59, heels
//! 23/13 (SA2_KNUX_LOG=1 prints `knux-rest:`; compared with Sonic's in
//! stage 13), so skate::MAP is used as it is.
use super::*;

/// MMO rule (mob 2026-11-21): the Chao's own motion in these states of his
/// (animation state, obj2 +0x178): punch 1 (109) = Chao karate 582, punch 2
/// (110) = 582 mirrored, glide (105) = Chao fly 130, climb (100) = Chao climb
/// 155. Every other state: the Chao's stand/walk/run with the overlay.
pub(super) fn own_motion(state: u16) -> Option<(u32, bool)> {
    match state {
        109 => Some((582, false)),
        110 => Some((582, true)),
        105 => Some((130, false)),
        100 => Some((155, false)),
        _ => None,
    }
}

/// The overlay's share one frame on: out while the Chao plays its own motion,
/// back in after, over the hub's skate times (0.28 s out, 0.42 s in at 60 a
/// second; src/shadow-skate.js SkateCadence), so no part snaps.
pub(super) fn overlay_step(p: f32, own: bool) -> f32 {
    if own { (p - 1.0 / (0.28 * 60.0)).max(0.0) } else { (p + 1.0 / (0.42 * 60.0)).min(1.0) }
}

thread_local! {
    static OVERLAY: Cell<f32> = const { Cell::new(1.0) };
    static SAID_REST: Cell<u32> = const { Cell::new(0) };
    static SAID_TABLE: Cell<u32> = const { Cell::new(0) };
    static ACTS_SAID: Cell<u64> = const { Cell::new(0) };
}

/// The overlay's share in a level (skate_blend under LEVEL_RIG).
pub(super) fn overlay(step: bool) -> f32 {
    if step {
        OVERLAY.set(overlay_step(OVERLAY.get(), OWN_MOTION.get()));
    }
    let p = OVERLAY.get();
    p * p * (3.0 - 2.0 * p)
}

/// The Chao motion frame for his motion frame: the same share of the way
/// through, in two pieces that meet where each motion's hit is (`hit`: his
/// hit frame, the Chao's hit frame from its motion start), so the Chao's
/// fist is out when his is. No hit pair: one piece, start to end.
pub(super) fn punch_frame(his: f32, his_len: f32, start: f32, end: f32, hit: Option<(f32, f32)>) -> f32 {
    let len = end - start;
    if !(his_len > 0.0) || !(len > 0.0) || !his.is_finite() {
        return start;
    }
    let his = his.clamp(0.0, his_len);
    match hit {
        Some((hh, ch)) if hh > 0.0 && hh < his_len && ch > 0.0 && ch < len => {
            if his <= hh { start + his / hh * ch } else { start + ch + (his - hh) / (his_len - hh) * (len - ch) }
        }
        _ => start + his / his_len * len,
    }
}

/// The hit frames, his and the Chao's: where each one's punching hand is
/// furthest forward, measured on the test copy with SA2_KNUX_LOG=1
/// (knux/r3-side, columns `his hands fwd` / chao `hands fwd`):
/// his punch 1 (state 109, 8 frames) first at its most, 4.56 units, at frame
/// 4.5; his punch 2 (110, 12 frames) first at 4.40 at frame 9.0; the Chao's
/// karate 582 (frames 0..8) at its most, 0.79, at frame 4.67 (sampled every
/// third of a frame, overlay off). Not the frame his hit counts in the game's
/// collision: that was not found.
pub(super) fn hit_pair(state: u16) -> Option<(f32, f32)> {
    match state {
        109 => Some((4.5, 4.67)),
        110 => Some((9.0, 4.67)),
        _ => None,
    }
}

/// ChaoMain AL_SetMotionLinkStep(task, motion, frames) (.text 0x1B4DC, the
/// wrapper of main.dol SetMotionLinkStep 0x8002CFF8: link speed 1 / frames).
pub(super) const AL_SET_MOTION_LINK_STEP: u32 = 0x1B4DC;

/// The frames his own action row links a state in over: 1 / its blend share
/// a frame (row +8: punch 1 and 2 = 1.0, glide and climb = 0.25). The Chao's
/// own table links karate 582 in too slowly for his 16-frame punch (test
/// copy 2026-11-24: still linking when his punch ended, no punch seen).
pub(super) fn link_frames(his_blend: f32) -> u32 {
    if his_blend > 0.0 && his_blend <= 1.0 { (1.0 / his_blend).round() as u32 } else { 1 }
}

/// Punch 1 and 2: the Chao's karate frame follows his motion frame.
/// The game's motion control (chaowk +0xAC; main.dol MotionControl
/// 0x8002D214): flag 0x2 = linking, the motion playing at +0x18 (frame +4,
/// start +8, end +0xC), the one linking in at +0x30 (same fields).
pub(super) fn punch_lock(m: &mut Mem, chao: u32, state: u16) {
    let cw = m.r32(chao + 0x34);
    let (mot, his) = RIDER_MOTION.get();
    if cw == 0 || !(0x8000_0000..0x8C00_0000).contains(&mot) || std::env::var("SA2_PUNCH_LOCK").is_ok_and(|v| v == "0") {
        return;
    }
    let ctrl = cw + CHAO_MOTION_CTRL;
    let info = ctrl + if m.r16(ctrl) & 2 != 0 { 0x30 } else { 0x18 };
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    let n = m.r32(mot + 4);
    let to = punch_frame(his, n as f32, f(m, info + 8), f(m, info + 0xC), hit_pair(state));
    m.w32(info + 4, to.to_bits());
}

/// His states in which his whole body turns right round or over in his
/// skeleton, his own heading staying as it was: punch 3 (111, the uppercut)
/// and the drill claw (115 the dive, head down; 116 its end).
pub(super) fn spins(state: u16) -> bool {
    matches!(state, 111 | 115 | 116)
}

thread_local! {
    pub(super) static SPIN: Cell<bool> = const { Cell::new(false) };
}

/// MMO rule (mob 2026-11-24, on the punch 3 clip: "chao doesnt fully spin
/// around", "knuck is facing the other way"): in his spin attack the Chao's
/// body row (node 1, which every other part hangs from) takes his body's
/// whole turn, with no cap and whatever the pause Extras' BODY LIMIT says, so
/// the Chao faces the way he faces on every frame of it.
pub(super) fn spin_limits(node: usize, w: f32, cap: f32) -> (f32, f32) {
    // The head (14: eyes, mouth and what it wears hang from it) and the arms and
    // hands (3, 4, 10, 11) as well: mob on the drill dive, "head doesnt rotate
    // nor do eyes or hand".
    if matches!(node, 1 | 14 | 3 | 4 | 10 | 11) && SPIN.get() && LAST_EXEC.get() == KNUX_EXEC { (1.0, 180.0) } else { (w, cap) }
}

/// The Chao's motion under the overlay in his stages when his state is not
/// his walk, jog or run (those go by his state, as on Sonic's gait): the one
/// it had, and its stand after six frames on one spot. Picked by the ground
/// covered a frame it changed between its walk and run at the top of every
/// jump and every few frames against a wall (test copy 2026-11-24).
pub(super) fn under_motion(still_frames: u32, last: u32) -> u32 {
    if still_frames >= 6 { 0 } else { last }
}

/// His run states (action numbers; his own table plays motions 8, 9 and 126 in them).
pub(super) fn runs(state: u16) -> bool {
    (8..=10).contains(&state)
}

/// The Air Shoes' jets in his stages: only while he is in a run state with the
/// Air Shoes shown and the Chao takes Shadow's skate (run_source). In the game
/// they belong to Shadow's skate motions only (mob 2026-11-24).
pub(super) fn jets() -> bool {
    LAST_EXEC.get() != KNUX_EXEC || (SKATING.get() && shadow_style())
}

thread_local! {
    /// Per stage: the motion numbers of Shadow's skate and of Sonic's run for his run states 8, 9, 10
    /// (0 = not loaded), and Shadow's body model the two are walked on.
    pub(super) static RUN_TABLES: Cell<Option<([u32; 3], [u32; 3], u32)>> = const { Cell::new(None) };
    /// The Chao is drawn from Shadow's skate this frame.
    static SKATING: Cell<bool> = const { Cell::new(false) };
    static RUN_SAID: Cell<u8> = const { Cell::new(0) };
}

/// A file name in main.dol's strings ("SONICMTN.PRS": Sonic's loader passes it to the motion load at 0x80087644).
fn find_name(m: &Mem, name: &[u8]) -> Option<u32> {
    (0x8017_4000u32..0x8017_6000).find(|&a| name.iter().enumerate().all(|(i, &b)| m.r8(a + i as u32) == b) && m.r8(a + name.len() as u32) == 0)
}

/// Free motion table entries (nothing loaded there) that his own table never plays.
fn spare_motions(m: &Mem, table: u32, n: usize) -> Vec<u32> {
    let used: std::collections::BTreeSet<u32> = (0..217u32).map(|k| u32::from(m.r16(table + 0x10 * k))).collect();
    (0..300u32).rev().filter(|k| m.r32(MOTIONS + 8 * k + 4) == 0 && !used.contains(k)).take(n).collect()
}

/// MMO rule (mob 2026-11-24: "i want just the skating when ur at running speed
/// for knuckles", "or sonic running", "if they have sonic shoes"): in his run
/// states the Chao's look follows the shoes it wears, as in the lobby
/// (shadow_style): Air Shoes shown = Shadow's skate with the jets; Light Shoes
/// or no shoes = Sonic's run. Standing, walking, jogging and every move of his
/// own stay his, and so do his speeds, physics and his own (hidden) animation:
/// nothing of his is changed. Only the skeleton the overlay reads is another
/// one in those states: Shadow's body model (TERIOSMDL, already loaded for the
/// gear) walked with Shadow's skate or Sonic's run motion of the same state,
/// at his run motion's place in its cycle (level_capture walks it right after
/// his own, drawing nothing). The motions come from the game's own files by
/// its own loader, kept at spare motion numbers.
/// Tried first and dropped: his own run rows playing those motions on his own
/// skeleton. The tree is the same but the motions are made for Sonic's and
/// Shadow's rest pose: he came out upside down (knux/rt-all.jpg).
/// This loads the motions once a stage. SA2_KNUX_RUN=own (debug): his own run.
pub(super) fn run_style(c: &mut Cpu, m: &mut Mem) {
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) || RUN_TABLES.get().is_some() || std::env::var("SA2_KNUX_RUN").is_ok_and(|v| v == "own") {
        return;
    }
    let own = m.r32(obj2 + OBJ2_PLACTPTR);
    // After the gear (Shadow's models) is in: his body is what the motions are walked on.
    let body = SHADOW_MODELS.with_borrow(|e| e.values().copied().collect::<Vec<_>>()).into_iter()
        .find(|&o| (0x8000_0000..0x8C00_0000).contains(&o) && crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]).nodes.len() == 62);
    let (Some(body), true) = (body, (0x8000_0000..0x8030_0000).contains(&own)) else { return };
    let mut keep: Vec<u32> = (0..0x960).step_by(4).map(|o| m.r32(MOTIONS + o)).collect();
    let heap_var = c.r[13].wrapping_sub(SY_HEAP_SDA);
    let keep_heap = m.r32(heap_var);
    let mut made = [[0u32; 3]; 2];
    // The look the shoes ask for now first: there may be room for only one file.
    // (Measured in Wild Canyon: Shadow's file takes about 600 KB; the gear heap had 790 KB
    // left after the gear, the game's own heap 1.3 MB.) A file goes into the gear heap
    // when 450 KB is free there, else into the game's own heap when it has 1250 KB free (about 650 KB stay).
    let looks = [(0usize, Some(SHADOW_MTN_NAME), SHADOW_ACTIONS), (1, find_name(m, b"SONICMTN.PRS"), SONIC_ACTIONS)];
    let order = if shadow_style() { [0, 1] } else { [1, 0] };
    for (which, name, actions) in order.map(|i| looks[i]) {
        let (gear, game) = (crate::chao_player::os_heap_free(m, GEAR_HEAP), crate::chao_player::os_heap_free(m, 0));
        let heap = if gear > 450 * 1024 { Some(GEAR_HEAP) } else if game > 1250 * 1024 { Some(0) } else { None };
        let (Some(name), Some(heap)) = (name, heap) else {
            eprintln!("chao-player knux: run look {which} not loaded (name {name:?}, gear heap free {} KB, game heap free {} KB): his own run there", gear / 1024, game / 1024);
            continue;
        };
        m.w32(heap_var, heap);
        let mtn = call(c, m, LOAD_MTN, &[name]);
        m.w32(heap_var, keep_heap);
        // The three motions of that character's run states, then the motion table as it was.
        let theirs: Vec<[u32; 2]> = (8..=10u32).map(|st| { let k = u32::from(m.r16(actions + 0x10 * st)); [m.r32(MOTIONS + 8 * k), m.r32(MOTIONS + 8 * k + 4)] }).collect();
        for (k, o) in (0..0x960u32).step_by(4).enumerate() { m.w32(MOTIONS + o, keep[k]); }
        let spare = spare_motions(m, own, 3);
        if spare.len() < 3 || theirs.iter().any(|t| t[1] == 0) {
            eprintln!("chao-player knux: run look {which}: motions {theirs:?}, spare numbers {spare:?}: his own run there");
            continue;
        }
        for (i, (e, &k)) in theirs.iter().zip(&spare).enumerate() {
            m.w32(MOTIONS + 8 * k, e[0]);
            m.w32(MOTIONS + 8 * k + 4, e[1]);
            (keep[2 * k as usize], keep[2 * k as usize + 1]) = (e[0], e[1]);
            made[which][i] = k;
        }
        eprintln!("chao-player knux: run look {which} ({}) loaded {mtn:#x} at motion numbers {spare:?}, gear heap free {} KB", if which == 0 { "Shadow's skate" } else { "Sonic's run" }, crate::chao_player::os_heap_free(m, GEAR_HEAP) / 1024);
    }
    RUN_TABLES.set(Some((made[0], made[1], body)));
}

/// In his run states: Shadow's body, the motion to walk it with and the frame, for the
/// overlay's skeleton (see run_style). `his`: his own motion and its frame this draw.
/// The frame is his frame's share of his cycle on the other motion's length (frame
/// count at motion +4, as punch_lock reads it).
pub(super) fn run_source(m: &Mem, his_motion: u32, his_frame: f32) -> Option<(u32, u32, f32)> {
    SKATING.set(false);
    let obj2 = m.r32(P1_OBJ2);
    let (skate, sonic, body) = RUN_TABLES.get()?;
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) || !(0x8000_0000..0x8C00_0000).contains(&his_motion) {
        return None;
    }
    let state = m.r16(obj2 + OBJ2_ACTION);
    if !runs(state) {
        return None;
    }
    let shadow = shadow_style();
    let k = if shadow { skate } else { sonic }[usize::from(state - 8)];
    let motion = if k == 0 { 0 } else { m.r32(MOTIONS + 8 * k + 4) };
    if !(0x8000_0000..0x8C00_0000).contains(&motion) {
        return None;
    }
    let (n_his, n) = (m.r32(his_motion + 4) as f32, m.r32(motion + 4) as f32);
    let frame = run_frame(his_frame, n_his, n);
    SKATING.set(shadow);
    let said = 1 + u8::from(shadow);
    if RUN_SAID.replace(said) != said {
        eprintln!("chao-player knux: in his run the Chao takes {} (Air Shoes shown: {shadow}; motion {k}, {n} frames, his {n_his})", if shadow { "Shadow's skate" } else { "Sonic's run" });
    }
    Some((body, motion, frame))
}

/// The other motion's frame at the same share of the cycle as his.
pub(super) fn run_frame(his_frame: f32, his_len: f32, len: f32) -> f32 {
    if his_len > 0.0 && len > 0.0 && his_frame.is_finite() { (his_frame / his_len).rem_euclid(1.0) * len } else { 0.0 }
}

/// The Chao stands on his spot with its own body collision on (ChaoMain's
/// collision entries 0 and 2): he pushed against it every frame (his push
/// state 19 on open ground, test copy 2026-11-24 knux/x4: 190 frames of it
/// holding one direction from the start), could not walk a straight line and
/// had it between him and a wall. ChaoMain AL_DisableTouch (.text 0x5A60 =
/// CCL_Disable(task, 0) and (task, 2)) each frame, as AL_DisableNade is: only
/// his own body is there for the game. SA2_KNUX_TOUCH=1 (debug): left on.
pub(super) fn no_touch(c: &mut Cpu, m: &mut Mem, chao: u32) {
    if !std::env::var("SA2_KNUX_TOUCH").is_ok_and(|v| v == "1") {
        call(c, m, rt::sec(m, 60, 1) + 0x5A60, &[chao]);
        // Its third entry (a sphere of 15 units, entry 1) stayed on and kept his collision's
        // hit flag set (collision work +4 bit 0x10, logged 0x8011 on every frame of a glide):
        // with that flag his wall checks stop before looking for a wall, so he never grabbed
        // one. The Chao's exec enters its collision only while its task work +0xA0 has bit 8
        // (as player_move clears it in Chao World): cleared, none of the Chao is solid.
        let cw = m.r32(chao + 0x34);
        if cw != 0 { m.w32(cw + 0xA0, m.r32(cw + 0xA0) & !8); }
    }
}

/// The player's after-image (main.dol display 0x80052620, exec 0x800525AC;
/// task work +0x40: player number +0x30, fade +0x34, the model +0x38, its
/// motion +0x3C and frame +0x44, texture list +0x40): a copy of the
/// character's body left behind in his drill dive (state 115; found by the
/// draw's return address 0x800526CC, SA2_TRACE_EVDRAW). His body is not drawn
/// while the Chao stands in for him, so neither is its copy.
/// SA2_SHOW_SONIC (debug: the character drawn) keeps it.
pub(super) const AFTER_IMAGE_DISP: u32 = 0x8005_2620;

pub(super) fn after_image(c: &mut Cpu, m: &mut Mem) {
    let w = m.r32(c.r[3].wrapping_add(0x40));
    let his = (0x8000_0000..0x8C00_0000).contains(&w) && m.r8(w + 0x30) == 0;
    if his && matches!(STEP.get(), Step::Live(_)) && std::env::var("SA2_SHOW_SONIC").map_or(true, |v| v.contains(',')) {
        return;
    }
    original(c, m, AFTER_IMAGE_DISP);
}

/// His wall checks (task work, move work, player work): in the glide
/// (main.dol 0x800936AC, from his exec at 0x8009FF18) and in the jump
/// (0x8009348C, from his action code at 0x8009C3F8). Result 2 = a wall he
/// grabs (0x80090C80 then starts the climb: mode 69, animation 106), 3 = a
/// surface flagged 0x2002 (no grab), 1 = a no-climb surface or his collision
/// touching an object, 0 = nothing ahead.
/// SA2_KNUX_LOG=1 (debug): each non-zero result with his collision's hit flag
/// (collision work +4 bit 0x10) and the kind of what it touches (+0).
pub(super) const GLIDE_WALL: u32 = 0x8009_36AC;
pub(super) const JUMP_WALL: u32 = 0x8009_348C;

pub(super) fn wall_check(c: &mut Cpu, m: &mut Mem, at: u32) {
    let twp = c.r[3];
    original(c, m, at);
    if c.r[3] != 0 && std::env::var("SA2_KNUX_LOG").is_ok_and(|v| v == "1") {
        let cwp = m.r32(twp.wrapping_add(0x2C));
        let (flags, kind) = if (0x8000_0000..0x8C00_0000).contains(&cwp) { (m.r16(cwp + 4), m.r16(cwp)) } else { (0, 0) };
        eprintln!("knux-wall: f{} check {at:#x} -> {} (collision flags {flags:#x} touching kind {kind})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), c.r[3]);
    }
}

thread_local! {
    static PROBE_SAID: Cell<u64> = const { Cell::new(0) };
    static PROBED: std::cell::RefCell<std::collections::BTreeMap<(u32, u32, usize), (u32, u32)>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// SA2_CHAO_DRAWLOG=1 (debug): the Ninja draw calls ChaoMain makes (main.dol
/// 0x8011D2F4..0x8011F720), by function, return address and the Chao node
/// the player's Chao's walk is at, with a count and the last model; the
/// whole table every five seconds. To find a part that is not drawn.
pub(super) fn draw_probe(c: &mut Cpu, m: &mut Mem, at: u32) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    PROBED.with_borrow_mut(|p| {
        let e = p.entry((at, c.lr, if NATIVE_DRAW.get() { CHAO_NODE.get() } else { 999 })).or_insert((0, 0));
        *e = (e.0 + 1, c.r[3]);
        if f % 300 == 0 && PROBE_SAID.replace(f) != f {
            let all: Vec<String> = p.iter().map(|(k, v)| format!("{:#x}<{:#x}@{}x{}:{:#x}", k.0, k.1, k.2, v.0, v.1)).collect();
            eprintln!("chao-draws: f{f} {}", all.join(" "));
            p.clear();
        }
    });
    original(c, m, at);
}

/// The game's SetPlayerChar(player, character) (main.dol 0x8001DDC0), called
/// by the Stage Select's start code and by start_stage.rs.
pub(super) const SET_PLAYER_CHAR: u32 = 0x8001_DDC0;

/// MMO rule (mob 2026-11-24): Rouge's stages run Knuckles, so the player's
/// Chao has his moves there. Player 0 asked for as Rouge (character 5)
/// becomes Knuckles (4); the game then loads his files, exec and table its
/// own way. SA2_ROUGE_SELF=1 (debug): Rouge herself, as before.
pub(super) fn swapped_char(player: u32, ch: u32, rouge_self: bool) -> u32 {
    if player == 0 && ch == 5 && !rouge_self { 4 } else { ch }
}

pub(super) fn set_char(c: &mut Cpu, m: &mut Mem) {
    let ch = swapped_char(c.r[3], c.r[4], std::env::var("SA2_ROUGE_SELF").is_ok_and(|v| v == "1"));
    if ch != c.r[4] {
        eprintln!("chao-player knux: player {} character {} -> {ch} (Rouge's stage runs Knuckles)", c.r[3], c.r[4]);
        c.r[4] = ch;
    }
    original(c, m, SET_PLAYER_CHAR);
}

/// Where his display (main.dol 0x80099BA0) draws his items with
/// njDrawObject 0x8011DF44, by the return address: the Air Necklace (upgrade
/// bit 0x2000) at 0x8009A1AC, then per hand the Shovel Claw (0x400, model
/// table +0x514 / +0x51C) and the Hammer Glove (0x1000, +0x524 / +0x52C).
/// Walked for the Chao his body draws nothing, but these did: the claws hung
/// at his own hidden hands, as large as he is, over the Chao (test copy
/// 2026-11-24, knux/r3-side, r5-side). Not drawn while the Chao stands in
/// for him. SA2_SHOW_SONIC (debug: the character drawn in his own place)
/// keeps them; SA2_SHOW_SONIC=ahead,side (he is drawn moved aside) does not:
/// the items are not moved with him and hung at the Chao (knux/w-front).
const ITEM_DRAWS: [u32; 5] = [0x8009_A1AC, 0x8009_A1E8, 0x8009_A20C, 0x8009_A23C, 0x8009_A260];

pub(super) fn item_draw(lr: u32) -> bool {
    ITEM_DRAWS.contains(&lr) && std::env::var("SA2_SHOW_SONIC").map_or(true, |v| v.contains(','))
}

/// SA2_KNUX_UPGRADES=hex (test): OR into his upgrade bits (obj2 +0x24, the
/// word fn_8004F838 fills from the save's upgrade bytes; sa2-runtime
/// player.rs upgrade_bits), to reach moves the test save has no item for
/// (0x400 = Shovel Claws, dig).
pub(super) fn test_upgrades(m: &mut Mem, obj2: u32) {
    if let Some(bits) = std::env::var("SA2_KNUX_UPGRADES").ok().and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok()) {
        m.w32(obj2 + 0x24, m.r32(obj2 + 0x24) | bits);
    }
}

/// SA2_KNUX_LOG=1 (debug): once per model, his rest tree (parent and
/// root-space rest position per node); once per table, his action table
/// (state: motion, next state); a line a frame with his state, motion frame,
/// height, hands, and the Chao's motion, frame and hands.
pub(super) fn log(m: &Mem, chao: u32, state: u16) {
    if !std::env::var("SA2_KNUX_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let obj2 = m.r32(P1_OBJ2);
    let obj = RIDER_OBJ.get();
    if (0x8000_0000..0x8C00_0000).contains(&obj) && SAID_REST.replace(obj) != obj {
        let r = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]);
        let w = r.worlds(&[]);
        let line: Vec<String> = r.nodes.iter().zip(&w).enumerate().map(|(i, (n, x))| format!("{i}<{} [{:.2} {:.2} {:.2}]", n.0, x.1[0], x.1[1], x.1[2])).collect();
        eprintln!("knux-rest: obj {obj:#x} character {} nodes {}: {}", m.r8(obj2 + 3), r.nodes.len(), line.join(" "));
    }
    let table = m.r32(obj2 + OBJ2_PLACTPTR);
    if (0x8000_0000..0x8C00_0000).contains(&table) && SAID_TABLE.replace(table) != table {
        let rows: Vec<String> = (0..217u32).map(|k| format!("{k}:{}>{}", m.r16(table + 0x10 * k), m.r16(table + 0x10 * k + 6))).collect();
        eprintln!("knux-table: {table:#x} state:motion>next {}", rows.join(" "));
    }
    let f = |a: u32| f32::from_bits(m.r32(a));
    let cw = m.r32(chao + 0x34);
    let ctrl = cw + CHAO_MOTION_CTRL;
    let (mot, his) = RIDER_MOTION.get();
    let n = if (0x8000_0000..0x8C00_0000).contains(&mot) { m.r32(mot + 4) } else { 0 };
    // His nodes in his root's space: he faces -x. Hands 40 and 59, pelvis 1.
    let (hands, pelvis) = SKEL.with_borrow(|s| if (s.len() == 62 && crate::chao_player::skel_site("k1")) {
        let r = |k: usize| inv_mul(&s[0], &s[k]);
        ([-r(40)[0][3], -r(59)[0][3]], r(1)[1][3])
    } else { ([f32::NAN; 2], f32::NAN) });
    // The Chao's hands in its placement's space: it faces +z.
    let p = CHAO_PLACE.get();
    let ch = HANDS.with_borrow(|h| h.map(|x| x.map_or(f32::NAN, |x| inv_mul(&p, &x)[2][3])));
    let data1 = RIDER.get();
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
        // Collision info (collision work +0xC: centre +8, sizes a b c +0x14) of his task and of the Chao's.
        let info = |twp: u32| { let cwp = m.r32(twp.wrapping_add(0x2C)); if (0x8000_0000..0x8C00_0000).contains(&cwp) { let i = m.r32(cwp + 0xC); if (0x8000_0000..0x8C00_0000).contains(&i) { return format!("n {} form {} push {} centre [{:.2} {:.2} {:.2}] size [{:.2} {:.2} {:.2}] flags {:#x}", m.r16(cwp + 2), m.r8(i + 1), m.r8(i + 2), f(i + 8), f(i + 0xC), f(i + 0x10), f(i + 0x14), f(i + 0x18), f(i + 0x1C), m.r16(cwp + 4)); } } "none".to_string() };
        // Every entry of a collision table (0x30 bytes each: kind, form, push, damage, attr, centre, sizes a b c, angles).
        let table = |twp: u32| { let cwp = m.r32(twp.wrapping_add(0x2C)); let mut out = String::new(); if (0x8000_0000..0x8C00_0000).contains(&cwp) { let i = m.r32(cwp + 0xC); let n = m.r16(cwp + 0xA).min(6); if (0x8000_0000..0x8C00_0000).contains(&i) { for k in 0..u32::from(n.max(1)) { let e = i + 0x30 * k; out += &format!(" [{k}: kind {} form {} push {:#x} attr {:#x} centre {:.2} {:.2} {:.2} size {:.2} {:.2} {:.2}]", m.r8(e), m.r8(e + 1), m.r8(e + 2), m.r32(e + 4), f(e + 8), f(e + 0xC), f(e + 0x10), f(e + 0x14), f(e + 0x18), f(e + 0x1C)); } } out += &format!(" (work words {:#x} {:#x} {:#x} {:#x})", m.r32(cwp), m.r32(cwp + 4), m.r32(cwp + 8), m.r32(cwp + 0xC)); } out };
        let his_task_twp = m.r32(m.r32(0x801E_7768));
        let mw = m.r32(chao + 0x38);
        let mwf: Vec<String> = (0..0x30u32).map(|k| format!("{:.3}", f(mw + 4 * k))).collect();
        eprintln!("knux-size2: his table{} | Chao's table{} | his physics radius {:.3} height {:.3} weight {:.3} camera y {:.3} centre height {:.3} | Chao move work floats {}", table(m.r32(0x801E_7768)), table(cw), f(obj2 + 0x130), f(obj2 + 0x134), f(obj2 + 0x138), f(obj2 + 0x13C), f(obj2 + 0x140), mwf.join(" "));
        let _ = his_task_twp;
        eprintln!("knux-size: his collision {} | Chao's {} | his player work +0x134 {:.3} +0x138 {:.3} +0x13c {:.3} +0x140 {:.3}", info(data1), info(cw), f(obj2 + 0x134), f(obj2 + 0x138), f(obj2 + 0x13C), f(obj2 + 0x140));
    }
    // His action window (player work +4: the actions on offer, their count at +0xC, the one picked at +0xD,
    // the one last done at +0xE; main.dol 0x80095334 does the picked one on B).
    let acts: Vec<u8> = (4..0xCu32).map(|o| m.r8(obj2 + o)).collect();
    if ACTS_SAID.replace(u64::from_le_bytes([acts[0], acts[1], acts[2], acts[3], m.r8(obj2 + 0xC), m.r8(obj2 + 0xD), m.r8(obj2 + 0xE), 0])) != u64::from_le_bytes([acts[0], acts[1], acts[2], acts[3], m.r8(obj2 + 0xC), m.r8(obj2 + 0xD), m.r8(obj2 + 0xE), 0]) {
        eprintln!("knux-acts: f{} offered {acts:?} count {} picked {} done {} (state {state} mode {})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r8(obj2 + 0xC), m.r8(obj2 + 0xD), m.r8(obj2 + 0xE), m.r8(data1));
    }
    eprintln!("knux: f{} state {state} mode {} his motion len {n} frame {his:.2} y {:.2} pelvis {pelvis:.2} sink {:.2} wallsink {:.2} his hands fwd {:.2} {:.2} | chao motion {} frame {:.2} of {:.0}..{:.0} link {} next {:.2} of {:.0}..{:.0} hands fwd {:.2} {:.2} overlay {:.2} skel {}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r8(data1), f(data1 + 0x18), SINK.get(), WALL_SINK.get(), hands[0], hands[1],
        LAST_MOTION.get(), f(ctrl + 0x1C), f(ctrl + 0x20), f(ctrl + 0x24), m.r16(ctrl) & 2 != 0, f(ctrl + 0x34), f(ctrl + 0x38), f(ctrl + 0x3C), ch[0], ch[1], OVERLAY.get(), SKEL.with_borrow(|s| s.len()));
}

/// Ninja colour blending mode (main.dol 0x801179F8: r3 0 = source, 1 =
/// destination; r4 the mode): the plain alpha blend, source 8 with
/// destination 6, as the game's own displays set before a see-through model
/// (0x80052638 / 0x80052644 in the after-image display).
/// SA2_KNUX_BLEND=0 (debug): not set, to compare.
pub(super) fn blend_reset(c: &mut Cpu, m: &mut Mem) {
    // TEST only (SA2_KNUX_BLEND=1): it did not bring the Chao's head back in his stages.
    if LAST_EXEC.get() == KNUX_EXEC && std::env::var("SA2_KNUX_BLEND").is_ok_and(|v| v == "1") {
        call(c, m, 0x8011_79F8, &[0, 8]);
        call(c, m, 0x8011_79F8, &[1, 6]);
    }
}

/// TEST (SA2_CAP_ZERO=1): his walked body's models go through the game's own
/// model draw with the current matrix's turn and size zeroed (nothing reaches
/// the screen), instead of the draw being skipped, to see whether skipping
/// it is what leaves the Chao's head undrawn in his stages.
pub(super) fn walk_unseen(c: &mut Cpu, m: &mut Mem) -> bool {
    if LAST_EXEC.get() != KNUX_EXEC || !std::env::var("SA2_CAP_ZERO").is_ok_and(|v| v == "1") {
        return false;
    }
    let cur = m.r32(NJ_CURRENT_MATRIX);
    let keep: [u32; 12] = std::array::from_fn(|k| m.r32(cur + 4 * k as u32));
    for r in 0..3 { for k in 0..3 { m.w32(cur + 4 * (4 * r + k) as u32, 0); } }
    original(c, m, CNK_CACHE_DRAW);
    for (k, v) in keep.into_iter().enumerate() { m.w32(cur + 4 * k as u32, v); }
    true
}

/// His task's second display (main.dol 0x8009ADFC) walks his body again with
/// the motion draw 0x8011E214, returning to 0x8009B358 / 0x8009B38C
/// (SA2_TRACE_EVDRAW in Wild Canyon: these two, every frame, outside his first
/// display 0x80099BA0 that the Chao's code already walks without drawing).
/// SA2_KNUX_LATE=1 (debug): left drawing.
pub(super) fn late_body(lr: u32) -> bool {
    // TEST only (SA2_KNUX_LATE=0 skips it): skipping it did not bring the Chao's head back.
    matches!(lr, 0x8009_B358 | 0x8009_B38C) && matches!(STEP.get(), Step::Live(_)) && std::env::var("SA2_KNUX_LATE").is_ok_and(|v| v == "0")
}

thread_local! {
    /// The Chao's two eye nodes and its root as this draw placed them (view space), for hit_log.
    static HIT_NODES: Cell<[[f32; 3]; 3]> = const { Cell::new([[0.0; 3]; 3]) };
}

/// SA2_HIT_LOG=1 (debug): keeps where the Chao's root (node 0) and eyes (18, 21) are drawn.
pub(super) fn hit_node(m: &Mem, i: usize) {
    let k = match i { 0 => 0, 18 => 1, 21 => 2, _ => return };
    if std::env::var_os("SA2_HIT_LOG").is_none() { return; }
    let x = mtx(m, m.r32(NJ_CURRENT_MATRIX));
    let mut n = HIT_NODES.get();
    n[k] = [x[0][3], x[1][3], x[2][3]];
    HIT_NODES.set(n);
}

/// SA2_HIT_LOG=1 (debug): a line a frame with, in view space, the Chao's root and eyes
/// and the hidden character's collision: his feet, the world's up, his land collision
/// (radius, height, centre height) and the shapes of his collision table (centre, three
/// sizes). test-build-inbox knux/hitdraw.py draws them over a still (the eyes give the
/// picture's scale). For every hidden character, in every stage.
pub(super) fn hit_log(m: &Mem, chao: u32) {
    if std::env::var_os("SA2_HIT_LOG").is_none() { return; }
    let (obj2, twp) = (m.r32(P1_OBJ2), m.r32(0x801E_7768));
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) || !(0x8000_0000..0x8C00_0000).contains(&twp) { return; }
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let f = |a: u32| f32::from_bits(m.r32(a));
    let view = |p: [f32; 3]| -> [f32; 3] { std::array::from_fn(|r| (0..3).map(|k| base[r][k] * p[k]).sum::<f32>() + base[r][3]) };
    let feet = view([f(twp + 0x14), f(twp + 0x18), f(twp + 0x1C)]);
    let up = [base[0][1], base[1][1], base[2][1]];
    let cwp = m.r32(twp + 0x2C);
    let mut shapes = String::new();
    if (0x8000_0000..0x8C00_0000).contains(&cwp) {
        let info = m.r32(cwp + 0xC);
        if (0x8000_0000..0x8C00_0000).contains(&info) {
            for k in 0..5u32 {
                let e = info + 0x30 * k;
                let c = view([f(e + 8), f(e + 0xC), f(e + 0x10)]);
                shapes += &format!(" shape{k} form {} on {} at {:.3} {:.3} {:.3} size {:.3} {:.3} {:.3}", m.r8(e + 1), u8::from(f(e + 8) != 0.0 || f(e + 0x10) != 0.0), c[0], c[1], c[2], f(e + 0x14), f(e + 0x18), f(e + 0x1C));
            }
        }
    }
    let n = HIT_NODES.get();
    let _ = chao;
    eprintln!("hit: f{} root {:.3} {:.3} {:.3} eyes {:.3} {:.3} {:.3} / {:.3} {:.3} {:.3} feet {:.3} {:.3} {:.3} up {:.4} {:.4} {:.4} land {:.3} {:.3} {:.3}{shapes}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), n[0][0], n[0][1], n[0][2], n[1][0], n[1][1], n[1][2], n[2][0], n[2][1], n[2][2],
        feet[0], feet[1], feet[2], up[0], up[1], up[2], f(obj2 + 0x130), f(obj2 + 0x134), f(obj2 + 0x140));
}

/// The Ninja draw state words the game's displays set and restore around
/// their draws (main.dol 0x8002B2D8..0x8002B35C: r13 -0x71D8 the 3D control
/// flags, -0x71DC / -0x71E0 the constant attribute and / or masks).
/// SA2_STATE_LOG=1 (debug): as the player's Chao's display finds them, every
/// two seconds.
pub(super) fn draw_state_log(c: &Cpu, m: &Mem) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 120 == 0 && std::env::var_os("SA2_STATE_LOG").is_some() {
        let g = |o: u32| m.r32(c.r[13].wrapping_sub(o));
        CHAO_REST.with_borrow(|r| if let Some(r) = r.as_ref() { eprintln!("chao-parts: f{f} vertices per node {:?}", r.verts.iter().map(Vec::len).collect::<Vec<_>>()); });
        eprintln!("chao-state: f{f} control3d {:#x} attr and {:#x} or {:#x} saved {:#x} {:#x} {:#x} r13 {:#x}", g(0x71D8), g(0x71DC), g(0x71E0), g(0x7970), g(0x7968), g(0x796C), c.r[13]);
    }
}

thread_local! {
    /// His root node's height over his own position at the last draw (world units).
    static SINK: Cell<f32> = const { Cell::new(0.0) };
}

/// His action modes (task work byte 0) in which his display draws him sunk
/// into the ground: 61 digging in, 62 coming back out (seen in Wild Canyon,
/// SA2_KNUX_LOG: his position stays on the ground while his root node goes
/// down and up again).
pub(super) fn digging(mode: u8) -> bool {
    matches!(mode, 61 | 62)
}

/// At the Chao's node 0 in a level, before its placement is taken: while he
/// digs, the Chao goes into the ground as far as his root node is under his
/// own position (SKEL is in world space; both numbers are the game's).
pub(super) fn at_root(m: &mut Mem) {
    let data1 = RIDER.get();
    let Some(root) = SKEL.with_borrow(|s| ((s.len() == 62 && crate::chao_player::skel_site("k2"))).then(|| s[0])) else { return };
    if data1 == 0 || LAST_EXEC.get() != KNUX_EXEC {
        return;
    }
    let sink = root[1][3] - f32::from_bits(m.r32(data1 + 0x18));
    SINK.set(sink);
    let on = std::env::var("SA2_KNUX_SINK").map_or(true, |v| v != "0");
    // His root node from his own position, in the world.
    let f = |o: u32| f32::from_bits(m.r32(data1 + o));
    let off = [root[0][3] - f(0x14), root[1][3] - f(0x18), root[2][3] - f(0x1C)];
    let (base, cur) = (mtx(m, m.r32(NJ_BASE_MATRIX)), m.r32(NJ_CURRENT_MATRIX));
    let mode = m.r8(data1);
    if digging(mode) && on {
        // World up in view space is the base matrix's y column.
        for r in 0..3u32 {
            let a = cur + 4 * (4 * r + 3);
            m.w32(a, (f32::from_bits(m.r32(a)) + base[r as usize][1] * sink).to_bits());
        }
    } else if wall_digging(mode) && on {
        // On a wall he goes into it along the wall's own direction: the Chao moves as far as
        // his root has moved from where it was against his position when the dig began.
        let was = WALL_OFF.get();
        let d = [off[0] - was[0], off[1] - was[1], off[2] - was[2]];
        for r in 0..3usize {
            let a = cur + 4 * (4 * r as u32 + 3);
            m.w32(a, (f32::from_bits(m.r32(a)) + (0..3).map(|k| base[r][k] * d[k]).sum::<f32>()).to_bits());
        }
        WALL_SINK.set((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
    } else {
        WALL_OFF.set(off);
        WALL_SINK.set(0.0);
    }
}

thread_local! {
    /// His root from his position on the last frame he was not digging into a wall.
    static WALL_OFF: Cell<[f32; 3]> = const { Cell::new([0.0; 3]) };
    /// How far the Chao is moved into the wall this frame (logged).
    pub(super) static WALL_SINK: Cell<f32> = const { Cell::new(0.0) };
}

/// His action modes digging on a wall: 65 going in, 66 coming back out (seen in Wild
/// Canyon, SA2_KNUX_LOG: animation states 95, 96 then 98).
pub(super) fn wall_digging(mode: u8) -> bool {
    matches!(mode, 65 | 66)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn own_motions_are_the_four_approved() {
        assert_eq!(own_motion(109), Some((582, false)));
        assert_eq!(own_motion(110), Some((582, true)));
        assert_eq!(own_motion(105), Some((130, false)));
        assert_eq!(own_motion(100), Some((155, false)));
        for s in [0, 90, 91, 92, 95, 96, 111, 138] { assert_eq!(own_motion(s), None, "{s}: overlay"); }
    }
    #[test]
    fn only_player_one_as_rouge_becomes_knuckles() {
        assert_eq!(swapped_char(0, 5, false), 4);
        assert_eq!(swapped_char(0, 5, true), 5);
        assert_eq!(swapped_char(1, 5, false), 5);
        for ch in [0, 1, 2, 3, 4, 6, 7] { assert_eq!(swapped_char(0, ch, false), ch); }
    }
    #[test]
    fn link_frames_from_his_rows_blend() {
        assert_eq!(link_frames(1.0), 1); // punch 1, 2
        assert_eq!(link_frames(0.25), 4); // glide, climb
        assert_eq!(link_frames(0.0625), 16);
        assert_eq!(link_frames(0.0), 1);
        assert_eq!(link_frames(f32::NAN), 1);
    }
    #[test]
    fn an_arm_pairs_with_the_other_arm_not_the_other_shoulder() {
        // root 0; shoulders 1 and 3 with arms 2 and 4 resting on the same spots; a midline node 5.
        let pos = [[0.0, 0.0, 0.0], [0.5, 2.0, 0.1], [0.5, 2.0, 0.1], [-0.5, 2.0, 0.1], [-0.5, 2.0, 0.1], [0.0, 3.0, 0.0]];
        let parent = [None, Some(0), Some(1), Some(0), Some(3), Some(0)];
        assert_eq!(super::super::partners_of(&pos, &parent), vec![0, 3, 4, 1, 2, 5]);
    }
    #[test]
    fn the_under_motion_holds_until_six_still_frames() {
        assert_eq!(under_motion(0, 119), 119);
        assert_eq!(under_motion(5, 578), 578);
        assert_eq!(under_motion(6, 119), 0);
        assert!(spins(111) && spins(115) && spins(116) && !spins(109) && !spins(110) && !spins(0));
        assert!(runs(8) && runs(9) && runs(10) && !runs(7) && !runs(6) && !runs(0) && !runs(11));
        // His 8-frame run at frame 6 = three quarters through: frame 3 of a 4-frame skate cycle.
        assert_eq!(run_frame(6.0, 8.0, 4.0), 3.0);
        assert_eq!(run_frame(9.0, 8.0, 4.0), 0.5);
        assert_eq!(run_frame(1.0, 0.0, 4.0), 0.0);
    }
    #[test]
    fn only_the_dig_modes_sink() {
        for mode in 0..=255u8 { assert_eq!(digging(mode), mode == 61 || mode == 62); }
        for mode in 0..=255u8 { assert_eq!(wall_digging(mode), mode == 65 || mode == 66); }
    }
    #[test]
    fn the_chaos_fist_is_out_when_his_is() {
        // Karate 582 is frames 0..8; his punch 1 is 8 frames, his punch 2 12.
        for (state, len) in [(109u16, 8.0f32), (110, 12.0)] {
            let (his, chao) = hit_pair(state).unwrap();
            assert!((punch_frame(his, len, 0.0, 8.0, hit_pair(state)) - chao).abs() < 1e-4);
            assert_eq!(punch_frame(0.0, len, 0.0, 8.0, hit_pair(state)), 0.0);
            assert_eq!(punch_frame(len, len, 0.0, 8.0, hit_pair(state)), 8.0);
        }
        assert_eq!(hit_pair(111), None);
    }
    #[test]
    fn only_his_item_draw_sites_are_skipped() {
        for lr in ITEM_DRAWS { assert!(ITEM_DRAWS.contains(&lr)); }
        // His body's motion draws return to these: never an item site.
        for lr in [0x8009_A070u32, 0x8009_A0D8, 0x8009_A10C] { assert!(!ITEM_DRAWS.contains(&lr)); }
    }
    #[test]
    fn overlay_goes_out_and_in_without_a_step() {
        let mut p = 1.0;
        let mut n = 0;
        while p > 0.0 { let q = overlay_step(p, true); assert!(p - q < 0.07); p = q; n += 1; }
        assert_eq!(n, 17); // 0.28 s
        while p < 1.0 { let q = overlay_step(p, false); assert!(q - p < 0.05); p = q; n += 1; }
        assert_eq!(n, 17 + 26); // 0.42 s
    }
    #[test]
    fn punch_frame_meets_at_the_hit_and_the_ends() {
        // His 20-frame punch hitting at 6, the Chao's 100..136 hitting 12 in.
        let f = |h| punch_frame(h, 20.0, 100.0, 136.0, Some((6.0, 12.0)));
        assert_eq!(f(0.0), 100.0);
        assert_eq!(f(6.0), 112.0);
        assert_eq!(f(20.0), 136.0);
        assert!(f(3.0) > 100.0 && f(3.0) < 112.0 && f(13.0) > 112.0 && f(13.0) < 136.0);
        assert_eq!(f(99.0), 136.0);
        // No hit pair: the plain share.
        assert_eq!(punch_frame(10.0, 20.0, 100.0, 136.0, None), 118.0);
        assert_eq!(punch_frame(f32::NAN, 20.0, 100.0, 136.0, None), 100.0);
    }
}
