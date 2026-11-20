//! The FIFO sink must turn the raw command stream (as the game writes it to
//! 0xCC008000, in pieces of any size) into the same draws as the GX API.
use sa2_gx::api::Ram;
use sa2_gx::fifo::GxFifo;

fn bp(out: &mut Vec<u8>, v: u32) {
    out.push(0x61);
    out.extend(v.to_be_bytes());
}
fn cp(out: &mut Vec<u8>, a: u8, v: u32) {
    out.extend([0x08, a]);
    out.extend(v.to_be_bytes());
}
fn xf(out: &mut Vec<u8>, addr: u16, words: &[u32]) {
    out.push(0x10);
    out.extend(((words.len() - 1) as u16).to_be_bytes());
    out.extend(addr.to_be_bytes());
    for w in words {
        out.extend(w.to_be_bytes());
    }
}

/// Position-only, F32 XYZ in format 0, then one triangle.
fn triangle() -> Vec<u8> {
    let mut s = Vec::new();
    cp(&mut s, 0x50, 1 << 9); // VCD: POS direct
    cp(&mut s, 0x60, 0);
    cp(&mut s, 0x70, 1 | (4 << 1)); // VAT0: POS XYZ F32
    s.extend([0x90, 0, 3]);
    for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            s.extend(c.to_be_bytes());
        }
    }
    s
}

#[test]
fn byte_at_a_time_draws_once() {
    let mut f = GxFifo::default();
    for b in triangle() {
        f.write(&[b], &Ram(&[]));
    }
    assert_eq!(f.pending(), 0);
    let frame = f.gx.take_frame();
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.draws[0].indices.len(), 3);
}

#[test]
fn partial_command_waits() {
    let mut f = GxFifo::default();
    let s = triangle();
    f.write(&s[..s.len() - 1], &Ram(&[]));
    assert_eq!(f.gx.take_frame().draws.len(), 0);
    f.write(&s[s.len() - 1..], &Ram(&[]));
    assert_eq!(f.gx.take_frame().draws.len(), 1);
}

#[test]
fn viewport_scissor_and_clear() {
    let mut f = GxFifo::default();
    let mut s = Vec::new();
    // GXSetViewport(10, 20, 320, 240, 0, 1).
    let z = 16_777_215.0f32;
    let vp = [
        160.0,
        -120.0,
        z,
        10.0 + 160.0 + 342.0,
        20.0 + 120.0 + 342.0,
        z,
    ];
    xf(&mut s, 0x101A, &vp.map(f32::to_bits));
    // GXSetScissor(4, 8, 100, 50).
    bp(&mut s, 0x20 << 24 | (4 + 342) << 12 | (8 + 342));
    bp(&mut s, 0x21 << 24 | (4 + 342 + 99) << 12 | (8 + 342 + 49));
    // GXSetCopyClear(r 1, g 2, b 3, a 4).
    bp(&mut s, 0x4F << 24 | 4 << 8 | 1);
    bp(&mut s, 0x50 << 24 | 2 << 8 | 3);
    s.extend(triangle());
    f.write(&s, &Ram(&[]));
    let frame = f.gx.take_frame();
    let d = &frame.draws[0];
    assert_eq!(d.viewport, [10.0, 20.0, 320.0, 240.0, 0.0, 1.0]);
    assert_eq!(d.scissor, [4, 8, 100, 50]);
    assert_eq!(frame.clear_color, [1.0, 2.0, 3.0, 4.0].map(|c| c / 255.0));
}

#[test]
fn perspective_projection_from_xf() {
    let mut f = GxFifo::default();
    let mut s = Vec::new();
    xf(
        &mut s,
        0x1020,
        &[2.0f32, 0.5, 3.0, 0.25, -1.0, -2.0].map(f32::to_bits),
    );
    xf(&mut s, 0x1026, &[0]);
    s.extend(triangle());
    f.write(&s, &Ram(&[]));
    // Row major [[2,0,.5,0],[0,3,.25,0],[0,0,-1,-2],[0,0,-1,0]]; column 2 of
    // the wgpu matrix is GX row 2 plus row 3 (z remap), transposed.
    let p = f.gx.take_frame().draws[0].projection;
    assert_eq!(p[0], [2.0, 0.0, 0.0, 0.0]);
    assert_eq!(p[2], [0.5, 0.25, -2.0, -1.0]);
    assert_eq!(p[3], [0.0, 0.0, -2.0, 0.0]);
}

#[test]
fn call_display_list_runs_and_cannot_recurse() {
    let tri = triangle();
    let mut ram = vec![0u8; 0x200];
    ram[0x100..0x100 + tri.len()].copy_from_slice(&tri);
    // A second list at 0x180 that calls itself.
    let me = [0x40u8, 0x80, 0, 1, 0x80, 0, 0, 0, 9];
    ram[0x180..0x189].copy_from_slice(&me);
    let mut s = vec![0x40];
    s.extend(0x8000_0100u32.to_be_bytes());
    s.extend((tri.len() as u32).to_be_bytes());
    s.extend(me);
    let mut f = GxFifo::default();
    f.write(&s, &Ram(&ram));
    assert_eq!(f.gx.take_frame().draws.len(), 1);
    assert_eq!(f.pending(), 0);
}

#[test]
fn texture_from_bp_registers() {
    let mut f = GxFifo::default();
    let mut s = Vec::new();
    // Map 0: 8x4 RGBA8 at 0x80000000, clamp, linear.
    bp(&mut s, 0x80 << 24 | 4 << 5 | 1 << 4);
    bp(&mut s, 0x88 << 24 | 6 << 20 | 3 << 10 | 7);
    bp(&mut s, 0x94 << 24);
    s.extend(triangle());
    let ram = vec![0xFFu8; 0x400];
    f.write(&s, &Ram(&ram));
    let frame = f.gx.take_frame();
    assert_eq!(frame.uploads.len(), 1);
    let (w, h, _) = &frame.uploads[0].levels[0];
    assert_eq!((*w, *h), (8, 4));
}

#[test]
fn unknown_bytes_are_skipped() {
    let mut f = GxFifo::default();
    let mut s = vec![0x07, 0xFF];
    s.extend(triangle());
    f.write(&s, &Ram(&[]));
    assert_eq!(f.gx.take_frame().draws.len(), 1);
}
