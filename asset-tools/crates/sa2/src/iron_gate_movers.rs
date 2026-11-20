//! Lift (MLIFT), elevator (ELV) and bridge (BRIDGE) motion for Iron Gate
//! and Prison Lane, from the installed stg11D.rel / stg09D.rel PowerPC
//! callbacks.
//!
//! The Steam executable's own x86 text is encrypted on disk, so the stage
//! module shipped beside it is the readable source. `verify` checks the
//! module digest, the object-list rows and every instruction word and
//! relocated constant these state machines depend on. The engine's
//! "player standing on this object" query is an input supplied by the host.
//! Visual-only work (bounce wobble, door meshes, sounds) is reported but not
//! drawn; the wobble's effect on collision height is not applied.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Serialize;

pub const LEVEL: u16 = 11;
pub const MODULE_FILE: &str = "stg11D.rel";

/// Constants and instruction words read from the verified module.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoverProgram {
    pub module_sha256: String,
    pub lift_exec: u32,
    pub elevator_exec: u32,
    /// Added to SET Scale.x for the lift's raised offset (sec4+0x120).
    pub lift_rise_base: f32,
    /// Units per frame for both lift directions (sec5+0x4E4BC).
    pub lift_speed: f32,
    /// Frames standing before a lowered lift rises (`cmpwi r0,0x1e`).
    pub lift_stand_frames: i32,
    /// Frames unoccupied before a raised lift returns (`cmplwi r0,0x5a`).
    pub lift_return_frames: u8,
    /// Units per frame along the elevator path (sec5+0x4C4C4).
    pub elevator_speed: f32,
    /// Door closing step per frame (sec5+0x4C4C8).
    pub elevator_door_step: f32,
    /// Open value of the inner door (task work 0x44); it starts open.
    pub elevator_door_open: f32,
    /// The outer door (0x48): initial value, open value and per-frame
    /// closing step. It opens at `elevator_door_step` and is the only door
    /// that reopens while the car rests at its far end.
    pub elevator_door_b_initial: f32,
    pub elevator_door_b_open: f32,
    pub elevator_door_b_close_step: f32,
    /// The elevator registers its dynamic collision object this far below
    /// its placement (`fsubs` from Position.y before 0x8002305C).
    pub elevator_collision_drop: f32,
    /// Initial and required unoccupied frames before the elevator starts.
    pub elevator_ready_frames: u8,
    pub bridge_exec: u32,
    /// Local X offset of the extended bridge (sec4+0x1C).
    pub bridge_extend_x: f32,
    /// Units per frame in both directions (sec5+0x466A4).
    pub bridge_speed: f32,
    /// An elevator that is not moving within this distance of the extended
    /// point keeps the bridge extended (sec4+0x54).
    pub bridge_elevator_range: f32,
    /// Initial and required cooldown frames between bridge motions.
    pub bridge_ready_frames: u8,
}

/// Where one stage module keeps the shared lift/elevator/bridge code, and
/// the values the verified module must yield. Iron Gate (stg11D.rel) and
/// Prison Lane (stg09D.rel) build the same object family from the same
/// source with different offsets and a few different constants.
pub struct MoverLayout {
    pub level: u16,
    pub module_file: &'static str,
    /// Section-5 object rows (name pointer; the callback pointer precedes it).
    rows: [u32; 3],
    /// Expected (lift, elevator, bridge) init callbacks in section 1.
    inits: [u32; 3],
    /// (section-1 offset, expected big-endian word with relocation fields zeroed)
    words: &'static [(u32, u32)],
    /// `lis` offsets of the exec pointers: lift, elevator, bridge.
    exec_lis: [u32; 3],
    /// `lis` offsets of: lift rise base, lift speed, elevator speed, door A
    /// step, door A open, door B initial, door B open, door B closing step,
    /// bridge extend X, bridge speed, bridge elevator range, and the drop
    /// of the elevator's dynamic collision object below its placement.
    const_lis: [u32; 12],
    /// Values the constants above must read as, in the same order.
    expect: [f32; 12],
    expect_exec: [u32; 3],
}

