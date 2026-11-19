//! Native display/input adapter for a source-port milestone, not a game engine.
//!
//! The caller produces each 240x160 straight-RGBA frame and implements all game
//! behavior. This module contains no pinball, Chao, save, reward or ROM logic.
//! It never opens a file, loads an asset, starts a service or changes a save.

use std::fmt;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const RGBA_BYTES: usize = WIDTH * HEIGHT * 4;
pub const WINDOW_SCALE: u32 = 3;

// Active-high GBA button order. A native KEYINPUT register adapter must invert
// these ten bits itself; host input is deliberately independent of MMIO.
pub const BUTTON_A: u16 = 1;
pub const BUTTON_B: u16 = 1 << 1;
pub const BUTTON_SELECT: u16 = 1 << 2;
pub const BUTTON_START: u16 = 1 << 3;
pub const BUTTON_RIGHT: u16 = 1 << 4;
pub const BUTTON_LEFT: u16 = 1 << 5;
pub const BUTTON_UP: u16 = 1 << 6;
pub const BUTTON_DOWN: u16 = 1 << 7;
pub const BUTTON_R: u16 = 1 << 8;
pub const BUTTON_L: u16 = 1 << 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    InvalidFrameBudget,
    InvalidTitle,
    UnsupportedPlatform,
    Allocation,
    BufferLength { rgba: usize, bgrx: usize },
    Callback { frame_index: u64, message: String },
    Windows { operation: &'static str, code: u32 },
    Presentation,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrameBudget => write!(f, "frame budget must be nonzero and finite"),
            Self::InvalidTitle => {
                write!(f, "host title must contain 1..256 UTF-8 bytes and no NUL")
            }
            Self::UnsupportedPlatform => {
                write!(f, "native source-port display host requires Windows")
            }
            Self::Allocation => write!(f, "could not allocate fixed framebuffer"),
            Self::BufferLength { rgba, bgrx } => write!(
                f,
                "RGBA/BGRX lengths must match whole pixels: {rgba}/{bgrx}"
            ),
            Self::Callback {
                frame_index,
                message,
            } => write!(f, "source-port frame {frame_index} stopped: {message}"),
            Self::Windows { operation, code } => {
                write!(f, "{operation} failed with Win32 code {code}")
            }
            Self::Presentation => write!(f, "native framebuffer presentation failed"),
        }
    }
}

