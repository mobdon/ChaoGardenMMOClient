//! The recompiled SA2 in a browser tab, page side: the game (sa2-web-game,
//! in web workers) hands over each finished frame's GX job, the pipe bytes
//! and a copy of RAM; this decodes it with sa2-gx's state machine and draws
//! it with sa2-gx's renderer, as the desktop window does: newest frame
//! shown, blended from the one before it when the display refreshes faster
//! than the game's 60, widened to the canvas unless 4:3 is asked for.
#![forbid(unsafe_code)]

use sa2_gx::api::{Frame, GxMemory, Ram};
use sa2_gx::fifo::GxFifo;
use sa2_gx::gpu::GxRenderer;
use sa2_gx::interp::Tween;
use wasm_bindgen::prelude::*;

/// Framing of a job, as sa2-web-game writes it to `/dev/gx`.
const GX_MAGIC: u32 = 0x4758_4A42;
/// A game frame (NTSC), in milliseconds.
const GAME_FRAME_MS: f64 = 1001.0 / 60.0;

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
pub struct Screen {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GxRenderer,
    fifo: GxFifo,
    last: Option<Frame>,
    tween: Option<Tween>,
    arrived_ms: f64,
    backend: String,
    /// Where the page copies each job (kept: one allocation, not one a frame).
    job: Vec<u8>,
}

/// The job's pipe bytes and RAM (`None` when it isn't one).
fn split(job: &[u8]) -> Option<(&[u8], &[u8])> {
    let word = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            job.get(4 * i..4 * i + 4)?.try_into().ok()?,
        ))
    };
    if word(0)? != GX_MAGIC {
        return None;
    }
    let (fifo, ram) = (word(2)? as usize, word(3)? as usize);
    let rest = job.get(16..)?;
    let fifo_bytes = rest.get(..fifo)?;
    let ram_bytes = rest.get(fifo..fifo.checked_add(ram)?)?;
    Some((fifo_bytes, ram_bytes))
}

/// Frames the decoder finished and the screen hasn't taken yet, with when
/// each was decoded. Both live in one shared memory (two workers run this
/// module), so the decoder hands over whole frames.
static READY: std::sync::Mutex<Vec<(Frame, f64)>> = std::sync::Mutex::new(Vec::new());

fn ready() -> std::sync::MutexGuard<'static, Vec<(Frame, f64)>> {
    READY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Decodes the game's GX jobs, on a worker of its own: decoding a frame
/// takes as long as drawing several, so the screen's worker only draws.
#[wasm_bindgen]
pub struct Decoder {
    fifo: GxFifo,
    /// Where the page copies each job (kept: one allocation, not one a frame).
    job: Vec<u8>,
    /// The game RAM as the page last patched it (changed-page jobs, GXJD):
    /// kept here so only changed pages are written each frame.
    ram: Vec<u8>,
    /// The 4 KiB RAM pages the last changed-page job's decode read, one bit
    /// each: the game sends only these pages' changes (see `used_at`).
    used: Vec<u32>,
}

/// The kept RAM, noting each 4 KiB page the decode reads.
struct Tracked<'a> {
    ram: &'a [u8],
    used: std::cell::RefCell<&'a mut Vec<u32>>,
    /// The last page marked: most reads stay on one page (vertex arrays).
    last: std::cell::Cell<usize>,
}

impl GxMemory for Tracked<'_> {
    fn read(&self, addr: u32, len: usize) -> Option<&[u8]> {
        let p = (addr & 0x3FFF_FFFF) as usize;
        let bytes = self.ram.get(p..p.checked_add(len)?)?;
        let (first, end) = (p >> 12, (p + len.max(1) - 1) >> 12);
        if first == end && first == self.last.get() {
            return Some(bytes);
        }
        self.last.set(first);
        // Whole words at a time: sa2-gx views each vertex array as a 4 MiB
        // window (api.rs array_views), so ranges of ~1024 pages are common.
        let mut used = self.used.borrow_mut();
        let (fw, ew) = (first >> 5, end >> 5);
        let head = u32::MAX << (first & 31);
        let tail = u32::MAX >> (31 - (end & 31));
        if fw == ew {
            used[fw] |= head & tail;
        } else {
            used[fw] |= head;
            used[fw + 1..ew].fill(u32::MAX);
            used[ew] |= tail;
        }
        Some(bytes)
    }
}

