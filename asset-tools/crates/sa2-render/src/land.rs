//! Landtable visual geometry: every chunk model in the object trees of a
//! landtable, transformed to world space and triangulated from its Ninja
//! strip chunks, with the material and texture state each strip was drawn
//! with. Read from the Steam stage module through `sa2::stage_land`.
use crate::gc::GcSource;
use sa2_asset_tools::model::{ChunkAttachment, Node};
use sa2_asset_tools::stage::Stage;
use std::collections::BTreeMap;

/// The stage reader's offsets are section offsets plus this header.
const STAGE_BASE: u32 = 16;

/// One vertex as the GPU takes it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Material diffuse, 0..1.
    pub color: [f32; 4],
}

/// A run of triangles with one texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
    /// Texture id in the stage's texture list, `None` untextured.
    pub texture: Option<u16>,
    pub first: u32,
    pub count: u32,
}

/// A GameCube model mesh as sa2-gx's vertex loader left it: vertices in
/// the loader's layout, in model space, placed by `model`.
#[derive(Debug, Clone)]
pub struct GxPart {
    pub layout: sa2_gx::vtx::LoadedVertexLayout,
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub texture: Option<u16>,
    /// The vertices carry color 0.
    pub colors: bool,
    pub model: sa2_gx::params::Mtx34,
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    /// Chunk geometry, in world space.
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<Draw>,
    /// GameCube model meshes.
    pub parts: Vec<GxPart>,
}

impl Mesh {
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &self.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
        }
        (lo, hi)
    }
}

type Mtx = [[f32; 4]; 4];

const IDENTITY: Mtx = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Column-vector product a·b.
fn mul(a: &Mtx, b: &Mtx) -> Mtx {
    let mut r = [[0.0; 4]; 4];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn rot(axis: usize, bams: i32) -> Mtx {
    let a = bams as f32 * (std::f32::consts::TAU / 65536.0);
    let (s, c) = a.sin_cos();
    let mut m = IDENTITY;
    let (i, j) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    m[i][i] = c;
    m[i][j] = -s;
    m[j][i] = s;
    m[j][j] = c;
    m
}

/// Ninja's object matrix: translate, then rotate Z·Y·X (or Y·X·Z with
/// NJD_EVAL_ZXY_ANG, 0x20), then scale; flags 1, 2, 4 skip each part.
fn local(n: &Node) -> Mtx {
    let mut m = IDENTITY;
    if n.flags & 1 == 0 {
        for k in 0..3 {
            m[k][3] = n.position[k];
        }
    }
    if n.flags & 2 == 0 {
        let r = n.rotation_bams;
        let order: [usize; 3] = if n.flags & 0x20 != 0 {
            [1, 0, 2]
        } else {
            [2, 1, 0]
        };
        for axis in order {
            m = mul(&m, &rot(axis, r[axis]));
        }
    }
    if n.flags & 4 == 0 {
        let mut s = IDENTITY;
        for k in 0..3 {
            s[k][k] = n.scale[k];
        }
        m = mul(&m, &s);
    }
    m
}

fn apply(m: &Mtx, p: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3] * w)
}

/// Extra 16-bit words per strip index after the index itself.
fn strip_extra(kind: u8) -> Option<(usize, Option<f32>)> {
    // (words, UV scale) for strip kinds 64..=75.
    Some(match kind {
        64 | 73 => (0, None),
        65 => (2, Some(1.0 / 256.0)),
        66 => (2, Some(1.0 / 1024.0)),
        67 => (3, None),
        68 => (5, Some(1.0 / 256.0)),
        69 => (5, Some(1.0 / 1024.0)),
        70 => (2, None),
        71 => (4, Some(1.0 / 256.0)),
        72 => (4, Some(1.0 / 1024.0)),
        74 => (4, Some(1.0 / 256.0)),
        75 => (4, Some(1.0 / 1024.0)),
        _ => return None,
    })
}

