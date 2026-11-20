//! The module's projected-shadow helpers (stg42D.rel section 1): a small
//! texture the object renders its shadow model into from above, then hands
//! to main.dol's shadow list with the matrix that projects it onto the
//! ground.
//!
//! * 0x7344 ([`init`]): the texture's buffer (`GXGetTexBufferSize` for a
//!   square IA4 texture, then `syCalloc`) and its `GXTexObj`;
//! * 0x7400 ([`free`]): the buffer is released;
//! * 0x6F94 / 0x6FC4 ([`capture`] / [`capture_ex`]): the viewport and
//!   scissor, an overhead camera 10000 units above the position (optionally
//!   turned by a Y and an X angle), its frustum as the projection and the
//!   texture matrix (light frustum times camera) kept in the block;
//! * 0x72D0 ([`end`]): the frame buffer is copied into the texture and the
//!   matrix stack and fog restored;
//! * 0x7440 ([`draw`]): fn_80066AC8 queues the shadow.
//!
//! The block (0x58 bytes, at work +0x84 for MSGER) is [`Shadow`]: the
//! texture matrix (+0), the `GXTexObj` (+0x30), the size (+0x50) and the
//! buffer (+0x54).
//!
//! Native here: the camera and projection math of main.dol
//! (`C_MTXLookAt` 0x800ED898, `C_MTXFrustum` 0x800EDBEC,
//! `C_MTXLightFrustum` 0x800EDA24, `C_MTXConcat` 0x800ED0C0,
//! `PSVECCrossProduct` 0x800EDE34; `PSVECNormalize` is
//! `ninja_axis::ps_vec_normalize`), `GXGetTexBufferSize` (0x80103128), the
//! unit matrix (fn_8011C21C) and the Ninja matrix stack. The GX state
//! calls, the fog switches and fn_80066AC8 are [`Gx`] calls the host
//! receives, named after their DOL addresses.
use super::trail::TrailHost;
use super::V3;
use crate::ninja_axis::ps_vec_normalize;
use crate::ninja_matrix::{Axis, MatrixStack, Mtx, IDENTITY};
use crate::ninja_trig::fmadds;

/// A 4x4 projection matrix (`Mtx44`, row-major).
pub type Mtx44 = [[f32; 4]; 4];

/// The shadow block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shadow {
    /// +0: the texture matrix [`capture_ex`] computes.
    pub matrix: Mtx,
    /// +0x50: the texture's width and height.
    pub size: u32,
    /// +0x54: the texture buffer (a `syCalloc` handle, 0 for none).
    pub buf: u32,
}

/// A rendering call the helpers make; `owner` names the block (the
/// texture object is its +0x30, the matrix its +0).
#[derive(Debug, Clone, PartialEq)]
pub enum Gx {
    /// GXInitTexObj(obj, buf, width, height, format, wrap_s, wrap_t,
    /// mipmap) (0x8010334C).
    InitTexObj {
        owner: u32,
        buf: u32,
        width: u16,
        height: u16,
        format: u32,
        wrap_s: u32,
        wrap_t: u32,
        mipmap: u8,
    },
    /// GXInitTexObjLOD(obj, min_filt, mag_filt, min_lod, max_lod, lod_bias,
    /// bias_clamp, edge_lod, max_aniso) (0x80103608).
    InitTexObjLod {
        owner: u32,
        min_filt: u32,
        mag_filt: u32,
        lods: [f32; 3],
        bias_clamp: u8,
        edge_lod: u8,
        max_aniso: u32,
    },
    /// GXInvalidateTexAll (0x80103BEC).
    InvalidateTexAll,
    /// GXSetViewport(x, y, w, h, near, far) (0x80105AA8).
    Viewport([f32; 6]),
    /// GXSetScissor(x, y, w, h) (0x80105B18).
    Scissor([u32; 4]),
    /// njEnableFog (0x8011C2B0, true) / njDisableFog (0x8011C2BC, false).
    Fog(bool),
    /// gjSetFog (0x8011C310).
    SetFog,
    /// GXSetProjection(m, kind) (0x80105678).
    Projection { m: Mtx44, kind: u32 },
    /// GXSetTexCopySrc(left, top, w, h) (0x80101E54).
    TexCopySrc([u16; 4]),
    /// GXSetTexCopyDst(w, h, format, mipmap) (0x80101F58).
    TexCopyDst {
        width: u16,
        height: u16,
        format: u32,
        mipmap: u8,
    },
    /// GXCopyTex(buf, clear) (0x80102648).
    CopyTex { buf: u32, clear: u8 },
    /// GXPixModeSync (0x801015CC).
    PixModeSync,
    /// fn_80066AC8(kind, pos, size, obj, buf, matrix): the shadow joins
    /// main.dol's shadow list (64 entries at 0x801FB958).
    Queue {
        kind: u32,
        pos: V3,
        size: f32,
        owner: u32,
        buf: u32,
    },
}

/// What the helpers ask of the engine: `syCalloc` / `syFree` (from
/// [`TrailHost`]) and the rendering calls.
pub trait ShadowHost: TrailHost {
    fn gx(&mut self, call: Gx);
}

