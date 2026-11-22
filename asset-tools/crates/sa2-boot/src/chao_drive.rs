//! Chao MMO glue: the player's Chao is the player (SA2_CHAO_SELF=1, Chao
//! World). No Sonic task moves it; the stick drives the Chao's own walk.
//!
//! Native parts (GSNE8P, ChaoMain .text offsets from the decomp symbols):
//! - AL_ForwardSpd (0x19FE4): twp pos += spd * (njSin, njCos)(angle Y), the
//!   call every Chao walk behaviour makes (e.g. fn_60_3BCD8 at 0x3BDB4).
//! - The Chao's own exec (0x31B80) then runs MOV_Control (0x8002F448, Velo +=
//!   Acc + Gravity, pos += Velo) and AL_DetectCollision (0x35D4): its floor and
//!   wall collision against the land, unchanged.
//! - AdjustAngle (0x8002B918) turns the heading, as MOV's turn-to-aim does.
//! - Speeds are the character's movement values: the physics row the game's
//!   0x8004F838 copies to playerwk +0xC0 (chao_player::physics_row puts
//!   Metal Sonic's row there, or chao_speed's): max_x_spd +0x10, jmp_y_spd
//!   +0x18, run_accel +0x3C, grd_frict +0x60.
//!
//! shortcut: speed is a plain approach to max_x_spd * stick (run_accel up,
//! grd_frict down), not Sonic's own ground physics; upgrade when the levels
//! need slopes/boost.
//! shortcut: the stick is turned into a world direction with the camera's
//! position from the view matrix (_nj_base_matrix), not the game's own analog
//! routine (not yet found on GC).
use sa2_recomp::rt::{Cpu, Mem};
use std::cell::Cell;

const AL_FORWARD_SPD: u32 = 0x1_9FE4;
const ADJUST_ANGLE: u32 = 0x8002_B918;
const NJ_BASE_MATRIX: u32 = 0x803A_E0C0;
const P1_OBJ2: u32 = 0x801E_7728;
/// shortcut: turn rate per frame (BAMS) and stick range are MMO choices.
const TURN: u32 = 0x1000;
const STICK_DEAD: f32 = 12.0;
const STICK_FULL: f32 = 72.0;
const PAD_A: u16 = 0x0100;
/// MOVE_WORK (task +0x38, decomp move.h): Velo y +0x04, Flag u16 +0x40.
const MWP_VELO_Y: u32 = 0x04;
const MWP_FLAG: u32 = 0x40;
const MOVE_FLAG_FLOOR: u16 = 0x0400;

thread_local! {
    static SPEED: Cell<f32> = const { Cell::new(0.0) };
    static LAST_BUTTONS: Cell<u16> = const { Cell::new(0) };
}

fn f(m: &Mem, a: u32) -> f32 {
    f32::from_bits(m.r32(a))
}

/// The stick (x right, y up) as a world angle Y (BAMS, AL_ForwardSpd's
/// convention: direction (sin, cos)), seen from the camera.
pub fn stick_angle(view: &[[f32; 4]; 3], at: [f32; 3], sx: f32, sy: f32) -> u32 {
    // View = [R | t] (column vectors): camera position = -R^T t.
    let cam: [f32; 3] = std::array::from_fn(|j| -(0..3).map(|i| view[i][j] * view[i][3]).sum::<f32>());
    let (fx, fz) = (at[0] - cam[0], at[2] - cam[2]);
    let l = (fx * fx + fz * fz).sqrt().max(1e-6);
    let (fx, fz) = (fx / l, fz / l);
    // Screen right in the world, from the view matrix's x row.
    let (rx, rz) = (view[0][0], view[0][2]);
    let rl = (rx * rx + rz * rz).sqrt().max(1e-6);
    let (rx, rz) = (rx / rl, rz / rl);
    let (dx, dz) = (fx * sy + rx * sx, fz * sy + rz * sx);
    ((dx.atan2(dz) / std::f32::consts::TAU * 65536.0).round() as i32 as u32) & 0xFFFF
}

