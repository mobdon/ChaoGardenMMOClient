//! The disc from the player's Steam files ([`sa2_runtime::dvd`]). Reads
//! finish at once: async reads mark the command block done and run their
//! callback before returning.
//! shortcut: DVDFastOpen has no symbol in symbols.txt and DVDChangeDir is ignored (paths resolve from the root); add both if the boot uses them.
use crate::{cstr, guest_call};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_runtime::dvd::{Dvd, GameFiles};
use std::cell::RefCell;

pub const DVD_CONVERT_PATH_TO_ENTRYNUM: u32 = 0x800E_ECFC;
pub const DVD_OPEN: u32 = 0x800E_EFF0;
pub const DVD_CLOSE: u32 = 0x800E_F0B8;
pub const DVD_CHANGE_DIR: u32 = 0x800E_F300;
pub const DVD_READ_ASYNC_PRIO: u32 = 0x800E_F360;
pub const DVD_READ_PRIO: u32 = 0x800E_F450;
pub const DVD_INIT: u32 = 0x800E_F5F8;
pub const DVD_GET_COMMAND_BLOCK_STATUS: u32 = 0x800F_135C;
pub const DVD_GET_DRIVE_STATUS: u32 = 0x800F_13A8;
pub const DVD_CANCEL: u32 = 0x800F_16D4;
pub const DVD_CHECK_DISK: u32 = 0x800F_17AC;

// DVDFileInfo: the command block (state at 0xC, transferredSize at 0x20),
// then startAddr (here the entry number) and length.
const CB_STATE: u32 = 0x0C;
const CB_TRANSFERRED: u32 = 0x20;
const FI_START: u32 = 0x30;
const FI_LENGTH: u32 = 0x34;

thread_local! {
    static DVD: RefCell<Option<Dvd<Box<dyn GameFiles>>>> = const { RefCell::new(None) };
}

/// The files the disc is served from.
pub fn set_files(files: Box<dyn GameFiles>) {
    DVD.set(Some(Dvd::new(files)));
}

fn entry(m: &Mem, path: u32) -> Option<i32> {
    let p = cstr(m, path);
    DVD.with_borrow_mut(|d| d.as_mut()?.entry(&p))
}

fn convert_path(c: &mut Cpu, m: &mut Mem) {
    c.r[3] = entry(m, c.r[3]).unwrap_or(-1) as u32;
}

fn open(c: &mut Cpu, m: &mut Mem) {
    let fi = c.r[4];
    let Some(e) = entry(m, c.r[3]) else {
        c.r[3] = 0;
        return;
    };
    let len = DVD
        .with_borrow(|d| d.as_ref().and_then(|d| d.length(e)))
        .unwrap_or(0);
    for o in (0..0x30).step_by(4) {
        m.w32(fi + o, 0);
    }
    m.w32(fi + FI_START, e as u32);
    m.w32(fi + FI_LENGTH, len.min(u64::from(u32::MAX)) as u32);
    c.r[3] = 1;
}

/// Read `len` bytes at `off` of the file behind `fi` to `addr`; bytes read.
fn read_into(m: &mut Mem, fi: u32, addr: u32, len: u32, off: u32) -> Option<u32> {
    let e = m.r32(fi + FI_START) as i32;
    let mut buf = vec![0u8; len as usize];
    let n = DVD.with_borrow(|d| d.as_ref()?.read(e, u64::from(off), &mut buf))?;
    m.load(addr, &buf[..n]);
    Some(n as u32)
}

fn read_prio(c: &mut Cpu, m: &mut Mem) {
    let [fi, addr, len, off] = [c.r[3], c.r[4], c.r[5], c.r[6]];
    c.r[3] = read_into(m, fi, addr, len, off).unwrap_or(u32::MAX);
}

fn read_async_prio(c: &mut Cpu, m: &mut Mem) {
    let [fi, addr, len, off, cb] = [c.r[3], c.r[4], c.r[5], c.r[6], c.r[7]];
    let result = read_into(m, fi, addr, len, off);
    m.w32(fi + CB_STATE, if result.is_some() { 0 } else { u32::MAX });
    m.w32(fi + CB_TRANSFERRED, result.unwrap_or(0));
    if cb != 0 {
        guest_call(c, m, cb, &[result.unwrap_or(u32::MAX), fi]);
    }
    c.r[3] = 1;
}

fn one(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 1;
}
fn zero(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 0;
}
fn nothing(_c: &mut Cpu, _m: &mut Mem) {}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(DVD_INIT, nothing);
    c.rt.set_override(DVD_CONVERT_PATH_TO_ENTRYNUM, convert_path);
    c.rt.set_override(DVD_OPEN, open);
    c.rt.set_override(DVD_READ_PRIO, read_prio);
    c.rt.set_override(DVD_READ_ASYNC_PRIO, read_async_prio);
    for a in [DVD_CLOSE, DVD_CHANGE_DIR, DVD_CHECK_DISK] {
        c.rt.set_override(a, one);
    }
    for a in [
        DVD_GET_COMMAND_BLOCK_STATUS,
        DVD_GET_DRIVE_STATUS,
        DVD_CANCEL,
    ] {
        c.rt.set_override(a, zero);
    }
}
