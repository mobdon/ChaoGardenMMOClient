//! The enemy motion sequencer, main.dol fn_80015C40 (and fn_80015BA8),
//! which steps the 0x30-byte block at enemy work +0x1E0 every frame. It
//! picks the motion, advances its frame and moves on to the next motion
//! the table names; enemy logic reads its frame and motion numbers.
//!
//! The table rows are 16 bytes in the stage REL: an NJS_MOTION pointer
//! (its +4 word is the frame count), the mode (s16), the next row (s16),
//! the blend step (f32) and the frame step (f32).
use crate::ninja_trig::fmadds;

/// One motion table row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionRow {
    /// Identifies the NJS_MOTION (its offset in the REL's data section).
    pub motion: u32,
    /// The NJS_MOTION's frame count (`nbFrame`).
    pub frames: u32,
    pub mode: i16,
    pub next: i16,
    pub blend: f32,
    pub speed: f32,
}

/// The block at enemy work +0x1E0.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Motion {
    /// +0x00: the frame.
    pub frame: f32,
    /// +0x08: the frame the next motion starts on (looping modes), or the
    /// frame during a blend.
    pub start: f32,
    /// +0x0C: frame step scale (modes 8 and 9).
    pub scale: f32,
    /// +0x10: the sequencer mode.
    pub mode: i8,
    /// +0x11: bit 0 the motion ended this frame, bit 1 it ends next frame.
    pub ended: u8,
    /// +0x12: the playing row.
    pub current: i8,
    /// +0x13: the requested row.
    pub request: i8,
    /// +0x14: the row playing at the start of this step.
    pub previous: i8,
    /// +0x15: the row after the current one.
    pub next: i8,
    /// +0x16: frames since the row last changed.
    pub count: i16,
    /// +0x20, +0x24: the motions a blend goes from and to.
    pub blend_from: Option<u32>,
    pub blend_to: Option<u32>,
    /// +0x28, +0x2C: the frames a blend goes from.
    pub blend_frame: f32,
    pub blend_start: f32,
}

fn frames(t: &[MotionRow], i: i8) -> f32 {
    t[i as usize].frames as f32
}

impl Motion {
    /// fn_80015BA8: the frame a newly started row begins on.
    fn start_frame(&mut self, t: &[MotionRow]) {
        match self.mode {
            6 | 7 => self.frame = frames(t, self.current) - 1.0,
            3 | 8 | 10 => {
                self.frame = self.start;
                self.start = 0.0;
            }
            _ => self.frame = 0.0,
        }
    }

    /// Move on to the row after the requested one (modes 4, 6 and 9).
    fn chain(&mut self, t: &[MotionRow]) {
        let r = self.request as usize;
        self.request = t[r].next as i8;
        self.current = self.request;
        self.mode = t[self.current as usize].mode as i8;
        self.next = t[self.request as usize].next as i8;
        self.ended |= 1;
        self.start_frame(t);
    }

    fn wrap(&mut self, n: f32) {
        while n <= self.frame {
            self.frame -= n;
        }
        while 0.0 > self.frame {
            self.frame += n;
        }
    }