/// stg11D.rel (Iron Gate). Both elevator doors share one constant set.
pub const IRON_GATE: MoverLayout = MoverLayout {
    level: 11,
    module_file: "stg11D.rel",
    rows: [0x5d358, 0x5d2c8, 0x5d318],
    inits: [0x3214, 0x1a84, 0x2f8],
    words: &[
        (0x3a38, 0x2c00_001e), // cmpwi r0,0x1e  (rise after 30 standing frames)
        (0x3a1c, 0x2800_005a), // cmplwi r0,0x5a (return after 90 idle frames)
        (0x32a8, 0x3800_005a), // li r0,0x5a     (initial idle counter)
        (0x3974, 0x2c03_2710), // cmpwi r3,0x2710 (standing counter clamp)
        (0x1b18, 0x3800_0078), // li r0,0x78     (initial ready counter)
        (0x26f4, 0x2800_0078), // cmplwi r0,0x78 (start requires 120)
        (0x27d0, 0x2803_0078), // cmplwi r3,0x78 (counter saturates at 120)
        (0x21bc, 0x801f_0008), // lwz r0,8(r31)  (Rotation.x bit 0: one-way)
        (0x038c, 0x3800_0078), // li r0,0x78     (initial bridge cooldown)
        (0x0b84, 0x2800_0078), // cmplwi r0,0x78 (motion requires 120)
        (0x0b3c, 0x2c18_0005), // cmpwi r24,5    (five registered elevator slots)
        (0x0aec, 0x2c00_0001), // cmpwi r0,1     (moving elevators are skipped)
    ],
    exec_lis: [0x326c, 0x1adc, 0x350],
    const_lis: [
        0x32cc, 0x369c, 0x2118, 0x2000, 0x1cf4, 0x1cf4, 0x25d8, 0x2000, 0x394, 0x74c, 0xac0, 0x1c60,
    ],
    expect: [
        20.0, 0.5, 0.3, 0.2, -9.5, -9.5, -9.5, 0.2, -140.0, 8.0, 450.0, 40.0,
    ],
    expect_exec: [0x35dc, 0x1f7c, 0x688],
};

/// stg09D.rel (Prison Lane). The lift is identical; the elevator travels at
/// 0.6, starts with its outer door (0x48) shut, opens it to -36.3 and closes
/// it at 0.32; the bridge moves at 1.5.
pub const PRISON_LANE: MoverLayout = MoverLayout {
    level: 9,
    module_file: "stg09D.rel",
    rows: [0x4ac5c, 0x4abec, 0x4ac1c],
    inits: [0x4704, 0x2f18, 0x2ec],
    words: &[
        (0x4efc, 0x2c00_001e),
        (0x4ee0, 0x2800_005a),
        (0x4798, 0x3800_005a),
        (0x4e38, 0x2c03_2710),
        (0x2fac, 0x3800_0078),
        (0x3bcc, 0x2800_0078),
        (0x3ca8, 0x2803_0078),
        (0x3660, 0x801f_0008),
        (0x0380, 0x3800_0078),
        (0x0b5c, 0x2800_0078),
        (0x0b14, 0x2c18_0005),
        (0x0ac4, 0x2c00_0001),
    ],
    exec_lis: [0x475c, 0x2f70, 0x344],
    const_lis: [
        0x47bc, 0x4b60, 0x35bc, 0x349c, 0x3188, 0x319c, 0x3a1c, 0x34b8, 0x388, 0x740, 0xa98, 0x30f4,
    ],
    expect: [
        20.0, 0.5, 0.6, 0.2, -9.5, 0.0, -36.3, 0.32, -140.0, 1.5, 450.0, 40.0,
    ],
    expect_exec: [0x4aa0, 0x3438, 0x67c],
};

pub const LAYOUTS: [&MoverLayout; 2] = [&IRON_GATE, &PRISON_LANE];

