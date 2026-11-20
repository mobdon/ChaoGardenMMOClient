//! main.dol's shared enemy helpers (GSNE8P addresses), as the stage RELs'
//! enemies call them. `tests/enemy_lib_ppc.py` runs the originals under
//! Unicorn and `tests/enemy_lib.rs` compares these bit for bit.
use super::{EnemyTask, EnemyWork, PlayerView, Rand, V3};
use crate::{
    msl_math::{atan2f, gekko_frsqrte},
    ninja_matrix::{Axis, MatrixStack},
};

/// BAMS per radian: the double at r2-0x7CD8 in main.dol (the stage RELs
/// carry the same bits, stg09D.rel section 4 +0x1958).
pub const BAMS_PER_RADIAN: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);

/// `DiffAngle` (0x8002B8E0): |(short)(b - a)| of the low 16 bits.
pub fn diff_angle(a: i32, b: i32) -> i32 {
    let d = ((b as u32 & 0xffff).wrapping_sub(a as u32 & 0xffff)) as u16 as i16 as i32;
    (d.unsigned_abs() as i32) & 0xffff
}

/// `AdjustAngle` (0x8002B918): turn `from` toward `to` by at most `step`.
/// Returns the low 16 bits of the result (or `from` unchanged, all 32
/// bits, when already within `step`).
pub fn adjust_angle(from: i32, to: i32, step: i32) -> i32 {
    let a = from as u32 & 0xffff;
    let b = to as u32 & 0xffff;
    let d = b.wrapping_sub(a) as u16 as i16 as i32;
    if d <= step && d >= step.wrapping_neg() {
        return b as i32;
    }
    let r = if d & 0x8000 != 0 {
        a.wrapping_sub(step as u32)
    } else {
        a.wrapping_add(step as u32)
    };
    (r & 0xffff) as i32
}

/// `njRoundOff` (0x80112320): MSL floorf on |x| with the sign put back,
/// which is truncation toward zero.
pub fn round_off(x: f32) -> f32 {
    x.trunc()
}

/// `njFraction` (0x8011217C): x minus its integer part, or 0 when x is
/// infinite or already integral (|x| >= 2^24 included). NaN, zero and
/// subnormals come back unchanged.
pub fn fraction(x: f32) -> f32 {
    if x.is_nan() || x == 0.0 || x.is_subnormal() {
        return x;
    }
    if x.is_infinite() {
        return 0.0;
    }
    let t = crate::ninja_trig::fctiwz(x) as f32;
    let f = x - t;
    if f.to_bits() == 0 || (x.to_bits() & 0x7f80_0000) >= 0x4b80_0000 {
        return 0.0;
    }
    f
}

/// MSL `sqrtf` as main.dol has it (0x8002F15C): three double-precision
/// Newton steps on Gekko's `frsqrte`, with fused `fnmsub`. x <= 0 (and
/// NaN) is returned unchanged.
pub fn sqrtf_dol(x: f32) -> f32 {
    if x <= 0.0 || x.is_nan() {
        return x;
    }
    let x = x as f64;
    let mut e = gekko_frsqrte(x);
    for i in 0..3 {
        let sq = e * e;
        let h = 0.5 * e;
        // fnmsub: -(x*sq - 3), fused, rounded to double.
        let t = -(x.mul_add(sq, -3.0));
        e = h * t;
        if i == 2 {
            return (x * e) as f32;
        }
    }
    unreachable!()
}

/// The REL's own weak `sqrtf` (stg09D.rel 0xBEDC): the same Newton steps
/// with a separate (unfused) multiply and subtract.
pub fn sqrtf_rel(x: f32) -> f32 {
    if x <= 0.0 || x.is_nan() {
        return x;
    }
    let x = x as f64;
    let mut e = gekko_frsqrte(x);
    for _ in 0..3 {
        let h = 0.5 * e;
        e = h * (3.0 - x * (e * e));
    }
    (x * e) as f32
}

/// `njDistanceP2P` (0x8011B65C).
pub fn distance(a: V3, b: V3) -> f32 {
    let z = (a[2] - b[2]) * (a[2] - b[2]);
    let x = (a[0] - b[0]) * (a[0] - b[0]);
    let y = (a[1] - b[1]) * (a[1] - b[1]);
    let s = z + (x + y);
    if s < f32::from_bits(0x3ccc_cccd) {
        0.0
    } else {
        sqrtf_dol(s)
    }
}

/// `fn_8002AE5C`: move `v` toward `target` by `step` without passing it.
pub fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v > target {
        let n = v - step;
        if n >= target {
            n
        } else {
            target
        }
    } else if v < target {
        let n = v + step;
        if n <= target {
            n
        } else {
            target
        }
    } else {
        v
    }
}

