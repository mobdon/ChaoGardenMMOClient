//! Chao MMO glue: SA2_COMBAT=1 with SA2_START_STAGE=60 turns Big Foot's
//! arena into a survival area. The boss is never made and the game's own
//! enemies arrive in waves, each bigger than the last.
//!
//! Everything that runs is the game's own code; this module only changes
//! what is asked of it:
//! - boss_bigfootD.rel (module 82 in the build) stage task, section 1 0x66BC:
//!   it builds the boss position from section 4 +0x500/+0x504, calls the boss
//!   constructor (section 1 +0) and stores the stage update 0x67D4 in its
//!   own task (+0x10). Here only the store is done. 0x67D4 is a call to the
//!   arena boundary 0x6724 and nothing else; here it also runs the waves.
//! - The player's start: the boss's opening (0xD90, at 0xE68..0xE8C) writes
//!   (-boss.x, 0, 0) to the player's work +0x14. Here that is written once
//!   from the same section 4 values.
//! - Enemies: the stage's object list (section 5 0x27BE8 in the Steam file,
//!   16-byte rows) has rows 33 "E KUMI" and 35 "E AI". An enemy is asked for
//!   the way a SET file asks: an entry in main.dol's SET table 0x801F5968
//!   (0x10 bytes, at most 0x5FF, count at r13-0x78AC), initialised as
//!   fn_800658EC does (flags 0x8000, +8 = the 0x20-byte SET record). The
//!   game's SET loader fn_80065C40 then creates the task from the row and
//!   copies the record's rotation, position and scale; `Dead` 0x800653E0
//!   clears 0x8000 when the enemy is destroyed.
//! - Each record's rotation and scale words are a real E KUMI / E AI record
//!   from the player's own set0013_s.bin (object ids 0x38 / 0x39 in
//!   stg13D.rel's list, section 5 0x1FF714); only the position is new: a
//!   random spot inside the arena radius (section 4 +0x508) at the arena's
//!   own height (+0x504). Spots, wave sizes and timing are MMO choices.
//!
//! unverified until run: that nothing else waits on the boss task (camera,
//! music, clear), that the arena has the enemies' textures loaded, and the
//! height E KUMI needs above the floor.
use crate::original;
use sa2_recomp::rt::{self, Cpu, Mem};
use sa2_recomp::{global, global::Global};

const OS_LINK: u32 = 0x800E_839C;
const BIGFOOT: u32 = 82;
const STAGE: u16 = 60;
const STAGE_TASK: u32 = 0x66BC;
const STAGE_EXEC: u32 = 0x67D4;
const ARENA_BOUND: u32 = 0x6724;
/// Section 4: the boss's x, its y and z (one float), the arena radius.
const BOSS_X: u32 = 0x500;
const BOSS_YZ: u32 = 0x504;
const RADIUS: u32 = 0x508;
const PLAYERTWP: u32 = 0x801E_7768;
const SET_TABLE: u32 = 0x801F_5968;
const SET_MAX: u32 = 0x5FF;
/// r13-relative: the SET entry count (s16) and the current object list
/// (its +4 is the rows).
const SET_COUNT: u32 = 0x78AC;
const OBJECT_LIST: u32 = 0x7894;
const VALID: u16 = 0x8000;
/// (object id in set0013_s.bin, row in Big Foot's list, the row's name).
const KINDS: [(u16, u16, &str); 2] = [(0x38, 33, "E KUMI"), (0x39, 35, "E AI")];
const SOURCE_SET: &str = "set0013_s.bin";
/// The wave records: raised MEM1 between chao_player's tables (end below
/// 0x82F0_2A00) and chao_cache (0x82F0_4000). Two halves, used by alternate
/// waves, so a record is rewritten only after a whole later wave is gone.
const RECORDS: u32 = 0x82F0_2A00;
const HALF: usize = 88;

struct State {
    placed: bool,
    /// The player's last position, logged when the stage starts over.
    at: [f32; 3],
    /// Frames since the stage (re)started.
    frames: u32,
    wave: usize,
    /// The SET table entry each record slot owns.
    slots: [Option<u32>; 2 * HALF],
    /// The table's count after the last wave; lower means the game reset it.
    count: u32,
    rng: u32,
    /// Real enemy records: (Big Foot row, the SET record).
    pool: Option<Vec<(u16, [u8; 0x20])>>,
}

static STATE: Global<State> = global!(State {
    placed: false,
    at: [0.0; 3],
    frames: 0,
    wave: 0,
    slots: [None; 2 * HALF],
    count: 0,
    rng: 0x2545_F491,
    pool: None,
});