fn be(bytes: &[u8], at: u32) -> Result<u32> {
    bytes
        .get(at as usize..at as usize + 4)
        .map(|w| u32::from_be_bytes(w.try_into().unwrap()))
        .ok_or_else(|| Error::invalid("mover instruction outside section"))
}

impl MoverProgram {
    /// Verify Iron Gate's stg11D.rel.
    pub fn verify(rel_bytes: &[u8]) -> Result<Self> {
        Self::verify_layout(&IRON_GATE, rel_bytes)
    }

    pub fn verify_layout(layout: &MoverLayout, rel_bytes: &[u8]) -> Result<Self> {
        let info = stage_land::catalog()
            .iter()
            .find(|i| i.filename == layout.module_file)
            .ok_or_else(|| Error::invalid("stage module is not catalogued"))?;
        let sha = manifest::sha256(rel_bytes);
        if rel_bytes.len() != info.size || sha != info.sha256 {
            return Err(Error::invalid("unsupported stage module revision"));
        }
        let rel = CityRel::read_identified(rel_bytes, info.module)?;
        let name = |row: u32, expect: &[u8]| -> Result<u32> {
            let (s, n) = rel
                .local_pointer(5, row)?
                .ok_or_else(|| Error::invalid("object row has no name"))?;
            if rel.data(s)?.get(n as usize..n as usize + expect.len()) != Some(expect) {
                return Err(Error::invalid("object row name mismatch"));
            }
            match rel.local_pointer(5, row - 4)? {
                Some((1, f)) => Ok(f),
                _ => Err(Error::invalid("object row callback is not in section 1")),
            }
        };
        let inits = [
            name(layout.rows[0], b"MLIFT\0")?,
            name(layout.rows[1], b"ELV\0")?,
            name(layout.rows[2], b"BRIDGE\0")?,
        ];
        if inits != layout.inits {
            return Err(Error::invalid("unexpected mover callbacks"));
        }
        let code = rel.data(1)?;
        for &(at, word) in layout.words {
            if be(code, at)? != word {
                return Err(Error::invalid(
                    "mover instruction differs from verified module",
                ));
            }
        }
        // Each constant is addressed by an ADDR16_HA/LO pair at `lis` into
        // the module's own data (section 4 or 5).
        let constant = |lis: u32| -> Result<f32> {
            let hi = rel.reference(1, lis + 2);
            let lo = rel.reference(1, lis + 6);
            match (hi, lo) {
                (Some(h), Some(l))
                    if (h.kind, l.kind) == (6, 4)
                        && h.section == l.section
                        && h.addend == l.addend
                        && matches!(h.section, 4 | 5) =>
                {
                    let d = rel.data(h.section)?;
                    Ok(f32::from_bits(be(d, h.addend)?))
                }
                _ => Err(Error::invalid("mover constant relocation differs")),
            }
        };
        let exec = |lis: u32| -> Result<u32> {
            match (rel.reference(1, lis + 2), rel.reference(1, lis + 6)) {
                (Some(h), Some(l)) if h.section == 1 && l.section == 1 && h.addend == l.addend => {
                    Ok(h.addend)
                }
                _ => Err(Error::invalid("mover exec relocation differs")),
            }
        };
        let execs = [
            exec(layout.exec_lis[0])?,
            exec(layout.exec_lis[1])?,
            exec(layout.exec_lis[2])?,
        ];
        if execs != layout.expect_exec {
            return Err(Error::invalid("unexpected mover exec callbacks"));
        }
        let mut c = [0f32; 12];
        for (i, &lis) in layout.const_lis.iter().enumerate() {
            c[i] = constant(lis)?;
            if c[i] != layout.expect[i] {
                return Err(Error::invalid(
                    "mover constants differ from the verified module",
                ));
            }
        }
        Ok(Self {
            module_sha256: sha,
            lift_exec: execs[0],
            elevator_exec: execs[1],
            lift_rise_base: c[0],
            lift_speed: c[1],
            lift_stand_frames: 0x1e,
            lift_return_frames: 0x5a,
            elevator_speed: c[2],
            elevator_door_step: c[3],
            elevator_door_open: c[4],
            elevator_door_b_initial: c[5],
            elevator_door_b_open: c[6],
            elevator_door_b_close_step: c[7],
            elevator_ready_frames: 0x78,
            bridge_exec: execs[2],
            bridge_extend_x: c[8],
            bridge_speed: c[9],
            bridge_elevator_range: c[10],
            elevator_collision_drop: c[11],
            bridge_ready_frames: 0x78,
        })
    }
}

