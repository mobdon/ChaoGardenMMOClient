//! SET rows and the object spawner (E4; main.dol GSNE8P):
//! - `fn_800651EC`, the join of the `_s` and `_u` SET files into one list;
//! - `fn_800658EC`, the entry setup of the 0x600-entry table at 0x801F5968;
//! - `fn_80064F08` (with `fn_80064258`), the stage's ring total in G+0x6E;
//! - `fn_80066274`, the SET task's exec, and `fn_80065C40`, the one-player
//!   spawner that creates a task for every entry in range of player 0.
//!
//! Entry flags (+2) live in [`Runtime::set_flags`], so `FreeTask` clears an
//! entry's spawned bit as main.dol does. Object definitions come from the
//! stage's object list (on PC, the Steam exe's list for the stage); a
//! definition's exec is created as is, so an object whose routine is not
//! registered fails closed as [`crate::Event::Unported`].
use crate::stage::SetState;
use crate::task::{Event, Natives, Runtime, TaskId};

/// `fn_80066274`, the SET task's exec.
pub const SET_EXEC: u32 = 0x8006_6274;
/// The table at 0x801F5968 has 0x600 entries; setup stops at 0x5FF.
pub const ENTRIES: usize = 0x600;
const LAST_ENTRY: usize = 0x5FF;
/// `fn_80065048` asks `fn_800651EC` for room for 0x800 records (calloc'd, so
/// records past the files read as zero).
const CAPACITY: usize = 0x7FF;
/// lbl_803AF370 / 0x803AF374 / 0x803AF378.
const ALWAYS_DIST: f32 = -100.0;
const DEFAULT_DIST: f32 = 160_000.0;
const KEEP_MARGIN: f32 = 100.0;

/// One 0x20-byte SET record.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SetRow {
    /// +0: object-list index (main.dol uses `id & 0x7FFF`).
    pub id: u16,
    /// +2, +4, +6, read as signed halves (`lha`).
    pub rot: [i16; 3],
    pub pos: [f32; 3],
    pub scale: [f32; 3],
}

impl SetRow {
    fn read(r: &[u8]) -> Self {
        let h = |i: usize| {
            r.get(i..i + 2)
                .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
        };
        let f = |i: usize| {
            r.get(i..i + 4)
                .map_or(0.0, |b| f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        };
        Self {
            id: h(0),
            rot: [h(2) as i16, h(4) as i16, h(6) as i16],
            pos: [f(8), f(12), f(16)],
            scale: [f(20), f(24), f(28)],
        }
    }
}

/// `fn_800651EC(s, u)`: the joined SET list. The count is the sum of both
/// headers; records are `s`'s then `u`'s, zero past what the files hold.
/// `None` when neither file exists (main.dol returns null).
pub fn join_files(s: Option<&[u8]>, u: Option<&[u8]>) -> Option<Vec<SetRow>> {
    if s.is_none() && u.is_none() {
        return None;
    }
    let count = |f: Option<&[u8]>| {
        f.and_then(|b| b.get(..4))
            .map_or(0u32, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let total = (count(s).wrapping_add(count(u)) as usize).min(CAPACITY);
    let mut rows: Vec<SetRow> = [s, u]
        .into_iter()
        .flatten()
        .flat_map(|b| b.get(0x20..).unwrap_or(&[]).as_chunks::<0x20>().0)
        .take(CAPACITY)
        .map(|r| SetRow::read(r))
        .collect();
    rows.resize(total.max(rows.len()), SetRow::default());
    Some(rows)
}

/// An object-list definition (0x10 bytes natively).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjectDef {
    /// +0: work blocks for `CreateElementalTask`.
    pub im: u8,
    /// +1: task level.
    pub level: u8,
    /// +2: bit 0 own distance, bit 1 always, bit 2 first pass, 0x70 ring kind.
    pub flags: u16,
    /// +4: squared spawn distance (with flag bit 0).
    pub distance: f32,
    /// +8: exec address.
    pub exec: u32,
    /// +0xC: name (kept as text; tasks get name 0).
    pub name: String,
}

/// One entry of the table at 0x801F5968 (flags in [`Runtime::set_flags`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SetEntry {
    /// +0: times spawned.
    pub count: u8,
    /// +1.
    pub b1: u8,
    /// +4: the spawned task.
    pub task: Option<TaskId>,
    /// +8: the record (index into [`SetTable::rows`]).
    pub row: Option<usize>,
    /// +0xC after a spawn: 100 + the spawn distance.
    pub keep: f32,
}

/// The SET data the spawner reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetTable {
    pub rows: Vec<SetRow>,
    pub defs: Vec<ObjectDef>,
    pub entries: Vec<SetEntry>,
    /// playertwp[0], [1] (0x801E7768): the tasks whose twp holds the position.
    pub playertwp: [Option<TaskId>; 2],
    /// 0x803ADB04's position (+0x14), used when there is no player 0.
    pub fallback: Option<[f32; 3]>,
}

