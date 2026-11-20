//! Stage load (milestone E4 part 2; ENGINE-MAP.md 4.1 and 4.2):
//! `fn_8001EB80` (stage module load) and Iron Gate's subprogram init
//! (stg11D fn_11_A434) with the tasks it creates: the stage task fn_11_AE18,
//! death zones (`fn_8003BCC4`), fn_11_B484, fn_11_16DE0, the SET task
//! (`fn_8006638C`), the object-list copy fn_11_A230 and the stage callback
//! tables.
//!
//! stg11D.rel's code is ported here, not loaded: REL addresses (callbacks,
//! data arguments) are written as the module's section base plus the offset,
//! with the bases in [`Stg11Layout`]. File loaders (landtable and lights, SET
//! files, light and texture lists) and the other engine routines are logged as
//! [`Event::Engine`] and not run; while the SET file loader is not ported it
//! returns null and the SET task starts with no rows, as main.dol does for a
//! null SET.
use crate::camera::camera_init;
use crate::player::{load_players, Ptr};
use crate::session::{Session, SessionHost};
use crate::task::{Event, Natives, Runtime, Slot, TaskId};

/// Where stg11D's sections sit (the port's choice of base; main.dol's REL
/// loader picks its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stg11Layout {
    pub text: u32,
    pub rodata: u32,
    pub data: u32,
    pub bss: u32,
}

/// What the stage load needs besides the session host.
pub trait StageHost: SessionHost {
    /// stg11D's object-list entries at .data+0x5B344: (index, value, pointer),
    /// up to the -1 terminator.
    fn object_list(&mut self) -> Vec<(i16, i16, u32)>;
}

/// SET globals `fn_8006638C` sets (0x803ADA08..0x803ADA2C, 0x803AD468).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetState {
    pub ada08: u32,
    pub ad468: u32,
    pub ada0c: u32,
    /// 0x803ADA10: the SET task.
    pub task: Option<TaskId>,
    /// 0x803ADA14.
    pub ada14: i16,
    pub ada18: u32,
    pub ada1c: u32,
    /// 0x803ADA20: SET rows (+0x20 of the file), 0 without SET data.
    pub rows: u32,
    /// 0x803ADA24: SET file.
    pub file: u32,
    /// 0x803ADA28: SET file argument.
    pub file_arg: u32,
    /// 0x803ADA2C: object list.
    pub object_list: u32,
    /// The 0x600-entry table at 0x801F5968: entries 0..0x5FE cleared.
    pub entries_cleared: bool,
}

/// The stage globals the load writes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageState {
    pub layout: Stg11Layout,
    /// 0x803AD81E, 0x803AD824.
    pub b81e: i16,
    pub b824: i16,
    /// 0x801E5870 (0x180 bytes) and 0x801E5630 (0x240 bytes) cleared.
    pub cleared_5870: bool,
    pub cleared_5630: bool,
    pub set: SetState,
    /// 0x801E5D28: 300 object-list entries (+2 value, +4 pointer).
    pub object_table: Vec<(i16, u32)>,
    /// 0x801A2A18: 12 stage callback words.
    pub callbacks: [u32; 12],
    /// 0x801E77D8: 14 words.
    pub table_77d8: [u32; 14],
    pub adc30: u32,
    pub ad718: u32,
    /// 0x803AD9D0 (death-zone task), 0x803AD9D4 (its data).
    pub death_task: Option<TaskId>,
    pub death_data: u32,
    /// stg11D .bss+0x20, +0x24, +0x4D0 and .data+0x5E534 (2 slots).
    pub bss20: Option<TaskId>,
    pub bss24: u32,
    pub bss4d0: Option<TaskId>,
    pub slots_5e534: [Option<TaskId>; 2],
}

impl StageState {
    /// A stage state for stg11D at `layout`, with the object table empty.
    pub fn new(layout: Stg11Layout) -> Self {
        // 0x803AD468 is 1 in main.dol's .sdata.
        let set = SetState {
            ad468: 1,
            ..SetState::default()
        };
        Self {
            layout,
            set,
            object_table: vec![(0, 0); 300],
            ..Self::default()
        }
    }
}

const STAGE_CALLS: [u32; 2] = [0x8011_C2C8, 0x8011_C2A4];
const FOG_GENERATE: u32 = 0x8011_F524;
const FOG_SET: u32 = 0x8011_F550;
const BGM_SET_VOLUME: u32 = 0x8001_1A04;
const STG_PLAY_MUSIC: u32 = 0x8001_1050;
const FOG_TABLE: u32 = 0x8029_8D10;

fn word(rt: &mut Runtime, t: TaskId, kind: u32, off: usize, v: u32) {
    if let Some(x) = rt.task_mut(t) {
        let w = match kind {
            0x34 => x.twp.as_mut(),
            0x38 => x.w38.as_mut(),
            _ => x.w40.as_mut(),
        };
        if let Some(w) = w.and_then(|w| w.get_mut(off..off + 4)) {
            w.copy_from_slice(&v.to_be_bytes());
        }
    }
}