fn step_toward(pos: &mut [f32; 3], target: [f32; 3], speed: f32) -> bool {
    let d = [target[0] - pos[0], target[1] - pos[1], target[2] - pos[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    // Native: scale = speed / |d| when |d| > speed, else exactly 1.0.
    let f = if len > speed { speed / len } else { 1.0 };
    for i in 0..3 {
        pos[i] += d[i] * f;
    }
    f == 1.0
}

/// Sine and cosine njRotateY uses for a BAMS yaw (`ninja_trig`).
pub fn rotate_sin_cos(bams: u16) -> (f32, f32) {
    // The SET loader (GSNE8P 0x80065E54) sign-extends rotations into the task.
    crate::ninja_trig::nj_rotate_sin_cos(i32::from(bams as i16))
}

/// MLIFT: rises by `20 + Scale.x` after the player stands on it for 30
/// frames and returns after 90 unoccupied frames at the top.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lift {
    pub base: [f32; 3],
    pub top: [f32; 3],
    pub position: [f32; 3],
    pub raised: bool,
    pub moving: bool,
    pub standing: i32,
    pub idle: u8,
}

impl Lift {
    pub fn new(p: &MoverProgram, position: [f32; 3], scale_x: f32) -> Self {
        let top = [
            position[0],
            position[1] + p.lift_rise_base + scale_x,
            position[2],
        ];
        Self {
            base: position,
            top,
            position,
            raised: false,
            moving: false,
            standing: 0,
            idle: p.lift_return_frames,
        }
    }

    /// One scheduled update. Returns true when motion started this frame.
    pub fn step(&mut self, p: &MoverProgram, stood: bool) -> bool {
        if self.moving {
            let target = if self.raised { self.base } else { self.top };
            if step_toward(&mut self.position, target, p.lift_speed) {
                self.moving = false;
                self.raised = !self.raised;
            }
            return false;
        }
        if stood {
            if self.standing < 10_000 {
                self.standing += 1;
            }
            self.idle = 0;
        } else {
            self.standing = 0;
            self.idle = self.idle.wrapping_add(1);
        }
        let start = if self.raised {
            self.idle > p.lift_return_frames
        } else {
            self.standing > p.lift_stand_frames
        };
        if start {
            self.moving = true;
            self.standing = 0;
            self.idle = 0;
        }
        start
    }
}

/// ELV: doors close, then the car travels from its placement to the SET
/// Scale vector at 0.3 units per frame. Rotation.x bit 0 makes it one-way.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Elevator {
    pub origin: [f32; 3],
    pub target: [f32; 3],
    pub position: [f32; 3],
    pub at_target: bool,
    /// 0 idle, 1 moving, 2 stopped for good.
    pub mode: u8,
    /// Inner door (task work 0x44).
    pub door: f32,
    /// Outer door (task work 0x48).
    pub door_b: f32,
    pub ready: u8,
    pub one_way: bool,
}

impl Elevator {
    pub fn new(p: &MoverProgram, position: [f32; 3], scale: [f32; 3], rotation_x: u16) -> Self {
        Self {
            origin: position,
            target: scale,
            position,
            at_target: false,
            mode: 0,
            door: p.elevator_door_open,
            door_b: p.elevator_door_b_initial,
            ready: p.elevator_ready_frames,
            one_way: rotation_x & 1 != 0,
        }
    }