/// `GX_TF_IA4`, the texture format (the constant 2 the helpers pass).
const IA4: u32 = 2;

/// `GXGetTexBufferSize(w, h, GX_TF_IA4, GX_FALSE, 0)` (0x80103128, its
/// path without mipmaps): 8x4 tiles of 32 bytes. For the sizes
/// `texture::gx_buffer_size` accepts it is that; here every 16-bit size
/// is taken, as the routine takes them.
pub fn tex_buffer_size(w: u16, h: u16) -> u32 {
    ((u32::from(w) + 7) >> 3) * ((u32::from(h) + 3) >> 2) * 32
}

/// 0x7344(block, size): a `size` x `size` IA4 texture: its buffer from
/// `syCalloc(1, n)` (0 when that fails, kept and passed on as it is), the
/// size, then the texture object (no wrap, no mipmaps) with point filters
/// and LODs 0..10 (s4 +0x1E0, +0x234).
pub fn init(sh: &mut Shadow, owner: u32, size: u32, host: &mut impl ShadowHost) {
    let s = size as u16;
    let buf = host.sy_calloc(1, tex_buffer_size(s, s)).unwrap_or(0);
    sh.buf = buf;
    sh.size = size;
    host.gx(Gx::InitTexObj {
        owner,
        buf,
        width: s,
        height: s,
        format: IA4,
        wrap_s: 0,
        wrap_t: 0,
        mipmap: 0,
    });
    host.gx(Gx::InitTexObjLod {
        owner,
        min_filt: 1,
        mag_filt: 1,
        lods: [0.0, 10.0, 0.0],
        bias_clamp: 0,
        edge_lod: 0,
        max_aniso: 0,
    });
}

/// 0x7400(block): the buffer is freed and cleared (nothing without one).
pub fn free(sh: &mut Shadow, host: &mut impl ShadowHost) {
    if sh.buf != 0 {
        host.sy_free(sh.buf);
        sh.buf = 0;
    }
}

/// 0x6F94(block, scale, pos, &x_angle, &y_angle, sin): [`capture_ex`] with
/// the texture scale 1.0 (s4 +0x1EC).
#[allow(clippy::too_many_arguments)]
pub fn capture(
    sh: &mut Shadow,
    scale: f32,
    pos: V3,
    x_angle: Option<i32>,
    y_angle: Option<i32>,
    sin: f32,
    stack: &mut MatrixStack,
    host: &mut impl ShadowHost,
) {
    capture_ex(sh, scale, 1.0, sin, pos, x_angle, y_angle, stack, host);
}

/// 0x6FC4(block, half_width, tex_scale, sin, pos, &x_angle, &y_angle): set
/// up the render into the texture. The viewport and scissor cover the
/// texture; fog goes off; a matrix is pushed and becomes the camera
/// looking down from 10000 units above `pos` (s4 +0x2C4; with angles the
/// offset is turned by njRotateY then njRotateX first), up (0, 0, 1)
/// (+0x2B8); the projection is the frustum `±half_width` wide, 500 to
/// 10000 deep (+0x2C8); and the block's matrix becomes the light frustum
/// (texture scale `0.5 * tex_scale`, offset `0.5 + 0.05 * sin`, +0x220,
/// +0x2CC) times the camera. [`end`] pops the matrix.
#[allow(clippy::too_many_arguments)]
pub fn capture_ex(
    sh: &mut Shadow,
    half_width: f32,
    tex_scale: f32,
    sin: f32,
    pos: V3,
    x_angle: Option<i32>,
    y_angle: Option<i32>,
    stack: &mut MatrixStack,
    host: &mut impl ShadowHost,
) {
    host.gx(Gx::InvalidateTexAll);
    // The u32 through the 0x43300000 magic double (s4 +0x2D0), then single.
    let s = sh.size as f32;
    host.gx(Gx::Viewport([0.0, 0.0, s, s, 0.0, 1.0]));
    host.gx(Gx::Scissor([0, 0, sh.size, sh.size]));
    host.gx(Gx::Fog(false));
    host.gx(Gx::SetFog);
    stack.push();
    *stack.current_mut() = IDENTITY;
    let mut v = [0.0, 10000.0, 0.0];
    if let Some(a) = y_angle {
        stack.rotate(Axis::Y, a);
    }
    if let Some(a) = x_angle {
        stack.rotate(Axis::X, a);
    }
    if x_angle.is_some() || y_angle.is_some() {
        v = stack.calc_vector(v);
        *stack.current_mut() = IDENTITY;
    }
    let eye = [pos[0] + v[0], pos[1] + v[1], pos[2] + v[2]];
    let target = [eye[0] + -v[0], eye[1] + -v[1], eye[2] + -v[2]];
    *stack.current_mut() = look_at(eye, [0.0, 0.0, 1.0], target);
    let w = half_width;
    let n = -w;
    let (near, far) = (500.0, 10000.0);
    host.gx(Gx::Projection {
        m: frustum(w, n, n, w, near, far),
        kind: 0,
    });
    let half = 0.5f32;
    let t = half + f32::from_bits(0x3d4c_cccd) * sin;
    let k = half * tex_scale;
    let light = light_frustum(n, w, n, w, near, k, k, t, t);
    sh.matrix = c_concat(&light, stack.current());
}

