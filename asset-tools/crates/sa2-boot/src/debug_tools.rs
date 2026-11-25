//! The game's own leftover debug tools, switched from the dev menu's Gear
//! panel ("set key value" lines in /mmo/gear-tune, chao_player `debug_set`):
//! - `dbgtext 1`: live values on screen with the game's debug text
//!   (main.dol 0x80114058 queues a line, 0x80114B28 draws the queue every
//!   frame from the main loop with the debug font).
//! - `freecam 1`: the camera table's EDITOR mode (0x80175410 row 4, exec
//!   0x800C8E18), pushed on screen 0's camera stack with the game's own
//!   0x800C3A58 and released with 0x800C3780. It turns with the C-stick, so
//!   while it is on the move keys (WASD) steer the camera, not the character.
//! - `fullgear 1`: every upgrade byte 0x801E50B4 + 0..0x1D set, as the old
//!   developer stage select's [Y] did (titleD); the bytes are put back when
//!   switched off, and gear_pickup tells the server nothing meanwhile.
//! The mission picker is start_stage's SA2_START_MISSION (page ?mission=1..5).
use crate::chao_player::debug_set;
use sa2_asset_tools::input::PadStatus;
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

const DBG_PRINT: u32 = 0x8011_4058;
const CAM_PUSH: u32 = 0x800C_3A58;
const CAM_RELEASE: u32 = 0x800C_3780;
/// Screen 0's camera block: +0x254 the top layer, layer i's mode at +0x258 + 0x114 * i.
const CAM: u32 = 0x801F_F424;
const CAM_EDITOR: u32 = 4;
pub(crate) const UPGRADES: u32 = 0x801E_50B4;
const UPGRADE_COUNT: u32 = 0x1D;
const STAGE_NUM: u32 = 0x803A_D820;
const GAME: u32 = 0x801C_C168;
const P1_OBJ2: u32 = 0x801E_7728;
/// Guest scratch for the text lines (free between knux_rig's 0x82F0_0400 and INFO 0x82F0_1000).
const TEXT: u32 = 0x82F0_0800;
const LINE: u32 = 0x40;

// Shared with PADRead and gear_pickup's frame hook, which may run on other host threads.
/// The camera layer holding EDITOR, and the stage it was pushed in.
static FREECAM: Global<Option<(u32, u16)>> = global!(None);
/// The upgrade bytes before Full equipment, put back when it is switched off.
static SAVED_GEAR: Global<Option<[u8; UPGRADE_COUNT as usize]>> = global!(None);

fn on(key: &str) -> bool {
    debug_set(key).is_some_and(|v| v == "1")
}

/// Whether Full equipment is on (gear_pickup reports nothing then).
pub fn full_gear() -> bool {
    SAVED_GEAR.get().is_some()
}

/// Once per frame from the player character's exec (chao_player), in stages and Chao World.
pub fn frame(c: &mut Cpu, m: &mut Mem, task: u32) {
    full_gear_frame(m);
    free_cam_frame(c, m);
    if on("dbgtext") {
        text(c, m, task);
    }
}

fn full_gear_frame(m: &mut Mem) {
    match (on("fullgear"), SAVED_GEAR.get()) {
        (true, None) => {
            SAVED_GEAR.set(Some(std::array::from_fn(|i| m.r8(UPGRADES + i as u32))));
            eprintln!("debug-tools: full equipment on");
        }
        (false, Some(saved)) => {
            for (i, b) in saved.into_iter().enumerate() {
                m.w8(UPGRADES + i as u32, b);
            }
            SAVED_GEAR.set(None);
            eprintln!("debug-tools: full equipment off");
        }
        _ => {}
    }
    if SAVED_GEAR.get().is_some() {
        for i in 0..UPGRADE_COUNT {
            m.w8(UPGRADES + i, 1);
        }
    }
}

fn layer_mode(m: &Mem, layer: u32) -> u32 {
    m.r32(CAM + 0x258 + 0x114 * layer)
}

