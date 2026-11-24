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
/// for him. SA2_SHOW_SONIC (debug: the character drawn) keeps them.
const ITEM_DRAWS: [u32; 5] = [0x8009_A1AC, 0x8009_A1E8, 0x8009_A20C, 0x8009_A23C, 0x8009_A260];

pub(super) fn item_draw(lr: u32) -> bool {
    ITEM_DRAWS.contains(&lr) && std::env::var_os("SA2_SHOW_SONIC").is_none()
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
    let (hands, pelvis) = SKEL.with_borrow(|s| if s.len() == 62 {
        let r = |k: usize| inv_mul(&s[0], &s[k]);
        ([-r(40)[0][3], -r(59)[0][3]], r(1)[1][3])
    } else { ([f32::NAN; 2], f32::NAN) });
    // The Chao's hands in its placement's space: it faces +z.
    let p = CHAO_PLACE.get();
    let ch = HANDS.with_borrow(|h| h.map(|x| x.map_or(f32::NAN, |x| inv_mul(&p, &x)[2][3])));
    let data1 = RIDER.get();
    eprintln!("knux: f{} state {state} mode {} his motion len {n} frame {his:.2} y {:.2} pelvis {pelvis:.2} sink {:.2} his hands fwd {:.2} {:.2} | chao motion {} frame {:.2} of {:.0}..{:.0} link {} next {:.2} of {:.0}..{:.0} hands fwd {:.2} {:.2} overlay {:.2} skel {}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r8(data1), f(data1 + 0x18), SINK.get(), hands[0], hands[1],
        LAST_MOTION.get(), f(ctrl + 0x1C), f(ctrl + 0x20), f(ctrl + 0x24), m.r16(ctrl) & 2 != 0, f(ctrl + 0x34), f(ctrl + 0x38), f(ctrl + 0x3C), ch[0], ch[1], OVERLAY.get(), SKEL.with_borrow(|s| s.len()));
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
    let Some(root) = SKEL.with_borrow(|s| (s.len() == 62).then(|| s[0])) else { return };
    if data1 == 0 || LAST_EXEC.get() != KNUX_EXEC {
        return;
    }
    let sink = root[1][3] - f32::from_bits(m.r32(data1 + 0x18));
    SINK.set(sink);
    if digging(m.r8(data1)) && std::env::var("SA2_KNUX_SINK").map_or(true, |v| v != "0") {
        // World up in view space is the base matrix's y column.
        let (base, cur) = (mtx(m, m.r32(NJ_BASE_MATRIX)), m.r32(NJ_CURRENT_MATRIX));
        for r in 0..3u32 {
            let a = cur + 4 * (4 * r + 3);
            m.w32(a, (f32::from_bits(m.r32(a)) + base[r as usize][1] * sink).to_bits());
        }
    }
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
    fn only_the_dig_modes_sink() {
        for mode in 0..=255u8 { assert_eq!(digging(mode), mode == 61 || mode == 62); }
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