/// `fn_8001EB80`: load the stage module and run its init.
pub fn stage_load(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    s: &mut Session,
    st: &mut StageState,
    host: &mut dyn StageHost,
) {
    let stage = s.players.stage;
    st.b81e = 0;
    st.b824 = stage;
    rt.engine(STAGE_CALLS[0], vec![0]);
    rt.engine(FOG_GENERATE, vec![FOG_TABLE]);
    rt.engine(FOG_SET, vec![FOG_TABLE]);
    rt.engine(STAGE_CALLS[1], vec![]);
    let c = s.players.chars[0];
    let swap = if stage == 0x5A {
        match c {
            7 => Some(3),
            6 => Some(2),
            _ => None,
        }
    } else if (3..=0x3C).contains(&stage) || stage == 0x46 {
        match c {
            3 => Some(7),
            2 => Some(6),
            _ => None,
        }
    } else {
        None
    };
    if let Some(new) = swap {
        match s.players.tp[0] {
            Some(Ptr::Task(t)) => rt.destroy_task(n, t),
            _ => rt
                .events
                .push(Event::Anomaly("stage load: player 0 task is null")),
        }
        s.players.chars[0] = new;
        load_players(rt, &mut s.players, host);
    }
    st.cleared_5870 = true;
    st.cleared_5630 = true;
    rt.engine(0x8006_7FF4, vec![]);
    rt.engine(0x8001_A3BC, vec![]);
    if stage == 11 {
        stg11_init(rt, n, st, &mut s.players.allocs, host);
    } else {
        rt.engine(0x8001_9E18, vec![]);
    }
    rt.engine(0x8001_EDFC, vec![]);
    if let Some(b) = s.players.g.get_mut(0x16) {
        *b = 1;
    }
    rt.engine(0x8001_DDD8, vec![]);
    let n = s.camera.viewports;
    camera_init(rt, &mut s.camera, stage, n);
    rt.engine(0x8002_6490, vec![]);
}

/// stg11D fn_11_A434: Iron Gate's init.
pub fn stg11_init(
    rt: &mut Runtime,
    n: &mut dyn Natives,
    st: &mut StageState,
    allocs: &mut Vec<u32>,
    host: &mut dyn StageHost,
) {
    let l = st.layout;
    let (t, d, ro, b) = (l.text, l.data, l.rodata, l.bss);
    rt.engine(0x8003_24E4, vec![0x80, d + 0x4434C]);
    rt.engine(
        0x8002_45EC,
        vec![d + 0x5D900, d + 0x5D868, d + 0x5D8E8, d + 0x5D8F4],
    );
    rt.engine(0x8007_2270, vec![d + 0x5CBC0, d + 0x5CAD4]);
    rt.engine(0x8007_2218, vec![d + 0x5CBC0, d + 0x5CAD4]);
    rt.engine(0x8006_5048, vec![0, 0x800]);
    st.bss24 = host.value(0x8006_5048, &[0, 0x800]);
    set_task(rt, st, d + 0x5D4FC, st.bss24);
    rt.engine(0x8001_A8A8, vec![]);
    rt.engine(0x8003_3710, vec![]);
    for a in [0x800C_3CD8, 0x8011_E154, 0x8011_E150] {
        rt.engine(a, vec![]);
    }
    for (i, off) in [(0, 0x5D810), (1, 0x5D830), (2, 0x5D810), (3, 0x5D810)] {
        rt.engine(0x8002_506C, vec![i, d + off]);
    }
    rt.engine(0x8002_5028, vec![0, 0]);
    rt.engine(0x8002_5028, vec![0, 1]);
    rt.engine(0x8002_5048, vec![0]);
    rt.engine(0x8002_503C, vec![2]);
    rt.engine(t + 0xC2F0, vec![d + 0x5D910]);
    rt.engine(BGM_SET_VOLUME, vec![0]);
    rt.engine(STG_PLAY_MUSIC, vec![d + 0x5D920]);
    if let Ok(task) = rt.create_elemental_task(2, 1, t + 0xAE18, d + 0x5D92C) {
        if st.bss20.is_none() {
            st.bss20 = Some(task);
        }
        st.ad718 = 1;
        if let Some(x) = rt.task_mut(task) {
            x.slots[Slot::Display as usize] = t + 0xAF34;
            x.slots[Slot::Destructor as usize] = t + 0xB3BC;
        }
    }
    death_zones(rt, st, allocs, d + 0x5D7F0);
    for (idx, value, ptr) in host.object_list() {
        if idx == -1 {
            break;
        }
        if !(0..300).contains(&idx) {
            continue;
        }
        let e = &mut st.object_table[idx as usize];
        if e.1 == 0 {
            *e = (value, ptr);
        }
    }
    for (i, f) in [(0, t + 0x38934), (2, t + 0x148D4), (10, t + 0x11618)] {
        if st.callbacks[i] == 0 {
            st.callbacks[i] = f;
        }
    }
    rt.engine(t + 0x152EC, vec![d + 0x5D934, b + 0x2BC]);
    rt.engine(t + 0x152EC, vec![d + 0x5D944, b + 0xAC]);
    task_b484(rt, n, st, allocs);
    rt.engine(t + 0x330DC, vec![d + 0x5D954]);
    rt.engine(t + 0x28038, vec![d + 0x5D95C]);
    rt.engine(0x8006_D500, vec![d + 0x5D964, ro + 0x4A8]);
    st.table_77d8 = [0; 14];
    st.table_77d8[..4].copy_from_slice(&[0x800E_014C, t + 0xFED8, t + 0x109CC, t + 0x10A20]);
    task_16de0(rt, st, 0, None);
    st.adc30 = t + 0xABA0;
}

