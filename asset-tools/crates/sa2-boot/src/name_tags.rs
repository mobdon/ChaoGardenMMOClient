//! Chao MMO glue: where each name tag goes. While a Chao draws, every node
//! ChaoMain's walker places (the view-space current matrix after its node
//! transform, chao_player::chao_node) is projected with the game's own 3D
//! projection (the last GX_PERSPECTIVE matrix given to GXSetProjection); the
//! node highest on screen is that Chao's tag spot. Once a frame the spots go
//! to SA2_TAGS_OUT as lines "label x y" (normalised device x and y, before
//! the page's widescreen widening), and the page draws the tags over them.
//! Labels: "me" (the player's Chao), "p<id>" (another player's), "r<type>"
//! (a hub resident: 23 Tails, 24 Knuckles, 25 Amy).
//!
//! Unverified until a build run: that ChaoMain's draw walk runs inside the
//! Chao's display in Chao World (a cutscene queues it later); the first spot
//! per label is logged with its node count to check.
use crate::original;
use sa2_recomp::rt::{Cpu, Mem};
use std::cell::{Cell, RefCell};

/// GXSetProjection(mtx, type) (sa2 stage_objects/messenger/shadow.rs).
const GX_SET_PROJECTION: u32 = 0x8010_5678;
/// GXProjectionType GX_PERSPECTIVE.
const GX_PERSPECTIVE: u32 = 0;

struct Drawing {
    label: String,
    top: Option<(f32, f32)>,
    nodes: u32,
}

thread_local! {
    static PROJ: Cell<Option<[[f32; 4]; 4]>> = const { Cell::new(None) };
    static CUR: RefCell<Option<Drawing>> = const { RefCell::new(None) };
    static FRAME: RefCell<Vec<(String, f32, f32)>> = const { RefCell::new(Vec::new()) };
    static SAID: RefCell<std::collections::BTreeSet<String>> = const { RefCell::new(std::collections::BTreeSet::new()) };
    static TREE_DONE: Cell<bool> = const { Cell::new(false) };
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if std::env::var("SA2_TAGS_OUT").is_ok() {
        c.rt.set_override(GX_SET_PROJECTION, set_projection);
    }
}

fn set_projection(c: &mut Cpu, m: &mut Mem) {
    if c.r[4] == GX_PERSPECTIVE {
        let a = c.r[3];
        PROJ.set(Some(std::array::from_fn(|i| std::array::from_fn(|j| f32::from_bits(m.r32(a + 4 * (4 * i + j) as u32))))));
    }
    original(c, m, GX_SET_PROJECTION);
}

/// A view-space point through a row-major GX projection: normalised device x, y
/// (None behind the camera).
fn project(p: &[[f32; 4]; 4], v: [f32; 3]) -> Option<(f32, f32)> {
    let clip = |r: usize| p[r][0] * v[0] + p[r][1] * v[1] + p[r][2] * v[2] + p[r][3];
    let w = clip(3);
    (w > 1e-4).then(|| (clip(0) / w, clip(1) / w))
}

/// A Chao's display starts (None: a Chao without a tag).
pub(crate) fn begin(label: Option<String>) {
    finish();
    CUR.set(label.map(|label| Drawing { label, top: None, nodes: 0 }));
}

/// One node placed: `cur` is the 3x4 view-space current matrix.
pub(crate) fn node(cur: [[f32; 4]; 3]) {
    let Some(p) = PROJ.get() else { return };
    CUR.with_borrow_mut(|d| {
        let Some(d) = d else { return };
        d.nodes += 1;
        if let Some(s) = project(&p, [cur[0][3], cur[1][3], cur[2][3]]) {
            if d.top.is_none_or(|t| s.1 > t.1) {
                d.top = Some(s);
            }
        }
    });
}

/// A Chao's display ended: its spot joins this frame's.
pub(crate) fn finish() {
    let Some(d) = CUR.take() else { return };
    if SAID.with_borrow_mut(|s| s.insert(d.label.clone())) {
        eprintln!("name-tags: {} drew {} nodes, spot {:?}", d.label, d.nodes, d.top);
    }
    if let Some((x, y)) = d.top {
        FRAME.with_borrow_mut(|f| f.push((d.label, x, y)));
    }
}

