//! The 2P ring rank of main.dol (GSNE8P fn_80063E34) and the routines it
//! reads the race state with: the start point of a player (fn_8001DB4C),
//! the goal point of the stage (fn_800736F0), the 2P mode word
//! (fn_8001DDA8) and the character a player plays (0x80038108).
//!
//! The tables they search are main.dol `.data` (the goal points at
//! 0x801BB2AC, the start points of each character from 0x8019109C), so the
//! port reads them through [`Ram`] like any other memory; only the jump
//! table that picks a character's start table (0x80195034) is written out.
//! The distances use the shared native njDistanceP2P port
//! (`player_range::distance`, 0x8011B65C, with sqrtf 0x8002F15C).
use super::{slot, Ram, GAME, PLAYER_PWP, PLAYER_TWP, STAGE_NUM};
use crate::player_range::distance;

/// r13 - 0x7A98 / - 0x7A94 (0x803AD828 / 0x803AD82C): each 2P player's
/// mode word (what decides how the rank is measured).
pub const MODE_WORDS: [u32; 2] = [0x803a_d828, 0x803a_d82c];
/// `lbl_801E7788`: a per-player pointer whose presence fn_8001DB4C needs.
pub const PLAYER_START: u32 = 0x801e_7788;
/// `lbl_801BB2AC` (.data): eleven goal points (stage word, x, y, z).
pub const GOALS: u32 = 0x801b_b2ac;
/// r13 - 0x7838 (0x803ADA88): the pointer to the 2P race block the rank
/// reads in modes 4 and 5 (each player's count at +0xA, three slots at
/// +0x2C, +0x3C, +0x4C).
pub const RACE: u32 = 0x803a_da88;

/// The stage a start table ends with.
const STAGE_END: i32 = 0x5b;
/// One start table entry: the stage halfword, three angles (+2), three
/// points (+8, 12 bytes each).
const START_ENTRY: u32 = 0x2c;

/// _rename_GetPlayerCharacter(n) (0x80038108): playerpwp\[n\] +1, or -1
/// without a pwp.
pub fn get_player_character(ram: &mut impl Ram, n: i32) -> i32 {
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if pwp == 0 {
        -1
    } else {
        ram.rs8(pwp + 1)
    }
}

/// fn_8001DDA8(n): the 2P mode word of player 0, or of player 1 for any
/// other number.
pub fn fn_8001dda8(ram: &mut impl Ram, n: i32) -> u32 {
    ram.r32(MODE_WORDS[usize::from(n != 0)])
}

/// The start table of each character (fn_8001DB4C's jump table at
/// 0x80195034, cases 0 to 10; 3, 8 and anything else have none).
/// The code forms each address from 0x801872E8 (`lis -0x7FE8`, `addi
/// 0x72E8`) with `addis 1` and the case's `addi` (0x8019109C is -0x624C).
fn start_table(chr: i32) -> u32 {
    match chr {
        0 => 0x8019_109c,
        1 => 0x8019_1438,
        2 => 0x8019_1d54,
        4 => 0x8019_1750,
        5 => 0x8019_1a68,
        6 => 0x8019_2174,
        7 => 0x8019_1dac,
        9 => 0x8019_24e4,
        10 => 0x8019_2568,
        _ => 0,
    }
}

/// What fn_8001DB4C leaves in its two buffers and returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartPoint {
    /// 0 when the player has no `PLAYER_START` pointer, else 1.
    pub ret: u32,
    /// The point (raw words; 0.0 each when nothing was found).
    pub pos: [u32; 3],
    /// The Y angle (the other two angles stay 0).
    pub ang_y: u32,
}

