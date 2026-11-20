//! A window for the recompiled game: GX frames from the game thread are drawn
//! with sa2-gx's renderer; a gamepad and the keyboard/mouse are controller 1.
//!
//! Keyboard: W/A/S/D = stick (ramps to full over 3 frames), Space = A (jump),
//! E or Left Shift = B (action), Q = X, R = Y, Enter or Esc = Start, 1/3 = L/R.
//! Mouse: left button = B, right button = A.
//! Gamepad (gilrs: XInput, PlayStation, ...): left stick = stick, South
//! (A/Cross) = A, West (X/Square) = B, East (B/Circle) = X, North
//! (Y/Triangle) = Y, Start, LT/RT = L/R, RB = Z, D-pad = D-pad.
use sa2_asset_tools::input::PadStatus;
use sa2_gx::api::Frame;
use sa2_gx::gpu::GxRenderer;
use sa2_gx::interp::Tween;
use std::sync::atomic::{AtomicU16, Ordering::Relaxed};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Inputs held, as bits of `KEYS` below (then the two mouse buttons).
static HELD: AtomicU16 = AtomicU16::new(0);
/// Bounded: when the window is this many frames behind, the game waits (no
/// frame or texture upload is dropped, and memory stays flat).
const QUEUED_FRAMES: usize = 2;
static TX: Mutex<Option<SyncSender<Frame>>> = Mutex::new(None);

/// What each input bit is: (key, GC button bits; 0 = a stick direction).
const KEYS: [(KeyCode, u16); 12] = [
    (KeyCode::KeyA, 0),
    (KeyCode::KeyD, 0),
    (KeyCode::KeyS, 0),
    (KeyCode::KeyW, 0),
    (KeyCode::Space, 0x0100),     // A
    (KeyCode::KeyE, 0x0200),      // B
    (KeyCode::ShiftLeft, 0x0200), // B
    (KeyCode::KeyQ, 0x0400),      // X
    (KeyCode::KeyR, 0x0800),      // Y
    (KeyCode::Enter, 0x1000),     // Start
    (KeyCode::Escape, 0x1000),    // Start
    (KeyCode::Digit1, 0x0040),    // L
];
/// Bits after `KEYS`: Digit3 (R), then the left and right mouse buttons.
const KEY_R: u16 = 1 << 12;
const MOUSE_LEFT: u16 = 1 << 13; // B
const MOUSE_RIGHT: u16 = 1 << 14; // A

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

/// The keyboard stick as (angle in radians, magnitude 0..=1).
static EASED: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));

/// Stick from W/A/S/D like a real stick moves: from rest the deflection
/// ramps to full over 3 frames; while any direction is held it stays full and
/// only the angle turns (about 34° a frame) toward the keys, so turning never
/// dips through the centre (SA2 reads a dip as stopping); diagonals are as
/// long as straights.
pub fn key_stick(left: bool, right: bool, down: bool, up: bool) -> [i8; 2] {
    const TURN: f32 = 0.6;
    const RAMP: f32 = 0.34;
    let dx = f32::from(i8::from(right) - i8::from(left));
    let dy = f32::from(i8::from(up) - i8::from(down));
    let mut e = EASED.lock().unwrap_or_else(|e| e.into_inner());
    let (angle, mag) = &mut *e;
    if dx != 0.0 || dy != 0.0 {
        let want = dy.atan2(dx);
        if *mag == 0.0 {
            *angle = want;
        } else {
            let pi = std::f32::consts::PI;
            let diff = (want - *angle + 3.0 * pi).rem_euclid(2.0 * pi) - pi;
            *angle += diff.clamp(-TURN, TURN);
        }
        *mag = (*mag + RAMP).min(1.0);
    } else {
        *mag = (*mag - RAMP).max(0.0);
    }
    [
        (angle.cos() * *mag * 100.0).round() as i8,
        (angle.sin() * *mag * 100.0).round() as i8,
    ]
}

/// This frame's controller 1: the gamepad and the keyboard/mouse together
/// (buttons from both; the pad's stick when it is pushed, else the keys').
pub fn pad() -> PadStatus {
    let h = HELD.load(Relaxed);
    let on = |i: usize| h >> i & 1 != 0;
    let mut p = PadStatus::default();
    for (i, (_, bit)) in KEYS.iter().enumerate() {
        if *bit != 0 && on(i) {
            p.button |= bit;
        }
    }
    if h & KEY_R != 0 {
        p.button |= 0x0020; // R
    }
    if h & MOUSE_LEFT != 0 {
        p.button |= 0x0200; // B
    }
    if h & MOUSE_RIGHT != 0 {
        p.button |= 0x0100; // A
    }
    p.stick = key_stick(on(0), on(1), on(2), on(3));
    if let Some(g) = crate::gamepad::state() {
        p.button |= g.button;
        if g.stick[0].unsigned_abs() > 12 || g.stick[1].unsigned_abs() > 12 {
            p.stick = g.stick;
        }
        p.substick = g.substick;
        p.triggers = [
            p.triggers[0].max(g.triggers[0]),
            p.triggers[1].max(g.triggers[1]),
        ];
    }
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
    /// How to draw between the frame before `last` and `last`, and when
    /// `last` arrived: the window refreshes faster than the game's 60.
    tween: Option<Tween>,
    arrived: std::time::Instant,
    error: Option<String>,
}

