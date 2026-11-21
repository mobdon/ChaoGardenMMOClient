//! The memory card in slot A, at the level of the CARD library: each card
//! file is a host file in the folder SA2_MEMCARD names (so the game's own
//! saving and loading work and progress stays between runs). Without
//! SA2_MEMCARD the slot is empty, as before.
//!
//! Each file is `<name in hex>.dat` (its bytes) beside `<name in hex>.stat`
//! (what CARDSetStatus stored: game, company, banner and icon formats,
//! addresses, time). Writes reach the disk before the call returns.
//! Asynchronous calls report on the next frame, as the card interrupt would.
use crate::{cstr, guest_call};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};
#[cfg(not(target_os = "wasi"))]
use std::path::PathBuf;

pub const CARD_INIT: u32 = 0x8016_D4DC;
pub const CARD_GET_RESULT_CODE: u32 = 0x8016_D6CC;
pub const CARD_FREE_BLOCKS: u32 = 0x8016_D6FC;
pub const CARD_GET_MEM_SIZE: u32 = 0x8016_D84C;
pub const CARD_GET_XFERRED_BYTES: u32 = 0x8016_EE4C;
pub const CARD_CHECK_EX_ASYNC: u32 = 0x8016_FE50;
pub const CARD_CHECK: u32 = 0x8017_03E0;
pub const CARD_PROBE_EX: u32 = 0x8017_0434;
pub const CARD_MOUNT_ASYNC: u32 = 0x8017_0B28;
pub const CARD_MOUNT: u32 = 0x8017_0CC8;
pub const CARD_UNMOUNT: u32 = 0x8017_0DAC;
pub const CARD_FORMAT: u32 = 0x8017_15F4;
pub const CARD_OPEN: u32 = 0x8017_18C8;
pub const CARD_CLOSE: u32 = 0x8017_1A40;
pub const CARD_CREATE_ASYNC: u32 = 0x8017_1BCC;
pub const CARD_CREATE: u32 = 0x8017_1DEC;
pub const CARD_READ_ASYNC: u32 = 0x8017_211C;
pub const CARD_READ: u32 = 0x8017_2264;
pub const CARD_CANCEL: u32 = 0x8017_22AC;
pub const CARD_WRITE_ASYNC: u32 = 0x8017_2558;
pub const CARD_WRITE: u32 = 0x8017_266C;
pub const CARD_DELETE_ASYNC: u32 = 0x8017_2758;
pub const CARD_DELETE: u32 = 0x8017_2868;
pub const CARD_GET_STATUS: u32 = 0x8017_2AA8;
pub const CARD_SET_STATUS_ASYNC: u32 = 0x8017_2BD4;
pub const CARD_SET_STATUS: u32 = 0x8017_2D48;
pub const CARD_SET_STATUS_EX: u32 = 0x8017_30A0;

const READY: i32 = 0;
const NOCARD: i32 = -3;
const NOFILE: i32 = -4;
const IOERROR: i32 = -5;
const EXIST: i32 = -7;
const INSSPACE: i32 = -9;
const LIMIT: i32 = -11;
const NAMETOOLONG: i32 = -12;

/// A Memory Card 59: 8 KB blocks, 59 for files, up to 127 files.
const BLOCK: u32 = 8192;
const BLOCKS: u32 = 59;
const MAX_FILES: usize = 127;
const NAME_MAX: usize = 32;

/// The result of the last call, per channel (CARDGetResultCode).
static RESULT: Global<[i32; 2]> = global!([0; 2]);
/// Callbacks to run on the next frame: (callback, channel, result).
static DONE: Global<Vec<(u32, u32, i32)>> = global!(Vec::new());

#[cfg(not(target_os = "wasi"))]
fn dir() -> Option<PathBuf> {
    std::env::var_os("SA2_MEMCARD").map(PathBuf::from)
}

fn trace(what: &str) {
    if std::env::var_os("SA2_TRACE_CARD").is_some() {
        eprintln!("card: {what}");
    }
}

