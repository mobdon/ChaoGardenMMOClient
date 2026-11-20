//! The native PAD path (milestone E3; ENGINE-MAP.md section 5).
//!
//! - [`PadSystem::read`]: `fn_80119A04`, run from the VI post-retrace callback.
//!   It keeps the previous `PADStatus` (0x80357F30), takes the new one
//!   (0x80357F60; `PADRead`'s output is the host input), runs `PADClamp` (the
//!   `sa2` crate's port) and fills the per-port records at 0x80357E60.
//! - [`PadSystem::tick`]: `fn_800226E0` at the start of every game tick. It
//!   maps players to ports (`fn_80119B84`), fills the pointer table
//!   0x801E55D8, copies or neutralises the eight records at 0x801E5438, and
//!   fills the stick tables 0x801E53B8 / 0x801E53F8 (`fn_80022048`).
//!
//! Host input enters only as raw [`PadStatus`]. Engine routines these call
//! that are not ported are logged as [`Event::Engine`] and not run:
//! `PADReset`, and `fn_80022BC4`, `fn_80021E68` (demo record/playback),
//! `fn_80021D0C`, `fn_80022A94` after the copy.
use crate::task::Event;
use sa2_asset_tools::input::{pad_clamp, PadStatus};
use sa2_asset_tools::msl_math::{atan2f, gekko_frsqrte};

/// `PADReset` (logged with its port mask).
pub const PAD_RESET: u32 = 0x800F_4368;
/// Routines `fn_800226E0` calls after the copy (logged, not run).
pub const AFTER_COPY: [u32; 3] = [0x8002_2BC4, 0x8002_1E68, 0x8002_1D0C];
/// `fn_80022A94(player)`, called for players 0..4 (logged, not run).
pub const PER_PLAYER: u32 = 0x8002_2A94;
/// The record each port's data lands in: 0x80357E60 + 0x34 * port.
pub const RECORDS: u32 = 0x8035_7E60;
/// The idle record 0x801950E8, used for players without a pad.
pub const IDLE: u32 = 0x8019_50E8;
/// Size of a pad record.
pub const RECORD: usize = 0x34;

/// Button table at 0x801D51A4 (count 0x803AD6F0 = 12): PAD bit → game bit.
const BUTTONS: [(u32, u32); 12] = [
    (0x0001, 0x0000_0040),
    (0x0002, 0x0000_0080),
    (0x0004, 0x0000_0020),
    (0x0008, 0x0000_0010),
    (0x0040, 0x0002_0000),
    (0x0020, 0x0001_0000),
    (0x0010, 0x0000_0100),
    (0x0100, 0x0000_0004),
    (0x0200, 0x0000_0002),
    (0x0400, 0x0000_0400),
    (0x0800, 0x0000_0200),
    (0x1000, 0x0000_0008),
];
/// fn_80022048 constants (.sdata2): dead zone 14.0 (r2-0x7B14) and -14.0
/// (r2-0x7B10), magnitude divisor 125000.0 (r2-0x7AF0), limit 1.0 (r2-0x7AEC),
/// push threshold 0.5 (r2-0x7AE8), radians to BAMS (r2-0x7B08), Newton 0.5 and
/// 3.0 (r2-0x7B00, r2-0x7AF8).
const DEAD: f32 = 14.0;
const DEAD_NEG: f32 = -14.0;
const MAG_DIV: f32 = 125_000.0;
const MAG_MAX: f32 = 1.0;
const PUSH: f32 = 0.5;
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);
const MASK_CONNECTED: u32 = 0x000F_060E;

/// Where a player's pointer in 0x801E55D8 points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadRef {
    /// A port record (0..4).
    Port(usize),
    /// The idle record 0x801950E8.
    Idle,
    /// Not set yet (the table is zero until the first tick).
    Null,
}

impl PadRef {
    /// The native address.
    pub fn addr(self) -> u32 {
        match self {
            PadRef::Port(p) => RECORDS + (RECORD * p) as u32,
            PadRef::Idle => IDLE,
            PadRef::Null => 0,
        }
    }
}

/// What the tick reads from the rest of the engine.
#[derive(Debug, Clone, Default)]
pub struct PadContext {
    /// G 0x801CC168 (+0x3E == 1: input on; +0x3F..+0x46: player i takes input).
    pub g: Vec<u8>,
    /// `*0x803ADB04` is non-null (a camera is active).
    pub camera_active: bool,
    /// 0x803ADAD4.
    pub viewports: i32,
    /// `[*(0x80175378 + 4i) + 0x10]` for i in 0..8: each viewport's camera angle.
    pub camera_angles: [u32; 8],
}