#[wasm_bindgen]
impl Decoder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Decoder {
        Decoder {
            fifo: GxFifo::default(),
            job: Vec::new(),
            ram: Vec::new(),
            used: Vec::new(),
        }
    }

    /// Room for a job of `len` bytes: the page copies it to this address in
    /// this module's memory, then calls [`Self::decode`].
    pub fn job_at(&mut self, len: usize) -> *mut u8 {
        self.job.resize(len, 0);
        self.job.as_mut_ptr()
    }

    /// The kept RAM, `len` bytes (zeroed when the length changes): the page
    /// writes a changed-page job's pages here, then calls [`Self::decode_pages`].
    pub fn ram_at(&mut self, len: usize) -> *mut u8 {
        if self.ram.len() != len {
            self.ram = vec![0; len];
        }
        self.ram.as_mut_ptr()
    }

    /// Decode a changed-page job: its pipe bytes copied in with [`Self::job_at`],
    /// its pages already written into the kept RAM.
    pub fn decode_pages(&mut self, now_ms: f64, aspect: f32) -> Result<(), JsValue> {
        self.used.clear();
        self.used.resize(self.ram.len().div_ceil(4096 * 32), 0);
        let mem = Tracked { ram: &self.ram, used: std::cell::RefCell::new(&mut self.used), last: std::cell::Cell::new(usize::MAX) };
        self.fifo.write(&self.job, &mem);
        self.finish(now_ms, aspect);
        Ok(())
    }

    /// The pages the last [`Self::decode_pages`] read: `used_len` u32 words
    /// of bits (bit n = 4 KiB page n) at this address.
    pub fn used_at(&self) -> *const u32 {
        self.used.as_ptr()
    }
    pub fn used_len(&self) -> usize {
        self.used.len()
    }

    /// Decode the job copied in (at `now_ms`, `performance.now()`), widened
    /// to `aspect` (width / height; 0 keeps 4:3), for the screen to take.
    pub fn decode(&mut self, now_ms: f64, aspect: f32) -> Result<(), JsValue> {
        let (fifo, ram) = split(&self.job).ok_or_else(|| err("not a GX job"))?;
        self.fifo.write(fifo, &Ram(ram));
        self.finish(now_ms, aspect);
        Ok(())
    }
}

impl Decoder {
    fn finish(&mut self, now_ms: f64, aspect: f32) {
        let mut f = self.fifo.gx.take_frame();
        sa2_gx::widescreen::clamp_hud_textures(&mut f);
        if aspect > 0.0 {
            sa2_gx::widescreen::widen(&mut f, aspect);
        }
        ready().push((f, now_ms));
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}
#[wasm_bindgen]
impl Screen {
    /// WebGPU when the browser has it, else WebGL2.
    pub async fn open(canvas: web_sys::HtmlCanvasElement) -> Result<Screen, JsValue> {
        let size = (canvas.width(), canvas.height());
        Self::open_on(wgpu::SurfaceTarget::Canvas(canvas), size).await
    }

    /// The same, on a canvas handed to a worker (the page's own thread
    /// then only reads input and plays sound).
    pub async fn open_offscreen(canvas: web_sys::OffscreenCanvas) -> Result<Screen, JsValue> {
        let size = (canvas.width(), canvas.height());
        Self::open_on(wgpu::SurfaceTarget::OffscreenCanvas(canvas), size).await
    }

