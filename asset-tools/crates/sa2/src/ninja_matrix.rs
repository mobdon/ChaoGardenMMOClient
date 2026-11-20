//! Ninja's matrix stack and the Dolphin SDK matrix routines underneath it,
//! as SA2B's main.dol (GameCube, GSNE8P) computes them.
//!
//! Matrices are 3x4 row-major (`m[row][col]`, column 3 is translation), as
//! the SDK's `Mtx` stores them. The SDK routines use paired singles. Each
//! lane rounds once to single precision, and each `ps_madd`/`ps_madds*` is
//! fused (`ninja_trig::fmadds`). The order of operations below is the order
//! of the compiled instructions, so results match bit for bit, down to the
//! signs of zeros. `tests/ninja_matrix_dol.rs` checks every routine against
//! main.dol run under `tests/dol_ppc.py`.
use crate::{
    msl_math::gekko_frsqrte,
    ninja_trig::{fmadds, fnmsubs, nj_rotate_sin_cos},
};

pub type Mtx = [[f32; 4]; 3];
pub type Vec3 = [f32; 3];

/// `njUnitMatrix` / `PSMTXIdentity`: the zeros are `1.0 - 1.0` (+0.0).
pub const IDENTITY: Mtx = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// `PSMTXConcat(a, b)`: a * b.
pub fn concat(a: &Mtx, b: &Mtx) -> Mtx {
    // The constant pair (0, 1) adds a's translation to columns 2 and 3.
    const UNIT01: [f32; 2] = [0.0, 1.0];
    let mut out = [[0.0; 4]; 3];
    for (i, row) in a.iter().enumerate() {
        for j in 0..4 {
            let mut v = b[0][j] * row[0];
            v = fmadds(b[1][j], row[1], v);
            v = fmadds(b[2][j], row[2], v);
            if j >= 2 {
                v = fmadds(UNIT01[j - 2], row[3], v);
            }
            out[i][j] = v;
        }
    }
    out
}

/// `PSMTXMultVecSR` (`njCalcVector`): rotate and scale, no translation.
pub fn mult_vec_sr(m: &Mtx, v: Vec3) -> Vec3 {
    let mut out = [0.0; 3];
    for (o, row) in out.iter_mut().zip(m) {
        let s = row[0] * v[0] + row[1] * v[1];
        *o = fmadds(row[2], v[2], s);
    }
    out
}