/// A game frame (NTSC).
const GAME_FRAME: f32 = 1001.0 / 60_000.0;

fn open(event_loop: &ActiveEventLoop) -> Result<Gpu, String> {
    let attrs = Window::default_attributes()
        .with_title("SA2 (recompiled)")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 960.0));
    let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance
        .create_surface(window.clone())
        .map_err(|e| e.to_string())?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .map_err(|e| format!("no wgpu adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits:
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
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
    Ok(Gpu {
        window,
        surface,
        device,
        queue,
        config,
        renderer,
    })
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
                    let bit = match KEYS.iter().position(|(k, _)| *k == code) {
                        Some(i) => 1u16 << i,
                        None if code == KeyCode::Digit3 => KEY_R,
                        None => 0,
                    };
                    hold(bit, event.state);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Left => hold(MOUSE_LEFT, state),
                winit::event::MouseButton::Right => hold(MOUSE_RIGHT, state),
                _ => {}
            },
            // Losing focus releases everything (no key stuck down).
            WindowEvent::Focused(false) => HELD.store(0, Relaxed),
            WindowEvent::RedrawRequested => {
                let t = std::time::Instant::now();
                // Every frame's texture uploads reach the renderer once; the
                // newest frame is drawn, blended from the one before it
                // (SA2_NO_TWEEN=1 shows game frames as they are).
                for mut f in self.rx.try_iter() {
                    for u in f.uploads.drain(..) {
                        g.renderer.replace_texture(u.id, &u.levels);
                    }
                    let prev = self.last.take();
                    self.tween = prev
                        .filter(|_| std::env::var_os("SA2_NO_TWEEN").is_none())
                        .map(|p| Tween::new(&p, &f));
                    self.last = Some(f);
                    self.arrived = std::time::Instant::now();
                }
                if let Some(f) = &mut self.last {
                    if let Some(tw) = &self.tween {
                        tw.apply(f, self.arrived.elapsed().as_secs_f32() / GAME_FRAME);
                    }
                    match g.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(tex)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => {
                            let view = tex.texture.create_view(&Default::default());
                            g.renderer
                                .render_frame(f, &view, [g.config.width, g.config.height]);
                            g.queue.present(tex);
                            crate::prof::PRESENTS.fetch_add(1, Relaxed);
                        }
                        wgpu::CurrentSurfaceTexture::Outdated => {
                            g.surface.configure(&g.device, &g.config)
                        }
                        _ => {}
                    }
                }
                // Free what finished frames used (each frame makes its own
                // buffers); without this wgpu kept them all.
                let _ = g.device.poll(wgpu::PollType::Poll);
                crate::prof::add(&crate::prof::RENDER, t);
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
    let mut app = App {
        rx,
        gpu: None,
        last: None,
        tween: None,
        arrived: std::time::Instant::now(),
        error: None,
    };
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

fn hold(bit: u16, state: ElementState) {
    match state {
        ElementState::Pressed => HELD.fetch_or(bit, Relaxed),
        ElementState::Released => HELD.fetch_and(!bit, Relaxed),
    };
}

#[cfg(test)]
mod tests {
    use super::key_stick;

    #[test]
    fn turning_keeps_full_deflection() {
        let mag = |s: [i8; 2]| (f32::from(s[0]).hypot(f32::from(s[1]))).round();
        for _ in 0..5 {
            key_stick(false, false, false, true); // W
        }
        assert_eq!(key_stick(false, false, false, true), [0, 100]);
        // W -> D: rotate at full length, never through the centre.
        for _ in 0..4 {
            assert!(mag(key_stick(false, true, false, false)) >= 99.0);
        }
        assert_eq!(key_stick(false, true, false, false), [100, 0]);
        // Diagonal W+D is as long as a straight.
        for _ in 0..3 {
            key_stick(false, true, false, true);
        }
        assert_eq!(key_stick(false, true, false, true), [71, 71]);
        // Released: back to centre within 3 frames.
        for _ in 0..3 {
            key_stick(false, false, false, false);
        }
        assert_eq!(key_stick(false, false, false, false), [0, 0]);
    }
}