/// Where Big Foot's section 1 was linked when the overrides were set.
static TEXT: Global<u32> = global!(0);

/// The combat area was asked for (with its stage).
pub fn wanted(stage: u16) -> bool {
    stage == STAGE && std::env::var("SA2_COMBAT").is_ok_and(|v| v.trim() == "1")
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    let stage = std::env::var("SA2_START_STAGE").ok().and_then(|v| v.trim().parse().ok());
    if !stage.is_some_and(wanted) {
        return;
    }
    if rt::mem1() | 0x8000_0000 < RECORDS + (2 * HALF as u32) * 0x20 {
        eprintln!("combat: needs SA2_MEM1_MB >= 48; the boss fight runs as usual");
        return;
    }
    c.rt.set_override(OS_LINK, os_link);
}

/// OSLink(module, bss): once Big Foot's module is linked, its stage task and
/// stage update are replaced.
fn os_link(c: &mut Cpu, m: &mut Mem) {
    let module = c.r[3];
    original(c, m, OS_LINK);
    if c.r[3] != 0 && m.r32(module) == BIGFOOT {
        let s1 = rt::sec(m, BIGFOOT, 1);
        TEXT.set(s1);
        eprintln!("combat: Big Foot linked, section 1 at {s1:#x}");
        c.rt.set_override(s1 + STAGE_TASK, stage_task);
        c.rt.set_override(s1 + STAGE_EXEC, stage_exec);
    }
}

/// Big Foot's module is no longer the one linked at [`TEXT`] (the game went
/// on to another stage): drop both overrides and run what is at `addr` now.
fn gone(c: &mut Cpu, m: &mut Mem, addr: u32) -> bool {
    let text = TEXT.get();
    let here = std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .any(|i| m.r32(i) == BIGFOOT && m.r32(m.r32(i + 0x10) + 8) & !1 == text);
    if !here {
        c.rt.overrides.remove(&(text + STAGE_TASK));
        c.rt.overrides.remove(&(text + STAGE_EXEC));
        c.rt.modules_changed();
        rt::call(c, m, text + addr);
    }
    !here
}

/// 0x66BC without the boss constructor: only the stage update is installed.
fn stage_task(c: &mut Cpu, m: &mut Mem) {
    if gone(c, m, STAGE_TASK) {
        return;
    }
    let exec = rt::sec(m, BIGFOOT, 1) + STAGE_EXEC;
    m.w32(c.r[3] + 0x10, exec);
    let (at, frames) = STATE.with_borrow(|s| (s.at, s.frames));
    eprintln!("combat: stage task {:#x}, no boss, stage update {exec:#x}, after {frames} frames, player was at {at:?}", c.r[3]);
    // The game runs this again on every restart after a death, with its SET
    // table kept: the entries stay owned, the waves start over.
    STATE.with_borrow_mut(|s| (s.placed, s.wave, s.frames) = (false, first_wave(), 0));
}

/// 0x67D4: the waves, then the arena boundary as the game calls it.
fn stage_exec(c: &mut Cpu, m: &mut Mem) {
    if gone(c, m, STAGE_EXEC) {
        return;
    }
    if m.r32(PLAYERTWP) != 0 {
        let s4 = rt::sec(m, BIGFOOT, 4);
        let (x, yz, radius) = (m.rf32(s4 + BOSS_X), m.rf32(s4 + BOSS_YZ), m.rf32(s4 + RADIUS));
        let r13 = c.r[13];
        STATE.with_borrow_mut(|s| {
            let pos = m.r32(PLAYERTWP) + 0x14;
            s.at = [0, 4, 8].map(|o| m.rf32(pos + o) as f32);
            s.frames += 1;
            // The player's path through the first seconds of each try (frame
            // 1 is where the game itself put the player).
            if s.frames <= 600 && (s.frames <= 3 || s.frames % 60 == 0) {
                eprintln!("combat: frame {}: player at {:?}", s.frames, s.at);
            }
            if !s.placed {
                s.placed = true;
                m.wf32(pos, -x);
                m.wf32(pos + 4, yz);
                m.wf32(pos + 8, yz);
            }
            if s.pool.is_none() {
                let pool = crate::dvd::read_file(SOURCE_SET).map(|f| pool(&f)).unwrap_or_default();
                if pool.is_empty() {
                    eprintln!("combat: no E KUMI / E AI records in {SOURCE_SET}; no waves");
                }
                s.pool = Some(pool);
            }
            waves(s, m, r13, -x as f32, yz as f32, radius as f32);
        });
    }
    let bound = rt::sec(m, BIGFOOT, 1) + ARENA_BOUND;
    rt::call(c, m, bound);
}

