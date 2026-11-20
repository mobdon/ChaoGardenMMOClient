//! The draw of [`Mesh`]es through sa2-gx's `GxRenderer` (noclip's GX
//! renderer in Rust): GameCube land meshes as the vertex loader left them,
//! chunk geometry packed into the same vertex layout. One material for all:
//! texture times the lit vertex color (one directional light and an
//! ambient level), alpha test at one half, no culling (Ninja strips carry
//! their own winding). Runs on WebGPU in the browser (WebGL2 through
//! wgpu's fallback) and on native backends for tests.
use crate::land::{GxPart, Mesh, Vertex};
use sa2_gx::enums::*;
use sa2_gx::gpu::{GpuDraw, GxRenderer};
use sa2_gx::material::{GxMaterial, GxMaterialBuilder, Light};
use sa2_gx::params::{
    color_kind, DrawParams, MaterialParams, SceneParams, TextureId, TextureMapping,
};
use sa2_gx::vtx::{
    compile_loaded_vertex_layout, InputFormat, LoadedVertexLayout, Vcd, VertexInput, VtxDesc,
    ATTR_COUNT,
};

/// An RGBA8 image for [`Renderer::set_textures`].
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Where the light comes from (the old land shader's direction), far away.
const LIGHT_DIR: [f32; 3] = [0.3, 0.8, 0.5];
const AMBIENT: f32 = 0.55;
const DIFFUSE: f32 = 0.45;

/// One draw ready for the renderer.
struct Part {
    layout: LoadedVertexLayout,
    vertices: Vec<u8>,
    indices: Vec<u32>,
    texture: Option<u16>,
    colors: bool,
    draw: DrawParams,
}

pub struct Renderer {
    pub gx: GxRenderer,
    pub format: wgpu::TextureFormat,
    parts: Vec<Part>,
    /// How many of `parts` are the land (kept by [`Self::set_extra`]).
    land: usize,
    textures: Vec<(TextureId, u32, u32)>,
    materials: [GxMaterial; 2],
}

/// The material: one channel lit by light 0 with the vertex color (or the
/// white material register) as material color, one TEV stage of texture
/// times rasterized color.
fn material(vertex_color: bool) -> GxMaterial {
    let mut b = GxMaterialBuilder::new();
    b.set_cull_mode(cull_mode::NONE);
    b.set_num_chans(1);
    let src = if vertex_color {
        color_src::VTX
    } else {
        color_src::REG
    };
    b.set_chan_ctrl(
        color_channel_id::COLOR0A0,
        true,
        color_src::REG,
        src,
        1,
        diffuse_function::CLAMP,
        attenuation_function::NONE,
    );
    b.set_num_tex_gens(1);
    b.set_tex_coord_gen(
        0,
        tex_gen_type::MTX2x4,
        tex_gen_src::TEX0,
        tex_gen_matrix::IDENTITY,
        false,
        post_tex_gen_matrix::PTIDENTITY,
    );
    b.set_num_tev_stages(1);
    b.set_tev_order(
        0,
        tex_coord_id::TEXCOORD0,
        tex_map_id::TEXMAP0,
        // The TEV order takes the rasterized channel id.
        ras_color_channel_id::COLOR0A0,
    );
    b.set_tev_color_in(0, cc::ZERO, cc::TEXC, cc::RASC, cc::ZERO);
    b.set_tev_alpha_in(0, ca::ZERO, ca::TEXA, ca::RASA, ca::ZERO);
    b.set_tev_color_op(
        0,
        tev_op::ADD,
        tev_bias::ZERO,
        tev_scale::SCALE_1,
        true,
        register::PREV,
    );
    b.set_tev_alpha_op(
        0,
        tev_op::ADD,
        tev_bias::ZERO,
        tev_scale::SCALE_1,
        true,
        register::PREV,
    );
    b.set_alpha_compare(
        compare_type::GREATER,
        127,
        alpha_op::AND,
        compare_type::ALWAYS,
        0,
    );
    b.finish()
}

/// The layout chunk geometry is packed in: position, normal, color 0 and
/// texcoord 0, direct.
fn chunk_layout() -> LoadedVertexLayout {
    let mut vcd: Vcd = [VtxDesc::default(); ATTR_COUNT];
    for a in [attr::POS, attr::NRM, attr::CLR0, attr::TEX0] {
        vcd[a as usize].ty = attr_type::DIRECT;
    }
    compile_loaded_vertex_layout(&vcd, false)
}

/// `vertices` in `layout` (little-endian floats, color as four bytes).
fn pack(layout: &LoadedVertexLayout, vertices: &[Vertex]) -> Vec<u8> {
    let mut out = vec![0u8; layout.stride * vertices.len()];
    for (v, dst) in vertices.iter().zip(out.chunks_exact_mut(layout.stride)) {
        for l in &layout.inputs {
            let floats: &[f32] = match l.input {
                VertexInput::Pos => &[v.pos[0], v.pos[1], v.pos[2], 0.0],
                VertexInput::Nrm => &v.normal,
                VertexInput::Tex01 => &[v.uv[0], v.uv[1], 0.0, 0.0],
                VertexInput::Clr0 => &v.color,
                _ => &[],
            };
            match l.format {
                InputFormat::Unorm8x4 => {
                    for (k, c) in floats.iter().enumerate().take(4) {
                        dst[l.offset + k] = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
                InputFormat::F32(n) => {
                    for (k, c) in floats.iter().enumerate().take(n as usize) {
                        dst[l.offset + 4 * k..l.offset + 4 * k + 4]
                            .copy_from_slice(&c.to_le_bytes());
                    }
                }
            }
        }
    }
    out
}

fn parts_of(mesh: &Mesh, chunk: &LoadedVertexLayout) -> Vec<Part> {
    let mut out = Vec::new();
    if !mesh.indices.is_empty() {
        let vertices = pack(chunk, &mesh.vertices);
        let whole = [crate::land::Draw {
            texture: None,
            first: 0,
            count: mesh.indices.len() as u32,
        }];
        let draws = if mesh.draws.is_empty() {
            &whole[..]
        } else {
            &mesh.draws[..]
        };
        for d in draws {
            let range = d.first as usize..(d.first + d.count) as usize;
            out.push(Part {
                layout: chunk.clone(),
                vertices: vertices.clone(),
                indices: mesh.indices.get(range).unwrap_or(&[]).to_vec(),
                texture: d.texture,
                colors: true,
                draw: DrawParams::default(),
            });
        }
    }
    out.extend(mesh.parts.iter().map(|p: &GxPart| {
        let mut draw = DrawParams::default();
        draw.pos_mtx[0] = p.model;
        Part {
            layout: p.layout.clone(),
            vertices: p.vertices.clone(),
            indices: p.indices.clone(),
            texture: p.texture,
            colors: p.colors,
            draw,
        }
    }));
    out
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gx: GxRenderer::new(device, queue, format),
            format,
            parts: Vec::new(),
            land: 0,
            textures: Vec::new(),
            materials: [material(false), material(true)],
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gx.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.gx.queue
    }

    /// The stage's textures, in texture-id order.
    pub fn set_textures(&mut self, images: &[Rgba]) {
        for (id, _, _) in self.textures.drain(..) {
            self.gx.destroy_texture(id);
        }
        self.textures = images
            .iter()
            .map(|i| {
                let (w, h) = (i.width.max(1), i.height.max(1));
                (self.gx.create_texture(&[(w, h, i.pixels.clone())]), w, h)
            })
            .collect();
    }

    /// Replace the drawn meshes; the first is the land.
    pub fn set_meshes(&mut self, meshes: &[&Mesh]) {
        let chunk = chunk_layout();
        self.parts.clear();
        self.land = 0;
        for (i, m) in meshes.iter().enumerate() {
            self.parts.extend(parts_of(m, &chunk));
            if i == 0 {
                self.land = self.parts.len();
            }
        }
    }

    /// Keep the land and replace the rest (moving things).
    pub fn set_extra(&mut self, meshes: &[&Mesh]) {
        let chunk = chunk_layout();
        self.parts.truncate(self.land);
        for m in meshes {
            self.parts.extend(parts_of(m, &chunk));
        }
    }

    /// Draw every mesh into `target` (of `size`) with this view-projection
    /// (column-major, clip space z 0..1). The model matrices stay in the
    /// draws' position matrices, so lighting is in world space.
    pub fn render(&mut self, target: &wgpu::TextureView, size: [u32; 2], view_proj: [[f32; 4]; 4]) {
        let scene = SceneParams {
            projection: view_proj,
            lod_bias: 0.0,
        };
        let params: Vec<MaterialParams> = self
            .parts
            .iter()
            .map(|p| {
                let mut m = MaterialParams::default();
                m.colors[color_kind::AMB0] = [AMBIENT, AMBIENT, AMBIENT, 1.0];
                m.lights[0] = Light {
                    position: LIGHT_DIR.map(|c| c * 1.0e6),
                    color: [DIFFUSE, DIFFUSE, DIFFUSE, 1.0],
                    ..Light::default()
                };
                if let Some(&(id, w, h)) = p.texture.and_then(|t| self.textures.get(usize::from(t)))
                {
                    m.texture_mappings[0] = TextureMapping {
                        texture: Some(id),
                        width: w as f32,
                        height: h as f32,
                        ..TextureMapping::default()
                    };
                }
                m
            })
            .collect();
        let draws: Vec<GpuDraw<'_>> = self
            .parts
            .iter()
            .zip(&params)
            .map(|(p, m)| GpuDraw {
                material: &self.materials[usize::from(p.colors)],
                layout: &p.layout,
                vertices: &p.vertices,
                indices: &p.indices,
                material_params: m,
                draw_params: &p.draw,
            })
            .collect();
        self.gx
            .render(target, size, Some([0.05, 0.06, 0.09, 1.0]), &scene, &draws);
    }
}

/// A perspective view looking from `eye` at `at` (Ninja's Y up), column
/// major for WGSL, depth 0..1.
pub fn look_at(
    eye: [f32; 3],
    at: [f32; 3],
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> [[f32; 4]; 4] {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let f = norm(sub(at, eye));
    let s = norm(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(s, f);
    // Rows of the view matrix (right-handed, looking down -Z).
    let view = [
        [s[0], s[1], s[2], -dot(s, eye)],
        [u[0], u[1], u[2], -dot(u, eye)],
        [-f[0], -f[1], -f[2], dot(f, eye)],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let t = 1.0 / (fov_y / 2.0).tan();
    let proj = [
        [t / aspect, 0.0, 0.0, 0.0],
        [0.0, t, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), near * far / (near - far)],
        [0.0, 0.0, -1.0, 0.0],
    ];
    // Row-major product, then transpose to column major.
    let mut m = [[0.0f32; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| proj[i][k] * view[k][j]).sum();
        }
    }
    std::array::from_fn(|c| std::array::from_fn(|r| m[r][c]))
}
