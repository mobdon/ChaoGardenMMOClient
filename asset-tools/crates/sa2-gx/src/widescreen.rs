//! Widescreen: the game draws for 4:3. For a wider window the 3D view gets a
//! wider horizontal field of view (more is seen, nothing is stretched), and
//! the HUD keeps its proportions: an element in the left or right third moves
//! to that edge, one in the middle stays centred, and one spanning the whole
//! screen (a backdrop, a fade) is stretched to cover it.
use crate::api::{DrawCmd, Frame};
use crate::vtx::{InputFormat, VertexInput};

/// How the HUD's screen-space sprites are drawn: a view matrix that maps
/// pixel coordinates (about 1/457 a pixel) with no translation.
fn is_hud(d: &DrawCmd) -> bool {
    let m = d.draw_params.pos_mtx[0];
    !d.material.use_pn_mtx_idx
        && m[0][3] == 0.0
        && m[1][3] == 0.0
        && m[2] == [0.0, 0.0, 1.0, 0.0]
        && m[0][0] != 0.0
        && m[0][0].abs() < 0.01
        && m[1][1].abs() < 0.01
}

/// Where the draw lands across the screen: (min, max) normalised x.
fn x_extent(d: &DrawCmd) -> Option<(f32, f32)> {
    let pos = d.layout.input(VertexInput::Pos)?;
    if pos.format != InputFormat::F32(4) && pos.format != InputFormat::F32(3) {
        return None;
    }
    let stride = d.layout.stride.max(1);
    let m = d.draw_params.pos_mtx[0];
    let p = d.projection;
    let f = |b: &[u8], o: usize| {
        b.get(o..o + 4)
            .map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]]))
    };
    let mut out: Option<(f32, f32)> = None;
    for v in d.vertices.chunks(stride) {
        let (x, y, z) = (
            f(v, pos.offset)?,
            f(v, pos.offset + 4)?,
            f(v, pos.offset + 8)?,
        );
        let view: [f32; 3] =
            std::array::from_fn(|r| m[r][0] * x + m[r][1] * y + m[r][2] * z + m[r][3]);
        let clip = |row: usize| {
            p[0][row] * view[0] + p[1][row] * view[1] + p[2][row] * view[2] + p[3][row]
        };
        let w = clip(3);
        if w.abs() < 1e-6 {
            continue;
        }
        let nx = clip(0) / w;
        out = Some(out.map_or((nx, nx), |(a, b)| (a.min(nx), b.max(nx))));
    }
    out
}

/// Set clip-space x to `scale * x + shift` (normalised), whatever the
/// projection (column-major).
fn remap_x(p: &mut [[f32; 4]; 4], scale: f32, shift: f32) {
    for col in p.iter_mut() {
        col[0] = col[0] * scale + shift * col[3];
    }
}

/// Fit `frame`, drawn for 4:3, to a target `aspect` (width / height) wider
/// than 4:3. Passes drawn into a texture (EFB copies) are left alone.
pub fn widen(frame: &mut Frame, aspect: f32) {
    let k = (4.0 / 3.0) / aspect;
    if !(k > 0.1 && k < 0.999) {
        return;
    }
    let first = frame.tex_copies.iter().map(|c| c.draw).max().unwrap_or(0);
    for d in frame.draws.iter_mut().skip(first) {
        // Only full-screen views (not split screen or a small window).
        if d.viewport[2] < 600.0 || d.viewport[3] < 440.0 {
            continue;
        }
        if !is_hud(d) {
            remap_x(&mut d.projection, k, 0.0);
            continue;
        }
        match x_extent(d) {
            Some((a, b)) if a <= -0.95 && b >= 0.95 => {} // spans the screen: stretch
            Some((a, b)) => {
                let c = (a + b) / 2.0;
                let edge = 1.0 - k;
                let shift = if c < -1.0 / 3.0 {
                    -edge
                } else if c > 1.0 / 3.0 {
                    edge
                } else {
                    0.0
                };
                remap_x(&mut d.projection, k, shift);
            }
            None => remap_x(&mut d.projection, k, 0.0),
        }
    }
}