    /// Native idle door code: at the placement both doors open; at the far
    /// end only the outer one does. True when the tested doors were already
    /// open at the start of the frame.
    fn doors_open(&mut self, p: &MoverProgram) -> bool {
        let both = !self.at_target;
        if self.door_b <= p.elevator_door_b_open && (!both || self.door <= p.elevator_door_open) {
            return true;
        }
        if both {
            self.door = (self.door - p.elevator_door_step).max(p.elevator_door_open);
        }
        self.door_b = (self.door_b - p.elevator_door_step).max(p.elevator_door_b_open);
        false
    }

    pub fn step(&mut self, p: &MoverProgram, stood: bool) -> bool {
        match self.mode {
            1 => {
                // Both doors close first; travel begins on the frame they reach 0.
                self.door = (self.door + p.elevator_door_step).min(0.0);
                self.door_b = (self.door_b + p.elevator_door_b_close_step).min(0.0);
                if self.door < 0.0 || self.door_b < 0.0 {
                    return false;
                }
                let target = if self.at_target {
                    self.origin
                } else {
                    self.target
                };
                if step_toward(&mut self.position, target, p.elevator_speed) {
                    self.at_target = !self.at_target;
                    self.mode = if self.one_way { 2 } else { 0 };
                }
                false
            }
            0 => {
                // Doors reopen while idle; occupancy is only tested once open.
                if !self.doors_open(p) {
                    return false;
                }
                if stood {
                    if self.ready >= p.elevator_ready_frames {
                        self.mode = 1;
                        self.ready = 0;
                        return true;
                    }
                    self.ready = 0;
                } else if self.ready < p.elevator_ready_frames {
                    self.ready += 1;
                }
                false
            }
            _ => {
                self.doors_open(p);
                false
            }
        }
    }
}

/// BRIDGE: extends by -140 along its local X while any registered elevator
/// that is not moving lies strictly within 450 units of the extended point,
/// and retracts otherwise, at 8 units per frame with a 120-frame cooldown.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bridge {
    pub retracted: [f32; 3],
    pub extended: [f32; 3],
    pub position: [f32; 3],
    pub is_extended: bool,
    pub moving: bool,
    pub ready: u8,
}

impl Bridge {
    /// stg09D.rel 0x388..0x3E8: unit, translate, njRotateY(yaw), then
    /// njCalcPoint of (bridge_extend_x, 0, 0).
    pub fn new(p: &MoverProgram, position: [f32; 3], yaw_bams: u16) -> Self {
        let (s, c) = rotate_sin_cos(yaw_bams);
        let x = p.bridge_extend_x;
        Self {
            retracted: position,
            extended: [position[0] + x * c, position[1], position[2] - x * s],
            position,
            is_extended: false,
            moving: false,
            ready: p.bridge_ready_frames,
        }
    }