fn flag(rt: &Runtime, i: usize) -> u16 {
    rt.set_flags.get(i).copied().unwrap_or(0)
}

fn set_flag(rt: &mut Runtime, i: usize, v: u16) {
    if rt.set_flags.len() < ENTRIES {
        rt.set_flags.resize(ENTRIES, 0);
    }
    if let Some(f) = rt.set_flags.get_mut(i) {
        *f = v;
    }
}

/// `fn_80065AE4`: clear entries 0..0x5FE and the count (0x803ADA14).
pub fn clear_entries(rt: &mut Runtime, t: &mut SetTable, ada14: &mut i16) {
    *ada14 = 0;
    rt.set_flags.resize(ENTRIES, 0);
    t.entries.resize(ENTRIES, SetEntry::default());
    for i in 0..LAST_ENTRY {
        set_flag(rt, i, 0);
        if let Some(e) = t.entries.get_mut(i) {
            *e = SetEntry::default();
        }
    }
}

/// `fn_800658EC`: set up entries from 0x803ADA14 for every record (the
/// native loop takes one more than the count, clamped at 0x5FF).
pub fn setup_entries(rt: &mut Runtime, t: &mut SetTable, ada14: &mut i16) {
    let cur = (*ada14).max(0) as usize;
    let end = cur.saturating_add(t.rows.len()).min(LAST_ENTRY);
    t.entries.resize(ENTRIES, SetEntry::default());
    for i in cur..=end {
        set_flag(rt, i, (flag(rt, i) | 0x8000) & !0x44);
        if let Some(e) = t.entries.get_mut(i) {
            e.count = 0;
            e.b1 = 0;
            e.row = Some(i - cur);
        }
    }
    *ada14 = end as i16;
}

/// `fn_80064F08` after `fn_80064258`: G+0x6E = G+0x70 plus the rings the
/// records place. Returns the total (also written to 0x803AD860's +0x10).
pub fn ring_total(t: &SetTable, g: &mut [u8]) -> i16 {
    let half = |g: &[u8], o: usize| {
        g.get(o..o + 2)
            .map_or(0, |b| i16::from_be_bytes([b[0], b[1]]))
    };
    let mut n = half(g, 0x70) as i32;
    if let Some(b) = g.get_mut(0x6E..0x70) {
        b.copy_from_slice(&(n as i16).to_be_bytes());
    }
    for r in &t.rows {
        let Some(d) = t.defs.get((r.id & 0x7FFF) as usize) else {
            continue;
        };
        n += match d.flags & 0x70 {
            0x10 => 1,
            0x20 => (r.scale[2] as i32).clamp(1, 8),
            0x30 => match r.scale[0] as i32 {
                1 => 5,
                3 => 10,
                4 => 20,
                _ => 0,
            },
            _ => 0,
        };
    }
    let n = n as i16;
    if let Some(b) = g.get_mut(0x6E..0x70) {
        b.copy_from_slice(&n.to_be_bytes());
    }
    n
}

/// `fn_80066180`: within `dist` of `p` (squared, axis by axis).
fn in_range(a: [f32; 3], p: [f32; 3], dist: f32) -> bool {
    let mut d = 0.0f32;
    for k in 0..3 {
        let x = p[k] - a[k];
        d = x.mul_add(x, d);
        if d > dist {
            return false;
        }
    }
    true
}

fn twp_pos(rt: &Runtime, t: TaskId) -> Option<[f32; 3]> {
    let x = rt.task(t)?;
    let f = |o| f32::from_bits(x.twp_word(o));
    x.twp.as_ref()?;
    Some([f(0x14), f(0x18), f(0x1C)])
}