/// `PSMTXMultVec` (`njCalcPoint`): with translation.
pub fn mult_vec(m: &Mtx, v: Vec3) -> Vec3 {
    let mut out = [0.0; 3];
    for (o, row) in out.iter_mut().zip(m) {
        let x = fmadds(row[2], v[2], row[0] * v[0]);
        let y = fmadds(row[3], 1.0, row[1] * v[1]);
        *o = x + y;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// `PSMTXRotTrig(axis, sin, cos)`.
pub fn rot_trig(axis: Axis, s: f32, c: f32) -> Mtx {
    let n = -s;
    match axis {
        Axis::X => [[1.0, 0.0, 0.0, 0.0], [0.0, c, n, 0.0], [0.0, s, c, 0.0]],
        Axis::Y => [[c, 0.0, s, 0.0], [0.0, 1.0, 0.0, 0.0], [n, 0.0, c, 0.0]],
        Axis::Z => [[c, n, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
    }
}

/// `njRotateX/Y/Z(m, angle)`: m = m * R(angle). The BAMS angle becomes
/// single-precision radians and goes through MSL sinf/cosf (`PSMTXRotRad`).
pub fn rotate(m: &mut Mtx, axis: Axis, angle: i32) {
    let (s, c) = nj_rotate_sin_cos(angle);
    *m = concat(m, &rot_trig(axis, s, c));
}

/// Ninja's matrix stack (fn_80119C74 sets it up with 0x20 entries).
/// `njPushMatrixEx` copies the current matrix up one slot with no bounds
/// check; `njPopMatrixEx` refuses to go below the base.
#[derive(Debug, Clone)]
pub struct MatrixStack {
    slots: Vec<Mtx>,
    top: usize,
}

impl Default for MatrixStack {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixStack {
    pub fn new() -> Self {
        Self {
            slots: vec![IDENTITY; 0x20],
            top: 0,
        }
    }

    pub fn current(&self) -> &Mtx {
        &self.slots[self.top]
    }

    pub fn current_mut(&mut self) -> &mut Mtx {
        &mut self.slots[self.top]
    }

    pub fn depth(&self) -> usize {
        self.top
    }

    /// `njPushMatrixEx`.
    pub fn push(&mut self) {
        let m = self.slots[self.top];
        self.top += 1;
        if self.top == self.slots.len() {
            self.slots.push(m);
        } else {
            self.slots[self.top] = m;
        }
    }

    /// `njPopMatrixEx`: false (and no change) at the base.
    pub fn pop(&mut self) -> bool {
        if self.top == 0 {
            return false;
        }
        self.top -= 1;
        true
    }

    /// `_rename_njPushUnitMatrix` (0x80027CC0): push, then `njUnitMatrix`.
    pub fn push_unit(&mut self) {
        self.push();
        *self.current_mut() = IDENTITY;
    }

    pub fn rotate(&mut self, axis: Axis, angle: i32) {
        rotate(self.current_mut(), axis, angle);
    }

    /// `njRotate` (0x80115218): m = m * R(axis, angle).
    pub fn rotate_axis(&mut self, axis: Vec3, angle: i32) {
        let (s, c) = nj_rotate_sin_cos(angle);
        let m = self.current_mut();
        *m = concat(m, &rot_axis(axis, s, c));
    }

    pub fn calc_vector(&self, v: Vec3) -> Vec3 {
        mult_vec_sr(self.current(), v)
    }

    pub fn calc_point(&self, v: Vec3) -> Vec3 {
        mult_vec(self.current(), v)
    }
}

/// `PSVECNormalize`: one Newton step on Gekko's frsqrte estimate.
pub fn normalize(v: Vec3) -> Vec3 {
    let sq = fmadds(v[2], v[2], v[0] * v[0]) + v[1] * v[1];
    let e = gekko_frsqrte(sq as f64);
    let e2 = (e * e) as f32;
    let half = (e * 0.5) as f32;
    let r = fnmsubs(e2, sq, 3.0) * half;
    v.map(|c| c * r)
}

/// `PSMTXRotAxisRad(axis, rad)` from the angle's sine and cosine, in the
/// compiled lane order.
pub fn rot_axis(axis: Vec3, s: f32, c: f32) -> Mtx {
    let [nx, ny, nz] = normalize(axis);
    let t = 1.0 - c;
    let (xt, yt, zt) = (nx * t, ny * t, nz * t);
    let (txx, txy, tyy) = (xt * nx, xt * ny, yt * ny);
    let (txz, tyz, tzz) = (xt * nz, yt * nz, zt * nz);
    let (xs, ys) = (nx * s, ny * s);
    [
        [txx + c, fnmsubs(nz, s, txy), txz + ys, 0.0],
        [fmadds(nz, s, txy), c + tyy, -xs + tyz, 0.0],
        [txz + -ys, xs + tyz, tzz + c, 0.0],
    ]
}

/// fn_8003E9E0: rotate `v` by Z, then X, then the negated Y angle of a task
/// (angles are the task work's +8, +0xC, +0x10 words).
pub fn rotate_by_angles(stack: &mut MatrixStack, angles: [i32; 3], v: Vec3) -> Vec3 {
    stack.push_unit();
    stack.rotate(Axis::Z, angles[2]);
    stack.rotate(Axis::X, angles[0]);
    stack.rotate(Axis::Y, angles[1].wrapping_neg());
    let out = stack.calc_vector(v);
    stack.pop();
    out
}

/// fn_8003EA6C: the reverse of `rotate_by_angles` (Y, then the negated X
/// and Z angles).
pub fn unrotate_by_angles(stack: &mut MatrixStack, angles: [i32; 3], v: Vec3) -> Vec3 {
    stack.push_unit();
    stack.rotate(Axis::Y, angles[1]);
    stack.rotate(Axis::X, angles[0].wrapping_neg());
    stack.rotate(Axis::Z, angles[2].wrapping_neg());
    let out = stack.calc_vector(v);
    stack.pop();
    out
}