/// fn_8001DB4C(n, pos, ang) without the buffers: both start zeroed (0.0
/// from .sdata2 0x803AE970, and 0); without a `PLAYER_START` pointer it
/// returns 0. Otherwise the character's start table is walked for the
/// stage (r13 - 0x7AA0) up to the stage 0x5B entry; the entry's column is 0
/// in 1P, except on stages 0x13, 0x14, 0x1D, 0x21 and 0x2A, and in 2P,
/// where player 0 takes column 1 and the others column 2.
/// The entry is read only for the buffers the caller has (`want_pos`,
/// `want_ang`).
pub fn start_point(ram: &mut impl Ram, n: i32, want_pos: bool, want_ang: bool) -> StartPoint {
    let mut out = StartPoint {
        ret: 0,
        pos: [0; 3],
        ang_y: 0,
    };
    if ram.r32(slot(PLAYER_START, n)) == 0 {
        return out;
    }
    let mut t = start_table(get_player_character(ram, n));
    let stage = ram.rs16(STAGE_NUM);
    let col = if ram.rs8(GAME + 0x1e) != 0 || [0x13, 0x2a, 0x14, 0x1d, 0x21].contains(&stage) {
        if n == 0 {
            1
        } else {
            2
        }
    } else {
        0
    };
    out.ret = 1;
    let stage = ram.rs16(STAGE_NUM);
    while t != 0 {
        let s = ram.rs16(t);
        if s == STAGE_END {
            break;
        }
        if s == stage {
            if want_ang {
                out.ang_y = ram.rs16(t + 2 + 2 * col) as u32;
            }
            let p = t + 8 + 12 * col;
            if want_pos {
                out.pos = [ram.r32(p), ram.r32(p + 4), ram.r32(p + 8)];
            }
            break;
        }
        t = t.wrapping_add(START_ENTRY);
    }
    out
}

/// fn_8001DB4C(n, pos, ang) (0x8001DB4C): [`start_point`] written to the
/// buffers that are not null (`ang` gets all three words).
pub fn fn_8001db4c(ram: &mut impl Ram, n: i32, pos: u32, ang: u32) -> u32 {
    let p = start_point(ram, n, pos != 0, ang != 0);
    if pos != 0 {
        for (k, w) in p.pos.into_iter().enumerate() {
            ram.w32(pos + 4 * k as u32, w);
        }
    }
    if ang != 0 {
        ram.w32(ang, 0);
        ram.w32(ang + 4, p.ang_y);
        ram.w32(ang + 8, 0);
    }
    p.ret
}

/// fn_800736F0(n, out, _) without the buffer: the goal point of the stage
/// (GetStageNum, sign-extended) from the eleven [`GOALS`]; 0.0 each (.sdata2
/// 0x803AF568) when the stage has none. The player number is not used.
/// The point is read only when `want` (a buffer to copy it to).
pub fn goal_point(ram: &mut impl Ram, want: bool) -> [u32; 3] {
    let stage = super::get_stage_num(ram);
    for k in 0..11u32 {
        let e = GOALS + 16 * k;
        if ram.r32(e) as i32 == stage {
            if !want {
                break;
            }
            return [ram.r32(e + 4), ram.r32(e + 8), ram.r32(e + 0xc)];
        }
    }
    [0; 3]
}

/// fn_800736F0(n, out, _) (0x800736F0): [`goal_point`] into `out` when it
/// is not null.
pub fn fn_800736f0(ram: &mut impl Ram, out: u32) {
    // The stage is read even with a null buffer.
    let g = goal_point(ram, out != 0);
    if out != 0 {
        for (k, w) in g.into_iter().enumerate() {
            ram.w32(out + 4 * k as u32, w);
        }
    }
}

/// .sdata2 0x803AF348 / 0x803AF34C / 0x803AF350: the race-progress bands.
const RACE_BANDS: [f32; 3] = [-0.2, -0.1, -0.05];
/// .sdata2 0x803AF354 / 0x803AF358 / 0x803AF35C: the mech HP bands.
const HP_BANDS: [f32; 3] = [7.5, 5.0, 2.5];

fn v3(w: [u32; 3]) -> [f32; 3] {
    w.map(f32::from_bits)
}