/// `fn_8003BCC4(data)`: the death-zone task "KillPlayerFallingDownStageP".
pub fn death_zones(rt: &mut Runtime, st: &mut StageState, allocs: &mut Vec<u32>, data: u32) {
    let Ok(t) = rt.create_elemental_task(0, 0, 0x8003_BD64, 0x8017_43E8) else {
        return;
    };
    allocs.push(8);
    if let Some(x) = rt.task_mut(t) {
        x.w40 = Some(vec![0; 8]);
        x.slots[Slot::Destructor as usize] = 0x8003_C144;
    }
    word(rt, t, 0x40, 4, data);
    st.death_task = Some(t);
    st.death_data = data;
}

/// `fn_8006638C(object_list, set_file)`: the SET task.
pub fn set_task(rt: &mut Runtime, st: &mut StageState, object_list: u32, file: u32) {
    let s = &mut st.set;
    s.ada08 = 0;
    s.ad468 = 1;
    s.ada0c = 0;
    s.task = None;
    s.object_list = 0;
    s.file_arg = 0;
    s.file = 0;
    s.rows = 0;
    s.ada1c = 0;
    s.ada18 = 0;
    s.ada14 = 0;
    s.entries_cleared = true;
    let Ok(t) = rt.create_elemental_task(2, 0, 0x8006_6274, 0x8017_4688) else {
        return;
    };
    st.set.task = Some(t);
    if let Some(x) = rt.task_mut(t) {
        x.slots[Slot::Destructor as usize] = 0x8006_6314;
    }
    st.set.object_list = object_list;
    st.set.ada14 = 0;
    st.set.file_arg = file;
    if file == 0 {
        st.set.file = 0;
        st.set.rows = 0;
    } else {
        st.set.file = file;
        st.set.rows = file + 0x20;
        rt.events.push(Event::Anomaly(
            "SET rows present: the SET spawner is not ported yet",
        ));
    }
    rt.engine(0x8006_4258, vec![]);
    if let Some(twp) = rt.task_mut(t).and_then(|x| x.twp.as_mut()) {
        twp[0] = 1;
    }
}

/// stg11D fn_11_B484.
fn task_b484(rt: &mut Runtime, n: &mut dyn Natives, st: &mut StageState, allocs: &mut Vec<u32>) {
    let t = st.layout.text;
    if let Some(old) = st.bss4d0 {
        rt.destroy_task(n, old);
    }
    let Ok(task) = rt.create_elemental_task(2, 1, t + 0xC158, st.layout.data + 0x5D988) else {
        st.bss4d0 = None;
        return;
    };
    st.bss4d0 = Some(task);
    allocs.push(0x404);
    if let Some(x) = rt.task_mut(task) {
        x.w38 = Some(vec![0; 0x404]);
        x.slots[Slot::Display as usize] = t + 0xC2EC;
        x.slots[Slot::Exec as usize] = t + 0xC158;
        x.slots[Slot::Destructor as usize] = t + 0xB57C;
    }
}

/// stg11D fn_11_16DE0(index, parent).
fn task_16de0(rt: &mut Runtime, st: &mut StageState, index: u32, parent: Option<TaskId>) {
    let t = st.layout.text;
    let Some(slot) = st.slots_5e534.get(index as usize).copied() else {
        return;
    };
    if slot.is_some() {
        return;
    }
    let task = match parent {
        Some(p) => rt.create_child_task(2, t + 0x16ECC, p),
        None => rt.create_elemental_task(2, 1, t + 0x16ECC, st.layout.data + 0x5E53C),
    };
    let Ok(task) = task else { return };
    if let Some(x) = rt.task_mut(task) {
        x.slots[Slot::Destructor as usize] = t + 0x16E9C;
        if let Some(twp) = x.twp.as_mut() {
            twp[3] = index as u8;
        }
    }
    st.slots_5e534[index as usize] = Some(task);
}
