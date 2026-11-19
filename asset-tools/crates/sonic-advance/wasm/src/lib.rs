//! Narrow integer-only WebAssembly ABI. Export attributes are the only unsafe
//! annotations; all readers, simulation and memory indexing use safe Rust.
#![deny(unsafe_code)]
use sonic_advance_assets::{
    binary, player::Input, runtime::NativeRun, title::Title, Error, Result,
};
use std::cell::RefCell;

#[derive(Default)]
struct Host {
    input: Vec<u8>,
    runtime: Option<NativeRun<'static>>,
    pixels: Vec<u8>,
    rgb: Vec<u32>,
    title: Option<Title>,
    title_pixels: Vec<u8>,
    snapshot: Vec<u8>,
    error: Vec<u8>,
}
thread_local! {static HOST:RefCell<Host>=RefCell::new(Host::default());}

// All accepted sessions use the same fingerprint. Keep one immutable source
// for the lifetime of this WASM instance; invalid or repeated loads never grow it.
static ROM: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
fn source() -> Result<&'static [u8]> {
    ROM.get()
        .map(Vec::as_slice)
        .ok_or_else(|| Error("no game loaded".into()))
}
fn redraw(host: &mut Host) -> Result<()> {
    let game = host
        .runtime
        .as_ref()
        .ok_or_else(|| Error("no game loaded".into()))?;
    host.rgb.resize(240 * 160, 0);
    game.render(&mut host.rgb)?;
    host.pixels.clear();
    host.pixels.extend(
        host.rgb
            .iter()
            .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8, 255]),
    );
    Ok(())
}
fn run(action: impl FnOnce(&mut Host) -> Result<()>) -> i32 {
    HOST.with(|cell| {
        let mut host = cell.borrow_mut();
        match action(&mut host) {
            Ok(()) => {
                host.error.clear();
                0
            }
            Err(error) => {
                host.error = error.to_string().into_bytes();
                -1
            }
        }
    })
}

// The host writes exactly this bounded allocation while no Rust function is
// active. It must reacquire memory views after every exported call; load takes
// ownership of the buffer and invalidates the former input allocation contract.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_allocate_rom(length: u32) -> u32 {
    let status = run(|host| {
        if length as usize != binary::ROM_LEN {
            return Err(Error("expected the exact 8 MiB ASOE source".into()));
        }
        host.input = Vec::new();
        host.input
            .try_reserve_exact(binary::ROM_LEN)
            .map_err(|_| Error("ROM allocation failed".into()))?;
        host.input.resize(binary::ROM_LEN, 0);
        Ok(())
    });
    if status < 0 {
        return 0;
    }
    HOST.with(|cell| cell.borrow_mut().input.as_mut_ptr() as usize as u32)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_load(length: u32) -> i32 {
    load(length, false)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_load_title(length: u32) -> i32 {
    load(length, true)
}

fn load(length: u32, at_title: bool) -> i32 {
    run(|host| {
        if length as usize != binary::ROM_LEN || host.input.len() != binary::ROM_LEN {
            return Err(Error("ROM input allocation/length mismatch".into()));
        }
        let rom = std::mem::take(&mut host.input);
        binary::verify_asoe(&rom)?;
        let bytes = ROM.get_or_init(|| rom);
        let replacement = NativeRun::load(bytes)?;
        let title = at_title.then(|| Title::load_asoe(bytes)).transpose()?;
        let pixels = title
            .as_ref()
            .map(Title::render)
            .transpose()?
            .unwrap_or_default();
        host.title = title;
        host.title_pixels = pixels;
        host.runtime = Some(replacement);
        host.snapshot.clear();
        redraw(host)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_step(mask: u32, frames: u32) -> i32 {
    run(|host| {
        if let Some(title) = host.title.as_mut() {
            let bits =
                u16::try_from(mask).map_err(|_| Error("unsupported title input bits".into()))?;
            title.step(bits, frames)?;
            return title.render_into(&mut host.title_pixels);
        }
        if mask & !0xb1 != 0 || !(1..=4).contains(&frames) {
            return Err(Error("invalid input bits or frame count".into()));
        }
        let game = host
            .runtime
            .as_mut()
            .ok_or_else(|| Error("no game loaded".into()))?;
        for _ in 0..frames {
            game.step_controls(
                Input {
                    left: mask & 32 != 0,
                    right: mask & 16 != 0,
                    jump: mask & 1 != 0,
                },
                mask & 128 != 0,
            )?;
        }
        redraw(host)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_select_act(index: u32) -> i32 {
    run(|host| {
        host.runtime = Some(NativeRun::load_stage(source()?, index as usize)?);
        redraw(host)?;
        host.title = None;
        host.title_pixels.clear();
        Ok(())
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_open_title() -> i32 {
    run(open_title)
}

fn open_title(host: &mut Host) -> Result<()> {
    let title = Title::load_asoe(source()?)?;
    let pixels = title.render()?;
    host.title = Some(title);
    host.title_pixels = pixels;
    Ok(())
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_reset() -> i32 {
    run(|host| {
        if host.title.is_some() {
            return open_title(host);
        }
        let index = host
            .runtime
            .as_ref()
            .ok_or_else(|| Error("no game loaded".into()))?
            .snapshot()
            .stage_index;
        host.runtime = Some(NativeRun::load_stage(source()?, index)?);
        redraw(host)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_frame_ptr() -> u32 {
    HOST.with(|cell| {
        let host = cell.borrow();
        if host.title.is_some() {
            host.title_pixels.as_ptr() as usize as u32
        } else if host.pixels.is_empty() {
            0
        } else {
            host.pixels.as_ptr() as usize as u32
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_frame_len() -> u32 {
    HOST.with(|cell| {
        let host = cell.borrow();
        if host.title.is_some() {
            host.title_pixels.len() as u32
        } else {
            host.pixels.len() as u32
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_snapshot_ptr() -> u32 {
    let status = run(|host| {
        host.snapshot.clear();
        if let Some(title) = &host.title {
            host.snapshot =
                serde_json::to_vec(&serde_json::json!({"scene":"title","title":title.snapshot()}))
                    .map_err(|e| Error(e.to_string()))?;
            return Ok(());
        }
        let state = host
            .runtime
            .as_ref()
            .ok_or_else(|| Error("no game loaded".into()))?
            .snapshot();
        host.snapshot = serde_json::to_vec(&state).map_err(|e| Error(e.to_string()))?;
        Ok(())
    });
    if status < 0 {
        return 0;
    }
    HOST.with(|cell| cell.borrow().snapshot.as_ptr() as usize as u32)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_snapshot_len() -> u32 {
    HOST.with(|cell| cell.borrow().snapshot.len() as u32)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_error_ptr() -> u32 {
    HOST.with(|cell| cell.borrow().error.as_ptr() as usize as u32)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_error_len() -> u32 {
    HOST.with(|cell| cell.borrow().error.len() as u32)
}