/// The E KUMI / E AI records of a SET file (count at 0, 0x20-byte records
/// from 0x20), each with the Big Foot row that makes the same enemy.
fn pool(set: &[u8]) -> Vec<(u16, [u8; 0x20])> {
    let count = set.get(..4).map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap())) as usize;
    set.get(0x20..)
        .unwrap_or_default()
        .chunks_exact(0x20)
        .take(count)
        .filter_map(|r| {
            let id = u16::from_be_bytes([r[0], r[1]]) & 0xFFF;
            let &(_, row, _) = KINDS.iter().find(|k| k.0 == id)?;
            Some((row, r.try_into().unwrap()))
        })
        .collect()
}

/// SA2_COMBAT_WAVE=n starts at wave n (a headless run cannot clear a wave,
/// so this is how a bigger wave gets looked at); the count before it.
fn first_wave() -> usize {
    let n = std::env::var("SA2_COMBAT_WAVE").ok().and_then(|v| v.trim().parse().ok());
    n.unwrap_or(1usize).clamp(1, HALF) - 1
}

/// Wave n has 2n enemies, up to one half of the records.
fn wave_size(wave: usize) -> usize {
    (2 * wave).min(HALF)
}

fn rand(s: &mut u32) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    (*s >> 8) as f32 / (1u32 << 24) as f32
}