/// `fn_80066274`: the SET task's exec (state at twp+0).
pub fn set_exec(rt: &mut Runtime, s: &mut SetState, task: TaskId) {
    let state = rt
        .task(task)
        .and_then(|x| x.twp.as_ref())
        .and_then(|w| w.first())
        .map_or(0, |b| *b as i8);
    match state {
        1 => {
            if s.table.playertwp[1].is_some() {
                // fn_80065EC8, the two-player spawner, is not ported.
                rt.events.push(Event::Anomaly(
                    "SET: two-player spawner fn_80065EC8 is not ported",
                ));
            } else {
                spawn_1p(rt, s);
            }
            s.ada0c = 1;
            if s.ada08 != 0 {
                if let Some(w) = rt.task_mut(task).and_then(|x| x.twp.as_mut()) {
                    w[0] = 2;
                    w[1] = 0;
                }
                s.ada08 = 0;
            }
        }
        // fn_80065B28 reloads the SET data (two-file switch); not ported.
        2 => rt
            .events
            .push(Event::Anomaly("SET: reload fn_80065B28 is not ported")),
        _ => {}
    }
}

/// `fn_80065C40`: spawn every set-up entry in range of player 0.
pub fn spawn_1p(rt: &mut Runtime, s: &mut SetState) {
    let (ada14, ada0c, t) = (s.ada14, s.ada0c, &mut s.table);
    let p = match t.playertwp[0] {
        Some(id) => twp_pos(rt, id),
        None => t.fallback,
    };
    let Some(p) = p else { return };
    for i in 0..(ada14.max(0) as usize).min(ENTRIES) {
        let f = flag(rt, i);
        // Bit 15: set up; bit 0: spawned.
        if f & 0x8000 == 0 || f & 1 != 0 {
            continue;
        }
        let Some(e) = t.entries.get(i).copied() else {
            continue;
        };
        let row = e
            .row
            .map(|r| t.rows.get(r).copied().unwrap_or_default())
            .unwrap_or_default();
        let Some(d) = t.defs.get((row.id & 0x7FFF) as usize) else {
            rt.events
                .push(Event::Anomaly("SET: object id past the object list"));
            continue;
        };
        let (dist, always) = if d.flags & 1 != 0 {
            (d.distance, false)
        } else if d.flags & 2 != 0 {
            (ALWAYS_DIST, true)
        } else {
            (DEFAULT_DIST, d.flags & 4 != 0 && ada0c == 0)
        };
        // Entry bit 1 (a saved twp copy at +0xC) is never set by the setup;
        // its route is not ported.
        if f & 2 != 0 {
            rt.events
                .push(Event::Anomaly("SET: saved-twp entry route is not ported"));
            continue;
        }
        if !always && !in_range(row.pos, p, dist) {
            continue;
        }
        let Ok(task) = rt.create_elemental_task(d.im as u16, d.level as u32, d.exec, 0) else {
            continue;
        };
        set_flag(rt, i, f | 1);
        if let Some(x) = rt.task_mut(task) {
            x.set_entry = Some(i);
            if let Some(w) = x.twp.as_mut() {
                let mut put = |o: usize, v: u32| {
                    if let Some(b) = w.get_mut(o..o + 4) {
                        b.copy_from_slice(&v.to_be_bytes());
                    }
                };
                for k in 0..3 {
                    put(0x8 + 4 * k, row.rot[k] as i32 as u32);
                    put(0x14 + 4 * k, row.pos[k].to_bits());
                    put(0x20 + 4 * k, row.scale[k].to_bits());
                }
            }
        }
        if let Some(e) = t.entries.get_mut(i) {
            e.count = e.count.wrapping_add(1);
            e.keep = KEEP_MARGIN + dist;
            e.task = Some(task);
        }
    }
}

/// Natives that run the SET task's exec and pass every other address on.
pub struct WithSet<'a> {
    pub inner: &'a mut dyn Natives,
    pub set: &'a mut SetState,
}