    async fn open_on(
        target: wgpu::SurfaceTarget<'static>,
        (w, h): (u32, u32),
    ) -> Result<Screen, JsValue> {
        let (w, h) = (w.max(1), h.max(1));
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
        let surface = instance.create_surface(target).map_err(err)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(err)?;
        let backend = format!("{:?}", adapter.get_info().backend);
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
            .ok_or_else(|| err("the canvas can't be drawn on with this adapter"))?;
        // The game's colours are already gamma-encoded: a non-sRGB format.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats.clear();
        surface.configure(&device, &config);
        let renderer = GxRenderer::new(device.clone(), queue.clone(), config.format);
        Ok(Screen {
            surface,
            device,
            queue,
            config,
            renderer,
            fifo: GxFifo::default(),
            last: None,
            tween: None,
            arrived_ms: 0.0,
            backend,
            job: Vec::new(),
        })
    }

    /// "BrowserWebGpu" or "Gl".
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// Room for a job of `len` bytes: the page copies it to this address in
    /// this module's memory, then calls [`Self::submit`]. One buffer kept
    /// for every frame (a fresh 24 MiB a frame grew the memory without end).
    pub fn job_at(&mut self, len: usize) -> *mut u8 {
        self.job.resize(len, 0);
        self.job.as_mut_ptr()
    }

    /// Decode the job the page copied in (at `now_ms`, `performance.now()`).
    /// `wide`: widen to the canvas; `blend`: draw between game frames.
    pub fn submit(&mut self, now_ms: f64, wide: bool, blend: bool) -> Result<(), JsValue> {
        let job = std::mem::take(&mut self.job);
        let r = self.decode(&job, now_ms, wide, blend);
        self.job = job;
        r
    }

    fn decode(&mut self, job: &[u8], now_ms: f64, wide: bool, blend: bool) -> Result<(), JsValue> {
        let (fifo, ram) = split(job).ok_or_else(|| err("not a GX job"))?;
        self.fifo.write(fifo, &Ram(ram));
        let mut f = self.fifo.gx.take_frame();
        for u in f.uploads.drain(..) {
            self.renderer.replace_texture(u.id, &u.levels);
        }
        sa2_gx::widescreen::clamp_hud_textures(&mut f);
        if wide {
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            sa2_gx::widescreen::widen(&mut f, aspect);
        }
        let prev = self.last.take();
        self.tween = prev.filter(|_| blend).map(|p| Tween::new(&p, &f));
        self.last = Some(f);
        self.arrived_ms = now_ms;
        Ok(())
    }

    /// Take the frames the decoder finished: every texture they upload,
    /// and the newest to draw (blended from the one before when `blend`).
    /// How many there were.
    pub fn take(&mut self, blend: bool) -> u32 {
        let frames = std::mem::take(&mut *ready());
        let n = frames.len() as u32;
        for (mut f, at) in frames {
            for u in f.uploads.drain(..) {
                self.renderer.replace_texture(u.id, &u.levels);
            }
            let prev = self.last.take();
            self.tween = prev.filter(|_| blend).map(|p| Tween::new(&p, &f));
            self.last = Some(f);
            self.arrived_ms = at;
        }
        n
    }

    /// Draw the newest frame at the canvas's size now.
    pub fn draw(&mut self, now_ms: f64, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
        let Some(f) = &mut self.last else { return };
        if let Some(tw) = &self.tween {
            tw.apply(f, ((now_ms - self.arrived_ms) / GAME_FRAME_MS) as f32);
        }
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex)
            | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => {
                let view = tex.texture.create_view(&Default::default());
                self.renderer
                    .render_frame(f, &view, [self.config.width, self.config.height]);
                self.queue.present(tex);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config)
            }
            _ => {}
        }
        // Free what finished frames used (each frame makes its own buffers).
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// GX the decoder doesn't handle yet, with counts (for the page's log).
    pub fn unhandled(&self) -> String {
        format!("{:?}", self.fifo.gx.unhandled())
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_a_job_and_refuses_others() {
        let mut job = Vec::new();
        for v in [super::GX_MAGIC, 0x8000_0000, 2, 3] {
            job.extend(v.to_le_bytes());
        }
        job.extend([1, 2, 9, 8, 7]);
        assert_eq!(split(&job), Some((&[1u8, 2][..], &[9u8, 8, 7][..])));
        assert_eq!(split(&job[..20]), None);
        job[0] = 0;
        assert_eq!(split(&job), None);
    }
}
