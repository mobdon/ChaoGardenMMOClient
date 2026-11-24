//! MMO glue: a cache of Chao for the whole session, in a region of the raised
//! MEM1 the game never allocates from or frees (above its 24 MB layout and
//! above chao_player's own ChaoMain copy). Each Chao is keyed by its id and
//! holds its save info (the 0x800-byte record CreateChaoExtra takes) and gear
//! flags; the game task is created from it again on every area change (the
//! game drops all tasks then). Shared models and textures stay loaded once in
//! chao_player.
//!
//! API (one per game thread; the page/server glue calls these):
//! - `get_or_load(m, id, info, gear)` -> the record's address (loads once).
//! - `info_addr(id)` -> the record's address if cached.
//! - `set_online(id, false)` on the server's player-left event: the Chao's
//!   task leaves the scene at once (the caller deletes it); its entry is kept
//!   for `GRACE` and reused if the player returns, then evicted by `tick`.
//! - `tick(now)` evicts expired entries; the owner's Chao is never evicted.
//! - `preload(m, list)` loads a list up front; `progress()` = (done, total)
//!   for the page's loading bar.
//! - `live(near)` picks the nearest-N online Chao to give a game task.
use sa2_recomp::rt::Mem;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// The region's start: right after chao_player's own use (ChaoMain, its
/// motion module, the strings, INFO and the mixed action table end below
/// 0x82F0_2A00), page-aligned; it runs to the top of MEM1 (SA2_MEM1_MB:
/// about 1 MB at 48 MB, enough for hundreds of 0x800-byte records).
pub const BASE: u32 = 0x82F0_4000;
/// The region's end: chao_player's remote-Chao heap starts here.
pub const END: u32 = 0x8300_0000;
/// The save info record CreateChaoExtra takes.
pub const INFO_SIZE: u32 = 0x800;
/// An offline player's Chao stays cached this long before eviction.
pub const GRACE: Duration = Duration::from_secs(3 * 60);

pub type ChaoId = u64;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub info: u32,
    pub gear: u32,
    pub own: bool,
    pub offline_since: Option<Instant>,
}

/// First-fit allocator over [base, end) with freed blocks reused.
#[derive(Default, Debug)]
pub struct Region {
    base: u32,
    end: u32,
    top: u32,
    free: Vec<(u32, u32)>,
}

impl Region {
    pub fn new(base: u32, end: u32) -> Self {
        Region { base, end, top: base, free: vec![] }
    }
    /// A region emptied except for `kept` blocks (address, size): they stay allocated, the gaps
    /// before and between them are free again, and new blocks go after the last of them.
    /// For an area change that drops every other block at once while files loaded into the
    /// region are still in use (Shadow's motions under the mixed action rows, chao_player.rs).
    pub fn with_kept(base: u32, end: u32, kept: &[(u32, u32)]) -> Self {
        let mut k: Vec<(u32, u32)> = kept.iter().map(|&(a, s)| (a, (s + 31) & !31)).filter(|&(a, s)| a >= base && a + s <= end).collect();
        k.sort();
        let (mut at, mut free) = (base, vec![]);
        for (a, s) in k {
            if a > at { free.push((at, a - at)); }
            at = at.max(a + s);
        }
        Region { base, end, top: at, free }
    }
    pub fn alloc(&mut self, size: u32) -> Option<u32> {
        let size = (size + 31) & !31;
        if let Some(i) = self.free.iter().position(|&(_, s)| s >= size) {
            let (a, s) = self.free[i];
            if s == size { self.free.remove(i); } else { self.free[i] = (a + size, s - size); }
            return Some(a);
        }
        (self.top + size <= self.end).then(|| { let a = self.top; self.top += size; a })
    }
    pub fn free(&mut self, a: u32, size: u32) {
        self.free.push((a, (size + 31) & !31));
    }
    pub fn used(&self) -> u32 {
        self.top - self.base - self.free.iter().map(|f| f.1).sum::<u32>()
    }
}

#[derive(Default)]
pub struct Cache {
    pub region: Region,
    pub entries: BTreeMap<ChaoId, Entry>,
    pub progress: (u32, u32),
}

thread_local! {
    pub static CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    CACHE.with_borrow_mut(|c| f(c.get_or_insert_with(|| {
        // Up to the remote Chao heap at 0x83000000 (chao_player).
        let end = (sa2_recomp::rt::mem1() | 0x8000_0000).min(END);
        if end <= BASE {
            eprintln!("chao-cache: disabled, MEM1 ends at {end:#x} below its region {BASE:#x} (raise SA2_MEM1_MB)");
            return Cache { region: Region::new(BASE, BASE), ..Default::default() };
        }
        eprintln!("chao-cache: region {BASE:#x}..{end:#x} ({} KB)", (end - BASE) / 1024);
        Cache { region: Region::new(BASE, end), ..Default::default() }
    })))
}