/// 0x72D0(block): the frame buffer's corner is copied into the texture
/// (GXCopyTex with clear), then the matrix [`capture_ex`] pushed is popped
/// and fog turned back on.
pub fn end(sh: &Shadow, stack: &mut MatrixStack, host: &mut impl ShadowHost) {
    let s = sh.size as u16;
    host.gx(Gx::TexCopySrc([0, 0, s, s]));
    host.gx(Gx::TexCopyDst {
        width: s,
        height: s,
        format: IA4,
        mipmap: 0,
    });
    host.gx(Gx::CopyTex {
        buf: sh.buf,
        clear: 1,
    });
    host.gx(Gx::PixModeSync);
    stack.pop();
    host.gx(Gx::Fog(true));
    host.gx(Gx::SetFog);
}

/// 0x7440(kind, pos, size, block): fn_80066AC8 queues the shadow with the
/// block's texture object, buffer and matrix.
pub fn draw(kind: u32, pos: V3, size: f32, owner: u32, sh: &Shadow, host: &mut impl ShadowHost) {
    host.gx(Gx::Queue {
        kind,
        pos,
        size,
        owner,
        buf: sh.buf,
    });
}

// --- main.dol's matrix library (the C versions) ----------------------------

/// `ps_msub`: a * c - b, fused, one rounding per lane.
fn fmsubs(a: f32, c: f32, b: f32) -> f32 {
    fmadds(a, c, -b)
}

/// `PSVECCrossProduct` (0x800EDE34): a x b, the y and z lanes negated
/// after their fused subtraction.
pub fn cross(a: V3, b: V3) -> V3 {
    let x = fmsubs(a[1], b[2], b[1] * a[2]);
    let y = -fmsubs(a[0], b[2], b[0] * a[2]);
    let z = -fmsubs(a[1], b[0], b[1] * a[0]);
    [x, y, z]
}

/// `(a0*b0 + a1*b1) + a2*b2`, each product and sum rounded.
fn dot3(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `C_MTXLookAt(m, eye, up, target)` (0x800ED898): the rows are the right
/// vector (`up x look`, normalized), the true up (`look x right`) and the
/// look direction (`eye - target`, normalized); the translations are their
/// negated dot products with the eye.
pub fn look_at(eye: V3, up: V3, target: V3) -> Mtx {
    let look = ps_vec_normalize([eye[0] - target[0], eye[1] - target[1], eye[2] - target[2]]);
    let right = ps_vec_normalize(cross(up, look));
    let up2 = cross(look, right);
    let row = |v: V3| [v[0], v[1], v[2], -dot3(eye, v)];
    [row(right), row(up2), row(look)]
}

/// `C_MTXFrustum(m, t, b, l, r, n, f)` (0x800EDBEC), constants 1, 2, 0, -1
/// at r2 -0x5D48..-0x5D3C.
pub fn frustum(t: f32, b: f32, l: f32, r: f32, n: f32, f: f32) -> Mtx44 {
    let rl = 1.0 / (r - l);
    let tb = 1.0 / (t - b);
    let nf = 1.0 / (f - n);
    let n2 = 2.0 * n;
    [
        [n2 * rl, 0.0, rl * (r + l), 0.0],
        [0.0, n2 * tb, tb * (t + b), 0.0],
        [0.0, 0.0, -n * nf, nf * -(f * n)],
        [0.0, 0.0, -1.0, 0.0],
    ]
}

/// `C_MTXLightFrustum(m, t, b, l, r, n, scale_s, scale_t, trans_s,
/// trans_t)` (0x800EDA24), constants 1, 0, 2, -1 at r2 -0x5D58..-0x5D4C.
#[allow(clippy::too_many_arguments)]
pub fn light_frustum(
    t: f32,
    b: f32,
    l: f32,
    r: f32,
    n: f32,
    scale_s: f32,
    scale_t: f32,
    trans_s: f32,
    trans_t: f32,
) -> Mtx {
    let rl = 1.0 / (r - l);
    let tb = 1.0 / (t - b);
    let n2 = 2.0 * n;
    [
        [
            scale_s * (n2 * rl),
            0.0,
            scale_s * (rl * (r + l)) - trans_s,
            0.0,
        ],
        [
            0.0,
            scale_t * (n2 * tb),
            scale_t * (tb * (t + b)) - trans_t,
            0.0,
        ],
        [0.0, 0.0, -1.0, 0.0],
    ]
}

/// `C_MTXConcat(a, b)` (0x800ED0C0): a * b in single precision, no fused
/// operations; the translation column adds a's last.
pub fn c_concat(a: &Mtx, b: &Mtx) -> Mtx {
    let mut out = [[0.0; 4]; 3];
    for (o, row) in out.iter_mut().zip(a) {
        for (j, x) in o.iter_mut().enumerate() {
            let s = row[0] * b[0][j] + row[1] * b[1][j] + row[2] * b[2][j];
            *x = if j == 3 { row[3] + s } else { s };
        }
    }
    out
}
