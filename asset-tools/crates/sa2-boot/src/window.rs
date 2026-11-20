//! A window for the recompiled game: GX frames from the game thread are drawn
//! with sa2-gx's renderer, and the keyboard is controller port 0.
//!
//! Keys: arrows = stick (and D-pad), X/Space = A, Z = B, C = X, V = Y,
//! Enter = Start, Q/E = L/R triggers.
use sa2_asset_tools::input::PadStatus;
use sa2_gx::api::Frame;
use sa2_gx::gpu::GxRenderer;
use std::sync::atomic::{AtomicU16, Ordering::Relaxed};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Keys held, as bits of `KEYS` below.
static HELD: AtomicU16 = AtomicU16::new(0);
/// Bounded: when the window is this many frames behind, the game waits (no
/// frame or texture upload is dropped, and memory stays flat).
const QUEUED_FRAMES: usize = 2;
static TX: Mutex<Option<SyncSender<Frame>>> = Mutex::new(None);

const KEYS: [KeyCode; 12] = [
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ArrowDown,
    KeyCode::ArrowUp,
    KeyCode::KeyX,
    KeyCode::Space,
    KeyCode::KeyZ,
    KeyCode::KeyC,
    KeyCode::KeyV,
    KeyCode::Enter,
    KeyCode::KeyQ,
    KeyCode::KeyE,
];

/// Open the window on its own thread; frames given to [`submit`] show there.
pub fn start() {
    let (tx, rx) = sync_channel(QUEUED_FRAMES);
    if let Ok(mut t) = TX.lock() {
        *t = Some(tx);
    }
    std::thread::spawn(move || {
        if let Err(e) = run(rx) {
            eprintln!("window: {e}");
        }
    });
}

/// Hand a finished frame to the window (no-op without one).
pub fn submit(f: Frame) {
    if let Ok(t) = TX.lock() {
        if let Some(tx) = t.as_ref() {
            let _ = tx.send(f);
        }
    }
}

/// This frame's controller 0 from the keys held.
pub fn pad() -> PadStatus {
    let h = HELD.load(Relaxed);
    let on = |i: usize| h >> i & 1 != 0;
    let mut p = PadStatus::default();
    let mut b = 0u16;
    for (i, bit) in [(0, 0x0001), (1, 0x0002), (2, 0x0004), (3, 0x0008)] {
        if on(i) {
            b |= bit;
        }
    }
    if on(4) || on(5) {
        b |= 0x0100; // A
    }
    if on(6) {
        b |= 0x0200; // B
    }
    if on(7) {
        b |= 0x0400; // X
    }
    if on(8) {
        b |= 0x0800; // Y
    }
    if on(9) {
        b |= 0x1000; // Start
    }
    if on(10) {
        b |= 0x0040; // L
    }
    if on(11) {
        b |= 0x0020; // R
    }
    p.button = b;
    let axis = |neg: bool, pos: bool| if neg == pos { 0 } else if pos { 100 } else { -100 };
    p.stick = [axis(on(0), on(1)), axis(on(2), on(3))];
    p
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GxRenderer,
}

struct App {
    rx: Receiver<Frame>,
    gpu: Option<Gpu>,
    last: Option<Frame>,
    error: Option<String>,
}

fn open(event_loop: &ActiveEventLoop) -> Result<Gpu, String> {
    let attrs = Window::default_attributes()
        .with_title("SA2 (recompiled)")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 960.0));
    let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .map_err(|e| format!("no wgpu adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| format!("no wgpu device: {e}"))?;
    let size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, size.width.max(1), size.height.max(1))
        .ok_or("surface not supported")?;
    // The game's colours are already gamma-encoded: use a non-sRGB format.
    config.format = config.format.remove_srgb_suffix();
    surface.configure(&device, &config);
    let renderer = GxRenderer::new(device.clone(), queue.clone(), config.format);
    Ok(Gpu { window, surface, device, queue, config, renderer })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_none() {
            match open(event_loop) {
                Ok(g) => self.gpu = Some(g),
                Err(e) => {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gpu.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                g.config.width = s.width.max(1);
                g.config.height = s.height.max(1);
                g.surface.configure(&g.device, &g.config);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(i) = KEYS.iter().position(|k| *k == code) {
                        let bit = 1u16 << i;
                        match event.state {
                            ElementState::Pressed => HELD.fetch_or(bit, Relaxed),
                            ElementState::Released => HELD.fetch_and(!bit, Relaxed),
                        };
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Every frame's texture uploads must reach the renderer;
                // only the newest is drawn.
                for f in self.rx.try_iter() {
                    if let Some(old) = self.last.replace(f) {
                        for u in &old.uploads {
                            g.renderer.replace_texture(u.id, &u.levels);
                        }
                    }
                }
                if let Some(f) = &self.last {
                    match g.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(tex) | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => {
                            let view = tex.texture.create_view(&Default::default());
                            g.renderer.render_frame(f, &view, [g.config.width, g.config.height]);
                            g.queue.present(tex);
                        }
                        wgpu::CurrentSurfaceTexture::Outdated => g.surface.configure(&g.device, &g.config),
                        _ => {}
                    }
                }
                // Free what finished frames used (each frame makes its own
                // buffers); without this wgpu kept them all.
                let _ = g.device.poll(wgpu::PollType::Poll);
                g.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn run(rx: Receiver<Frame>) -> Result<(), String> {
    let mut builder = EventLoop::builder();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let mut app = App { rx, gpu: None, last: None, error: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

/// Hold the game to 60 frames a second of real time (the headless runs go
/// as fast as they can; a played window must not).
pub fn pace() {
    use std::time::{Duration, Instant};
    static NEXT: Mutex<Option<Instant>> = Mutex::new(None);
    let frame = Duration::from_nanos(1_000_000_000 / 60);
    let Ok(mut next) = NEXT.lock() else { return };
    let now = Instant::now();
    let due = next.map_or(now, |t| t.max(now - frame));
    if due > now {
        std::thread::sleep(due - now);
    }
    *next = Some(due + frame);
}