/// All PAD state the native path keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct PadSystem {
    /// 0x80357F30, the previous (clamped) status.
    pub prev: [PadStatus; 4],
    /// 0x80357F60, the current (clamped) status.
    pub cur: [PadStatus; 4],
    /// 0x80357E60: the four port records, native byte layout.
    pub records: [[u8; RECORD]; 4],
    /// The word each record's +0x30 points to: 0x101 when read, 0 on error.
    pub status: [u32; 4],
    /// 0x801950E8, the idle record.
    pub idle: [u8; RECORD],
    /// 0x8019511C: each player's port code (0, 6, 12, 18 for ports 0..3).
    pub port_config: [u32; 4],
    /// 0x801E55D8.
    pub pointers: [PadRef; 4],
    /// 0x801E5368: player has a pad.
    pub connected: [u32; 4],
    /// 0x801E5358: player index, or -1.
    pub port_index: [i32; 4],
    /// 0x801E5438: the eight per-player copies.
    pub copies: [[u8; RECORD]; 8],
    /// 0x801E53B8: angle (BAMS, relative to the camera) and magnitude, always.
    pub raw_sticks: [(u32, f32); 8],
    /// 0x801E53F8: the same, or zero when the player takes no input.
    pub sticks: [(u32, f32); 8],
    /// 0x803AD874: the stick just crossed 0.5.
    pub pushed: [u8; 8],
}

impl Default for PadSystem {
    fn default() -> Self {
        Self {
            prev: [PadStatus::default(); 4],
            cur: [PadStatus::default(); 4],
            records: [[0; RECORD]; 4],
            status: [0; 4],
            idle: [0; RECORD],
            port_config: [0, 6, 12, 18],
            pointers: [PadRef::Null; 4],
            connected: [0; 4],
            port_index: [0; 4],
            copies: [[0; RECORD]; 8],
            raw_sticks: [(0, 0.0); 8],
            sticks: [(0, 0.0); 8],
            pushed: [0; 8],
        }
    }
}

fn put32(r: &mut [u8; RECORD], off: usize, v: u32) {
    r[off..off + 4].copy_from_slice(&v.to_be_bytes());
}
fn put16(r: &mut [u8; RECORD], off: usize, v: u16) {
    r[off..off + 2].copy_from_slice(&v.to_be_bytes());
}
fn get32(r: &[u8; RECORD], off: usize) -> u32 {
    u32::from_be_bytes([r[off], r[off + 1], r[off + 2], r[off + 3]])
}
fn get16(r: &[u8; RECORD], off: usize) -> i16 {
    i16::from_be_bytes([r[off], r[off + 1]])
}

/// `fn_80119BD8`: PAD buttons to game buttons.
pub fn convert_buttons(pad: u16) -> u32 {
    BUTTONS
        .iter()
        .filter(|(p, _)| u32::from(pad) & p != 0)
        .fold(0, |acc, (_, g)| acc | g)
}

/// `fn_80119C4C`: stick value * 128 / 72, rounded toward zero.
pub fn scale_stick(v: i8) -> i16 {
    let x = i32::from(v) << 7;
    let hi = ((i64::from(0x38E3_8E39_i32) * i64::from(x)) >> 32) as i32;
    let t = hi >> 4;
    (t + ((t as u32) >> 31) as i32) as i16
}

/// `fn_80119B84`: a player's port code to a port record.
pub fn port_of(code: u32) -> usize {
    match code {
        6 => 1,
        12 => 2,
        18 => 3,
        _ => 0,
    }
}

fn neutralise(r: &mut [u8; RECORD]) {
    let held = get32(r, 0x8);
    put32(r, 0x2C, held);
    put32(r, 0x8, 0);
    put32(r, 0xC, 0xFFFF_FFFF);
    put32(r, 0x10, 0);
    put32(r, 0x14, 0);
    for off in [0x1C, 0x1E, 0x1A, 0x18, 0x20, 0x22] {
        put16(r, off, 0);
    }
}

/// The inline square root of fn_80022048: `frsqrte` and three Newton steps in
/// double precision (fused `fnmsub`), times the input, rounded to single.
fn sqrt_newton(sq: f32) -> f32 {
    let f2 = f64::from(sq);
    let mut f1 = gekko_frsqrte(f2);
    for _ in 0..2 {
        let t = f1 * f1;
        f1 *= 0.5;
        let t = -f2.mul_add(t, -3.0);
        f1 *= t;
    }
    let t = f1 * f1;
    f1 *= 0.5;
    let t = -f2.mul_add(t, -3.0);
    let f0 = f1 * t;
    (f2 * f0) as f32
}

impl PadSystem {
    fn pointer_record(&self, p: PadRef) -> [u8; RECORD] {
        match p {
            PadRef::Port(i) => self.records[i],
            PadRef::Idle => self.idle,
            // Never reached: tick sets every pointer before copying.
            PadRef::Null => [0; RECORD],
        }
    }