fn free_cam_frame(c: &mut Cpu, m: &mut Mem) {
    let stage = m.r16(STAGE_NUM);
    // A new stage or the game's own camera change drops the layer: take it as gone.
    let live = FREECAM.get().filter(|&(layer, s)| s == stage && layer_mode(m, layer) == CAM_EDITOR);
    FREECAM.set(live);
    match (on("freecam"), live) {
        (true, None) => {
            crate::guest_call(c, m, CAM_PUSH, &[0, CAM_EDITOR]);
            let top = m.r32(CAM + 0x254);
            if top < 16 && layer_mode(m, top) == CAM_EDITOR {
                FREECAM.set(Some((top, stage)));
                eprintln!("debug-tools: free camera on (layer {top})");
            }
        }
        (false, Some((layer, _))) => {
            crate::guest_call(c, m, CAM_RELEASE, &[0, layer]);
            FREECAM.set(None);
            eprintln!("debug-tools: free camera off");
        }
        _ => {}
    }
}

/// While the free camera is on, the stick turns it (EDITOR reads the C-stick) and the character stands.
pub(crate) fn pad_filter(mut p: PadStatus) -> PadStatus {
    if FREECAM.get().is_some() {
        if p.stick != [0, 0] {
            p.substick = p.stick;
        }
        p.stick = [0, 0];
    }
    p
}

/// The lines shown with `dbgtext`: stage, mission, place, action and speed.
fn lines(m: &Mem, task: u32) -> Vec<String> {
    let f = |a: u32| f32::from_bits(m.r32(a));
    let frame = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let mut out = vec![format!("STAGE {} MISSION {} FRAME {frame}", m.r16(STAGE_NUM), m.r8(GAME + 0x23) + 1)];
    let twp = m.r32(task + 0x34);
    if (0x8000_0000..0x8C00_0000).contains(&twp) {
        out.push(format!("POS {:.1} {:.1} {:.1}", f(twp + 0x14), f(twp + 0x18), f(twp + 0x1C)));
    }
    let obj2 = m.r32(P1_OBJ2);
    if (0x8000_0000..0x8C00_0000).contains(&obj2) {
        out.push(format!("ACTION {} SPEED {:.2}", m.r16(obj2 + 0x178), f(obj2 + 0x64)));
    }
    let mut flags = Vec::new();
    if FREECAM.get().is_some() {
        flags.push("FREE CAMERA");
    }
    if full_gear() {
        flags.push("FULL EQUIPMENT");
    }
    if !flags.is_empty() {
        out.push(flags.join(" "));
    }
    out
}

fn text(c: &mut Cpu, m: &mut Mem, task: u32) {
    for (i, l) in lines(m, task).iter().enumerate() {
        let at = TEXT + LINE * i as u32;
        let bytes = l.as_bytes();
        let n = bytes.len().min(LINE as usize - 1);
        for (j, b) in bytes[..n].iter().enumerate() {
            m.w8(at + j as u32, *b);
        }
        m.w8(at + n as u32, 0);
        // Column 1, rows 20.. (bottom left, clear of the HUD).
        crate::guest_call(c, m, DBG_PRINT, &[(1 << 16) | (20 + i as u32), at]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_cam_moves_the_stick_to_the_c_stick_only_while_on() {
        let p = PadStatus { stick: [40, -20], ..Default::default() };
        assert_eq!(pad_filter(p).stick, [40, -20]);
        FREECAM.set(Some((3, 13)));
        let q = pad_filter(p);
        assert_eq!((q.stick, q.substick), ([0, 0], [40, -20]));
        FREECAM.set(None);
    }

    #[test]
    fn lines_show_stage_mission_and_flags() {
        let mut m = Mem::default();
        m.w16(STAGE_NUM, 13);
        m.w8(GAME + 0x23, 4);
        SAVED_GEAR.set(Some([0; 0x1D]));
        let l = lines(&m, 0);
        assert!(l[0].starts_with("STAGE 13 MISSION 5 "), "{l:?}");
        assert!(l.last().is_some_and(|s| s.ends_with("FULL EQUIPMENT")), "{l:?}");
        SAVED_GEAR.set(None);
        assert!(lines(&m, 0).iter().all(|s| s.len() < LINE as usize));
    }
}
