//! A3VE OBJ affine matrix writer (0x236C) and allocator (0x2514).
//! Native full-turn tables at 0x66C78/0x66E78 are independent of pinball trig.
use super::{
    frame::{FrameError, FrameMemory},
    math::signed_divide,
};

pub const MATRIX_BASE: u32 = 0x03000e50;
pub const COUNT: u32 = 0x0300005c;
pub const SIN_BASE: u32 = 0x08066c78;
pub const COS_BASE: u32 = 0x08066e78;

#[derive(Debug, Clone, Copy)]
pub struct Request {
    pub index: u32,
    pub angle: u32,
    pub scale_x: u32,
    pub scale_y: u32,
    pub flags: u32,
}

/// The low byte selects one of 32 matrices. Other IDs are native no-ops.
/// Zero scales mean 0x100. Angle zero bypasses both tables and ignores flips.
pub fn write(memory: &mut dyn FrameMemory, request: Request) -> Result<(), FrameError> {
    let index = request.index as u8;
    if index & 0xe0 != 0 {
        return Ok(());
    }
    let angle = request.angle as u8;
    let sx = i32::from(match request.scale_x as u16 {
        0 => 256,
        v => v,
    });
    let sy = i32::from(match request.scale_y as u16 {
        0 => 256,
        v => v,
    });
    let base = MATRIX_BASE + u32::from(index) * 32;
    if angle == 0 {
        memory.write_u16(base + 6, signed_divide(65536, sx) as u16)?;
        memory.write_u16(base + 14, 0)?;
        memory.write_u16(base + 22, 0)?;
        memory.write_u16(base + 30, signed_divide(65536, sy) as u16)?;
    } else {
        let offset = u32::from(angle) * 2;
        let cosine = i32::from(memory.read_u16(COS_BASE + offset)? as i16) * 256;
        let x_sign = if request.flags & 1 == 0 { 1 } else { -1 };
        let y_sign = if request.flags & 2 == 0 { 1 } else { -1 };
        // The source writes PA before its first sine-table read.
        memory.write_u16(base + 6, signed_divide(cosine * x_sign, sx) as u16)?;
        let sine = i32::from(memory.read_u16(SIN_BASE + offset)? as i16) * 256;
        memory.write_u16(base + 14, signed_divide(-sine * x_sign, sx) as u16)?;
        memory.write_u16(base + 22, signed_divide(sine * y_sign, sy) as u16)?;
        memory.write_u16(base + 30, signed_divide(cosine * y_sign, sy) as u16)?;
    }
    Ok(())
}