    /// `fn_80119A04` with `raw` as `PADRead`'s output.
    pub fn read(&mut self, raw: [PadStatus; 4], events: &mut Vec<Event>) {
        self.prev = self.cur;
        self.cur = raw;
        pad_clamp(&mut self.cur);
        for port in 0..4 {
            let s = self.cur[port];
            let r = &mut self.records[port];
            if s.err == 0 {
                let (pb, cb) = (self.prev[port].button, s.button);
                let changed = pb ^ cb;
                put32(r, 0x4, 0x001F_07FE);
                let held = convert_buttons(cb);
                put32(r, 0x8, held);
                put32(r, 0xC, !held);
                put32(r, 0x10, convert_buttons(cb & changed));
                put32(r, 0x14, convert_buttons(pb & changed));
                put16(r, 0x1C, scale_stick(s.stick[0]) as u16);
                put16(r, 0x1E, scale_stick(s.stick[1]).wrapping_neg() as u16);
                put16(r, 0x20, i16::from(s.substick[0]) as u16);
                put16(r, 0x22, i16::from(s.substick[1]) as u16);
                put16(r, 0x1A, u16::from(s.triggers[0]));
                put16(r, 0x18, u16::from(s.triggers[1]));
                self.status[port] = 0x101;
            } else {
                if s.err == -1 {
                    events.push(Event::Engine {
                        addr: PAD_RESET,
                        args: vec![0x8000_0000 >> port],
                    });
                }
                self.status[port] = 0;
                put32(r, 0x4, 0);
            }
        }
    }

    /// `fn_800226E0`: the per-tick player view of the pads.
    pub fn tick(&mut self, ctx: &PadContext, events: &mut Vec<Event>) {
        let g = |off: usize| ctx.g.get(off).copied().unwrap_or(0);
        let mut count = 0;
        for i in 0..4 {
            let port = port_of(self.port_config[i]);
            self.pointers[i] = PadRef::Port(port);
            if get32(&self.records[port], 0x4) & MASK_CONNECTED == MASK_CONNECTED {
                self.connected[i] = 1;
                count += 1;
                self.port_index[i] = i as i32;
            } else {
                self.connected[i] = 0;
                self.pointers[i] = PadRef::Idle;
                self.port_index[i] = -1;
            }
        }
        if count == 0 {
            self.pointers[0] = PadRef::Idle;
        }
        if g(0x3E) != 1 {
            self.copies.iter_mut().for_each(neutralise);
        } else {
            for i in 0..8 {
                if i == 0 && g(0x3F) as i8 == 1 {
                    self.copies[0] = self.pointer_record(self.pointers[0]);
                } else if i == 1 && g(0x40) as i8 == 1 {
                    self.copies[1] = self.pointer_record(self.pointers[1]);
                } else {
                    neutralise(&mut self.copies[i]);
                }
            }
        }
        events.push(Event::Engine {
            addr: AFTER_COPY[0],
            args: vec![],
        });
        self.fill_sticks(ctx);
        for a in &AFTER_COPY[1..] {
            events.push(Event::Engine {
                addr: *a,
                args: vec![],
            });
        }
        for p in 0..4 {
            events.push(Event::Engine {
                addr: PER_PLAYER,
                args: vec![p],
            });
        }
    }

    /// `fn_80022048`: stick angle and magnitude for the eight players.
    pub fn fill_sticks(&mut self, ctx: &PadContext) {
        let g = |off: usize| ctx.g.get(off).copied().unwrap_or(0);
        for i in 0..8 {
            let prev_mag = self.sticks[i].1;
            let (mut x, mut y) = if i < 2 {
                (
                    f32::from(get16(&self.copies[i], 0x1C)),
                    f32::from(get16(&self.copies[i], 0x1E)),
                )
            } else {
                (0.0, 0.0)
            };
            let (angle, mag) = if x > DEAD || x < DEAD_NEG || y > DEAD || y < DEAD_NEG {
                x = if x > DEAD {
                    x - DEAD
                } else if x < DEAD_NEG {
                    x + DEAD
                } else {
                    0.0
                };
                y = if y > DEAD {
                    y - DEAD
                } else if y < DEAD_NEG {
                    y + DEAD
                } else {
                    0.0
                };
                let a: u32 = if x == 0.0 {
                    if y > 0.0 {
                        0x4000
                    } else {
                        0xC000
                    }
                } else if y == 0.0 {
                    if x > 0.0 {
                        0
                    } else {
                        0x8000
                    }
                } else {
                    (BAMS_PER_RAD * f64::from(atan2f(y, x))) as i32 as u32
                };
                let a = if !ctx.camera_active {
                    a
                } else if (i as i32) < ctx.viewports {
                    a.wrapping_sub(ctx.camera_angles[i])
                } else {
                    a.wrapping_sub(ctx.camera_angles[0])
                };
                let yy = y * y;
                let sq = (f64::from(x) * f64::from(x) + f64::from(yy)) as f32;
                let root = if sq > 0.0 { sqrt_newton(sq) } else { sq };
                let mut m = (f64::from(sq) * f64::from(root)) as f32 / MAG_DIV;
                if m > MAG_MAX {
                    m = MAG_MAX;
                }
                (a & 0xFFFF, m)
            } else {
                (0, 0.0)
            };
            self.raw_sticks[i] = (angle, mag);
            self.sticks[i] = if g(0x3E) == 1 && g(0x3F + i) == 1 {
                (angle, mag)
            } else {
                (0, 0.0)
            };
            // "Not >= 0.5" natively, so a NaN previous magnitude counts as below.
            let was_pushed = matches!(
                prev_mag.partial_cmp(&PUSH),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            );
            self.pushed[i] = u8::from(self.sticks[i].1 >= PUSH && !was_pushed);
        }
    }
}
