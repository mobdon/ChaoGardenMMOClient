//! A 4:3 frame fitted to 16:9: wider 3D view, HUD kept in proportion at the edges.
use sa2_gx::api::{Frame, Gx, Ram};
use sa2_gx::enums::*;
use sa2_gx::widescreen::widen;

/// SA2's perspective (as its frames carry it) and its HUD sprite matrix.
const PERSP: [[f32; 4]; 4] = [
    [1.4284168, 0.0, 0.0, 0.0],
    [0.0, 1.9045559, 0.0, 0.0],
    [0.0, 0.0, -1.0, -1.0],
    [0.0, 0.0, -1.0, 0.0],
];
const HUD: [[f32; 4]; 3] = [
    [-0.0021877366, 0.0, 0.7000757, 0.0],
    [0.0, 0.0021877366, -0.5250568, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

fn frame() -> Frame {
    let mut gx = Gx::new();
    gx.clear_vtx_desc();
    gx.set_vtx_desc(attr::POS, attr_type::DIRECT);
    gx.set_vtx_attr_fmt(0, attr::POS, comp_cnt::POS_XYZ, comp_type::F32, 0);
    gx.set_viewport(0.0, 0.0, 640.0, 480.0, 0.0, 1.0);
    let mem = Ram(&[]);
    let quad = |gx: &mut Gx, pts: [(f32, f32, f32); 4]| {
        gx.begin(command::DRAW_QUADS, 0, 4);
        for (x, y, z) in pts {
            gx.position3f32(x, y, z);
        }
        gx.end(&mem);
    };
    // A 3D quad in front of the camera.
    gx.load_pos_mtx_imm(
        &[
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, -10.0],
        ],
        0,
    );
    quad(
        &mut gx,
        [
            (-1.0, -1.0, 0.0),
            (1.0, -1.0, 0.0),
            (1.0, 1.0, 0.0),
            (-1.0, 1.0, 0.0),
        ],
    );
    // HUD sprites in pixels (x runs negative in SA2's sprite matrix, z = -1):
    // the lives icon at the left, the bar in the middle.
    gx.load_pos_mtx_imm(&HUD, 0);
    quad(
        &mut gx,
        [
            (-30.0, 420.0, -1.0),
            (-90.0, 420.0, -1.0),
            (-90.0, 460.0, -1.0),
            (-30.0, 460.0, -1.0),
        ],
    );
    quad(
        &mut gx,
        [
            (-250.0, 430.0, -1.0),
            (-390.0, 430.0, -1.0),
            (-390.0, 450.0, -1.0),
            (-250.0, 450.0, -1.0),
        ],
    );
    let mut f = gx.take_frame();
    for d in &mut f.draws {
        d.projection = PERSP;
    }
    f
}

#[test]
fn wider_view_and_hud_at_the_edges() {
    let mut f = frame();
    let before: Vec<_> = f.draws.iter().map(|d| d.projection).collect();
    widen(&mut f, 16.0 / 9.0);
    let k = 0.75; // (4/3) / (16/9)
                  // 3D: x scaled by k, nothing else.
    assert!((f.draws[0].projection[0][0] - before[0][0][0] * k).abs() < 1e-6);
    assert_eq!(f.draws[0].projection[2][0], 0.0);
    // Left HUD: scaled by k and shifted left by 1 - k (x' = k x - 0.25 w).
    assert!((f.draws[1].projection[0][0] - before[1][0][0] * k).abs() < 1e-6);
    assert!((f.draws[1].projection[2][0] - (-0.25 * -1.0)).abs() < 1e-6);
    // Middle HUD: scaled, not shifted.
    assert_eq!(f.draws[2].projection[2][0], 0.0);
    // 4:3 itself is left alone.
    let mut g = frame();
    widen(&mut g, 4.0 / 3.0);
    assert_eq!(g.draws[0].projection, before[0]);
}