/// The live-enemy count per switch group (main.dol 0x801E4AB8).
pub type GroupCounts = [u8; 256];

/// fn_80018D28: the enemy work's defaults (syCalloc(1, 0x210), then these
/// fields). `home` is the task position at the time.
pub fn new_work(task: &EnemyTask) -> EnemyWork {
    EnemyWork {
        home: task.pos,
        f_cc: 15.0,
        probe_offset: [0.0, 5.0, 0.0],
        f_c8: 10.0,
        hover: -1.0,
        f_d4: 50.0,
        f_dc: 0.9,
        f_e0: 0.8,
        f_e4: 0.1,
        f_d8: 0.7,
        i_e8: 0x400,
        f_54: 3.0,
        b_44: -1,
        group: -1,
        i_1d4: 0x38e3,
        f_1d8: 22500.0,
        notice_range_sq: 1225.0,
        turn_rate: 0x100,
        flags: 0x200,
        ..EnemyWork::default()
    }
}

/// fn_80018C88: the work set-up for enemies that take part in switch
/// groups. The group is the low byte of the task's Rotation.y, which keeps
/// only its high byte; the group's live count goes up. (Its fn_8008CEB0
/// call for groups other than -1 is left to the host.)
pub fn new_counted_work(task: &mut EnemyTask, counts: &mut GroupCounts) -> EnemyWork {
    let mut w = new_work(task);
    w.group = task.ang[1] as i8;
    task.ang[1] &= 0xff00;
    // A group of -1 skips only the fn_8008CEB0 call; it is counted too.
    let c = &mut counts[w.group as u8 as usize];
    *c = c.wrapping_add(1);
    w.flags |= 0x20;
    w
}

/// fn_80018B88 (the group part): the enemy leaves its group's count.
pub fn leave_group(w: &mut EnemyWork, counts: &mut GroupCounts) {
    if w.flags & 0x20 != 0 {
        let c = &mut counts[w.group as u8 as usize];
        *c = c.wrapping_sub(1);
        w.flags &= !0x20;
    }
}

/// fn_80017BE4: the current position becomes home.
pub fn set_home(task: &EnemyTask, w: &mut EnemyWork) {
    w.home = task.pos;
}

/// fn_80017B94 (minus its fn_8008C8F8 call for grouped enemies): keep the
/// position the frame started from.
pub fn save_position(task: &EnemyTask, w: &mut EnemyWork) {
    w.last_pos = task.pos;
}

/// fn_80015B18: the home offset (|1000 * frac(Scale.z)|, 0, 0) turned by
/// the task's yaw, and Scale.z rounded toward zero.
pub fn home_offset(stack: &mut MatrixStack, task: &mut EnemyTask, w: &mut EnemyWork) {
    let off = (1000.0f32 * fraction(task.scl[2])).abs();
    stack.push();
    *stack.current_mut() = crate::ninja_matrix::IDENTITY;
    stack.rotate(Axis::Y, task.ang[1]);
    w.home_offset = stack.calc_vector([off, 0.0, 0.0]);
    stack.pop();
    task.scl[2] = round_off(task.scl[2]);
}

/// fn_80017A90: squared horizontal distance from home.
pub fn home_distance_sq(task: &EnemyTask, w: &EnemyWork) -> f32 {
    let x = w.home[0] - task.pos[0];
    let z = w.home[2] - task.pos[2];
    x * x + z * z
}

/// Whether players are tested at all: the byte at 0x801CC168+0x39 and the
/// one at +0x42 must both differ from 1.
fn players_tested(game_39: u8, game_42: u8) -> bool {
    game_39 != 1 && game_42 != 1
}

/// fn_800187C4 / fn_800188B0 / fn_8001897C: which player (1 or 2, the
/// last one wins) is horizontally within the notice range of a point:
/// home plus the home offset, home, or the enemy itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeFrom {
    HomeOffset,
    Home,
    Body,
}

pub fn player_in_range(
    from: NoticeFrom,
    task: &EnemyTask,
    w: &EnemyWork,
    players: &[Option<PlayerView>; 2],
    game_39: u8,
    game_42: u8,
) -> i32 {
    if !players_tested(game_39, game_42) {
        return 0;
    }
    let mut hit = 0;
    for (i, p) in players.iter().enumerate() {
        let Some(p) = p else { continue };
        let (dx, dz) = match from {
            NoticeFrom::HomeOffset => (
                (p.pos[0] - w.home[0]) - w.home_offset[0],
                (p.pos[2] - w.home[2]) - w.home_offset[2],
            ),
            NoticeFrom::Home => (p.pos[0] - w.home[0], p.pos[2] - w.home[2]),
            NoticeFrom::Body => (p.pos[0] - task.pos[0], p.pos[2] - task.pos[2]),
        };
        if dx * dx + dz * dz < w.notice_range_sq {
            hit = i as i32 + 1;
        }
    }
    hit
}