impl Natives for WithSet<'_> {
    fn run(&mut self, rt: &mut Runtime, addr: u32, task: TaskId) -> bool {
        if addr == SET_EXEC {
            set_exec(rt, self.set, task);
            true
        } else {
            self.inner.run(rt, addr, task)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rows: &[(u16, [f32; 3], [f32; 3])]) -> Vec<u8> {
        let mut b = vec![0u8; 0x20];
        b[..4].copy_from_slice(&(rows.len() as u32).to_be_bytes());
        for (id, pos, scale) in rows {
            let mut r = vec![0u8; 0x20];
            r[..2].copy_from_slice(&id.to_be_bytes());
            r[2..4].copy_from_slice(&(-2i16).to_be_bytes());
            for k in 0..3 {
                r[8 + 4 * k..12 + 4 * k].copy_from_slice(&pos[k].to_be_bytes());
                r[20 + 4 * k..24 + 4 * k].copy_from_slice(&scale[k].to_be_bytes());
            }
            b.extend(r);
        }
        b
    }

    fn def(flags: u16, distance: f32, exec: u32) -> ObjectDef {
        ObjectDef {
            im: 2,
            level: 3,
            flags,
            distance,
            exec,
            name: String::new(),
        }
    }

    fn table() -> SetTable {
        let s = file(&[
            (0, [10.0, 0.0, 0.0], [0.0; 3]),
            (1, [5000.0, 0.0, 0.0], [0.0; 3]),
        ]);
        let u = file(&[(2, [9000.0, 0.0, 0.0], [3.0, 0.0, 0.0])]);
        SetTable {
            rows: join_files(Some(&s), Some(&u)).unwrap(),
            defs: vec![def(0x10, 0.0, 0xA), def(0x30, 0.0, 0xB), def(4, 0.0, 0xC)],
            playertwp: [None, None],
            fallback: Some([0.0; 3]),
            ..SetTable::default()
        }
    }

    #[test]
    fn join_keeps_order_and_counts_both_headers() {
        let t = table();
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[2].pos[0], 9000.0);
        assert_eq!(t.rows[0].rot[0], -2);
        assert_eq!(join_files(None, None), None);
    }

    #[test]
    fn rings_counted_by_kind() {
        let t = table();
        let mut g = vec![0u8; 0xF0];
        g[0x70..0x72].copy_from_slice(&3i16.to_be_bytes());
        // 3 carried + 1 (kind 0x10) + 0 (kind 0x30, scale.x 0)
        assert_eq!(ring_total(&t, &mut g), 4);
        assert_eq!(&g[0x6E..0x70], &4i16.to_be_bytes());
    }

    #[test]
    fn spawns_in_range_and_first_pass_only() {
        let mut rt = Runtime::new();
        let mut n = crate::Registry::new();
        let mut s = SetState {
            table: table(),
            ..SetState::default()
        };
        clear_entries(&mut rt, &mut s.table, &mut s.ada14);
        setup_entries(&mut rt, &mut s.table, &mut s.ada14);
        assert_eq!(s.ada14, 3);
        assert_eq!(rt.set_flags[3], 0x8000, "setup covers count + 1");
        let set = rt.create_elemental_task(2, 0, SET_EXEC, 0).unwrap();
        rt.task_mut(set).unwrap().twp.as_mut().unwrap()[0] = 1;
        set_exec(&mut rt, &mut s, set);
        // Row 0 is 10 units away; row 1 (5000) is out of 400; row 2 has
        // flag 4 and spawns anywhere on the first pass only.
        assert_eq!(rt.set_flags[..3], [0x8001, 0x8000, 0x8001]);
        let (a, c) = (
            s.table.entries[0].task.unwrap(),
            s.table.entries[2].task.unwrap(),
        );
        let x = rt.task(a).unwrap();
        assert_eq!(x.slots[0], 0xA);
        assert_eq!(x.set_entry, Some(0));
        assert_eq!(f32::from_bits(x.twp_word(0x14)), 10.0);
        assert_eq!(x.twp_word(0x8) as i32, -2);
        assert_eq!(s.table.entries[0].keep, 160_100.0);
        assert_eq!(s.ada0c, 1);
        // Destroying an object clears its spawned bit: row 0 respawns, row 2
        // (out of range after the first pass) does not.
        rt.destroy_task(&mut n, a);
        rt.destroy_task(&mut n, c);
        assert_eq!(rt.set_flags[..3], [0x8000, 0x8000, 0x8000]);
        set_exec(&mut rt, &mut s, set);
        assert_eq!(s.table.entries[0].count, 2);
        assert_eq!(s.table.entries[2].count, 1);
        assert_eq!(rt.set_flags[2], 0x8000);
    }
}
