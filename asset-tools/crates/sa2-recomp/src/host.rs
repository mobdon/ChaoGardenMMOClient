//! Headless host: load main.dol into memory, wire the recompiled dispatch and
//! REL modules, stub the hardware, call the entry point and run until the
//! first panic, the call budget or the time limit. Used by the generated
//! `sa2-host` crate (`sa2-recomp host`), which links the generated code in.

use crate::dol::{self, Dol};
use crate::rt::{self, Cpu, Func, Mem, Runtime, CALLS};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;
use std::time::Duration;

/// Hardware registers touched so far: address -> (reads, writes).
static IO: Mutex<BTreeMap<u32, (u64, u64)>> = Mutex::new(BTreeMap::new());
/// Most recent hardware access, for a stuck report.
static LAST_IO: Mutex<Option<(u32, bool)>> = Mutex::new(None);

fn io_touch(a: u32, write: bool) {
    let mut io = IO.lock().unwrap_or_else(|e| e.into_inner());
    let first = !io.contains_key(&a);
    let e = io.entry(a).or_default();
    if write {
        e.1 += 1;
    } else {
        e.0 += 1;
    }
    *LAST_IO.lock().unwrap_or_else(|e| e.into_inner()) = Some((a, write));
    if first {
        eprintln!("io: first {} {a:#010x}", if write { "write" } else { "read" });
    }
}

/// Boot globals the IPL and apploader leave in low memory before the DOL runs
/// (GameCube boot conventions: magic, memory size, console type, clocks).
fn boot_globals(m: &mut Mem) {
    m.w32(0x8000_0020, 0x0D15_EA5E);
    m.w32(0x8000_0024, 1);
    m.w32(0x8000_0028, rt::MEM1);
    m.w32(0x8000_002C, 0x1000_0006); // retail console
    m.w32(0x8000_00CC, 0); // NTSC
    m.w32(0x8000_00F0, rt::MEM1);
    m.w32(0x8000_00F8, 162_000_000); // bus clock
    m.w32(0x8000_00FC, 486_000_000); // CPU clock
}

/// Run main.dol from its entry point and print where it stopped: a panic, the
/// call budget, or still running after `secs` (then the process exits).
pub fn run(dol_bytes: &[u8], symbols: &str, dispatch: fn(u32) -> Option<Func>, register: fn(&mut Runtime), budget: u64, secs: u64) -> Result<(), String> {
    let dol = Dol::parse(dol_bytes)?;
    let entry = dol.entry;
    let (tx, rx) = std::sync::mpsc::channel();
    // Recompiled code nests a Rust frame per guest call; give it room.
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            let mut m = Mem { io_read: |a, _| { io_touch(a, false); 0 }, io_write: |a, _, _| io_touch(a, true), ..Mem::default() };
            dol.load(&mut m);
            boot_globals(&mut m);
            let mut c = Cpu::default();
            c.rt.generated = dispatch;
            c.rt.budget = budget;
            register(&mut c.rt);
            crate::os::install(&mut c);
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt::call(&mut c, &mut m, entry)));
            let msg = match r {
                Ok(()) => "entry point returned".to_string(),
                Err(p) => p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(),
            };
            let _ = tx.send(msg);
        })
        .map_err(|e| e.to_string())?;
    let stopped = match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(msg) => format!("stopped: {msg}"),
        Err(_) => {
            let io = *LAST_IO.lock().unwrap_or_else(|e| e.into_inner());
            format!("still running after {secs}s (stuck in a loop without calls?); last hardware access {io:x?}")
        }
    };
    print(symbols, &stopped);
    Ok(())
}

fn print(symbols: &str, stopped: &str) {
    let names: BTreeMap<u32, String> =
        dol::parse_symbols(symbols).into_iter().filter(|s| s.function).map(|s| (s.addr, s.name)).collect();
    let name = |a: u32| match names.range(..=a).next_back() {
        Some((s, n)) if *s == a => n.clone(),
        Some((s, n)) => format!("{n}+{:#x}", a - s),
        None => "?".into(),
    };
    println!("calls {}", CALLS.load(Relaxed));
    println!("last calls (oldest first):");
    for a in rt::trace() {
        println!("  {a:#010x} {}", name(a));
    }
    println!("{stopped}");
    let io = IO.lock().unwrap_or_else(|e| e.into_inner());
    let regions: BTreeSet<u32> = io.keys().map(|a| a & !0xFFF).collect();
    println!("hardware addresses touched: {} in {} 4K regions {:x?}", io.len(), regions.len(), regions);
}
