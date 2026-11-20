//! main.dol `MOV_Control` (GSNE8P 0x8002F448), the per-frame movement
//! integrator Chao race and karate tasks call. The movement block is task+0x38:
//! +0 velocity, +0xC acceleration, +0x40 flags, +0x48 gravity; the position is
//! task+0x34 +0x14. Checked against the PowerPC by tests/mov_control.rs.
use crate::land_nodes::nj_scalor;
use crate::ninja_vector::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mover {
    pub velocity: Vec3,
    pub accel: Vec3,
    pub flags: u16,
    pub gravity: f32,
}

/// One frame. `pushed` is CCL_IsPushed (0x80006774) for the task.
pub fn mov_control(m: &mut Mover, position: &mut Vec3, pushed: bool) {
    m.accel[1] += m.gravity;
    for k in 0..3 {
        m.velocity[k] += m.accel[k];
    }
    m.accel = [0.0; 3];
    if m.flags & 8 != 0 {
        if m.flags & 0x10 != 0 {
            // At rest: stay put unless pushed or moving faster than 0.01.
            if !pushed && nj_scalor(m.velocity) <= 0.01 {
                return;
            }
            m.flags &= !0x10;
        } else if m.flags & 0x400 != 0 && m.flags & 0x800 == 0 {
            // Grounded: a horizontal speed below 0.01 comes to rest.
            if nj_scalor([m.velocity[0], 0.0, m.velocity[2]]) < 0.01 {
                m.velocity = [0.0; 3];
                m.flags |= 0x10;
                return;
            }
        }
    }
    for k in 0..3 {
        position[k] += m.velocity[k];
    }
}