struct Builder<'a> {
    attach: BTreeMap<u32, &'a ChunkAttachment>,
    gc: Option<&'a GcSource<'a>>,
    /// The entry being walked has GC visual models.
    gc_entry: bool,
    mesh: Mesh,
    /// Unsupported chunk kinds met, with counts.
    pub skipped: BTreeMap<u8, usize>,
    /// Keep the vertex cache across attachments (character models).
    shared: bool,
    cache: BTreeMap<usize, ([f32; 3], [f32; 3])>,
}

impl Builder<'_> {
    fn model(&mut self, m: &Mtx, a: &ChunkAttachment) {
        // Character models share one vertex cache across their nodes
        // (a node's strips may index vertices an earlier node wrote).
        let mut pos = if self.shared {
            std::mem::take(&mut self.cache)
        } else {
            BTreeMap::new()
        };
        for vc in &a.vertex_chunks {
            for (i, v) in vc.vertices.iter().enumerate() {
                let p = apply(m, v.position, 1.0);
                let n = v.normal.map_or([0.0, 1.0, 0.0], |n| apply(m, n, 0.0));
                pos.insert(usize::from(vc.index_offset) + i, (p, n));
            }
        }
        let mut color = [1.0f32; 4];
        let mut texture = None;
        for pc in &a.polygon_chunks {
            match pc.kind {
                8 | 9 => texture = pc.texture_id,
                16..=31 => {
                    if let Some(c) = pc.diffuse_rgba {
                        color = c.map(|x| f32::from(x) / 255.0);
                    }
                }
                64..=75 => self.strips(pc.kind, &pc.raw_words, &pos, color, texture),
                0..=5 => {}
                k => *self.skipped.entry(k).or_default() += 1,
            }
        }
        if self.shared {
            self.cache = pos;
        }
    }

    fn strips(
        &mut self,
        kind: u8,
        w: &[u16],
        pos: &BTreeMap<usize, ([f32; 3], [f32; 3])>,
        color: [f32; 4],
        texture: Option<u16>,
    ) {
        let Some((extra, uv_scale)) = strip_extra(kind) else {
            return;
        };
        let Some(&head) = w.get(2) else { return };
        let user = usize::from(head >> 14);
        let strips = usize::from(head & 0x3FFF);
        let first = self.mesh.indices.len() as u32;
        let mut at = 3;
        for _ in 0..strips {
            let Some(&len) = w.get(at) else { return };
            at += 1;
            let len16 = len as i16;
            let flip = len16 < 0;
            let n = usize::from(len16.unsigned_abs());
            let mut verts = Vec::with_capacity(n);
            for k in 0..n {
                let Some(&idx) = w.get(at) else { return };
                let uv = match uv_scale {
                    Some(s) => {
                        let u = w.get(at + 1).map_or(0, |x| *x as i16);
                        let v = w.get(at + 2).map_or(0, |x| *x as i16);
                        [f32::from(u) * s, f32::from(v) * s]
                    }
                    None => [0.0, 0.0],
                };
                at += 1 + extra;
                if k >= 2 {
                    at += user;
                }
                let (p, nrm) = pos
                    .get(&usize::from(idx))
                    .copied()
                    .unwrap_or(([0.0; 3], [0.0, 1.0, 0.0]));
                verts.push(self.mesh.vertices.len() as u32);
                self.mesh.vertices.push(Vertex {
                    pos: p,
                    normal: nrm,
                    uv,
                    color,
                });
            }
            for i in 2..verts.len() {
                let odd = (i % 2 == 1) ^ flip;
                let (a, b, c) = (verts[i - 2], verts[i - 1], verts[i]);
                if odd {
                    self.mesh.indices.extend([b, a, c]);
                } else {
                    self.mesh.indices.extend([a, b, c]);
                }
            }
        }
        let count = self.mesh.indices.len() as u32 - first;
        if count > 0 {
            self.mesh.draws.push(Draw {
                texture,
                first,
                count,
            });
        }
    }

    fn emit(&mut self, n: &Node, m: &Mtx) {
        if let Some(a) = self.attach.get(&n.attach_offset).copied() {
            self.model(m, a);
        } else if let (true, Some(gc), Some(at)) = (
            self.gc_entry,
            self.gc,
            n.attach_offset.checked_sub(STAGE_BASE),
        ) {
            let model = [m[0], m[1], m[2]];
            self.mesh
                .parts
                .extend(gc.model(at).into_iter().map(|p| GxPart { model, ..p }));
        }
    }

    fn tree(&mut self, nodes: &BTreeMap<u32, &Node>, root: u32, parent: &Mtx, depth: usize) {
        let mut at = root;
        while at != 0 && depth < 64 {
            let Some(n) = nodes.get(&at) else { return };
            let m = mul(parent, &local(n));
            self.emit(n, &m);
            if n.child_offset != 0 {
                self.tree(nodes, n.child_offset, &m, depth + 1);
            }
            at = n.sibling_offset;
        }
    }
}

