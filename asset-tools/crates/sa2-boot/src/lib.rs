//! Host hooks for the recompiled game (`sa2-recomp`). Each module has an
//! `install(&mut Cpu, &mut Mem)` that registers its `Runtime::set_override`
//! ports (GSNE8P addresses from the decomp's symbols.txt) and claims its
//! hardware addresses through [`io`].
//!
//! Hooks are plain `fn` pointers, so each module keeps its state in a
//! thread-local: install on the thread that runs the game.
#![forbid(unsafe_code)]

pub mod audio;
pub mod card;
pub mod card_pack;
pub mod chao_cache;
pub mod chao_player;
pub mod chao_retarget;
pub mod clock;
pub mod dvd;
#[cfg(not(target_os = "wasi"))]
pub mod gamepad;
pub mod gx;
pub mod io;
#[cfg(not(target_os = "wasi"))]
pub mod mmo;
pub mod pad;
pub mod prof;
pub mod serve;
#[cfg(not(target_os = "wasi"))]
pub mod snap;
pub mod start_stage;
pub mod karate;
pub mod race_probe;
pub mod steam_sound;
pub mod vi;
pub mod walker;
pub mod wide;
#[cfg(not(target_os = "wasi"))]
pub mod window;

use sa2_recomp::rt::{call, Cpu, Mem};

/// Every hook. `files` is the player's game folder (or what the browser read).
pub fn install_all(c: &mut Cpu, m: &mut Mem, files: Box<dyn sa2_runtime::dvd::GameFiles>) {
    dvd::set_files(files);
    dvd::install(c, m);
    gx::install(c, m);
    vi::install(c, m);
    pad::install(c, m);
    audio::install(c, m);
    steam_sound::install(c, m);
    clock::install(c, m);
    card::install(c, m);
    wide::install(c, m);
    walker::install(c, m);
    chao_player::install(c, m);
    start_stage::install(c, m);
    race_probe::install(c, m);
}

/// Read a NUL-terminated guest string (at most 255 bytes).
pub(crate) fn cstr(m: &Mem, mut a: u32) -> String {
    let mut s = Vec::new();
    while s.len() < 255 {
        let b = m.r8(a);
        if b == 0 {
            break;
        }
        s.push(b);
        a = a.wrapping_add(1);
    }
    String::from_utf8_lossy(&s).into_owned()
}

/// Call a guest function (a callback) from a hook with `args` in r3.., keeping
/// the hook's caller-visible registers. Returns the callee's r3.
pub(crate) fn guest_call(c: &mut Cpu, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
    let saved = (c.r, c.lr, c.ctr, c.cr);
    for (i, a) in args.iter().enumerate() {
        c.r[3 + i] = *a;
    }
    call(c, m, f);
    let r3 = c.r[3];
    (c.r, c.lr, c.ctr, c.cr) = saved;
    r3
}

/// Run the recompiled original at `addr` (for hooks that wrap it).
pub(crate) fn original(c: &mut Cpu, m: &mut Mem, addr: u32) {
    if let Some(f) = (c.rt.generated)(addr) {
        f(c, m);
    }
}