    /// `elevators` gives each registered elevator's position and whether it is moving.
    pub fn step(&mut self, p: &MoverProgram, elevators: &[([f32; 3], bool)]) -> bool {
        if self.moving {
            let target = if self.is_extended {
                self.retracted
            } else {
                self.extended
            };
            if step_toward(&mut self.position, target, p.bridge_speed) {
                self.moving = false;
                self.is_extended = !self.is_extended;
            }
            return false;
        }
        let near = elevators.iter().take(5).any(|&(e, moving)| {
            let d = [
                e[0] - self.extended[0],
                e[1] - self.extended[1],
                e[2] - self.extended[2],
            ];
            !moving && (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < p.bridge_elevator_range
        });
        let wanted = near != self.is_extended;
        if wanted && self.ready >= p.bridge_ready_frames {
            self.moving = true;
            self.ready = 0;
            return true;
        }
        if wanted {
            self.ready = 0;
        } else if self.ready < p.bridge_ready_frames {
            self.ready += 1;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> MoverProgram {
        MoverProgram {
            module_sha256: String::new(),
            lift_exec: 0x35dc,
            elevator_exec: 0x1f7c,
            lift_rise_base: 20.0,
            lift_speed: 0.5,
            lift_stand_frames: 30,
            lift_return_frames: 90,
            elevator_speed: 0.3,
            elevator_door_step: 0.2,
            elevator_door_open: -9.5,
            elevator_door_b_initial: -9.5,
            elevator_door_b_open: -9.5,
            elevator_door_b_close_step: 0.2,
            elevator_ready_frames: 120,
            bridge_exec: 0x688,
            bridge_extend_x: -140.0,
            bridge_speed: 8.0,
            bridge_elevator_range: 450.0,
            elevator_collision_drop: 40.0,
            bridge_ready_frames: 120,
        }
    }

    #[test]
    fn bridge_extends_toward_a_resting_elevator() {
        let p = program();
        let mut b = Bridge::new(&p, [2127.0, -669.5, -1027.0], 0);
        assert_eq!(b.extended, [1987.0, -669.5, -1027.0]);
        let top = ([1576.0, -270.0, -1027.5], false);
        let bottom = ([1803.5, -663.0, -1027.5], false);
        assert!(!b.step(&p, &[top]));
        assert!(!b.step(&p, &[(bottom.0, true)]));
        assert!(b.step(&p, &[bottom]));
        let mut frames = 0;
        while b.moving {
            b.step(&p, &[bottom]);
            frames += 1;
        }
        assert_eq!(frames, 18);
        assert!(b.is_extended && b.position == b.extended);
    }

    #[test]
    fn lift_rises_after_thirty_frames_and_returns() {
        let p = program();
        let mut l = Lift::new(&p, [0.0, -270.0, 0.0], 50.0);
        assert_eq!(l.top[1], -200.0);
        let started = (0..31).filter(|_| l.step(&p, true)).count();
        assert_eq!(started, 1);
        assert!(l.moving);
        let mut frames = 0;
        while l.moving {
            l.step(&p, true);
            frames += 1;
        }
        assert_eq!(frames, 140);
        assert!(l.raised && l.position[1] == -200.0);
        // Standing at the top never sends it back.
        for _ in 0..500 {
            assert!(!l.step(&p, true));
        }
        let mut idle = 0;
        while !l.step(&p, false) {
            idle += 1;
        }
        assert_eq!(idle, 90);
    }

    #[test]
    fn one_way_elevator_closes_doors_then_travels() {
        let p = program();
        let mut e = Elevator::new(&p, [0.0, 0.0, 0.0], [0.0, -3.0, 0.0], 1);
        assert!(e.step(&p, true));
        let mut frames = 0;
        while e.mode == 1 {
            e.step(&p, true);
            frames += 1;
        }
        // 47 door-only frames; binary32 leaves 0.3 x 10 just short of 3, so 11 travel frames.
        assert_eq!(frames, 58);
        assert_eq!(e.position, [0.0, -3.0, 0.0]);
        assert_eq!(e.mode, 2);
        assert!(!e.step(&p, true));
    }

    #[test]
    fn prison_lane_elevator_opens_its_outer_door_before_it_can_start() {
        let p = MoverProgram {
            elevator_speed: 0.6,
            elevator_door_b_initial: 0.0,
            elevator_door_b_open: -36.3,
            elevator_door_b_close_step: 0.32,
            bridge_speed: 1.5,
            ..program()
        };
        let mut e = Elevator::new(&p, [0.0, 0.0, 0.0], [0.0, 6.0, 0.0], 1);
        // The outer door opens from shut at 0.2 per frame (182 frames).
        let mut frames = 0;
        while !e.step(&p, true) {
            frames += 1;
            assert!(frames < 400);
        }
        assert_eq!(frames, 182);
        let mut frames = 0;
        while e.mode == 1 {
            e.step(&p, true);
            frames += 1;
        }
        // The outer door closes last: 36.3 / 0.32 rounds up to 114 frames,
        // then 10 frames of travel at 0.6.
        assert!((123..=125).contains(&frames), "{frames}");
        assert_eq!(e.position, [0.0, 6.0, 0.0]);
    }
}
