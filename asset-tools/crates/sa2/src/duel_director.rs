//! Stage 42's duel director, task `ControlShadow` (stg42D.rel update +0x3CC)
//! with its gameplay helpers (+0x0 lane clamp, +0x8C camera target,
//! +0x168 / +0x280 / +0x334 facing checks). It is a 13-mode state machine
//! that paces the rival (speed scaling by the gauge), aims the camera, plays
//! voice lines, detects the fall/lose and win/turn outcomes and stages the
//! ending. Engine queries are inputs (a per-address queue of returns);
//! every engine call and the module's local task-spawning helpers
//! (+0x22070 dialogue window, +0x18620 message effect, +0x20470 gauge) are
//! recorded with their arguments. Player structures and engine globals it
//! writes are part of the returned world.
use crate::{duel_platform, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

/// Player object (`0x801E7368[i]`) fields the director reads.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Body {
    pub action: i8,
    /// +4 flags (halfword) and +6 (unsigned halfword).
    pub flags4: i16,
    pub field6: u16,
    /// +0xC rotation Y.
    pub yaw: i32,
    pub pos: [f32; 3],
}
/// Player data (`0x801E7328[i]`) fields the director reads or writes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Data {
    pub byte1: i8,
    /// +3 character.
    pub character: i8,
    pub flags10: i16,
    pub field1c: i16,
    /// +0x48 gauge.
    pub gauge: f32,
    pub speed: f32,
    pub field68: f32,
    pub field6c: f32,
    pub fieldec: f32,
    pub field1b4: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct World {
    /// Game bytes at 0x801CBD88 + 0x30, 0x31, 0x32, 0x34, 0x37, 0x39, 0x42.
    pub g30: i8,
    pub g31: i8,
    pub g32: i8,
    pub g34: i8,
    pub g37: i8,
    pub g39: i8,
    pub g42: i8,
    /// Game word +0x7C.
    pub g7c: u32,
    /// Camera block at 0x801E4FF8: word, float, angle word, float (raw bits).
    pub camera: [u32; 4],
    /// Bytes at 0x803AD484+1, 0x803AD488+1, 0x803AD494+1, 0x803AD498+1.
    pub flags: [u8; 4],
    /// 0x803AD5D0 is non-null with a zero +0xC word.
    pub ready: bool,
    pub bodies: [Body; 2],
    pub data: [Data; 2],
    /// Task +0xC (child task) and 0x801E7388 slots, as opaque values.
    pub child: u32,
    pub slots: [u32; 2],
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Work {
    pub mode: i8,
    pub flag: i8,
    pub timer: i16,
    pub limit: i16,
    /// +8..+0x10 and +0x14..+0x1C.
    pub point: [f32; 3],
    pub target: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorConstants {
    pub f: Vec<f32>,
    pub bams_per_radian: f64,
    pub lane: f32,
    pub point_base: [f32; 3],
    pub dialogue: [u32; 2],
}
pub fn constants(rel_bytes: &[u8]) -> Result<DirectorConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let s4 = rel.data(4)?;
    let s5 = rel.data(5)?;
    let word = |d: &[u8], at: usize| -> Result<u32> {
        d.get(at..at + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    };
    let mut f = vec![0f32; 48];
    for (i, v) in f.iter_mut().enumerate() {
        *v = f32::from_bits(word(s4, 4 * i)?);
    }
    // The table the director's constants come from: every lfs/lfd it issues
    // resolves into section 4 +0x0..+0xBC; spot-check its anchors.
    if duel_platform::target(&rel, 0x492, 4)? != 0x24
        || duel_platform::target(&rel, 0x1da, 4)? != 0x10
    {
        return Err(duel_platform::invalid());
    }
    let hi = word(s4, 0x18)? as u64;
    let lo = word(s4, 0x1c)? as u64;
    let base = duel_platform::target(&rel, 0x7b2, 5)?;
    Ok(DirectorConstants {
        f,
        bams_per_radian: f64::from_bits(hi << 32 | lo),
        lane: f32::from_bits(word(s5, duel_platform::target(&rel, 0x22, 5)? as usize)?),
        point_base: [
            f32::from_bits(word(s5, base as usize)?),
            f32::from_bits(word(s5, base as usize + 4)?),
            f32::from_bits(word(s5, base as usize + 8)?),
        ],
        dialogue: [
            duel_platform::target(&rel, 0x692, 5)?,
            duel_platform::target(&rel, 0x6ae, 5)?,
        ],
    })
}

/// Engine return values by function address (hex), consumed in call order.
/// Integers are numbers; floating-point returns are exact IEEE double bit
/// patterns as "0x..." strings.
pub type Returns = BTreeMap<String, VecDeque<Value>>;

struct Run<'a> {
    c: &'a DirectorConstants,
    w: &'a mut World,
    k: &'a mut Work,
    returns: &'a mut Returns,
    calls: Vec<Value>,
}
/// `stfs` of a double: the architectural single-precision store, which takes
/// the sign, exponent and leading fraction bits without rounding (PowerPC
/// Book I, "Floating-Point Store Instructions"; QEMU's helper_tosingle).
pub(crate) fn stfs(v: f64) -> f32 {
    let arg = v.to_bits();
    let exp = (arg >> 52) & 0x7ff;
    let word = if exp > 896 || arg & !(1u64 << 63) == 0 {
        ((arg >> 32) & 0xc000_0000) | ((arg >> 29) & 0x3fff_ffff)
    } else if exp >= 874 {
        let sign = arg >> 63;
        let frac = ((1u64 << 52) | (arg & ((1u64 << 52) - 1))) >> (897 - exp);
        (sign << 31) | (frac >> 29)
    } else {
        (arg >> 63) << 31
    };
    f32::from_bits(word as u32)
}
fn bits(v: f32) -> u32 {
    v.to_bits()
}
impl Run<'_> {
    fn k(&self, off: usize) -> f32 {
        self.c.f[off / 4]
    }
    fn ret(&mut self, a: u32) -> Result<f64> {
        let v = self
            .returns
            .get_mut(&format!("0x{a:08x}"))
            .and_then(|q| q.pop_front())
            .ok_or_else(|| Error::invalid(format!("missing return for 0x{a:08x}")))?;
        match &v {
            Value::String(h) => u64::from_str_radix(h.trim_start_matches("0x"), 16)
                .map(f64::from_bits)
                .map_err(|_| Error::invalid("bad float bits")),
            _ => v
                .as_f64()
                .ok_or_else(|| Error::invalid("bad engine return")),
        }
    }
    fn call(&mut self, a: u32, args: Value) {
        self.calls
            .push(json!({ "engine": format!("0x{a:08x}"), "args": args }));
    }
    fn query(&mut self, a: u32, args: Value) -> Result<f64> {
        let v = self.ret(a)?;
        self.calls
            .push(json!({ "engine": format!("0x{a:08x}"), "args": args, "returned": v }));
        Ok(v)
    }
    /// trunc(base + range * rand / 32768) into the work limit.
    fn rand_limit(&mut self, base: usize, range: usize) -> Result<i16> {
        let r = self.query(0x8010d940, json!([]))? as i32;
        let u = self.k(0x4c) * (r as f64 as f32);
        let v = self.k(base) + self.k(range) * u;
        Ok(v as i32 as i16)
    }
    fn close_menus(&mut self) -> Result<()> {
        for i in 0..2 {
            if self.query(0x800118e0, json!([3, i]))? as i32 == 1 {
                self.call(0x8001162c, json!([i]));
            }
        }
        Ok(())
    }
    /// Facing test shared by the helpers: atan2 of player 1 relative to player 2.
    fn facing(&mut self) -> Result<bool> {
        let (p1, p2) = (self.w.bodies[0], self.w.bodies[1]);
        let dx = p1.pos[0] - p2.pos[0];
        let dz = p1.pos[2] - p2.pos[2];
        let a = self.query(0x80110e20, json!([bits(dx), bits(dz)]))?;
        let ang = 0x4000i32.wrapping_sub((self.c.bams_per_radian * a) as i32);
        let diff = self.query(0x8002b678, json!([ang, p2.yaw]))? as i32;
        Ok(diff < 0x5000)
    }
    fn distance(&mut self) -> Result<f64> {
        let (p1, p2) = (self.w.bodies[0].pos, self.w.bodies[1].pos);
        self.query(0x8011b314, json!([p1.map(bits), p2.map(bits)]))
    }
    /// +0x168
    fn h168(&mut self) -> Result<bool> {
        let a = self.w.bodies[0].action;
        let check = match a {
            0xd => !(self.w.data[1].gauge > self.k(0x10)),
            0 | 1 | 3 | 4 | 0x42 | 0x43 => true,
            _ => false,
        };
        if !check {
            return Ok(false);
        }
        if self.facing()? {
            return Ok(true);
        }
        Ok(self.distance()? < self.k(0x20) as f64)
    }
    /// +0x280
    fn h280(&mut self) -> Result<bool> {
        if self.w.bodies[0].action != 0xd {
            return Ok(false);
        }
        if self.facing()? {
            return Ok(true);
        }
        Ok(self.w.data[1].speed < self.w.data[1].fieldec)
    }
    /// +0x334
    fn h334(&mut self) -> Result<bool> {
        if !(0x3d..=0x44).contains(&self.w.bodies[0].action) {
            return Ok(false);
        }
        self.facing()
    }
    fn pace(&mut self, scale: f32) {
        let d = self.w.data[1];
        self.call(
            0x8003a0b0,
            json!([1, bits(scale * d.speed), bits(d.field68), bits(d.field6c)]),
        );
    }
    fn gauge_factor(&self) -> f32 {
        let g = self.w.data[1].gauge;
        if g < self.k(0x10) {
            self.k(0x64)
        } else if g < self.k(0xc) {
            self.k(0x68)
        } else if g < self.k(0x6c) {
            self.k(0x70)
        } else {
            self.k(0x74)
        }
    }
    fn fast(&self) -> bool {
        let (d1, d2) = (self.w.data[0], self.w.data[1]);
        d1.speed > d1.fieldec && d2.speed > d2.fieldec
    }
    fn signal(&mut self) {
        self.w.flags[1] = 1;
        self.w.flags[3] = 1;
    }
    fn camera_angle(&mut self, angle: i32, weight: usize) {
        self.w.camera[2] = angle as u32;
        self.w.camera[3] = bits(self.k(weight));
    }
    fn landed_turn(&self) -> bool {
        self.k.timer >= 2 && self.w.bodies[1].flags4 & 3 != 0
    }
    fn high_above(&self) -> bool {
        self.w.bodies[1].pos[1] - self.w.data[0].field1b4 > self.k(0x78)
    }

    fn update(&mut self) -> Result<()> {
        let w = &self.w;
        if w.g39 == 1 || w.g37 & 2 != 0 || w.g42 == 1 {
            return Ok(());
        }
        if self.w.data[0].flags10 & 0x4000 != 0 || self.w.data[1].flags10 & 0x4000 != 0 {
            self.k.mode = 0xd;
            self.w.g31 = 0;
            self.w.g32 = 0;
        }
        let (p1, p2) = (self.w.bodies[0], self.w.bodies[1]);
        // fcmpo/blt then fcmpo/bge: lost when below Y -50 or 150 behind.
        let lost = p2.pos[1] < self.k(0x24) || p2.pos[0] < p1.pos[0] - self.k(0x28);
        if lost && self.k.mode != 9 && self.k.mode != 0xd {
            self.call(0x80087ebc, json!([1, "data[1]+0x230"]));
            self.k.mode = 9;
            self.k.timer = 0;
            self.call(0x800466b4, json!([0, "data[1]"]));
            self.close_menus()?;
            match self.w.data[1].character {
                0 => self.call(0x800117ec, json!([0x8ce, 3])),
                1 => self.call(0x800117ec, json!([0x8b6, 3])),
                _ => {}
            }
        }
        let m = self.query(0x8001ccbc, json!([]))? as i32;
        if m == 7 || self.k.mode == 0 || self.k.mode == 1 {
            self.w.camera = [0, bits(self.k(0x2c)), 0, bits(self.k(0x2c))];
            if self.k.mode != 0 && self.k.mode != 1 {
                self.k.mode = 0xb;
            }
            for slot in 0..2 {
                self.call(
                    0x8003a0b0,
                    json!([slot, bits(self.k(0x30)), bits(self.k(0)), bits(self.k(0))]),
                );
            }
        } else if self.k.mode == 0xb {
            self.k.mode = 2;
        }
        match self.k.mode {
            0 => self.mode0()?,
            1 => self.mode1()?,
            2 | 12 => self.mode2()?,
            3 => self.mode3(),
            4 => self.mode4(),
            5 => self.mode5()?,
            6 => self.mode6()?,
            7 => {
                if self.landed_turn() {
                    self.k.mode = 2;
                    self.k.timer = 0;
                }
            }
            8 => {
                if self.k.timer > self.k.limit || self.w.bodies[1].field6 != 0 {
                    self.k.mode = 2;
                    self.k.timer = 0;
                }
            }
            9 => self.mode9()?,
            10 => self.mode10()?,
            _ => {}
        }
        self.tail()
    }
    fn mode0(&mut self) -> Result<()> {
        if self.query(0x8001ccbc, json!([]))? as i32 != 0x10 {
            return Ok(());
        }
        self.call(0x80022bdc, json!([3]));
        self.call(0x8001dccc, json!([]));
        self.k.mode = 1;
        self.k.limit = 0x168;
        match self.w.data[1].character {
            0 => self.calls.push(json!({ "local": "0x22070", "args": [0, format!("s5+0x{:x}", self.c.dialogue[0])] })),
            1 => self.calls.push(json!({ "local": "0x22070", "args": [0, format!("s5+0x{:x}", self.c.dialogue[1])] })),
            _ => {}
        }
        let (a, b) = if self.w.data[1].byte1 == 0 {
            (0xf000, 0x4000)
        } else {
            (0x4000, 0x8000)
        };
        self.place_point();
        self.call(
            0x800c7844,
            json!([0, { "f0": bits(self.k(0x2c)), "w1": a, "point": self.k.point.map(bits),
                        "f3": bits(self.k(0x3c)), "f4": bits(self.k(0x40)), "w5": b }]),
        );
        Ok(())
    }
    fn place_point(&mut self) {
        self.k.point = self.w.bodies[1].pos;
        self.k.point[1] = self.k(0x34) + self.c.point_base[1];
        self.k.point[0] += self.k(0x38);
    }
    fn mode1(&mut self) -> Result<()> {
        self.place_point();
        let v = [self.k(0), self.k(8), self.k(0)].map(bits);
        self.call(0x8003a2e4, json!([1, self.w.slots[0], v, 3]));
        self.call(0x8003a2e4, json!([0, self.w.slots[1], v, 3]));
        if self.k.timer > self.k.limit {
            if self.k.flag == 0 {
                let mut at = self.c.point_base;
                at[1] += self.k(0x44);
                self.k.flag = 1;
                let m = self.query(0x8001dd1c, json!([]))? as i32;
                let pick = match m {
                    0 => -1,
                    1 => 0,
                    2 => 1,
                    _ => 2,
                };
                if pick != -1 {
                    let pick = if self.w.data[0].byte1 == 0 {
                        pick + 3
                    } else {
                        pick
                    };
                    self.calls.push(
                        json!({ "local": "0x18620", "args": [at.map(bits), 0, 0, pick, 2, 0] }),
                    );
                }
            }
            self.call(0x80022c1c, json!([0]));
            self.call(0x8001dce0, json!([]));
            self.k.timer = 0;
            self.k.mode = 5;
            self.k.limit = self.rand_limit(0x3c, 0x48)?;
        } else if self.k.timer as i32 > self.k.limit as i32 - 0x14 {
            if self.query(0x800c3974, json!([0]))? as i32 == 0xd {
                self.call(0x800c7540, json!([0]));
            }
            self.call(0x800cd434, json!([0, { "w0": 0, "from": "bodies[0]+0x14", "to": "work+0x14", "f3": bits(self.k(0x50)) }]));
        } else if self.k.timer == 0x78 {
            let line = match self.w.data[1].character {
                0 => Some(0x8c7),
                1 => Some(0x8b0),
                _ => None,
            };
            if let Some(line) = line {
                self.close_menus()?;
                self.call(0x800117ec, json!([line, 3]));
            }
        }
        Ok(())
    }
    fn mode2(&mut self) -> Result<()> {
        let (p1, p2) = (self.w.bodies[0], self.w.bodies[1]);
        if p2.pos[0] < p1.pos[0] - self.k(0x54) {
            let a = self.query(
                0x80110e20,
                json!([bits(p1.pos[0] - p2.pos[0]), bits(p1.pos[2] - p2.pos[2])]),
            )?;
            let ang = 0x4000i32.wrapping_sub((self.c.bams_per_radian * a) as i32);
            self.camera_angle(ang, 0x2c);
            if self.fast() {
                self.pace(self.k(0x58));
            }
        } else {
            self.camera_angle(0, 0x2c);
            if p2.pos[0] > self.k(0x5c) + p1.pos[0] && self.fast() {
                let f = if p2.action == 0xd {
                    self.k(0x60)
                } else {
                    self.gauge_factor()
                };
                self.pace(f);
            }
        }
        if self.h280()? || self.h334()? {
            self.k.timer = 0;
            self.signal();
            self.k.mode = 3;
            return Ok(());
        }
        if self.w.data[1].gauge <= self.k(0xc) && self.h168()? {
            self.signal();
            self.k.mode = 6;
            self.k.timer = 0;
            return Ok(());
        }
        if self.k.timer > 0x3c {
            self.k.timer = 0;
            self.k.mode = 5;
            self.k.limit = self.rand_limit(0x5c, 0x48)?;
        }
        Ok(())
    }
    fn mode3(&mut self) {
        if self.landed_turn() {
            self.k.mode = 0xc;
            return;
        }
        self.w.flags[3] = 1;
        if self.k(0) == self.w.data[1].field68 && self.high_above() {
            self.k.mode = 4;
            self.k.timer = 0;
        }
    }
    fn mode4(&mut self) {
        if self.landed_turn() {
            self.k.mode = 0xc;
            return;
        }
        if self.w.data[1].field68 <= self.k(0x7c) && self.high_above() {
            self.k.mode = 7;
            self.k.timer = 0;
            self.signal();
        }
    }
    fn mode5(&mut self) -> Result<()> {
        if self.h280()? || self.h334()? {
            self.k.timer = 0;
            self.signal();
            self.k.mode = 3;
            return Ok(());
        }
        if self.k.timer > self.k.limit {
            self.k.mode = 2;
            self.k.timer = 0;
            return Ok(());
        }
        let (p1, p2) = (self.w.bodies[0], self.w.bodies[1]);
        if p2.pos[1] < self.k(0x80) {
            let x = (p2.pos[0] - self.k(0x28)) / self.k(0x44);
            let v = self.query(0x80111e34, json!([bits(x)]))?;
            if v >= self.k(0x84) as f64 {
                self.camera_angle(0, 0x2c);
                self.k.timer = 0;
                self.signal();
                self.k.mode = 6;
                return Ok(());
            }
        }
        let reach = self.k(0x88) + (self.k(0x8c) + p1.pos[0]) - self.k(0x90) * self.w.data[1].gauge;
        if p2.pos[0] > reach && self.w.data[0].flags10 & 0x4000 == 0 && self.w.ready {
            self.k.timer = 0;
            self.k.mode = 0xa;
            self.w.g30 = 0;
            self.w.g31 = 1;
            self.w.g32 = 0;
            self.close_menus()?;
            match self.w.data[1].character {
                1 => self.call(0x800117ec, json!([0x8a8, 3])),
                0 => self.call(0x800117ec, json!([0x8ca, 3])),
                _ => {}
            }
            self.w.g34 = 0x3c;
            return Ok(());
        }
        self.camera_angle(0, 0x2c);
        if self.w.data[1].gauge <= self.k(0x10) && self.h168()? {
            self.signal();
            self.k.mode = 6;
            self.k.timer = 0;
            return Ok(());
        }
        let (p1x, p2x) = (p1.pos[0], p2.pos[0]);
        if p2x < self.k(0x94) + p1x && self.fast() {
            self.pace(self.k(0x98));
            return Ok(());
        }
        if p2x > self.k(0x9c) + p1x && self.fast() {
            let f = self.gauge_factor();
            self.pace(f);
            return Ok(());
        }
        if p2x < p1x - self.k(0x54) && self.fast() {
            self.pace(self.k(0x58));
        }
        Ok(())
    }
    fn mode6(&mut self) -> Result<()> {
        self.camera_angle(0, 0x2c);
        if self.landed_turn() {
            self.k.mode = 5;
            self.k.timer = 0;
            self.k.limit = self.rand_limit(0x48, 0x3c)?;
            return Ok(());
        }
        self.w.flags[3] = 1;
        let f68 = self.w.data[1].field68;
        let mut turn = self.k(0) == f68 && self.high_above();
        if !turn && self.w.bodies[1].pos[1] > self.k(0x9c) && self.h168()? {
            turn = true;
        }
        if turn || f68 < self.k(0) {
            self.k.mode = 4;
            self.k.timer = 0;
        }
        Ok(())
    }
    fn mode9(&mut self) -> Result<()> {
        let t = self.k.timer;
        if t == 0x3c {
            if self.w.data[1].character == 0 {
                self.close_menus()?;
                self.call(0x800117ec, json!([0x8cf, 3]));
            }
        } else if t < 0x12c {
            let mut v = self.w.bodies[0].pos;
            v[0] += self.k(0xa4);
            v[1] = self.k(0xa8);
            v[2] = self.k(0);
            self.call(0x80039754, json!([1, bits(v[0]), bits(v[1]), bits(v[2])]));
        } else if t == 0x12c {
            let mut v = self.w.bodies[1].pos;
            v[1] = self.k(0xac);
            v[2] = self.k(0);
            self.call(0x80087ebc, json!([1, v.map(bits)]));
            self.call(0x8006b710, json!([8, 0, 0, 0]));
        } else if t == 0x14a {
            let mut v = self.w.bodies[1].pos;
            v[1] = self.k(0x90);
            v[2] = self.k(0);
            self.call(0x80039754, json!([1, bits(v[0]), bits(v[1]), bits(v[2])]));
            self.k.mode = 2;
            self.k.timer = 0;
        }
        Ok(())
    }
    fn mode10(&mut self) -> Result<()> {
        if self.w.data[0].field1c & 3 == 0 {
            let (p1, p2) = (self.w.bodies[0], self.w.bodies[1]);
            let a = self.query(
                0x80110e20,
                json!([bits(p1.pos[0] - p2.pos[0]), bits(p1.pos[2] - p2.pos[2])]),
            )?;
            let ang = 0x4000i32.wrapping_sub((self.c.bams_per_radian * a) as i32);
            self.camera_angle(ang, 0xa0);
        }
        if self.w.bodies[1].field6 != 0 {
            self.k.mode = 2;
            self.k.timer = 0;
            return Ok(());
        }
        if self.w.g34 > 0 && self.k.timer < 0x78 {
            return Ok(());
        }
        if self.w.data[0].flags10 & 0x4000 != 0 || !self.w.ready {
            return Ok(());
        }
        self.k.timer = 0;
        self.k.mode = 8;
        self.w.flags[0] = 1;
        self.w.flags[2] = 1;
        self.k.limit = 0xb4;
        Ok(())
    }
    fn tail(&mut self) -> Result<()> {
        // +0x0: keep the rival inside the lane.
        let p2 = self.w.bodies[1];
        if p2.pos[2].abs() >= self.c.lane {
            let z = if p2.pos[2] < self.k(0) {
                -self.c.lane
            } else {
                self.c.lane
            };
            self.call(
                0x80039754,
                json!([1, bits(p2.pos[0]), bits(p2.pos[1]), bits(z)]),
            );
        }
        // +0x8C: camera target.
        let d = self.distance()?;
        if d > self.k(4) as f64 {
            let p1x = self.w.bodies[0].pos[0];
            let mut t = self.k.target;
            let goals = [
                (self.k(4) + p1x, self.k(8)),
                (self.k(0), self.k(0xc)),
                (self.k(0), self.k(0x10)),
            ];
            for (i, (goal, rate)) in goals.into_iter().enumerate() {
                t[i] = stfs(self.query(0x8002abf4, json!([bits(t[i]), bits(goal), bits(rate)]))?);
            }
            self.k.target = t;
        } else {
            self.k.target = self.w.bodies[1].pos;
        }
        let m = self.query(0x8001ccbc, json!([]))? as i32;
        if m == 7 || self.w.g7c == 0 {
            self.w.data[1].gauge = self.k(0xb0);
            if self.w.child != 0 {
                self.call(0x80071888, json!([self.w.child]));
            }
            self.k.flag = 0;
            self.k.timer = 0;
            self.k.mode = 0;
        } else {
            self.k.timer = self.k.timer.wrapping_add(1);
        }
        if self.w.data[1].gauge > self.k(0) && self.w.child == 0 {
            self.calls
                .push(json!({ "local": "0x20470", "args": ["task", "data[1]+0x48"] }));
        }
        if self.w.data[1].gauge <= self.k(0) {
            self.call(0x8003a2e4, json!([0, self.w.slots[1], 0, 3]));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    world: World,
    work: Work,
    returns: Returns,
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let c = constants(rel_bytes)?;
    let mut r: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("director request: {e}")))?;
    let mut run = Run {
        c: &c,
        w: &mut r.world,
        k: &mut r.work,
        returns: &mut r.returns,
        calls: Vec::new(),
    };
    run.update()?;
    let calls = std::mem::take(&mut run.calls);
    Ok(json!({ "calls": calls, "world": r.world, "work": r.work }))
}
