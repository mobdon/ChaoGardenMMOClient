//! Stage 42's registered setup (stg42D.rel +0x2CF0) and teardown (+0x2F88)
//! with the module helpers they call: the rival title setup (+0x18AC and
//! +0x20470), the light loader (+0x352C, the same algorithm as
//! `stage_lighting`), the fog loader (+0xAFEC, +0xA9D0, +0xA988, +0xAF64) and
//! the message banks (+0x14278, +0xE6E4, +0xE804, +0x141F4). Engine results
//! (returns, loaded file contents, prior memory) are inputs. The output is the
//! ordered engine calls and the final value of every location the module
//! writes. Tasks it creates are recorded with their callbacks, not executed.
use crate::{duel_platform, stage_lighting, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn s(section: u8, offset: u32) -> String {
    format!("s{section}+0x{offset:x}")
}

struct Data<'a> {
    rel: crate::city_rel::CityRel<'a>,
}
impl Data<'_> {
    fn s5(&self, pc: u32) -> Result<u32> {
        duel_platform::target(&self.rel, pc, 5)
    }
    fn s4(&self, pc: u32) -> Result<u32> {
        duel_platform::target(&self.rel, pc, 4)
    }
    fn f4(&self, pc: u32) -> Result<u32> {
        Ok(duel_platform::float(&self.rel, pc)?.to_bits())
    }
    fn word5(&self, at: u32) -> Result<u32> {
        let d = self.rel.data(5)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    }
    fn byte5(&self, at: i64) -> Result<u8> {
        usize::try_from(at)
            .ok()
            .and_then(|a| self.rel.data(5).ok()?.get(a).copied())
            .ok_or_else(duel_platform::invalid)
    }
    fn cstr(&self, at: u32) -> Result<String> {
        let d = self.rel.data(5)?;
        let tail = d.get(at as usize..).ok_or_else(duel_platform::invalid)?;
        let end = tail
            .iter()
            .take(64)
            .position(|b| *b == 0)
            .ok_or_else(duel_platform::invalid)?;
        String::from_utf8(tail[..end].to_vec()).map_err(|_| duel_platform::invalid())
    }
}

