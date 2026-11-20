//! Iron Gate in the browser (milestone E8): the page hands over the Steam
//! files it read from the folder the player picked; this runs the native
//! game core (`sa2-runtime`) and draws it with `sa2-render` on WebGPU, or
//! WebGL2 through wgpu's fallback. Nothing from the game is bundled.
#![forbid(unsafe_code)]

use sa2_asset_tools::input::PadStatus;
use sa2_asset_tools::{city_rel::CityRel, stage_land};
use sa2_render::gc::GcSource;
use sa2_render::gpu::{look_at, Renderer};
use sa2_render::land::{land_mesh, Mesh, Vertex};
use sa2_runtime::game::{Game, SteamData};
use sa2_runtime::walker_slot;
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// A box of half-size `h` at `p`, the stand-in for the walker's model.
fn marker(p: [f32; 3], h: f32) -> Mesh {
    let mut m = Mesh::default();
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    for (n, a, b) in faces {
        let base = m.vertices.len() as u32;
        for (s, t) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let pos = std::array::from_fn(|k| {
                p[k] + h * (n[k] + s * a[k] + t * b[k]) + h * [0.0, 1.0, 0.0][k]
            });
            m.vertices.push(Vertex {
                pos,
                normal: n,
                uv: [0.0; 2],
                color: [0.95, 0.35, 0.15, 1.0],
            });
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

#[wasm_bindgen]
pub struct App {
    game: Game,
    renderer: Renderer,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    player: Option<sa2_runtime::TaskId>,
    /// The walker's model in its own space (bind pose).
    walker: Option<Mesh>,
    camera: sa2_runtime::camera::DefaultCamera,
    pub triangles: u32,
}

/// Bank row of `ewalkmdl.prs` drawn as the walker: the display fn_800B15B8
/// draws CharacterModels (0x801E6688)[action row +2], and 137 of Mech
/// Eggman's 139 action rows name 248 (the loader 0x800B5D2C reads
/// EWALKMDL.PRS, or EWALK2MDL.PRS for costume 1). shortcut: drawn in its
/// bind pose; per-action models (268, 287) and motions come later.
const WALKER_BODY: i32 = 248;

/// The walker body from `ewalkmdl.prs`, its textures (`ewalktex.prs`)
/// appended to `textures` with the draws' ids moved past the land's.
fn walker_mesh(
    mdl: &[u8],
    tex: Option<&[u8]>,
    textures: &mut Vec<sa2_render::gpu::Rgba>,
) -> Result<Mesh, JsValue> {
    let bank = sa2_asset_tools::model_bank::read_prs(mdl).map_err(err)?;
    let row = bank
        .iter()
        .find(|r| r.id == WALKER_BODY)
        .ok_or_else(|| err("ewalkmdl.prs has no walker body row"))?;
    let (mut mesh, _) = sa2_render::land::model_mesh(&row.model);
    let base = u16::try_from(textures.len()).map_err(err)?;
    match tex {
        Some(t) => {
            textures.extend(sa2_render::textures::gvm_textures(t).map_err(err)?);
            for d in &mut mesh.draws {
                d.texture = d.texture.and_then(|i| i.checked_add(base));
            }
        }
        None => mesh.draws.iter_mut().for_each(|d| d.texture = None),
    }
    Ok(mesh)
}

#[wasm_bindgen]
impl App {
    /// Load Iron Gate from the Steam files and draw into `canvas`.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        canvas: web_sys::HtmlCanvasElement,
        exe: Vec<u8>,
        stg11: Vec<u8>,
        set_s: Option<Vec<u8>>,
        set_u: Option<Vec<u8>>,
        motions: Option<Vec<u8>>,
        landtx: Option<Vec<u8>>,
        walker_mdl: Option<Vec<u8>>,
        walker_tex: Option<Vec<u8>>,
    ) -> Result<App, JsValue> {
        let data = SteamData {
            exe,
            stg11,
            set_s,
            set_u,
        };
        let mut game = Game::iron_gate(&data).map_err(err)?;
        let stage = stage_land::read(&data.stg11).map_err(err)?;
        let row = stage_land::catalog()
            .iter()
            .find(|r| r.filename == "stg11D.rel")
            .ok_or_else(|| err("stg11D.rel is not catalogued"))?;
        let rel = CityRel::read_identified(&data.stg11, row.module).map_err(err)?;
        let gc = GcSource {
            rel: &rel,
            section: row.section,
        };
        let (land, _) = land_mesh(&stage, Some(&gc));
        // The walker thread's adapter in the exec slot (with Iron Gate's
        // collision land, the Steam physics rows and action table).
        let rows = sa2_runtime::player::steam_physics_rows(&data.exe).map_err(err)?;
        let actions = walker_slot::mech_eggman_actions(&data.exe).map_err(err)?;
        game.walker = Some(Box::new(walker_slot::AdapterWalker::new(
            walker_slot::Land::from_stage(&stage),
            walker_slot::physics_rows(&rows),
            actions,
            motions.as_deref().unwrap_or(&[]),
        )));

        let (w, h) = (canvas.width().max(1), canvas.height().max(1));
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(err)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(err)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(err)?;
        let mut config = surface
            .get_default_config(&adapter, w, h)
            .ok_or_else(|| err("surface not supported by the adapter"))?;
        // WebGPU canvases offer only non-sRGB formats (bgra8unorm); the shader
        // writes linear colour, so draw through the sRGB view to encode it.
        let srgb = config.format.add_srgb_suffix();
        if srgb != config.format {
            config.view_formats = vec![srgb];
        }
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(device, queue, srgb);
        renderer.set_meshes(&[&land]);
        let mut textures = match landtx {
            Some(t) => sa2_render::textures::gvm_textures(&t).map_err(err)?,
            None => Vec::new(),
        };
        let walker = match walker_mdl {
            Some(m) => Some(walker_mesh(&m, walker_tex.as_deref(), &mut textures)?),
            None => None,
        };
        renderer.set_textures(&textures);
        let player = game.frame.player_tasks[0];
        Ok(App {
            game,
            renderer,
            surface,
            config,
            player,
            walker,
            camera: Default::default(),
            triangles: (land.indices.len() / 3) as u32,
        })
    }

    /// One frame: pad 0's stick (-128..127, up positive) and buttons (GC
    /// PAD bits: 0x100 A, 0x200 B, 0x1000 Start), then draw.
    pub fn frame(&mut self, stick_x: i8, stick_y: i8, buttons: u16) -> String {
        let mut pads = [PadStatus::default(); 4];
        pads[0].stick = [stick_x, stick_y];
        pads[0].button = buttons;
        self.game.step(pads);
        let (pos, ang, height) = self
            .player
            .and_then(|t| self.game.rt.task(t))
            .map(|t| {
                let f = |o| f32::from_bits(t.twp_word(o));
                let a = |o| t.twp_word(o) as i32;
                // Player work +0x13C (the camera's target height).
                let h = t
                    .w40
                    .as_ref()
                    .and_then(|w| w.get(0x13C..0x140))
                    .map_or(0.0, |b| f32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                ([f(0x14), f(0x18), f(0x1C)], [a(0x8), a(0xC), a(0x10)], h)
            })
            .unwrap_or(([0.0; 3], [0; 3], 0.0));
        let cam = self.camera.step(pos, ang, height);
        match &self.walker {
            Some(m) => self
                .renderer
                .set_extra(&[&sa2_render::land::place(m, pos, ang)]),
            None => self.renderer.set_extra(&[&marker(pos, 8.0)]),
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return "frame skipped".into(),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.renderer.format),
            ..Default::default()
        });
        let size = [self.config.width, self.config.height];
        let vp = look_at(
            cam.pos,
            cam.target,
            1.0,
            size[0] as f32 / size[1] as f32,
            5.0,
            30000.0,
        );
        self.renderer.render(&view, size, vp);
        self.renderer.queue.present(frame);
        format!(
            "frame {} · player {:.1} {:.1} {:.1}",
            self.game.frames, pos[0], pos[1], pos[2]
        )
    }

    /// Sound commands the game made since the last call, as JSON for
    /// `sa2-audio.js` (call once a frame, after `frame`).
    pub fn sounds(&mut self) -> String {
        self.game.sound.drain_json()
    }

    /// Resize the drawing surface to the canvas's new size.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.renderer.device, &self.config);
    }
}