/// fn_80037C84: the index (0..8) of the nearest player task, by
/// `njScalor` of the offset; 0 when there are none. Ties keep the first.
pub fn nearest_player(players: &[Option<PlayerView>; 2], from: V3) -> usize {
    let mut best = 1.0e7f32;
    let mut index = 0;
    for (i, p) in players.iter().enumerate() {
        let Some(p) = p else { continue };
        let d = scalor([from[0] - p.pos[0], from[1] - p.pos[1], from[2] - p.pos[2]]);
        if best > d {
            best = d;
            index = i;
        }
    }
    index
}

/// `njScalor` (0x801151F4): fres(frsqrte(x*x + y*y + z*z)), with Gekko's
/// estimates for both.
pub fn scalor(v: V3) -> f32 {
    crate::ninja_vector::scalor(v)
}

/// `njUnitVector` with its return value: Gekko's `fres` of the estimate
/// (the length, roughly) and the scaled vector.
pub fn unit_vector_len(v: V3) -> (f32, V3) {
    let (n, len) = crate::ninja_vector::unit_vector(v);
    (len, n)
}

/// `njUnitVector` (0x80115D24): v times frsqrte of its squared length
/// (PowerPC's positive default NaN for a zero vector).
pub fn unit_vector(v: V3) -> V3 {
    crate::ninja_vector::unit_vector(v).0
}

/// fn_80017918: when the enemy is `range` or more from home (3D distance),
/// pull it back onto the sphere of that radius around home, in X and Z.
pub fn leash(task: &mut EnemyTask, w: &EnemyWork, range: f32) {
    if distance(w.home, task.pos) < range {
        return;
    }
    let d = unit_vector([
        task.pos[0] - w.home[0],
        task.pos[1] - w.home[1],
        task.pos[2] - w.home[2],
    ]);
    // njAddVector(home, scaled) with the Y component zeroed.
    let v = [d[0] * range, 0.0, d[2] * range];
    task.pos[0] = v[0] + w.home[0];
    task.pos[2] = v[2] + w.home[2];
}

/// fn_800176AC: aim at player `index` and turn toward it by the work's
/// turn rate. Returns the remaining angle (`DiffAngle` before the turn).
pub fn face_player(task: &mut EnemyTask, w: &mut EnemyWork, player: V3) -> i32 {
    let a = atan2f(player[0] - task.pos[0], player[2] - task.pos[2]);
    // fctiwz on the double product (no rounding to single first).
    w.target_yaw = fctiwz_f64(BAMS_PER_RADIAN * a as f64).wrapping_sub(0x4000);
    let left = diff_angle(w.target_yaw, task.ang[1]);
    task.ang[1] = adjust_angle(task.ang[1], w.target_yaw, w.turn_rate);
    left
}

/// `fctiwz` on a double: truncate toward zero, 0x80000000 out of range.
pub fn fctiwz_f64(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// `rand` (0x8010DC88): the MSL linear congruential generator; the state
/// is the global `next` at 0x803AD6A0.
impl Rand {
    pub fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        ((self.0 >> 16) & 0x7fff) as i32
    }
}

/// fn_80018A4C: a player (1 or 2, the first found) is within the notice
/// range in 3D, or within the outer range (+0x1D8, squared) and inside
/// the view angle (+0x1D4) of the enemy's facing.
pub fn player_in_view(
    task: &EnemyTask,
    w: &EnemyWork,
    players: &[Option<PlayerView>; 2],
    game_39: u8,
    game_42: u8,
) -> i32 {
    if !players_tested(game_39, game_42) {
        return 0;
    }
    for (i, p) in players.iter().enumerate() {
        let Some(p) = p else { continue };
        let dx = p.pos[0] - task.pos[0];
        let dy = p.pos[1] - task.pos[1];
        let dz = p.pos[2] - task.pos[2];
        let d = dz * dz + (dx * dx + dy * dy);
        if d < w.notice_range_sq {
            return i as i32 + 1;
        }
        if d < w.f_1d8 {
            let yaw = fctiwz_f64(BAMS_PER_RADIAN * atan2f(dx, dz) as f64);
            if diff_angle(yaw.wrapping_sub(0x4000), task.ang[1]) < w.i_1d4 {
                return i as i32 + 1;
            }
        }
    }
    0
}