impl std::error::Error for HostError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameContext {
    /// Deterministic callback index: 0 through frame_limit - 1.
    pub frame_index: u64,
    /// Active-high A, B, Select, Start, Right, Left, Up, Down, R, L bits.
    pub button_mask: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    FrameBudget,
    WindowClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReport {
    /// Number of completed game callbacks; repaint messages do not advance it.
    pub frames: u64,
    /// Successful raster presentations, including repaints of the latest frame.
    pub presentations: u64,
    pub stopped_by: StopReason,
    pub width: usize,
    pub height: usize,
    pub initial_integer_scale: u32,
}

/// Windows virtual-key mapping: Z/X = A/B, Backspace/Enter = Select/Start,
/// arrows = D-pad, A/S = L/R. Escape belongs to the host's close action.
pub fn button_for_virtual_key(key: u32) -> u16 {
    match key {
        0x5a => BUTTON_A,
        0x58 => BUTTON_B,
        0x08 => BUTTON_SELECT,
        0x0d => BUTTON_START,
        0x27 => BUTTON_RIGHT,
        0x25 => BUTTON_LEFT,
        0x26 => BUTTON_UP,
        0x28 => BUTTON_DOWN,
        0x53 => BUTTON_R,
        0x41 => BUTTON_L,
        _ => 0,
    }
}

/// Update one held key; auto-repeat remains idempotent and unrelated keys leave
/// the mask unchanged. Focus loss clears the entire mask in the window adapter.
pub fn update_button_mask(mask: u16, virtual_key: u32, pressed: bool) -> u16 {
    let button = button_for_virtual_key(virtual_key);
    if pressed {
        mask | button
    } else {
        mask & !button
    }
}

/// Convert straight RGBA to an opaque, top-down Windows BGRX raster. Partial
/// alpha is composited over the host's black background with nearest rounding.
/// A complete native GBA compositor should normally produce opaque RGBA frames.
pub fn rgba_to_bgrx(rgba: &[u8], bgrx: &mut [u8]) -> Result<(), HostError> {
    if rgba.len() != bgrx.len() || !rgba.len().is_multiple_of(4) {
        return Err(HostError::BufferLength {
            rgba: rgba.len(),
            bgrx: bgrx.len(),
        });
    }
    for (source, target) in rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgrx.as_chunks_mut::<4>().0.iter_mut())
    {
        let over_black =
            |channel: u8| ((u16::from(channel) * u16::from(source[3]) + 127) / 255) as u8;
        target.copy_from_slice(&[
            over_black(source[2]),
            over_black(source[1]),
            over_black(source[0]),
            0,
        ]);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegerViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: u32,
}

/// Center the largest whole-number upscale. A client smaller than the native
/// framebuffer displays black rather than silently resampling source pixels.
pub fn integer_viewport(client_width: i32, client_height: i32) -> Option<IntegerViewport> {
    if client_width < WIDTH as i32 || client_height < HEIGHT as i32 {
        return None;
    }
    let scale = (client_width / WIDTH as i32).min(client_height / HEIGHT as i32);
    let width = WIDTH as i32 * scale;
    let height = HEIGHT as i32 * scale;
    Some(IntegerViewport {
        x: (client_width - width) / 2,
        y: (client_height - height) / 2,
        width,
        height,
        scale: scale as u32,
    })
}

/// Run a finite source-port diagnostic session. Each callback receives the same
/// fixed-size buffer; its contents are retained until the caller changes them.
/// Windows scheduling affects display pacing, never callback indices or budget.
/// The visible title always states that this is an incomplete source-port.
pub fn run<F>(title: &str, frame_limit: u64, tick: F) -> Result<HostReport, HostError>
where
    F: FnMut(FrameContext, &mut [u8]) -> Result<(), String>,
{
    if frame_limit == 0 {
        return Err(HostError::InvalidFrameBudget);
    }
    if title.is_empty() || title.len() > 256 || title.contains('\0') {
        return Err(HostError::InvalidTitle);
    }
    #[cfg(windows)]
    {
        platform::run(title, frame_limit, tick)
    }
    #[cfg(not(windows))]
    {
        let _ = tick;
        Err(HostError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        ffi::c_void,
        mem::size_of,
        ptr::{null, null_mut},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, EndPaint, FillRect, GetStockObject, InvalidateRect, SetStretchBltMode,
            StretchDIBits, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLACK_BRUSH,
            COLORONCOLOR, DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GetClientRect, GetWindowLongPtrW, IsWindow, IsWindowVisible, LoadCursorW, PeekMessageW,
            RegisterClassW, SetWindowLongPtrW, ShowWindow, TranslateMessage, UnregisterClassW,
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG,
            PM_REMOVE, SW_SHOW, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP,
            WM_KILLFOCUS, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WNDCLASSW, WS_MAXIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_THICKFRAME,
        },
    };

    // Approximate native GBA scanout period. This throttles presentation only;
    // the game callback receives no wall-clock time and advances one frame.
    const FRAME_PERIOD: Duration = Duration::from_nanos(16_742_706);
    const MESSAGES_PER_FRAME: usize = 256;
    static CLASS_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct WindowState {
        buttons: Cell<u16>,
        closed: Cell<bool>,
        presentation_failed: Cell<bool>,
        presentations: Cell<u64>,
        raster: RefCell<Vec<u8>>,
    }

    struct WindowClass {
        instance: HINSTANCE,
        name: Vec<u16>,
    }

    impl Drop for WindowClass {
        fn drop(&mut self) {
            // SAFETY: this guard outlives its window and retains its class name.
            unsafe { UnregisterClassW(self.name.as_ptr(), self.instance) };
        }
    }

    struct Window(HWND);

    impl Drop for Window {
        fn drop(&mut self) {
            // SAFETY: this guard is dropped before WindowState and WindowClass.
            // A user close may already have destroyed this handle.
            unsafe {
                if IsWindow(self.0) != 0 {
                    DestroyWindow(self.0);
                }
            }
        }
    }

    fn win_error(operation: &'static str) -> HostError {
        // SAFETY: GetLastError takes no pointers and reads this thread's error.
        HostError::Windows {
            operation,
            code: unsafe { GetLastError() },
        }
    }

    fn framebuffer() -> Result<Vec<u8>, HostError> {
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(RGBA_BYTES)
            .map_err(|_| HostError::Allocation)?;
        pixels.resize(RGBA_BYTES, 0);
        Ok(pixels)
    }

    unsafe fn paint(hwnd: HWND, state: &WindowState) {
        let mut paint = PAINTSTRUCT::default();
        // SAFETY: hwnd is the window currently dispatching WM_PAINT. PAINTSTRUCT
        // and RECT are initialized, writable values valid for the entire call.
        let dc = unsafe { BeginPaint(hwnd, &mut paint) };
        if dc.is_null() {
            state.presentation_failed.set(true);
            return;
        }
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) } == 0 {
            state.presentation_failed.set(true);
        } else {
            let black = unsafe { GetStockObject(BLACK_BRUSH) };
            unsafe { FillRect(dc, &client, black) };
            if let Some(viewport) =
                integer_viewport(client.right - client.left, client.bottom - client.top)
            {
                // RefCell avoids a Rust aliasing violation if a Windows call
                // causes a nested paint while another raster access is active.
                if let Ok(raster) = state.raster.try_borrow() {
                    let info = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: WIDTH as i32,
                            biHeight: -(HEIGHT as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB,
                            biSizeImage: RGBA_BYTES as u32,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    unsafe { SetStretchBltMode(dc, COLORONCOLOR) };
                    // SAFETY: raster has exactly RGBA_BYTES top-down BGRX bytes.
                    // BITMAPINFO describes that fixed allocation; destination
                    // bounds come from the current client rectangle.
                    let lines = unsafe {
                        StretchDIBits(
                            dc,
                            viewport.x,
                            viewport.y,
                            viewport.width,
                            viewport.height,
                            0,
                            0,
                            WIDTH as i32,
                            HEIGHT as i32,
                            raster.as_ptr().cast::<c_void>(),
                            &info,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        )
                    };
                    if lines == 0 || lines == -1 {
                        state.presentation_failed.set(true);
                    } else {
                        state
                            .presentations
                            .set(state.presentations.get().saturating_add(1));
                    }
                }
            }
        }
        // SAFETY: pairs with the successful BeginPaint above, with the same
        // window and unchanged PAINTSTRUCT. No user callback runs in this path.
        unsafe { EndPaint(hwnd, &paint) };
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            if lparam == 0 {
                return 0;
            }
            // SAFETY: Windows supplies CREATESTRUCTW for WM_NCCREATE. The
            // creation parameter points to the stable Box owned by run().
            let creation = unsafe { &*(lparam as *const CREATESTRUCTW) };
            if creation.lpCreateParams.is_null() {
                return 0;
            }
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, creation.lpCreateParams as isize) };
            return 1;
        }
        // SAFETY: this slot was installed above and remains valid until the
        // window guard destroys the window, before the boxed state is dropped.
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
        if !pointer.is_null() {
            let state = unsafe { &*pointer };
            match message {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    if wparam == 0x1b {
                        state.closed.set(true);
                        unsafe { DestroyWindow(hwnd) };
                        return 0;
                    }
                    if button_for_virtual_key(wparam as u32) != 0 {
                        state.buttons.set(update_button_mask(
                            state.buttons.get(),
                            wparam as u32,
                            true,
                        ));
                        return 0;
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    if button_for_virtual_key(wparam as u32) != 0 {
                        state.buttons.set(update_button_mask(
                            state.buttons.get(),
                            wparam as u32,
                            false,
                        ));
                        return 0;
                    }
                }
                WM_KILLFOCUS => state.buttons.set(0),
                WM_PAINT => {
                    unsafe { paint(hwnd, state) };
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_CLOSE => {
                    state.closed.set(true);
                    unsafe { DestroyWindow(hwnd) };
                    return 0;
                }
                WM_DESTROY => state.closed.set(true),
                WM_NCDESTROY => {
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                }
                _ => {}
            }
        }
        // SAFETY: unhandled messages retain normal Windows processing, including
        // Alt+F4. This callback never invokes user code or unwinds across Win32.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    pub(super) fn run<F>(
        title: &str,
        frame_limit: u64,
        mut tick: F,
    ) -> Result<HostReport, HostError>
    where
        F: FnMut(FrameContext, &mut [u8]) -> Result<(), String>,
    {
        let state = Box::new(WindowState {
            buttons: Cell::new(0),
            closed: Cell::new(false),
            presentation_failed: Cell::new(false),
            presentations: Cell::new(0),
            raster: RefCell::new(framebuffer()?),
        });
        let mut rgba = framebuffer()?;
        // SAFETY: null requests this executable's module handle without changing
        // its reference count. It remains loaded throughout the host session.
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            return Err(win_error("GetModuleHandleW"));
        }
        let sequence = CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name: Vec<u16> = format!("A3VE_Rust_SourcePort_Host_{sequence}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
        let definition = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: cursor,
            hbrBackground: unsafe { GetStockObject(BLACK_BRUSH) },
            lpszClassName: name.as_ptr(),
            ..Default::default()
        };
        // SAFETY: class definition and its UTF-16 name remain live until the
        // class guard is dropped. window_proc has the required system ABI.
        if unsafe { RegisterClassW(&definition) } == 0 {
            return Err(win_error("RegisterClassW"));
        }
        let class = WindowClass { instance, name };
        let style = WS_OVERLAPPEDWINDOW & !(WS_THICKFRAME | WS_MAXIMIZEBOX);
        let mut dimensions = RECT {
            left: 0,
            top: 0,
            right: WIDTH as i32 * WINDOW_SCALE as i32,
            bottom: HEIGHT as i32 * WINDOW_SCALE as i32,
        };
        if unsafe { AdjustWindowRect(&mut dimensions, style, 0) } == 0 {
            return Err(win_error("AdjustWindowRect"));
        }
        let caption: Vec<u16> = format!("Rust source-port milestone (incomplete) | {title}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: all UTF-16 strings are NUL-terminated. The state Box stays at
        // one address, outlives the window and is accessed only on this thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.name.as_ptr(),
                caption.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                dimensions.right - dimensions.left,
                dimensions.bottom - dimensions.top,
                null_mut(),
                null_mut(),
                instance,
                (&*state as *const WindowState).cast_mut().cast::<c_void>(),
            )
        };
        if hwnd.is_null() {
            return Err(win_error("CreateWindowExW"));
        }
        let window = Window(hwnd);
        unsafe { ShowWindow(window.0, SW_SHOW) };
        // A hidden parent console may supply STARTF_USESHOWWINDOW. Windows
        // applies that value to the first ShowWindow call; explicitly expose
        // this requested interactive framebuffer while keeping the console hidden.
        if unsafe { IsWindowVisible(window.0) } == 0 {
            unsafe { ShowWindow(window.0, SW_SHOW) };
        }
        let mut frames = 0;
        let mut deadline = Instant::now();
        while frames < frame_limit && !state.closed.get() {
            for _ in 0..MESSAGES_PER_FRAME {
                let mut message = MSG::default();
                if unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } == 0 {
                    break;
                }
                if message.message == WM_QUIT {
                    state.closed.set(true);
                    break;
                }
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if state.closed.get() {
                break;
            }
            tick(
                FrameContext {
                    frame_index: frames,
                    button_mask: state.buttons.get(),
                },
                &mut rgba,
            )
            .map_err(|message| HostError::Callback {
                frame_index: frames,
                message,
            })?;
            rgba_to_bgrx(&rgba, &mut state.raster.borrow_mut())?;
            // The raster borrow ends before synchronous Windows callbacks.
            unsafe {
                InvalidateRect(window.0, null(), 0);
                UpdateWindow(window.0);
            }
            if state.presentation_failed.get() {
                return Err(HostError::Presentation);
            }
            frames += 1;
            if frames < frame_limit {
                deadline += FRAME_PERIOD;
                let now = Instant::now();
                if deadline > now {
                    std::thread::sleep(deadline - now);
                } else {
                    // Slow callbacks never synthesize skipped game frames.
                    deadline = now;
                }
            }
        }
        Ok(HostReport {
            frames,
            presentations: state.presentations.get(),
            stopped_by: if state.closed.get() {
                StopReason::WindowClosed
            } else {
                StopReason::FrameBudget
            },
            width: WIDTH,
            height: HEIGHT,
            initial_integer_scale: WINDOW_SCALE,
        })
        // Drop order: Window, WindowClass, then WindowState. No callback retains
        // the state pointer after WM_NCDESTROY clears the user-data slot.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_order_and_simultaneous_release_match_gba_bits() {
        let keys = [0x5a, 0x58, 0x08, 0x0d, 0x27, 0x25, 0x26, 0x28, 0x53, 0x41];
        let mut mask = 0;
        for (bit, key) in keys.into_iter().enumerate() {
            assert_eq!(button_for_virtual_key(key), 1 << bit);
            mask = update_button_mask(mask, key, true);
            assert_eq!(update_button_mask(mask, key, true), mask);
        }
        assert_eq!(mask, 0x3ff);
        mask = update_button_mask(mask, 0x5a, false);
        assert_eq!(mask, 0x3fe);
        assert_eq!(update_button_mask(mask, 0x1b, true), mask);
        assert_eq!(button_for_virtual_key(u32::MAX), 0);
    }

    #[test]
    fn channel_order_and_straight_alpha_over_black_are_exact() {
        let source = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 99, 88, 77, 0,
        ];
        let mut raster = [0xff; 16];
        rgba_to_bgrx(&source, &mut raster).unwrap();
        assert_eq!(
            raster,
            [0, 0, 255, 0, 0, 255, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0]
        );
        assert!(matches!(
            rgba_to_bgrx(&[0; 3], &mut [0; 3]),
            Err(HostError::BufferLength { .. })
        ));
        assert!(matches!(
            rgba_to_bgrx(&[0; 4], &mut [0; 8]),
            Err(HostError::BufferLength { .. })
        ));
    }

    #[test]
    fn integer_scaling_centers_without_fractional_resampling() {
        assert_eq!(
            integer_viewport(720, 480),
            Some(IntegerViewport {
                x: 0,
                y: 0,
                width: 720,
                height: 480,
                scale: 3
            })
        );
        assert_eq!(
            integer_viewport(1000, 700),
            Some(IntegerViewport {
                x: 20,
                y: 30,
                width: 960,
                height: 640,
                scale: 4
            })
        );
        assert_eq!(integer_viewport(239, 160), None);
        assert_eq!(integer_viewport(240, -1), None);
        assert_eq!(
            integer_viewport(i32::MAX, i32::MAX).unwrap().width % WIDTH as i32,
            0
        );
    }

    #[test]
    fn invalid_launch_arguments_never_create_a_window() {
        let tick = |_: FrameContext, _: &mut [u8]| -> Result<(), String> { panic!("must not run") };
        assert_eq!(run("A3VE", 0, tick), Err(HostError::InvalidFrameBudget));
        assert_eq!(run("", 1, tick), Err(HostError::InvalidTitle));
        assert_eq!(run("nul\0title", 1, tick), Err(HostError::InvalidTitle));
        assert_eq!(run(&"x".repeat(257), 1, tick), Err(HostError::InvalidTitle));
    }
}