    /// fn_80015C40: one frame.
    pub fn step(&mut self, t: &[MotionRow]) {
        let req = self.request;
        self.previous = self.current;
        self.ended = 0;
        if self.current != req && self.mode != 2 && self.mode != 0 && self.mode != 11 {
            self.mode = 1;
            self.count = 0;
            if t[req as usize].mode == 8 && t[self.current as usize].mode == 8 {
                // Carry the phase across two looping (mode 8) rows.
                let cur = frames(t, self.current);
                let to = frames(t, req);
                let s = to * (cur - self.frame) / cur;
                self.start = fmadds(f32::from_bits(0x3dcc_cccd), to, s);
                if self.start >= to {
                    self.start -= to;
                }
            }
        } else {
            self.count = self.count.wrapping_add(1);
        }
        match self.mode {
            0 => {
                self.frame = 0.0;
                self.current = req;
                self.mode = 1;
                self.start = 0.0;
                self.frame = 0.0;
            }
            1 | 2 => {
                if self.mode == 1 {
                    let row = t[req as usize];
                    if row.blend != 1.0 {
                        self.blend_from = Some(t[self.current as usize].motion);
                        self.blend_to = Some(row.motion);
                        self.blend_frame = self.frame;
                        self.blend_start = self.start;
                        self.frame = 0.0;
                    } else {
                        self.frame = 1.0;
                    }
                    self.current = req;
                    self.mode = 2;
                }
                self.frame += t[self.current as usize].blend;
                if self.frame >= 1.0 {
                    self.mode = t[self.current as usize].mode as i8;
                    self.blend_from = None;
                    self.blend_to = None;
                    self.blend_frame = 0.0;
                    self.blend_start = 0.0;
                    match self.mode {
                        6 | 7 => self.frame = frames(t, self.current) - 1.0,
                        3 | 8 | 10 => {
                            self.frame = self.start;
                            self.start = 0.0;
                        }
                        _ => self.frame = 0.0,
                    }
                }
            }
            3 => {
                let row = t[req as usize];
                self.frame += row.speed;
                let n = row.frames as f32;
                if n - 1.0 <= self.frame + row.speed {
                    self.ended |= 2;
                }
                self.wrap(n);
            }
            4 => {
                let row = t[req as usize];
                self.frame += row.speed;
                let last = row.frames as f32 - 1.0;
                if last <= self.frame {
                    self.chain(t);
                } else if last <= self.frame + row.speed {
                    self.ended |= 2;
                }
            }
            5 => {
                let row = t[req as usize];
                self.frame += row.speed;
                let last = row.frames as f32 - 1.0;
                if last <= self.frame {
                    self.frame = last;
                    self.ended |= 1;
                } else if last <= self.frame + row.speed {
                    self.ended |= 2;
                }
            }
            6 => {
                self.frame -= t[req as usize].speed;
                if 0.0 > self.frame {
                    self.chain(t);
                }
            }
            7 => {
                self.frame -= t[req as usize].speed;
                if 0.0 > self.frame {
                    self.frame = 0.0;
                    self.ended |= 1;
                }
            }
            8 => {
                let row = t[req as usize];
                self.frame = fmadds(self.scale, row.speed, self.frame);
                self.wrap(row.frames as f32);
            }
            9 => {
                self.frame += self.scale;
                if frames(t, req) - 1.0 <= self.frame {
                    self.chain(t);
                }
            }
            10 => self.wrap(frames(t, req)),
            _ => {
                self.blend_from = None;
                self.blend_to = None;
                self.blend_frame = 0.0;
                self.blend_start = 0.0;
            }
        }
    }
}

/// Read `count` motion table rows at `section`:`offset` of a stage REL
/// (module id `module`). Each row's motion pointer must be a relocation
/// into the REL itself.
pub fn read_table(
    rel_bytes: &[u8],
    module: u32,
    section: u8,
    offset: u32,
    count: usize,
) -> crate::Result<Vec<MotionRow>> {
    let rel = crate::city_rel::CityRel::read_identified(rel_bytes, module)?;
    let bad = || crate::Error::invalid("enemy motion table row");
    let word = |s: u8, at: u32| -> crate::Result<u32> {
        let d = rel.data(s)?;
        let b = d.get(at as usize..at as usize + 4).ok_or_else(bad)?;
        Ok(u32::from_be_bytes(b.try_into().unwrap()))
    };
    (0..count as u32)
        .map(|i| {
            let at = offset + 16 * i;
            let (ms, mo) = rel.local_pointer(section, at)?.ok_or_else(bad)?;
            let half = word(section, at + 4)?;
            Ok(MotionRow {
                motion: mo,
                frames: word(ms, mo + 4)?,
                mode: (half >> 16) as i16,
                next: half as i16,
                blend: f32::from_bits(word(section, at + 8)?),
                speed: f32::from_bits(word(section, at + 12)?),
            })
        })
        .collect()
}
