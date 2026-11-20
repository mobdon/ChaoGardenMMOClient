//! Work areas kept in their native GameCube layout (big-endian bytes at
//! main.dol's offsets), for ports that touch many fields: the task work
//! (`twp`, 0x30 bytes), the motion work (`mwp`, 0x40) and the player work
//! (`pwp`, up to 0x454 for a walker).
use crate::ninja_matrix::Vec3;

/// One work area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work(pub Vec<u8>);

impl Work {
    pub fn new(len: usize) -> Self {
        Self(vec![0; len])
    }

    pub fn b(&self, o: usize) -> i8 {
        self.0[o] as i8
    }

    pub fn ub(&self, o: usize) -> u8 {
        self.0[o]
    }

    pub fn set_b(&mut self, o: usize, v: i8) {
        self.0[o] = v as u8;
    }

    pub fn h(&self, o: usize) -> i16 {
        i16::from_be_bytes([self.0[o], self.0[o + 1]])
    }

    pub fn set_h(&mut self, o: usize, v: i16) {
        self.0[o..o + 2].copy_from_slice(&v.to_be_bytes());
    }

    pub fn w(&self, o: usize) -> u32 {
        u32::from_be_bytes(self.0[o..o + 4].try_into().unwrap())
    }

    pub fn set_w(&mut self, o: usize, v: u32) {
        self.0[o..o + 4].copy_from_slice(&v.to_be_bytes());
    }

    pub fn i(&self, o: usize) -> i32 {
        self.w(o) as i32
    }

    pub fn set_i(&mut self, o: usize, v: i32) {
        self.set_w(o, v as u32);
    }

    pub fn f(&self, o: usize) -> f32 {
        f32::from_bits(self.w(o))
    }

    pub fn set_f(&mut self, o: usize, v: f32) {
        self.set_w(o, v.to_bits());
    }

    pub fn v3(&self, o: usize) -> Vec3 {
        [self.f(o), self.f(o + 4), self.f(o + 8)]
    }

    pub fn set_v3(&mut self, o: usize, v: Vec3) {
        for (j, x) in v.iter().enumerate() {
            self.set_f(o + 4 * j, *x);
        }
    }

    pub fn ang3(&self, o: usize) -> [i32; 3] {
        [self.i(o), self.i(o + 4), self.i(o + 8)]
    }

    pub fn set_ang3(&mut self, o: usize, a: [i32; 3]) {
        for (j, x) in a.iter().enumerate() {
            self.set_i(o + 4 * j, *x);
        }
    }

    /// `h |= bits` on a halfword.
    pub fn or_h(&mut self, o: usize, bits: u16) {
        self.set_h(o, (self.h(o) as u16 | bits) as i16);
    }

    /// `h &= !bits` on a halfword.
    pub fn clear_h(&mut self, o: usize, bits: u16) {
        self.set_h(o, (self.h(o) as u16 & !bits) as i16);
    }

    pub fn or_w(&mut self, o: usize, bits: u32) {
        self.set_w(o, self.w(o) | bits);
    }

    pub fn clear_w(&mut self, o: usize, bits: u32) {
        self.set_w(o, self.w(o) & !bits);
    }
}

/// Task work offsets.
pub mod tw {
    pub const MODE: usize = 0;
    pub const SMODE: usize = 1;
    pub const FLAG: usize = 4;
    pub const WTIMER: usize = 6;
    pub const ANG: usize = 8;
    pub const POS: usize = 0x14;
}

/// Player work offsets used across the ports.
pub mod pw {
    pub const PNUM: usize = 0;
    pub const CHAR: usize = 1;
    pub const STATUS: usize = 0x10;
    pub const SPD: usize = 0x64;
    pub const ACTION: usize = 0x176;
}