fn twp_pos(ram: &mut impl Ram, twp: u32) -> [f32; 3] {
    [ram.rf(twp + 0x14), ram.rf(twp + 0x18), ram.rf(twp + 0x1c)]
}

/// fn_80063E34(p) (0x80063E34): how far player `p` (0 or 1) is behind the
/// other in 2P, 0 to 3, plus a bonus when GAME +0x5F is 2 (4 for the
/// walkers, pwp +1 of 4 or 5; 3 for the others). Outside 2P it returns `p`
/// itself (quirk: r3 is left alone); with GAME +0x5F 0, another player
/// number or a missing twp, 0. By the player's mode word (fn_8001DDA8):
///
/// * 0, 1: the race. `(d(start, p) - d(start, other)) / d(goal, start)`
///   below -0.2, -0.1 or -0.05 gives 3, 2 or 1; otherwise 0 (no bonus).
/// * 4, 5: the race block: 0 (no bonus) when `p`'s count is above the
///   other's or `p` holds more of the three slots marked -2; 2 when the
///   other holds more.
/// * 6, 7: the mech HP: the other's HP minus `p`'s at least 7.5, 5.0 or
///   2.5 gives 3, 2 or 1; otherwise 0 (no bonus).
/// * anything else: 0.
pub fn fn_80063e34(ram: &mut impl Ram, p: i32) -> i32 {
    if ram.rs8(GAME + 0x1e) == 0 {
        return p;
    }
    if ram.rs8(GAME + 0x5f) == 0 {
        return 0;
    }
    let o = match p {
        0 => 1,
        1 => 0,
        _ => return 0,
    };
    let ptwp = ram.r32(slot(PLAYER_TWP, p));
    if ptwp == 0 {
        return 0;
    }
    let otwp = ram.r32(slot(PLAYER_TWP, o));
    if otwp == 0 {
        return 0;
    }
    let mut rank = 0;
    match fn_8001dda8(ram, p) as i32 {
        0 | 1 => {
            let goal = v3(goal_point(ram, true));
            let start = v3(start_point(ram, p, true, false).pos);
            let pp = twp_pos(ram, ptwp);
            let dp = distance(start, pp);
            let op = twp_pos(ram, otwp);
            let d_o = distance(start, op);
            let d = distance(goal, start);
            let r = (dp - d_o) / d;
            rank = match RACE_BANDS.iter().position(|&b| r < b) {
                Some(k) => 3 - k as i32,
                None => return 0,
            };
        }
        4 | 5 => {
            let race = ram.r32(RACE);
            let a = ram.rs8(race + 0xa + p as u32);
            let b = ram.rs8(race + 0xa + o as u32);
            if a > b {
                return 0;
            }
            let (mut mine, mut theirs) = (0, 0);
            for k in 0..3u32 {
                let s = race + 0x2c + 0x10 * k;
                if ram.rs8(s) == -2 {
                    let who = ram.rs8(s + 1);
                    if who == p {
                        mine += 1;
                    } else if who == o {
                        theirs += 1;
                    }
                }
            }
            if mine > theirs {
                return 0;
            }
            if theirs > mine {
                rank = 2;
            }
            // Never true after the count check above; kept as compiled.
            if a > b.wrapping_add(1) {
                rank += 2;
            } else if a > b {
                rank += 1;
            }
        }
        6 | 7 => {
            let opwp = ram.r32(slot(PLAYER_PWP, o));
            let ppwp = ram.r32(slot(PLAYER_PWP, p));
            let d = ram.rf(opwp + 0x48) - ram.rf(ppwp + 0x48);
            rank = match HP_BANDS.iter().position(|&b| d >= b) {
                Some(k) => 3 - k as i32,
                None => return 0,
            };
        }
        _ => {}
    }
    let pwp = ram.r32(slot(PLAYER_PWP, p));
    let k = ram.rs8(pwp + 1);
    if ram.r8(GAME + 0x5f) == 2 {
        rank += if (4..6).contains(&k) { 4 } else { 3 };
    }
    rank
}