/// The visual (chunk) geometry of every landtable entry. Returns the mesh
/// and the polygon chunk kinds it could not draw.
pub fn land_mesh(stage: &Stage, gc: Option<&GcSource<'_>>) -> (Mesh, BTreeMap<u8, usize>) {
    let mut b = Builder {
        attach: stage
            .chunk_attachments
            .iter()
            .map(|a| (a.offset, a))
            .collect(),
        gc,
        gc_entry: false,
        mesh: Mesh::default(),
        skipped: BTreeMap::new(),
        shared: false,
        cache: BTreeMap::new(),
    };
    for e in &stage.entries {
        b.gc_entry = e.geometry_format == "gc-opaque";
        let nodes: BTreeMap<u32, &Node> = e.nodes.iter().map(|n| (n.offset, n)).collect();
        // Entries draw their root only (its siblings belong to the tree).
        if let Some(n) = nodes.get(&e.root_offset) {
            let m = local(n);
            b.emit(n, &m);
            if n.child_offset != 0 {
                b.tree(&nodes, n.child_offset, &m, 1);
            }
        }
    }
    (b.mesh, b.skipped)
}

/// A character model (one row of a `*mdl.prs` bank) in its own space, in
/// its bind pose: every node's attachment through the node tree, sharing one
/// vertex cache. Weighted vertex chunks are taken at the last node that
/// wrote them (no skinning yet). Returns the polygon kinds it could not draw.
pub fn model_mesh(model: &sa2_asset_tools::model::Model) -> (Mesh, BTreeMap<u8, usize>) {
    let mut b = Builder {
        attach: model.attachments.iter().map(|a| (a.offset, a)).collect(),
        gc: None,
        gc_entry: false,
        mesh: Mesh::default(),
        skipped: BTreeMap::new(),
        shared: true,
        cache: BTreeMap::new(),
    };
    let nodes: BTreeMap<u32, &Node> = model.nodes.iter().map(|n| (n.offset, n)).collect();
    b.tree(&nodes, model.root_offset, &IDENTITY, 0);
    (b.mesh, b.skipped)
}

/// `mesh` placed as the walker display fn_800B15B8 places the player model:
/// translate to `pos`, then njRotateZ(ang.z), njRotateX(ang.x),
/// njRotateY(0x8000 - ang.y) (twp +0x10, +0x8, +0xC). shortcut: its extra
/// translation (a rotated offset from the work) and twp +0x20 scale are
/// left out, and the model stays in its bind pose (no motion yet).
pub fn place(mesh: &Mesh, pos: [f32; 3], ang: [i32; 3]) -> Mesh {
    let mut m = IDENTITY;
    for k in 0..3 {
        m[k][3] = pos[k];
    }
    m = mul(&m, &rot(2, ang[2]));
    m = mul(&m, &rot(0, ang[0]));
    m = mul(&m, &rot(1, 0x8000i32.wrapping_sub(ang[1])));
    let mut out = mesh.clone();
    for v in &mut out.vertices {
        v.pos = apply(&m, v.pos, 1.0);
        v.normal = apply(&m, v.normal, 0.0);
    }
    for p in &mut out.parts {
        let mut pm = IDENTITY;
        pm[..3].copy_from_slice(&p.model);
        let r = mul(&m, &pm);
        p.model = [r[0], r[1], r[2]];
    }
    out
}
