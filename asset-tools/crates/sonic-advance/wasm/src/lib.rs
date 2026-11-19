//! Narrow integer-only WebAssembly ABI. Export attributes are the only unsafe
//! annotations; all readers, simulation and memory indexing use safe Rust.
#![deny(unsafe_code)]
mod garden;
use sonic_advance_assets::{
    binary,
    character_select::{screen::Screen, Character, Selection},
    player::Input,
    runtime::NativeRun,
    title::{
        menu::{Destination, MainMenu},
        Title, TitleEvent,
    },
    Error, Result,
};
use std::cell::RefCell;

#[derive(Default)]
struct Host {
    input: Vec<u8>,
    flash_copy: Vec<u8>,
    runtime: Option<NativeRun<'static>>,
    pixels: Vec<u8>,
    rgb: Vec<u32>,
    title: Option<Title>,
    menu: Option<MainMenu>,
    garden: Option<garden::Garden>,
    characters: Option<Screen<'static>>,
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
fn step_stage(game: &mut NativeRun<'_>, mask: u32) -> Result<()> {
    game.step_controls(
        Input {
            left: mask & 32 != 0,
            right: mask & 16 != 0,
            jump: mask & 1 != 0,
        },
        mask & 128 != 0,
    )
    .map(|_| ())
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
        host.garden = None;
        host.characters = None;
        host.menu = None;
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
            if bits & !0x03ff != 0 || !(1..=4).contains(&frames) {
                return Err(Error("invalid title/menu input or frame count".into()));
            }
            for tick in 0..frames {
                if let Some(garden) = &mut host.garden {
                    if !garden.step(source()?, bits)? {
                        host.garden = None;
                        host.menu = Some(MainMenu::from_title(source()?, title, bits)?);
                    }
                } else if let Some(screen) = &mut host.characters {
                    screen.step(bits)?;
                    if screen.confirmation_finished()
                        && screen.selection()
                            == Some(Selection::Confirm {
                                character: Character::Sonic,
                                cpu_tails_sequence: false,
                            })
                    {
                        // This adapter starts a fresh local session with no loaded save.
                        let mut game = NativeRun::load_stage(source()?, 0)?;
                        for _ in tick + 1..frames {
                            step_stage(&mut game, mask)?;
                        }
                        host.runtime = Some(game);
                        host.characters = None;
                        host.menu = None;
                        host.title = None;
                        host.title_pixels.clear();
                        return redraw(host);
                    }
                    if screen.selection() == Some(Selection::Back) {
                        host.characters = None;
                        host.menu = Some(MainMenu::from_title(source()?, title, bits)?);
                    }
                } else if let Some(menu) = &mut host.menu {
                    menu.step(bits, 1)?;
                    if menu.snapshot().destination == Some(Destination::CharacterSelect) {
                        host.characters = Some(Screen::load(source()?, bits)?);
                    } else if menu.snapshot().destination == Some(Destination::TinyGarden) {
                        host.garden = Some(garden::Garden::load(source()?, bits)?);
                    }
                } else if title.step(bits, 1)? == TitleEvent::MainMenuRequested {
                    host.menu = Some(MainMenu::from_title(source()?, title, bits)?);
                }
            }
            if let Some(garden) = &host.garden {
                host.title_pixels.clear();
                host.title_pixels.extend_from_slice(garden.pixels());
                return Ok(());
            }
            if let Some(screen) = &host.characters {
                return screen.render_into(&mut host.title_pixels);
            }
            if let Some(menu) = &host.menu {
                return menu.render_into(&mut host.title_pixels);
            }
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
            step_stage(game, mask)?;
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
        host.garden = None;
        host.characters = None;
        host.menu = None;
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
    host.garden = None;
    host.characters = None;
    host.menu = None;
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
        if let Some(garden) = &host.garden {
            host.snapshot =
                serde_json::to_vec(&garden.snapshot()).map_err(|e| Error(e.to_string()))?;
            return Ok(());
        }
        if let Some(screen) = &host.characters {
            host.snapshot=serde_json::to_vec(&serde_json::json!({"scene":"characters","frame":screen.frame(),"character":format!("{:?}",screen.character()),"confirmed":screen.confirmation_finished()})).map_err(|e|Error(e.to_string()))?;
            return Ok(());
        }
        if let Some(menu) = &host.menu {
            host.snapshot =
                serde_json::to_vec(&serde_json::json!({"scene":"mainMenu","menu":menu.snapshot()}))
                    .map_err(|e| Error(e.to_string()))?;
            return Ok(());
        }
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

// A browser file input supplies a copy. No API here writes to the selected file.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_flash_buffer(length: u32) -> u32 {
    if run(|host| {
        source()?;
        if length as usize != sonic_advance_assets::flash::FLASH_BYTES {
            return Err(Error("expected a 64 KiB ASOE save".into()));
        }
        host.flash_copy = vec![0; length as usize];
        Ok(())
    }) != 0
    {
        return 0;
    }
    HOST.with(|cell| cell.borrow_mut().flash_copy.as_mut_ptr() as usize as u32)
}
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn asoe_garden_save_copy() -> u32 {
    if run(|host| {
        let flash = &host.flash_copy;
        let scan = sonic_advance_assets::flash::inspect(flash)?;
        let sector = scan
            .selected_game_sector
            .ok_or_else(|| Error("no valid main game record for launch score".into()))?;
        let offset = sector * sonic_advance_assets::flash::SECTOR_BYTES + 0x428;
        let score = u32::from_le_bytes(flash[offset..offset + 4].try_into().unwrap());
        let game = sonic_advance_assets::garden_cartridge::CartridgeGarden::load(flash, score)?;
        let out = game.save_copy();
        if sonic_advance_assets::flash::inspect(&out)?.selected_tiny_sector
            != Some(scan.next_tiny_sector)
        {
            return Err(Error(
                "new generation would not be selected on reload".into(),
            ));
        }
        host.flash_copy = out;
        Ok(())
    }) != 0
    {
        return 0;
    }
    HOST.with(|cell| cell.borrow().flash_copy.as_ptr() as usize as u32)
}
