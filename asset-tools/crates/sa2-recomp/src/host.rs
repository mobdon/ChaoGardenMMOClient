//! Headless host: load main.dol into memory, wire the recompiled dispatch and
//! REL modules, stub the hardware, call the entry point and run until the
//! first panic (or the call budget). Used by the generated `sa2-host` crate
//! (`sa2-recomp host`), which links the generated code into this.

use crate::dol::{self, Dol};
use crate::rt::{self, Cpu, Func, Mem, Runtime, TRACE};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// Hardware registers touched so far: address -> (reads, writes).
static IO: Mutex<BTreeMap<u32, (u64, u64)>> = Mutex::new(BTreeMap::new());

fn io_touch(a: u32, write: bool) {
    let mut io = IO.lock().unwrap_or_else(|e| e.into_inner());
    let first = !io.contains_key(&a);
    let e = io.entry(a).or_default();
    if write {
        e.1 += 1;
    } else {
        e.0 += 1;
    }
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

pub struct Report {
    pub calls: u64,
    pub panic: Option<String>,
    /// Most recent call targets, oldest first.
    pub trace: Vec<u32>,
}

/// Run main.dol from its entry point. `budget` caps the number of calls.
pub fn run(dol_bytes: &[u8], dispatch: fn(u32) -> Option<Func>, register: fn(&mut Runtime), budget: u64) -> Result<Report, String> {
    let dol = Dol::parse(dol_bytes)?;
    let entry = dol.entry;
    // Recompiled code nests a Rust frame per guest call; give it room.
    let worker = std::thread::Builder::new().stack_size(1 << 30).spawn(move || {
        let mut m = Mem { io_read: |a, _| { io_touch(a, false); 0 }, io_write: |a, _, _| io_touch(a, true), ..Mem::default() };
        dol.load(&mut m);
        boot_globals(&mut m);
        let mut c = Cpu::default();
        c.rt.generated = dispatch;
        c.rt.budget = budget;
        register(&mut c.rt);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt::call(&mut c, &mut m, entry)));
        let panic = r.err().map(|p| {
            p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
        });
        let n = c.rt.calls;
        let trace = (n.saturating_sub(TRACE as u64)..n).map(|i| c.rt.trace[i as usize % TRACE]).collect();
        Report { calls: n, panic, trace }
    });
    worker.map_err(|e| e.to_string())?.join().map_err(|_| "host thread died".to_string())
}

/// Print a run's report, naming addresses from the decomp's symbols.
pub fn print(r: &Report, symbols: &str) {
    let names: BTreeMap<u32, String> =
        dol::parse_symbols(symbols).into_iter().filter(|s| s.function).map(|s| (s.addr, s.name)).collect();
    let name = |a: u32| match names.range(..=a).next_back() {
        Some((s, n)) if *s == a => n.clone(),
        Some((s, n)) => format!("{n}+{:#x}", a - s),
        None => "?".into(),
    };
    println!("calls {}", r.calls);
    println!("last calls (oldest first):");
    for a in &r.trace {
        println!("  {a:#010x} {}", name(*a));
    }
    match &r.panic {
        Some(p) => println!("stopped: {p}"),
        None => println!("entry point returned"),
    }
    let io = IO.lock().unwrap_or_else(|e| e.into_inner());
    let regions: BTreeSet<u32> = io.keys().map(|a| a & !0xFFF).collect();
    println!("hardware addresses touched: {} in {} 4K regions {:x?}", io.len(), regions.len(), regions);
}
