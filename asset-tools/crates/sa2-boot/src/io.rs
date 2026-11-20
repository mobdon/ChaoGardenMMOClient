//! `Mem.io_read`/`io_write` take one function each; this routes address
//! ranges to the modules that claimed them and the rest to whatever the
//! host had installed before.
use sa2_recomp::rt::Mem;
use std::cell::RefCell;

pub type Read = fn(u32, u32) -> u32;
pub type Write = fn(u32, u32, u32);

struct Routes {
    reads: Vec<(u32, u32, Read)>,
    writes: Vec<(u32, u32, Write)>,
    read_rest: Read,
    write_rest: Write,
}

thread_local! {
    static ROUTES: RefCell<Routes> = RefCell::new(Routes {
        reads: Vec::new(),
        writes: Vec::new(),
        read_rest: |_, _| 0,
        write_rest: |_, _, _| {},
    });
}

fn dispatch_read(a: u32, size: u32) -> u32 {
    let f = ROUTES.with_borrow(|r| {
        r.reads
            .iter()
            .find(|(s, e, _)| (*s..*e).contains(&a))
            .map_or(r.read_rest, |x| x.2)
    });
    f(a, size)
}

fn dispatch_write(a: u32, size: u32, v: u32) {
    let f = ROUTES.with_borrow(|r| {
        r.writes
            .iter()
            .find(|(s, e, _)| (*s..*e).contains(&a))
            .map_or(r.write_rest, |x| x.2)
    });
    f(a, size, v)
}

/// Put the router in front of `m`'s hooks (once per `Mem`).
fn take_over(m: &mut Mem) {
    ROUTES.with_borrow_mut(|r| {
        if !std::ptr::fn_addr_eq(m.io_read, dispatch_read as Read) {
            r.read_rest = m.io_read;
            m.io_read = dispatch_read;
        }
        if !std::ptr::fn_addr_eq(m.io_write, dispatch_write as Write) {
            r.write_rest = m.io_write;
            m.io_write = dispatch_write;
        }
    });
}

/// Reads of `start..end` go to `f`.
pub fn route_read(m: &mut Mem, start: u32, end: u32, f: Read) {
    take_over(m);
    ROUTES.with_borrow_mut(|r| r.reads.insert(0, (start, end, f)));
}

/// Writes to `start..end` go to `f`.
pub fn route_write(m: &mut Mem, start: u32, end: u32, f: Write) {
    take_over(m);
    ROUTES.with_borrow_mut(|r| r.writes.insert(0, (start, end, f)));
}

/// The bytes of a `size`-byte store of `v`, as they hit the bus.
pub fn bytes(size: u32, v: u32) -> Vec<u8> {
    match size {
        1 => vec![v as u8],
        2 => (v as u16).to_be_bytes().to_vec(),
        _ => v.to_be_bytes().to_vec(),
    }
}