fn lines(spots: &[(String, f32, f32)]) -> String {
    spots.iter().map(|(l, x, y)| format!("{l} {x:.4} {y:.4}\n")).collect()
}

/// Once a frame: this frame's spots to SA2_TAGS_OUT (an empty file: no tags).
pub(crate) fn flush() {
    finish();
    let spots = FRAME.with_borrow_mut(std::mem::take);
    if let Ok(path) = std::env::var("SA2_TAGS_OUT") {
        let _ = std::fs::write(path, lines(&spots));
    }
    if !TREE_DONE.replace(true) {
        tree_icon();
    }
}

/// The Tiny Chao Garden tree (al_tex_odekake_menu_en, texture
/// add_odekake_parts_en, left column under the truck) from the player's own
/// game files, to SA2_TREE_OUT as u32 LE width, height, then RGBA.
/// TREE is its cell as a share of the texture (16x16 at 2,44 of 256x128),
/// measured on the converted texture; unverified against module 73's own
/// sprite table.
const TREE: [f32; 4] = [2.0 / 256.0, 44.0 / 128.0, 16.0 / 256.0, 16.0 / 128.0];
const TREE_FILE: &str = "al_tex_odekake_menu_en.prs";
const TREE_TEXTURE: &str = "add_odekake_parts_en";

fn tree_icon() {
    let Ok(path) = std::env::var("SA2_TREE_OUT") else { return };
    let Some(out) = tree_rgba() else {
        eprintln!("name-tags: no tree icon ({TREE_FILE} / {TREE_TEXTURE} not read)");
        return;
    };
    let _ = std::fs::write(path, out);
}

fn tree_rgba() -> Option<Vec<u8>> {
    use sa2_asset_tools::{archive, texture};
    let prs = crate::dvd::read_file(TREE_FILE)?;
    let gvm = archive::decompress_prs(&prs, 8 << 20).ok()?;
    let entries = archive::read_gvm(&gvm).ok()?;
    let e = entries.iter().find(|e| e.name.split('.').next().is_some_and(|n| n.eq_ignore_ascii_case(TREE_TEXTURE)))?;
    let img = texture::decode_gvr(e.data, None).ok()?;
    Some(crop(&img.rgba, img.width, img.height, TREE))
}

fn crop(rgba: &[u8], w: u32, h: u32, cell: [f32; 4]) -> Vec<u8> {
    let [x, y, cw, ch] = [cell[0] * w as f32, cell[1] * h as f32, cell[2] * w as f32, cell[3] * h as f32].map(|v| v.round() as u32);
    let mut out = Vec::with_capacity(8 + (cw * ch * 4) as usize);
    out.extend_from_slice(&cw.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    for row in y..y + ch {
        let at = ((row * w + x) * 4) as usize;
        out.extend_from_slice(rgba.get(at..at + (cw * 4) as usize).unwrap_or(&[]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A GX perspective (row-major, w = -z), 90 degree field of view.
    const P: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 0.0, -1.0, 0.0]];

    #[test]
    fn projects_in_front_and_drops_behind() {
        assert_eq!(project(&P, [1.0, 2.0, -4.0]), Some((0.25, 0.5)));
        assert_eq!(project(&P, [1.0, 2.0, 4.0]), None);
    }

    #[test]
    fn highest_node_on_screen_is_the_spot_and_lines_list_it() {
        PROJ.set(Some(P));
        let at = |x: f32, y: f32| [[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, -2.0]];
        begin(Some("p7".into()));
        node(at(0.0, 0.0));
        node(at(0.2, 1.0));
        node(at(0.0, 0.5));
        begin(None);
        node(at(0.0, 9.0)); // not a tagged Chao
        finish();
        let spots = FRAME.with_borrow_mut(std::mem::take);
        assert_eq!(lines(&spots), "p7 0.1000 0.5000\n");
    }

    #[test]
    fn crop_takes_the_cell_with_a_size_header() {
        let (w, h) = (8u32, 4u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [i as u8; 4]).collect();
        let out = crop(&rgba, w, h, [0.25, 0.5, 0.25, 0.5]);
        assert_eq!(&out[..8], &[2, 0, 0, 0, 2, 0, 0, 0]);
        let px: Vec<u8> = out[8..].chunks(4).map(|p| p[0]).collect();
        assert_eq!(px, vec![18, 19, 26, 27]);
    }
}