pub fn drive(c: &mut Cpu, m: &mut Mem, chao: u32, text: u32) {
    let cw = m.r32(chao + 0x34);
    let mw = m.r32(chao + 0x38);
    let obj2 = m.r32(P1_OBJ2);
    if cw == 0 || !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    let p = crate::pad::port0();
    let (sx, sy) = (f32::from(p.stick[0]), f32::from(p.stick[1]));
    let len = (sx * sx + sy * sy).sqrt();
    let k = if len < STICK_DEAD { 0.0 } else { ((len - STICK_DEAD) / (STICK_FULL - STICK_DEAD)).min(1.0) };
    let row = |o: u32| f(m, obj2 + 0xC0 + o);
    let (max, jump, acc, frict) = (row(0x10), row(0x18), row(0x3C), row(0x60));
    let target = max * k;
    let s = SPEED.get();
    let s = if s < target { (s + acc).min(target) } else { (s + frict).max(target) };
    SPEED.set(s);
    let view = std::array::from_fn(|i| std::array::from_fn(|j| f(m, m.r32(NJ_BASE_MATRIX) + 4 * (4 * i + j) as u32)));
    let at = [f(m, cw + 0x14), f(m, cw + 0x18), f(m, cw + 0x1C)];
    if k > 0.0 {
        let aim = stick_angle(&view, at, sx, sy);
        let a = crate::guest_call(c, m, ADJUST_ANGLE, &[m.r32(cw + 0x0C) & 0xFFFF, aim, TURN]);
        m.w32(cw + 0x0C, a & 0xFFFF);
    }
    if s > 0.0 {
        let f1 = c.f[1];
        c.f[1].ps0 = f64::from(s);
        crate::guest_call(c, m, text + AL_FORWARD_SPD, &[chao]);
        c.f[1] = f1;
    }
    // Jump: the row's jmp_y_spd into the Chao's own MOVE_WORK velocity, on the floor.
    let pressed = p.button & !LAST_BUTTONS.replace(p.button);
    let floor = mw != 0 && m.r16(mw + MWP_FLAG) & MOVE_FLAG_FLOOR != 0;
    if pressed & PAD_A != 0 && floor {
        m.w32(mw + MWP_VELO_Y, jump.to_bits());
        eprintln!("chao-drive: jump {jump}");
    }
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
        let cam: [f32; 3] = std::array::from_fn(|j| -(0..3).map(|i| view[i][j] * view[i][3]).sum::<f32>());
        let vx: f32 = (0..3).map(|j| view[0][j] * at[j]).sum::<f32>() + view[0][3];
        eprintln!(
            "chao-drive: stick ({sx},{sy}) spd {s:.2} yaw {:#x} pos ({:.1},{:.1},{:.1}) cam ({:.1},{:.1},{:.1}) view_x {vx:.1} twp+a0 {:#x} mwp flag {:#x} grav {:.3} vy {:.3}",
            m.r32(cw + 0x0C), at[0], at[1], at[2], cam[0], cam[1], cam[2], m.r32(cw + 0xA0),
            if mw != 0 { m.r16(mw + MWP_FLAG) } else { 0 }, if mw != 0 { f(m, mw + 0x48) } else { 0.0 }, if mw != 0 { f(m, mw + MWP_VELO_Y) } else { 0.0 },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Camera at the origin looking down +z (view = identity rotation): stick
    /// up walks along +z (angle 0), stick right along the view's x row (+x, 0x4000).
    #[test]
    fn stick_follows_the_camera() {
        let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        assert_eq!(stick_angle(&id, [0.0, 0.0, 10.0], 0.0, 1.0), 0);
        assert_eq!(stick_angle(&id, [0.0, 0.0, 10.0], 1.0, 0.0), 0x4000);
    }
}