impl Cache {
    pub fn get_or_load(&mut self, m: &mut Mem, id: ChaoId, info: &[u8], gear: u32, own: bool) -> Option<u32> {
        if let Some(e) = self.entries.get_mut(&id) {
            e.offline_since = None;
            return Some(e.info);
        }
        let a = self.region.alloc(INFO_SIZE)?;
        let n = info.len().min(INFO_SIZE as usize);
        m.load(a, &info[..n]);
        if n < INFO_SIZE as usize {
            m.load(a + n as u32, &vec![0; INFO_SIZE as usize - n]);
        }
        self.entries.insert(id, Entry { info: a, gear, own, offline_since: None });
        Some(a)
    }
    pub fn set_online(&mut self, id: ChaoId, online: bool, now: Instant) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.offline_since = if online { None } else { Some(now) };
        }
    }
    pub fn tick(&mut self, now: Instant) {
        let gone: Vec<ChaoId> = self.entries.iter()
            .filter(|(_, e)| !e.own && e.offline_since.is_some_and(|t| now.duration_since(t) >= GRACE))
            .map(|(&id, _)| id).collect();
        for id in gone {
            if let Some(e) = self.entries.remove(&id) {
                self.region.free(e.info, INFO_SIZE);
            }
        }
    }
    /// The nearest `n` online Chao by squared distance (caller supplies it).
    pub fn live(&self, n: usize, dist2: impl Fn(ChaoId) -> f32) -> Vec<ChaoId> {
        let mut on: Vec<ChaoId> = self.entries.iter().filter(|(_, e)| e.offline_since.is_none()).map(|(&id, _)| id).collect();
        on.sort_by(|a, b| dist2(*a).total_cmp(&dist2(*b)));
        on.truncate(n);
        on
    }
}

pub fn get_or_load(m: &mut Mem, id: ChaoId, info: &[u8], gear: u32, own: bool) -> Option<u32> {
    with(|c| c.get_or_load(m, id, info, gear, own))
}
/// Overwrite a cached Chao's record (a newer version from the server);
/// creates the entry when missing.
pub fn replace(m: &mut Mem, id: ChaoId, info: &[u8]) -> Option<u32> {
    with(|c| {
        let a = match c.entries.get(&id) {
            Some(e) => e.info,
            None => return c.get_or_load(m, id, info, 0, false),
        };
        let n = info.len().min(INFO_SIZE as usize);
        m.load(a, &info[..n]);
        Some(a)
    })
}
pub fn info_addr(id: ChaoId) -> Option<u32> {
    with(|c| c.entries.get(&id).map(|e| e.info))
}
pub fn set_online(id: ChaoId, online: bool) {
    with(|c| c.set_online(id, online, Instant::now()))
}
pub fn tick() {
    with(|c| c.tick(Instant::now()))
}
/// Load a list up front (boot preload); `progress()` reports (done, total).
pub fn preload(m: &mut Mem, list: &[(ChaoId, Vec<u8>, u32)]) {
    with(|c| {
        c.progress = (0, list.len() as u32);
        for (id, info, gear) in list {
            c.get_or_load(m, *id, info, *gear, false);
            c.progress.0 += 1;
        }
    })
}
pub fn progress() -> (u32, u32) {
    with(|c| c.progress)
}
pub fn used_bytes() -> u32 {
    with(|c| c.region.used())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_region_emptied_around_kept_blocks_never_hands_them_out() {
        // Two files loaded early, other blocks between and after; all but the files dropped.
        let mut r = Region::new(0x1000, 0x9000);
        let a = r.alloc(0x400).unwrap();
        let _x = r.alloc(0x200).unwrap();
        let b = r.alloc(0x300).unwrap();
        let _y = r.alloc(0x800).unwrap();
        let kept = [(a, 0x400), (b, 0x300)];
        let mut r = Region::with_kept(0x1000, 0x9000, &kept);
        assert_eq!(r.used(), 0x700);
        let hits = |p: u32, n: u32| kept.iter().any(|&(k, s)| p < k + s && k < p + n);
        for n in [0x200, 0x100, 0x800, 0x40, 0x2000] {
            let p = r.alloc(n).unwrap();
            assert!(!hits(p, n), "{p:#x}+{n:#x} overlaps a kept block");
        }
        // Nothing kept: the whole region again.
        assert_eq!(Region::with_kept(0x1000, 0x9000, &[]).alloc(0x8000), Some(0x1000));
    }

    use super::*;

    #[test]
    fn region_reuses_freed_blocks() {
        let mut r = Region::new(0x1000, 0x1000 + 3 * 0x800);
        let a = r.alloc(0x800).unwrap();
        let b = r.alloc(0x800).unwrap();
        r.free(a, 0x800);
        assert_eq!(r.alloc(0x800), Some(a));
        assert_eq!(r.used(), 2 * 0x800);
        assert!(r.alloc(0x800).is_some());
        assert_eq!(r.alloc(0x800), None);
        let _ = b;
    }

    #[test]
    fn offline_entries_leave_after_grace_but_not_the_owner() {
        let mut c = Cache { region: Region::new(0x1000, 0x10_0000), ..Default::default() };
        let t0 = Instant::now();
        for (id, own) in [(1, true), (2, false), (3, false)] {
            c.entries.insert(id, Entry { info: c.region.alloc(INFO_SIZE).unwrap(), gear: 0, own, offline_since: None });
        }
        c.set_online(1, false, t0);
        c.set_online(2, false, t0);
        c.tick(t0 + GRACE - Duration::from_secs(1));
        assert_eq!(c.entries.len(), 3);
        c.set_online(3, false, t0);
        c.set_online(3, true, t0);
        c.tick(t0 + GRACE);
        assert!(c.entries.contains_key(&1) && !c.entries.contains_key(&2) && c.entries.contains_key(&3));
        assert_eq!(c.live(1, |id| id as f32), vec![3]);
    }
}