/// Where the card's files live. Desktop: the SA2_MEMCARD folder. Browser:
/// memory, whose only copy is the server card (the page's `/dev/card`).
#[cfg(not(target_os = "wasi"))]
mod store {
    use super::dir;
    use std::path::PathBuf;

    fn hex(name: &str) -> String {
        name.bytes().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(s: &str) -> Option<String> {
        let bytes: Option<Vec<u8>> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
            .collect();
        String::from_utf8(bytes?).ok()
    }

    fn data_path(name: &str) -> Option<PathBuf> {
        Some(dir()?.join(format!("{}.dat", hex(name))))
    }
    fn stat_path(name: &str) -> Option<PathBuf> {
        Some(dir()?.join(format!("{}.stat", hex(name))))
    }
    /// Write through a temporary file, so a crash mid-write never leaves half a save.
    fn write_file(path: Option<PathBuf>, bytes: &[u8]) -> bool {
        let Some(path) = path else { return false };
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok()
    }
    pub fn available() -> bool {
        dir().is_some()
    }
    pub fn init() {
        if let Some(d) = dir() {
            let _ = std::fs::create_dir_all(d);
        }
    }
    /// The card's files by name, in a stable order (their file numbers).
    pub fn names() -> Vec<String> {
        let Some(d) = dir() else { return Vec::new() };
        let mut names: Vec<String> = std::fs::read_dir(&d)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        unhex(n.strip_suffix(".dat")?)
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
    pub fn size(name: &str) -> u32 {
        data_path(name)
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or(0, |m| m.len() as u32)
    }
    pub fn data(name: &str) -> Vec<u8> {
        data_path(name)
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_default()
    }
    pub fn stat(name: &str) -> Vec<u8> {
        stat_path(name)
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_default()
    }
    pub fn put_data(name: &str, bytes: &[u8]) -> bool {
        write_file(data_path(name), bytes)
    }
    pub fn put_stat(name: &str, bytes: &[u8]) -> bool {
        write_file(stat_path(name), bytes)
    }
    pub fn remove(name: &str) {
        for p in [data_path(name), stat_path(name)].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(target_os = "wasi")]
mod store {
    use sa2_recomp::{global, global::Global};
    use std::collections::BTreeMap;

    /// name -> (status, data)
    static FILES: Global<BTreeMap<String, (Vec<u8>, Vec<u8>)>> = global!(BTreeMap::new());

    /// The server card is in the slot only while signed in.
    pub fn available() -> bool {
        std::env::var("SA2_MMO_TOKEN").is_ok_and(|t| !t.is_empty())
    }
    pub fn init() {}
    pub fn names() -> Vec<String> {
        FILES.with_borrow(|f| f.keys().cloned().collect())
    }
    pub fn size(name: &str) -> u32 {
        FILES.with_borrow(|f| f.get(name).map_or(0, |e| e.1.len() as u32))
    }
    pub fn data(name: &str) -> Vec<u8> {
        FILES.with_borrow(|f| f.get(name).map(|e| e.1.clone()).unwrap_or_default())
    }
    pub fn stat(name: &str) -> Vec<u8> {
        FILES.with_borrow(|f| f.get(name).map(|e| e.0.clone()).unwrap_or_default())
    }
    pub fn put_data(name: &str, bytes: &[u8]) -> bool {
        FILES.with_borrow_mut(|f| f.entry(name.into()).or_default().1 = bytes.to_vec());
        true
    }
    pub fn put_stat(name: &str, bytes: &[u8]) -> bool {
        FILES.with_borrow_mut(|f| f.entry(name.into()).or_default().0 = bytes.to_vec());
        true
    }
    pub fn remove(name: &str) {
        FILES.with_borrow_mut(|f| f.remove(name));
    }
}

fn files() -> Vec<String> {
    store::names()
}

fn size_of(name: &str) -> u32 {
    store::size(name)
}

fn blocks(size: u32) -> u32 {
    size.div_ceil(BLOCK)
}

fn used_blocks() -> u32 {
    files().iter().map(|n| blocks(size_of(n))).sum()
}

fn present(chan: u32) -> bool {
    chan == 0 && store::available() && mmo_card_in()
}

/// The MMO server card is in the slot only while signed in.
fn mmo_card_in() -> bool {
    #[cfg(not(target_os = "wasi"))]
    return !crate::mmo::configured() || crate::mmo::signed_in();
    #[cfg(target_os = "wasi")]
    return true;
}

/// Put a card from the server into the store. Server files overwrite local
/// ones of the same name; local files the server lacks stay (and go up with
/// the next change), so nothing is lost.
fn store_card(files: Vec<crate::card_pack::CardFile>) -> Result<(), String> {
    for (name, stat, data) in files {
        if !store::put_data(&name, &data) || (!stat.is_empty() && !store::put_stat(&name, &stat)) {
            return Err(format!("could not store {name:?}"));
        }
    }
    Ok(())
}

fn whole_card() -> Vec<crate::card_pack::CardFile> {
    files()
        .into_iter()
        .map(|n| (n.clone(), store::stat(&n), store::data(&n)))
        .collect()
}

/// The browser's server card: the page answers reads of this device with the
/// checked-out card and sends writes to the server (lease and revision kept
/// there), so the game thread never waits on a socket of its own.
#[cfg(target_os = "wasi")]
const CARD_DEVICE: &str = "/dev/card";
#[cfg(target_os = "wasi")]
static CHECKED_OUT: Global<bool> = global!(false);

/// Mounting the server card checks the whole card out.
fn mmo_mount() -> i32 {
    #[cfg(not(target_os = "wasi"))]
    let stored = if crate::mmo::configured() {
        crate::mmo::pull(store_card)
    } else {
        Ok(())
    };
    #[cfg(target_os = "wasi")]
    let stored = if CHECKED_OUT.with_borrow(|c| *c) {
        Ok(())
    } else {
        std::fs::read(CARD_DEVICE)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                if bytes.is_empty() {
                    return Ok(()); // a new card: the game's first save fills it
                }
                store_card(crate::card_pack::unpack(&bytes).ok_or("the server card is damaged")?)
            })
            .map(|()| CHECKED_OUT.set(true))
    };
    if let Err(e) = stored {
        eprintln!("card (mmo): could not load the card: {e}");
        return IOERROR;
    }
    READY
}

/// After the game changed a card file, the server must have the card too,
/// or the game is told the change failed.
fn mmo_changed() -> i32 {
    #[cfg(not(target_os = "wasi"))]
    let sent = if crate::mmo::configured() {
        crate::mmo::push(&whole_card())
    } else {
        Ok(())
    };
    #[cfg(target_os = "wasi")]
    // Synced: the page answers sync with the server's yes or no.
    let sent = (|| {
        use std::io::Write;
        let mut device = std::fs::File::create(CARD_DEVICE)?;
        device.write_all(&crate::card_pack::pack(&whole_card()))?;
        device.sync_all()
    })()
    .map_err(|e| e.to_string());
    if let Err(e) = sent {
        eprintln!("card (mmo): could not save the card: {e}");
        return IOERROR;
    }
    READY
}

fn set_result(c: &mut Cpu, chan: u32, r: i32) {
    RESULT.with_borrow_mut(|a| {
        if let Some(x) = a.get_mut(chan as usize & 1) {
            *x = r;
        }
    });
    c.r[3] = r as u32;
}

/// An asynchronous call: an error now, or READY now and the result on the
/// next frame through `cb`.
fn finish_async(c: &mut Cpu, chan: u32, r: i32, cb: u32) {
    if r < 0 && r != NOFILE && r != EXIST {
        set_result(c, chan, r);
        return;
    }
    if cb != 0 {
        DONE.with_borrow_mut(|q| q.push((cb, chan, r)));
    }
    set_result(c, chan, if cb != 0 { READY } else { r });
}

/// Frame hook: the callbacks of asynchronous card calls.
fn done(c: &mut Cpu, m: &mut Mem) {
    #[cfg(not(target_os = "wasi"))]
    crate::mmo::renew_if_due();
    for (cb, chan, r) in DONE.take() {
        guest_call(c, m, cb, &[chan, r as u32]);
    }
}

fn nothing(c: &mut Cpu, _m: &mut Mem) {
    c.r[3] = 0;
}

// CARDProbeEx(chan, s32* memSize, s32* sectorSize)
fn probe_ex(c: &mut Cpu, m: &mut Mem) {
    trace(&format!(
        "probe_ex r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3];
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    if c.r[4] != 0 {
        m.w32(c.r[4], 4); // megabits
    }
    if c.r[5] != 0 {
        m.w32(c.r[5], BLOCK);
    }
    set_result(c, chan, READY);
}

// CARDGetMemSize(chan, u16* size)
fn get_mem_size(c: &mut Cpu, m: &mut Mem) {
    trace(&format!(
        "get_mem_size r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3];
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    if c.r[4] != 0 {
        m.w16(c.r[4], 4);
    }
    set_result(c, chan, READY);
}

// CARDMount(chan, workArea, detachCallback)
fn mount(c: &mut Cpu, _m: &mut Mem) {
    let chan = c.r[3];
    let r = if present(chan) { mmo_mount() } else { NOCARD };
    trace(&format!("mount {chan} -> {r}"));
    set_result(c, chan, r);
}

// CARDMountAsync(chan, workArea, detachCallback, attachCallback)
fn mount_async(c: &mut Cpu, _m: &mut Mem) {
    let chan = c.r[3];
    let r = if present(chan) { mmo_mount() } else { NOCARD };
    trace(&format!("mount async {chan} -> {r}"));
    finish_async(c, chan, r, c.r[6]);
}

// CARDCheck(chan) / CARDUnmount(chan) / CARDFormat(chan)
fn check(c: &mut Cpu, _m: &mut Mem) {
    trace(&format!(
        "check r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3];
    set_result(c, chan, if present(chan) { READY } else { NOCARD });
}

// CARDCheckExAsync(chan, s32* xferBytes, cb)
fn check_ex_async(c: &mut Cpu, m: &mut Mem) {
    trace(&format!(
        "check_ex_async r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3];
    if c.r[4] != 0 {
        m.w32(c.r[4], 0);
    }
    let r = if present(chan) { READY } else { NOCARD };
    finish_async(c, chan, r, c.r[5]);
}

fn format(c: &mut Cpu, _m: &mut Mem) {
    let chan = c.r[3];
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    for n in files() {
        store::remove(&n);
    }
    trace("format");
    set_result(c, chan, READY);
}

// CARDFreeBlocks(chan, s32* bytesNotUsed, s32* filesNotUsed)
fn free_blocks(c: &mut Cpu, m: &mut Mem) {
    trace(&format!(
        "free_blocks r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3];
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    let free = BLOCKS.saturating_sub(used_blocks());
    if c.r[4] != 0 {
        m.w32(c.r[4], free * BLOCK);
    }
    if c.r[5] != 0 {
        m.w32(c.r[5], (MAX_FILES - files().len().min(MAX_FILES)) as u32);
    }
    set_result(c, chan, READY);
}

fn get_result_code(c: &mut Cpu, _m: &mut Mem) {
    trace(&format!(
        "get_result_code r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let chan = c.r[3] as usize & 1;
    c.r[3] = RESULT.with_borrow(|a| a[chan]) as u32;
}

/// Fill a CARDFileInfo for file `name` (number `no`).
fn fill_info(m: &mut Mem, fi: u32, chan: u32, no: usize, name: &str) {
    m.w32(fi, chan);
    m.w32(fi + 4, no as u32);
    m.w32(fi + 8, 0);
    m.w32(fi + 0xC, size_of(name));
    m.w16(fi + 0x10, 5 + no as u16);
}

fn name_arg(m: &Mem, a: u32) -> Result<String, i32> {
    let n = cstr(m, a);
    if n.is_empty() || n.len() > NAME_MAX {
        return Err(NAMETOOLONG);
    }
    Ok(n)
}

// CARDOpen(chan, fileName, CARDFileInfo*)
fn open(c: &mut Cpu, m: &mut Mem) {
    let chan = c.r[3];
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    let r = match name_arg(m, c.r[4]) {
        Err(e) => e,
        Ok(name) => match files().iter().position(|n| *n == name) {
            Some(no) => {
                fill_info(m, c.r[5], chan, no, &name);
                READY
            }
            None => NOFILE,
        },
    };
    trace(&format!("open {:?} -> {r}", cstr(m, c.r[4])));
    set_result(c, chan, r);
}

fn create_file(m: &mut Mem, chan: u32, name_at: u32, size: u32, fi: u32) -> i32 {
    if !present(chan) {
        return NOCARD;
    }
    let name = match name_arg(m, name_at) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let all = files();
    if all.contains(&name) {
        return EXIST;
    }
    if all.len() >= MAX_FILES {
        return LIMIT;
    }
    if used_blocks() + blocks(size) > BLOCKS {
        return INSSPACE;
    }
    let ok = store::put_data(&name, &vec![0; size as usize]);
    if !ok {
        return IOERROR;
    }
    let no = files().iter().position(|n| *n == name).unwrap_or(0);
    fill_info(m, fi, chan, no, &name);
    trace(&format!("create {name:?} {size} bytes"));
    mmo_changed()
}

// CARDCreate(chan, fileName, size, CARDFileInfo*)
fn create(c: &mut Cpu, m: &mut Mem) {
    let r = create_file(m, c.r[3], c.r[4], c.r[5], c.r[6]);
    set_result(c, c.r[3], r);
}

// CARDCreateAsync(chan, fileName, size, CARDFileInfo*, cb)
fn create_async(c: &mut Cpu, m: &mut Mem) {
    let chan = c.r[3];
    let r = create_file(m, chan, c.r[4], c.r[5], c.r[6]);
    finish_async(c, chan, r, c.r[7]);
}

/// The open file a CARDFileInfo names: (channel, name).
fn file_of(m: &Mem, fi: u32) -> Option<(u32, String)> {
    let chan = m.r32(fi);
    let no = m.r32(fi + 4) as usize;
    files().get(no).cloned().map(|n| (chan, n))
}

fn read_file(m: &mut Mem, fi: u32, buf: u32, len: u32, off: u32) -> (u32, i32) {
    let Some((chan, name)) = file_of(m, fi) else {
        return (0, NOFILE);
    };
    let data = store::data(&name);
    let start = (off as usize).min(data.len());
    let end = (off as usize).saturating_add(len as usize).min(data.len());
    m.load(buf, &data[start..end]);
    trace(&format!("read {name:?} {len} at {off}"));
    (chan, READY)
}

fn write_to_file(m: &mut Mem, fi: u32, buf: u32, len: u32, off: u32) -> (u32, i32) {
    let Some((chan, name)) = file_of(m, fi) else {
        return (0, NOFILE);
    };
    let mut data = store::data(&name);
    let end = off as usize + len as usize;
    if data.len() < end {
        data.resize(end, 0);
    }
    for i in 0..len as usize {
        data[off as usize + i] = m.r8(buf.wrapping_add(i as u32));
    }
    trace(&format!("write {name:?} {len} at {off}"));
    (
        chan,
        if store::put_data(&name, &data) {
            mmo_changed()
        } else {
            IOERROR
        },
    )
}

// CARDRead(fi, buf, length, offset)
fn read(c: &mut Cpu, m: &mut Mem) {
    let (chan, r) = read_file(m, c.r[3], c.r[4], c.r[5], c.r[6]);
    set_result(c, chan, r);
}

// CARDReadAsync(fi, buf, length, offset, cb)
fn read_async(c: &mut Cpu, m: &mut Mem) {
    let (chan, r) = read_file(m, c.r[3], c.r[4], c.r[5], c.r[6]);
    finish_async(c, chan, r, c.r[7]);
}

// CARDWrite(fi, buf, length, offset)
fn write(c: &mut Cpu, m: &mut Mem) {
    let (chan, r) = write_to_file(m, c.r[3], c.r[4], c.r[5], c.r[6]);
    set_result(c, chan, r);
}

// CARDWriteAsync(fi, buf, length, offset, cb)
fn write_async(c: &mut Cpu, m: &mut Mem) {
    let (chan, r) = write_to_file(m, c.r[3], c.r[4], c.r[5], c.r[6]);
    finish_async(c, chan, r, c.r[7]);
}

// CARDClose(fi)
fn close(c: &mut Cpu, m: &mut Mem) {
    let chan = m.r32(c.r[3]);
    m.w32(c.r[3], u32::MAX);
    set_result(c, chan, READY);
}

fn delete_file(m: &Mem, chan: u32, name_at: u32) -> i32 {
    if !present(chan) {
        return NOCARD;
    }
    let name = match name_arg(m, name_at) {
        Ok(n) => n,
        Err(e) => return e,
    };
    if !files().contains(&name) {
        return NOFILE;
    }
    store::remove(&name);
    trace(&format!("delete {name:?}"));
    mmo_changed()
}

// CARDDelete(chan, fileName)
fn delete(c: &mut Cpu, m: &mut Mem) {
    let r = delete_file(m, c.r[3], c.r[4]);
    set_result(c, c.r[3], r);
}

// CARDDeleteAsync(chan, fileName, cb)
fn delete_async(c: &mut Cpu, m: &mut Mem) {
    let chan = c.r[3];
    let r = delete_file(m, chan, c.r[4]);
    finish_async(c, chan, r, c.r[5]);
}

/// CARDStat bytes 0x24..0x3C as CARDSetStatus gave them (time, game,
/// company, banner format, icon address, formats, speed, comment address).
const STAT_FROM: u32 = 0x24;
const STAT_TO: u32 = 0x3C;

/// The offsets CARDGetStatus derives from the icon address and formats.
fn icon_offsets(m: &mut Mem, st: u32) {
    let icon_addr = m.r32(st + 0x30);
    let banner = m.r8(st + 0x2E) & 3;
    let icon_format = m.r16(st + 0x34);
    let none = u32::MAX;
    if icon_addr == none {
        for o in (0x3C..=0x68).step_by(4) {
            m.w32(st + o, none);
        }
        m.w32(st + 0x68, 0);
        return;
    }
    let mut at = icon_addr;
    match banner {
        1 => {
            m.w32(st + 0x3C, at);
            m.w32(st + 0x40, at + 96 * 32);
            at += 96 * 32 + 512;
        }
        2 => {
            m.w32(st + 0x3C, at);
            m.w32(st + 0x40, none);
            at += 96 * 32 * 2;
        }
        _ => {
            m.w32(st + 0x3C, none);
            m.w32(st + 0x40, none);
        }
    }
    let mut shared_tlut = false;
    for i in 0..8u32 {
        match (icon_format >> (2 * i)) & 3 {
            1 => {
                m.w32(st + 0x44 + 4 * i, at);
                at += 32 * 32;
                shared_tlut = true;
            }
            2 => {
                m.w32(st + 0x44 + 4 * i, at);
                at += 32 * 32 * 2;
            }
            _ => m.w32(st + 0x44 + 4 * i, none),
        }
    }
    if shared_tlut {
        m.w32(st + 0x64, at);
        at += 512;
    } else {
        m.w32(st + 0x64, none);
    }
    m.w32(st + 0x68, at);
}

// CARDGetStatus(chan, fileNo, CARDStat*)
fn get_status(c: &mut Cpu, m: &mut Mem) {
    trace(&format!(
        "get_status r3={:#x} r4={:#x} r5={:#x}",
        c.r[3], c.r[4], c.r[5]
    ));
    let (chan, no, st) = (c.r[3], c.r[4] as usize, c.r[5]);
    if !present(chan) {
        set_result(c, chan, NOCARD);
        return;
    }
    let Some(name) = files().get(no).cloned() else {
        set_result(c, chan, NOFILE);
        return;
    };
    for i in 0..0x6C {
        m.w8(st + i, 0);
    }
    for (i, b) in name.bytes().take(NAME_MAX).enumerate() {
        m.w8(st + i as u32, b);
    }
    m.w32(st + 0x20, size_of(&name));
    let saved = store::stat(&name);
    if saved.len() == (STAT_TO - STAT_FROM) as usize {
        m.load(st + STAT_FROM, &saved);
    } else {
        m.w32(st + 0x30, u32::MAX); // no icon
    }
    icon_offsets(m, st);
    set_result(c, chan, READY);
}

fn set_status_of(m: &Mem, chan: u32, no: usize, st: u32) -> i32 {
    if !present(chan) {
        return NOCARD;
    }
    let Some(name) = files().get(no).cloned() else {
        return NOFILE;
    };
    let bytes: Vec<u8> = (STAT_FROM..STAT_TO).map(|i| m.r8(st + i)).collect();
    let ok = store::put_stat(&name, &bytes);
    trace(&format!("set status {name:?}"));
    if ok {
        mmo_changed()
    } else {
        IOERROR
    }
}

// CARDSetStatus(chan, fileNo, CARDStat*)
fn set_status(c: &mut Cpu, m: &mut Mem) {
    let r = set_status_of(m, c.r[3], c.r[4] as usize, c.r[5]);
    set_result(c, c.r[3], r);
}

// __CARDSetStatusEx(chan, fileNo, CARDDir*): the Chao World save sets its
// directory entry this way after CARDSetStatus. The host card keeps no
// directory blocks (CARDSetStatus already stored the status), so it only
// reports the result; unhooked, it reached the real library and failed.
fn set_status_ex(c: &mut Cpu, _m: &mut Mem) {
    let chan = c.r[3];
    let r = if !present(chan) {
        NOCARD
    } else if files().get(c.r[4] as usize).is_none() {
        NOFILE
    } else {
        READY
    };
    trace(&format!("set_status_ex {} -> {r}", c.r[4]));
    set_result(c, chan, r);
}

// CARDSetStatusAsync(chan, fileNo, CARDStat*, cb)
fn set_status_async(c: &mut Cpu, m: &mut Mem) {
    let chan = c.r[3];
    let r = set_status_of(m, chan, c.r[4] as usize, c.r[5]);
    finish_async(c, chan, r, c.r[6]);
}

/// Only with SA2_MEMCARD: otherwise the game's own CARD library finds the
/// slot empty, as before.
pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !store::available() {
        return;
    }
    store::init();
    for (addr, f) in [
        (CARD_INIT, nothing as sa2_recomp::rt::Func),
        (CARD_GET_RESULT_CODE, get_result_code),
        (CARD_FREE_BLOCKS, free_blocks),
        (CARD_GET_MEM_SIZE, get_mem_size),
        (CARD_GET_XFERRED_BYTES, nothing),
        (CARD_CHECK_EX_ASYNC, check_ex_async),
        (CARD_CHECK, check),
        (CARD_PROBE_EX, probe_ex),
        (CARD_MOUNT_ASYNC, mount_async),
        (CARD_MOUNT, mount),
        (CARD_UNMOUNT, check),
        (CARD_FORMAT, format),
        (CARD_OPEN, open),
        (CARD_CLOSE, close),
        (CARD_CREATE_ASYNC, create_async),
        (CARD_CREATE, create),
        (CARD_READ_ASYNC, read_async),
        (CARD_READ, read),
        (CARD_CANCEL, nothing),
        (CARD_WRITE_ASYNC, write_async),
        (CARD_WRITE, write),
        (CARD_DELETE_ASYNC, delete_async),
        (CARD_DELETE, delete),
        (CARD_GET_STATUS, get_status),
        (CARD_SET_STATUS_ASYNC, set_status_async),
        (CARD_SET_STATUS, set_status),
        (CARD_SET_STATUS_EX, set_status_ex),
    ] {
        c.rt.set_override(addr, f);
    }
    sa2_recomp::os::add_frame_hook(done);
}
