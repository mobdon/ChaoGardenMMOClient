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

/// The hit frames (his motion's, the Chao karate 582's from its start), by
/// his motion's length: where each one's punching hand is furthest forward,
/// measured with SA2_KNUX_LOG=1 (`knux:` lines, columns `his hands fwd` and
/// `chao hands fwd`). None until measured: the plain share.
fn hit_pair(_his_len: u32) -> Option<(f32, f32)> {
    None
}

/// Punch 1 and 2: the Chao's karate frame follows his motion frame.
pub(super) fn punch_lock(m: &mut Mem, chao: u32) {
    let cw = m.r32(chao + 0x34);
    let (mot, his) = RIDER_MOTION.get();
    if cw == 0 || !(0x8000_0000..0x8C00_0000).contains(&mot) || std::env::var("SA2_PUNCH_LOCK").is_ok_and(|v| v == "0") {
        return;
    }
    let ctrl = cw + CHAO_MOTION_CTRL;
    // Not while the Chao's motion is still linking in (flag 0x2), as stride_lock.
    if m.r16(ctrl) & 2 != 0 {
        return;
    }
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    let n = m.r32(mot + 4);
    let to = punch_frame(his, n as f32, f(m, ctrl + 0x20), f(m, ctrl + 0x24), hit_pair(n));
    m.w32(ctrl + 0x1C, to.to_bits());
}

/// B on the ground made him pick the Chao up (action mode 19, animation
/// 31; test copy 2026-11-24: 3 presses, 3 pick-ups, no punch): the Chao
/// stands on his spot and its body collision can be held. ChaoMain
/// AL_DisableTouch (.text 0x5A60 = CCL_Disable(task, 0) and (task, 2)) each
/// frame, as AL_DisableNade is. MMO rule: the player's own Chao cannot be
/// picked up by the character it stands in for.
/// SA2_KNUX_PICKUP=1 (debug): left as it was.
pub(super) fn no_pick_up(c: &mut Cpu, m: &mut Mem, chao: u32) {
    if !std::env::var("SA2_KNUX_PICKUP").is_ok_and(|v| v == "1") {
        call(c, m, rt::sec(m, 60, 1) + 0x5A60, &[chao]);
    }
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

/// SA2_KNUX_UPGRADES=hex (test): OR into his upgrades word (obj2 +0x10), to
/// reach moves the test save has no item for (0x400 = Shovel Claws, dig).
pub(super) fn test_upgrades(m: &mut Mem, obj2: u32) {
    if let Some(bits) = std::env::var("SA2_KNUX_UPGRADES").ok().and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok()) {
        m.w32(obj2 + 0x10, m.r32(obj2 + 0x10) | bits);
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
    eprintln!("knux: f{} state {state} mode {} his motion len {n} frame {his:.2} y {:.2} pelvis {pelvis:.2} sink {:.2} his hands fwd {:.2} {:.2} | chao motion {} frame {:.2} of {:.0}..{:.0} link {} hands fwd {:.2} {:.2} overlay {:.2} skel {}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r8(data1), f(data1 + 0x18), SINK.get(), hands[0], hands[1],
        LAST_MOTION.get(), f(ctrl + 0x1C), f(ctrl + 0x20), f(ctrl + 0x24), m.r16(ctrl) & 2 != 0, ch[0], ch[1], OVERLAY.get(), SKEL.with_borrow(|s| s.len()));
}

thread_local! {
    /// His root node's height over the Chao's placement at the last draw
    /// (the Chao's own units along its up axis; measured, not yet applied).
    static SINK: Cell<f32> = const { Cell::new(0.0) };
}

/// At the Chao's node 0 in a level: his root node against the Chao's
/// placement (both view space, this draw).
pub(super) fn at_root(m: &Mem) {
    let p = mtx(m, m.r32(NJ_CURRENT_MATRIX));
    if let Some(root) = SKEL.with_borrow(|s| s.first().copied()) {
        let d = inv_mul(&p, &root);
        SINK.set(d[1][3]);
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