#[derive(Default)]
struct Script {
    calls: Vec<Value>,
    state: BTreeMap<String, Value>,
}
impl Script {
    fn call(&mut self, address: u32, args: Value) {
        self.calls
            .push(json!({ "engine": format!("0x{address:08x}"), "args": args }));
    }
    fn write(&mut self, at: impl Into<String>, value: Value) {
        self.state.insert(at.into(), value);
    }
    fn finish(self) -> Value {
        json!({ "calls": self.calls, "state": self.state })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetupInputs {
    /// Returns of the first and (if reached) second 0x8001DD24(0) calls.
    pub mode_queries: Vec<i32>,
    /// 0x801E7388 slot 1 is filled; then player data slot 1 byte +3.
    pub rival_character: Option<i8>,
    /// Engine task allocator results in call order (true: non-null).
    pub tasks: Vec<bool>,
    /// 0x801194B4 results in call order.
    pub allocations: Vec<bool>,
    /// 0x80064F84 (SET load) return.
    pub set_handle: u32,
    /// 0x801E5470 after the base light load: the 384 bytes the derive reads.
    pub light_source_memory: Vec<u8>,
    /// Returns of the base and `_gc` light file loads.
    pub light_loads: [i32; 2],
    /// Return of the fog file load (zero is success).
    pub fog_load: i32,
    /// Module state before setup: fog manager task and message banks.
    pub fog_task: u32,
    pub message_banks: [u32; 2],
    /// Game byte +0x11 (index into the module's language letters).
    pub language: i8,
    /// Message files the engine finds, by name: load address and words
    /// (through the 0xFFFFFFFF terminator).
    pub message_files: BTreeMap<String, (u32, Vec<u32>)>,
    /// Prior contents of the engine callback slots at 0x801A2638 and +8.
    pub callback_slots: [u32; 2],
}

fn take(v: &mut std::slice::Iter<'_, bool>, what: &str) -> Result<bool> {
    v.next()
        .copied()
        .ok_or_else(|| Error::invalid(format!("missing {what} result")))
}

pub fn setup(rel_bytes: &[u8], i: &SetupInputs) -> Result<Value> {
    let d = Data {
        rel: duel_platform::module(rel_bytes)?,
    };
    let mut sc = Script::default();
    let mut tasks = i.tasks.iter();
    let mut allocs = i.allocations.iter();
    let mut modes = i.mode_queries.iter().copied();
    let mut mode = |sc: &mut Script| -> Result<i32> {
        let v = modes
            .next()
            .ok_or_else(|| Error::invalid("missing mode query result"))?;
        sc.call(0x8001dd24, json!([0, v]));
        Ok(v)
    };
    if mode(&mut sc)? == 0 {
        sc.call(0x80086bd4, json!([1]));
    } else if mode(&mut sc)? == 1 {
        sc.call(0x80087258, json!([1]));
    }
    if let Some(character) = i.rival_character {
        sc.call(0x8001d6c8, json!([1]));
        sc.write("player1[1]+0x2", json!(3));
        sc.call(0x80022bdc, json!([1]));
        // +0x18AC
        let made = take(&mut tasks, "task")?;
        sc.call(
            0x80071ba4,
            json!([0, 0, s(1, 0x3cc), d.cstr(d.s5(0x18da)?)?, made]),
        );
        if made {
            let got = take(&mut allocs, "allocation")?;
            sc.call(0x801194b4, json!([1, 0x20, got]));
            if !got {
                sc.call(0x80071878, json!(["task0"]));
            } else {
                sc.write("task0+0x40", json!("alloc0"));
                let gauge = d.f4(0x191a)?;
                sc.write("player2[1]+0x48", json!(gauge));
                // +0x20470 with a parent adds a child task (no null check).
                sc.call(0x80070790, json!([2, s(1, 0x2052c)]));
                sc.write("child.work+0x8", json!("player2[1]+0x48"));
                sc.write("child.work+0x28", json!(gauge));
                sc.write("child.work+0x14", json!(d.f4(0x204da)?));
                sc.write("child+0x1c", json!(s(1, 0x20700)));
                sc.write("child+0x18", json!(s(1, 0x20528)));
                let table = d.s5(0x20502)?;
                sc.write("child.work+0x1c", json!(d.word5(table + 4)?));
                sc.write("child.work+0x24", json!(d.word5(table + 8)?));
                sc.write("alloc0+0x0", json!(0));
                let titles = match character {
                    0 => Some((0x1966, 0x196e, 0x185c)),
                    1 => Some((0x1992, 0x199a, 0x1884)),
                    _ => None,
                };
                if let Some((tex, list, delete)) = titles {
                    sc.call(0x800725b8, json!([d.cstr(d.s5(tex)?)?, s(5, d.s5(list)?)]));
                    sc.write("task0+0x18", json!(s(1, delete)));
                }
            }
        }
    }
    for (a, b) in [(0x2d7a, 0x2d86), (0x2da2, 0x2dae)] {
        let (a, b) = (s(5, d.s5(a)?), s(5, d.s5(b)?));
        sc.call(0x80072008, json!([a, b]));
        sc.call(0x80071fb0, json!([a, b]));
    }
    sc.call(
        0x80064f84,
        json!([
            d.cstr(d.s5(0x2dca)?)?,
            d.cstr(d.s5(0x2dd2)?)?,
            0x200,
            i.set_handle
        ]),
    );
    sc.write("s6+0x0", json!(i.set_handle));
    sc.call(0x80066124, json!([s(5, d.s5(0x2dee)?), i.set_handle]));
    sc.call(0x8001a8a8, json!([]));
    sc.call(0x800334a8, json!([]));
    let made = take(&mut tasks, "task")?;
    sc.call(
        0x80071ba4,
        json!([2, 1, s(1, 0x3128), d.cstr(d.s5(0x2e16)?)?, made]),
    );
    if made {
        sc.write("task1+0x14", json!(s(1, 0x312c)));
    }
    // +0x352C
    let name = d.cstr(d.s5(0x2e36)?)?;
    let suffix = d.cstr(d.s5(0x3586)?)?;
    let gc = match name.rfind('.') {
        Some(p) => format!("{}{suffix}", &name[..p]),
        None => format!("{name}{suffix}"),
    };
    sc.write("0x802b6c20", json!(name));
    sc.call(0x8000532c, json!(["0x801e5470", 0, 0x20]));
    sc.call(0x8001a13c, json!([name, "0x80fffe60", i.light_loads[0]]));
    if i.light_loads[0] > 0 {
        sc.call(
            0x801193a4,
            json!(["0x801e5470", "0x80fffe60", i.light_loads[0]]),
        );
    }
    sc.call(0x8000532c, json!(["0x801e5230", 0, 0x30]));
    for (k, row) in stage_lighting::derive(&i.light_source_memory)?
        .iter()
        .enumerate()
    {
        sc.write(format!("0x{:08x}", 0x801e5230 + 0x30 * k), json!(row));
    }
    sc.call(0x8001a13c, json!([gc, "0x80fffe60", i.light_loads[1]]));
    if i.light_loads[1] > 0 {
        sc.call(
            0x801193a4,
            json!(["0x801e5230", "0x80fffe60", i.light_loads[1]]),
        );
    }
    // +0xAFEC
    if i.fog_task == 0 {
        sc.write("s6+0x644", json!(s(6, 0xc)));
        sc.write("s6+0x648", json!(0));
        sc.write("0x803ad7c8", json!(s(6, 0x644)));
        let fog = d.cstr(d.s5(0x2e42)?)?;
        sc.write("0x802b6c00", json!(fog));
        sc.call(0x8001a16c, json!([fog, "0x80fffe60", i.fog_load]));
        if i.fog_load == 0 {
            sc.call(0x801193a4, json!([s(6, 0xc), "0x80fffe60", 0x210]));
        } else {
            sc.call(0x80119430, json!([s(6, 0xc), 0, 0x210]));
            sc.write("s6+0x14", json!(d.f4(0xa9b2)?));
        }
        sc.call(0x8011bf74, json!([]));
        sc.call(0x8011bfc8, json!([]));
        let made = take(&mut tasks, "task")?;
        sc.call(
            0x80071ba4,
            json!([0, 1, s(1, 0xae04), d.cstr(d.s5(0xb06a)?)?, made]),
        );
        sc.write("s6+0x654", if made { json!("task2") } else { json!(0) });
        if made {
            sc.write("task2+0x18", json!(s(1, 0xae94)));
        }
        sc.write("s6+0x650", json!(0));
    }
    sc.call(0x8011bf68, json!([]));
    sc.call(0x8011bfc8, json!([]));
    // +0x14278
    if i.message_banks == [0, 0] {
        let letters = d.s5(0xe716)?;
        let ext = d.cstr(d.s5(0xe74e)?)?;
        let letter = |k: i64| -> Result<String> {
            let c = d.byte5(letters as i64 + k)?;
            Ok(if c == 0 {
                String::new()
            } else {
                (c as char).to_string()
            })
        };
        let mut banks = [0u32; 2];
        for (bank, (name_pc, count_pc, slot)) in [
            (0x2e5e_u32, 0x142ca_u32, 0x698u32),
            (0x142de, 0x142e6, 0x69c),
        ]
        .into_iter()
        .enumerate()
        {
            let base = d.cstr(d.s5(name_pc)?)?;
            let count_slot = d.s5(count_pc)?;
            for candidate in [
                format!("{base}{}{ext}", letter(i.language as i64)?),
                format!("{base}{}{ext}", letter(0)?),
                format!("{base}{ext}"),
            ] {
                let Some((at, words)) = i.message_files.get(&candidate) else {
                    sc.call(0x8001a190, json!([candidate, 0]));
                    continue;
                };
                sc.call(0x8001a190, json!([candidate, at]));
                let end = words
                    .iter()
                    .position(|w| *w == 0xffff_ffff)
                    .ok_or_else(|| Error::invalid("message file has no terminator"))?;
                let relocated: Vec<u32> = words[..end]
                    .iter()
                    .map(|w| if *w != 0 { w.wrapping_add(*at) } else { 0 })
                    .collect();
                sc.write(format!("message@0x{at:08x}"), json!(relocated));
                sc.write(s(5, count_slot), json!(end));
                banks[bank] = *at;
                break;
            }
            sc.write(s(6, slot), json!(banks[bank]));
        }
        if banks.contains(&0) {
            free_messages(&d, &mut sc, banks)?;
        }
    }
    sc.call(
        0x80117844,
        json!([0xff00_1020u32, 0xff00_1020u32, 0xff00_1020u32]),
    );
    sc.call(0x8003ba5c, json!([s(5, d.s5(0x2e7e)?)]));
    if i.callback_slots[0] == 0 {
        sc.write("0x801a2638", json!(s(1, 0x19d70)));
    }
    if i.callback_slots[1] == 0 {
        sc.write("0x801a2640", json!(s(1, 0x9fb8)));
    }
    let mut table = vec![
        json!("0x800dfe70"),
        json!(s(1, 0x635c)),
        json!(s(1, 0x6e50)),
        json!(s(1, 0x6ea4)),
    ];
    table.extend((0..10).map(|_| json!(0)));
    sc.write("0x801e73d8", json!(table));
    sc.call(
        0x8006d298,
        json!([d.cstr(d.s5(0x2f32)?)?, s(4, d.s4(0x2f3a)?)]),
    );
    sc.call(0x80011a04, json!([0]));
    sc.call(0x80011050, json!([d.cstr(d.s5(0x2f4e)?)?]));
    sc.call(0x80112278, json!([d.f4(0x2f5a)?, d.f4(0x2f66)?]));
    Ok(sc.finish())
}

/// +0x141F4: free whichever message bank is loaded.
fn free_messages(d: &Data<'_>, sc: &mut Script, banks: [u32; 2]) -> Result<()> {
    for (bank, (slot, count_pc)) in [(0x698u32, 0x1422e_u32), (0x69c, 0x1425e)]
        .into_iter()
        .enumerate()
    {
        if banks[bank] != 0 {
            sc.call(0x801194e0, json!([banks[bank]]));
            sc.write(s(6, slot), json!(0));
            sc.write(s(5, d.s5(count_pc)?), json!(-1));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TeardownInputs {
    pub message_banks: [u32; 2],
    /// Current contents of 0x801A2638 and +8 (a module address as "s1+0x..").
    pub callback_slots: [String; 2],
    pub fog_task: u32,
    pub set_handle: u32,
}

pub fn teardown(rel_bytes: &[u8], i: &TeardownInputs) -> Result<Value> {
    let d = Data {
        rel: duel_platform::module(rel_bytes)?,
    };
    let mut sc = Script::default();
    free_messages(&d, &mut sc, i.message_banks)?;
    if i.callback_slots[0] == s(1, 0x19d70) {
        sc.write("0x801a2638", json!(0));
    }
    if i.callback_slots[1] == s(1, 0x9fb8) {
        sc.write("0x801a2640", json!(0));
    }
    // +0xAF64
    if i.fog_task != 0 {
        sc.call(0x80071888, json!([i.fog_task]));
        sc.write("s6+0x654", json!(0));
        sc.write("0x803ad7c8", json!(0));
    }
    sc.call(0x8011bfc0, json!([0]));
    sc.call(0x8011bfb4, json!([d.f4(0xafae)?, d.f4(0xafba)?]));
    sc.call(0x8011bf80, json!([-1]));
    for a in [
        0x8011bf74, 0x8011bf74, 0x8011bfc8, 0x8011bf74, 0x8011bfc8, 0x80033474,
    ] {
        sc.call(a, json!([]));
    }
    for (a, b) in [(0x2ffe, 0x3006), (0x3012, 0x301a)] {
        sc.call(0x80071eb0, json!([s(5, d.s5(a)?), s(5, d.s5(b)?)]));
    }
    sc.call(0x800660fc, json!([]));
    if i.set_handle != 0 {
        sc.call(0x8001a2b0, json!([i.set_handle]));
        sc.write("s6+0x0", json!(0));
    }
    sc.call(0x80024370, json!([]));
    sc.call(0x80112278, json!([d.f4(0x304e)?, d.f4(0x305a)?]));
    Ok(sc.finish())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "call")]
enum Request {
    Setup(SetupInputs),
    Teardown(TeardownInputs),
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("stage setup request: {e}")))?;
    match &req {
        Request::Setup(i) => setup(rel_bytes, i),
        Request::Teardown(i) => teardown(rel_bytes, i),
    }
}