/// Native return value is zero on exhaustion as well as successful slot zero.
/// Caller-visible count and original low-byte/halfword truncations are retained.
pub fn allocate(
    memory: &mut dyn FrameMemory,
    angle: u32,
    scale_x: u32,
    scale_y: u32,
    flags: u32,
) -> Result<u8, FrameError> {
    let index = memory.read_u8(COUNT)?;
    if index & 0xe0 != 0 {
        return Ok(0);
    }
    write(
        memory,
        Request {
            index: u32::from(index),
            angle,
            scale_x,
            scale_y,
            flags,
        },
    )?;
    let index = memory.read_u8(COUNT)?;
    memory.write_u8(COUNT, index.wrapping_add(1))?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    struct Memory<'a> {
        oam: Vec<u8>,
        count: u8,
        writes: Vec<Value>,
        rom: Option<&'a [u8]>,
    }
    impl FrameMemory for Memory<'_> {
        fn read_u8(&self, a: u32) -> Result<u8, FrameError> {
            if a == COUNT {
                return Ok(self.count);
            }
            if let Some(i) = a.checked_sub(MATRIX_BASE) {
                if let Some(v) = self.oam.get(i as usize) {
                    return Ok(*v);
                }
            }
            if let Some(i) = a.checked_sub(0x08000000) {
                if let Some(rom) = self.rom {
                    if let Some(v) = rom.get(i as usize) {
                        return Ok(*v);
                    }
                }
            }
            Err(FrameError::MissingRead {
                address: a,
                width: 1,
            })
        }
        fn read_u16(&self, a: u32) -> Result<u16, FrameError> {
            if self.rom.is_none() && (SIN_BASE..SIN_BASE + 1024).contains(&a) && a & 1 == 0 {
                let i = (a - SIN_BASE) / 2;
                return Ok((i.wrapping_mul(997).wrapping_add(0x8001)) as u16);
            }
            Ok(u16::from_le_bytes([self.read_u8(a)?, self.read_u8(a + 1)?]))
        }
        fn read_u32(&self, a: u32) -> Result<u32, FrameError> {
            Ok(u32::from(self.read_u16(a)?) | (u32::from(self.read_u16(a + 2)?) << 16))
        }
        fn write_u8(&mut self, a: u32, v: u8) -> Result<(), FrameError> {
            if a == COUNT {
                self.count = v;
                self.writes.push(json!([a, 1, v]));
                return Ok(());
            }
            let i = a.checked_sub(MATRIX_BASE).ok_or(FrameError::MissingWrite {
                address: a,
                width: 1,
            })? as usize;
            *self.oam.get_mut(i).ok_or(FrameError::MissingWrite {
                address: a,
                width: 1,
            })? = v;
            Ok(())
        }
        fn write_u16(&mut self, a: u32, v: u16) -> Result<(), FrameError> {
            let i = a.checked_sub(MATRIX_BASE).ok_or(FrameError::MissingWrite {
                address: a,
                width: 2,
            })? as usize;
            self.oam
                .get_mut(i..i + 2)
                .ok_or(FrameError::MissingWrite {
                    address: a,
                    width: 2,
                })?
                .copy_from_slice(&v.to_le_bytes());
            self.writes.push(json!([a, 2, v]));
            Ok(())
        }
    }
    fn check(cases: &Value, rom: Option<&[u8]>) {
        for c in cases.as_array().unwrap() {
            let seed = c["seed"].as_u64().unwrap() as u8;
            let mut m = Memory {
                oam: (0..1024)
                    .map(|i| seed.wrapping_add((i as u8).wrapping_mul(37)))
                    .collect(),
                count: c["count"].as_u64().unwrap() as u8,
                writes: Vec::new(),
                rom,
            };
            let a: Vec<u32> = c["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            if c["operation"] == "write" {
                write(
                    &mut m,
                    Request {
                        index: a[0],
                        angle: a[1],
                        scale_x: a[2],
                        scale_y: a[3],
                        flags: a[4],
                    },
                )
                .unwrap();
            } else {
                assert_eq!(
                    allocate(&mut m, a[0], a[1], a[2], a[3]).unwrap(),
                    c["result"].as_u64().unwrap() as u8
                );
            }
            assert_eq!(m.count, c["afterCount"].as_u64().unwrap() as u8);
            assert_eq!(json!(m.writes), c["writes"]);
            assert_eq!(crate::sha256(&m.oam), c["sha256"].as_str().unwrap());
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn original_affine_writer_and_allocator_match_synthetic_tables() {
        let v: Value = serde_json::from_str(vectors!("affine-vectors.json")).unwrap();
        check(&v["synthetic"], None);
    }
    #[test]
    #[ignore = "Requires local SPP_ROM for independent original affine trig tables"]
    fn original_affine_tables_match_native_outputs() {
        let path = std::env::var_os("SPP_ROM").expect("Set local SPP_ROM");
        use std::io::Read;
        let mut rom = Vec::new();
        std::fs::File::open(path)
            .unwrap()
            .take((crate::catalog::ROM_LEN + 1) as u64)
            .read_to_end(&mut rom)
            .unwrap();
        crate::catalog::validate_rom(&rom).unwrap();
        let v: Value = serde_json::from_str(vectors!("affine-vectors.json")).unwrap();
        check(&v["native"], Some(&rom));
    }
}