/// Start the next wave once every enemy of the last one is destroyed.
fn waves(s: &mut State, m: &mut Mem, r13: u32, start_x: f32, y: f32, radius: f32) {
    let count_at = r13.wrapping_sub(SET_COUNT);
    let mut count = u32::from(m.r16(count_at));
    if count < s.count {
        // The game emptied its SET table (a restart): the entries are gone.
        (s.slots, s.wave) = ([None; 2 * HALF], first_wave());
    }
    s.count = count;
    let entry = |i: u32| SET_TABLE + i * 0x10;
    if s.slots.iter().flatten().any(|&i| m.r16(entry(i) + 2) & VALID != 0) {
        return;
    }
    let Some(pool) = s.pool.as_ref().filter(|p| !p.is_empty()) else {
        return;
    };
    let rows = m.r32(m.r32(r13.wrapping_sub(OBJECT_LIST)) + 4);
    s.wave += 1;
    let first = (s.wave % 2) * HALF;
    for slot in first..first + wave_size(s.wave) {
        let (row, mut rec) = pool[(rand(&mut s.rng) * pool.len() as f32) as usize % pool.len()];
        let name = crate::cstr(m, m.r32(rows + u32::from(row) * 0x10 + 0xC));
        if KINDS.iter().all(|k| (k.1, k.2) != (row, name.as_str())) {
            eprintln!("combat: object list row {row} is {name:?}; enemy not made");
            continue;
        }
        let (x, z) = loop {
            let (x, z) = (rand(&mut s.rng) * 2.0 - 1.0, rand(&mut s.rng) * 2.0 - 1.0);
            let (x, z) = (x * radius, z * radius);
            // Inside the arena, and not within half its radius of where the
            // player starts (an MMO choice: no enemy on top of the start).
            if x * x + z * z <= radius * radius && (x - start_x).powi(2) + (z - y).powi(2) >= radius * radius / 4.0 {
                break (x, z);
            }
        };
        rec[..2].copy_from_slice(&row.to_be_bytes());
        for (i, v) in [x, y, z].into_iter().enumerate() {
            rec[8 + 4 * i..12 + 4 * i].copy_from_slice(&v.to_be_bytes());
        }
        let i = match s.slots[slot] {
            Some(i) => i,
            None if count < SET_MAX => {
                count += 1;
                s.slots[slot] = Some(count - 1);
                count - 1
            }
            None => break,
        };
        let at = RECORDS + slot as u32 * 0x20;
        m.load(at, &rec);
        // A fresh entry, as fn_800658EC leaves one.
        let e = entry(i);
        m.w32(e, u32::from(VALID));
        m.w32(e + 4, 0);
        m.w32(e + 8, at);
        m.w32(e + 0xC, 0);
    }
    m.w16(count_at, count as u16);
    eprintln!("combat: wave {}: {} enemies, SET entries {} -> {count}, object rows {rows:#x}", s.wave, wave_size(s.wave), s.count);
    s.count = count;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic SET bytes (test input only, not game data).
    fn set() -> Vec<u8> {
        let mut f = vec![0u8; 0x20 + 3 * 0x20];
        f[3] = 3;
        for (n, id) in [0x8038u16, 0x0006, 0x0039].into_iter().enumerate() {
            let at = 0x20 + n * 0x20;
            f[at..at + 2].copy_from_slice(&id.to_be_bytes());
            f[at + 0x1F] = n as u8 + 1;
        }
        f
    }

    #[test]
    fn combat_pool_keeps_only_the_enemy_records() {
        let p = pool(&set());
        assert_eq!(p.iter().map(|r| (r.0, r.1[0x1F])).collect::<Vec<_>>(), [(33, 1), (35, 3)]);
        assert!(pool(&[]).is_empty());
    }

    #[test]
    fn combat_waves_grow_and_wait_for_the_last_one() {
        const R13: u32 = 0x8030_0000;
        const ROWS: u32 = 0x8040_0000;
        let mut m = Mem::default();
        m.w32(R13 - OBJECT_LIST, ROWS - 0x100);
        m.w32(ROWS - 0x100 + 4, ROWS);
        for (_, row, name) in KINDS {
            let at = ROWS + 0x1000 + u32::from(row) * 0x10;
            m.load(at, name.as_bytes());
            m.w32(ROWS + u32::from(row) * 0x10 + 0xC, at);
        }
        m.w16(R13 - SET_COUNT, 23);
        let mut s = State { placed: true, at: [0.0; 3], frames: 0, wave: 0, slots: [None; 2 * HALF], count: 0, rng: 1, pool: Some(pool(&set())) };
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (1, 25));
        for i in 23..25 {
            let e = SET_TABLE + i * 0x10;
            let rec = m.r32(e + 8);
            assert_eq!(m.r32(e), 0x8000);
            assert!([33, 35].contains(&m.r16(rec)));
            let (x, z) = (m.rf32(rec + 8), m.rf32(rec + 16));
            assert!(x * x + z * z <= 120.0 * 120.0 && m.rf32(rec + 12) == 0.0);
            assert!((x + 90.0).powi(2) + z * z >= 60.0 * 60.0);
        }
        // Nothing new while one enemy is left.
        m.w16(SET_TABLE + 23 * 0x10 + 2, 0);
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (1, 25));
        // All destroyed: wave 2 has four, in the other half of the records.
        m.w16(SET_TABLE + 24 * 0x10 + 2, 0);
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (2, 29));
        assert_eq!(m.r32(SET_TABLE + 25 * 0x10 + 8), RECORDS);
        // Wave 3 takes wave 1's entries again instead of new ones.
        for i in 25..29 {
            m.w16(SET_TABLE + i * 0x10 + 2, 0);
        }
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (3, 33));
        assert_eq!(m.r16(SET_TABLE + 23 * 0x10 + 2), VALID);
        // A restart (the stage task runs again) keeps the entries: wave 1
        // waits for what is left, then reuses them instead of adding more.
        s.wave = 0;
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (0, 33));
        for i in [23, 24, 29, 30, 31, 32] {
            m.w16(SET_TABLE + i * 0x10 + 2, 0);
        }
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (1, 33));
        // The game emptied its table: start over from wave 1.
        m.w16(R13 - SET_COUNT, 0);
        waves(&mut s, &mut m, R13, -90.0, 0.0, 120.0);
        assert_eq!((s.wave, m.r16(R13 - SET_COUNT)), (1, 2));
        assert_eq!(wave_size(1000), HALF);
    }

    #[test]
    fn combat_stage_task_installs_only_the_stage_update() {
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        // Module 82 in the module list, section 1 at 0x8100_0000.
        m.w32(rt::OS_MODULE_HEAD, 0x8040_0000);
        m.w32(0x8040_0000, BIGFOOT);
        m.w32(0x8040_0010, 0x8040_0200);
        m.w32(0x8040_0208, 0x8100_0001);
        TEXT.set(0x8100_0000);
        c.r[3] = 0x8050_0000;
        stage_task(&mut c, &mut m);
        assert_eq!(m.r32(0x8050_0010), 0x8100_0000 + STAGE_EXEC);
        // Another module at that address later: the override steps aside
        // and what is linked there runs.
        fn other(c: &mut Cpu, _: &mut Mem) {
            c.r[3] = 7;
        }
        m.w32(0x8040_0000, 13);
        c.rt.set_override(0x8100_0000 + STAGE_TASK, stage_task);
        c.rt.generated = |a| (a == 0x8100_0000 + STAGE_TASK).then_some(other as rt::Func);
        m.w32(0x8050_0010, 0);
        stage_task(&mut c, &mut m);
        assert_eq!((c.r[3], m.r32(0x8050_0010)), (7, 0));
    }
}
