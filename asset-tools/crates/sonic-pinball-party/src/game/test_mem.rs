//! The byte memory the table tests run their fixtures on (tests only).
use super::frame::{FrameError, FrameMemory};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(crate) struct Mem(pub(crate) BTreeMap<u32, u8>);
impl Mem {
    pub(crate) fn put(&mut self, a: u32, n: u32, v: u32) {
        for i in 0..n {
            self.0.insert(a + i, (v >> (8 * i)) as u8);
        }
    }
    pub(crate) fn load(&self, a: u32, n: u32) -> u32 {
        (0..n).fold(0, |v, i| {
            v | (u32::from(*self.0.get(&(a + i)).unwrap_or(&0)) << (8 * i))
        })
    }
    pub(crate) fn patches(&mut self, v: &Value) {
        for p in v.as_array().into_iter().flatten() {
            self.put(num(&p[0]), num(&p[1]), num(&p[2]));
        }
    }
}
impl FrameMemory for Mem {
    fn read_u8(&self, a: u32) -> Result<u8, FrameError> {
        Ok(self.load(a, 1) as u8)
    }
    fn read_u16(&self, a: u32) -> Result<u16, FrameError> {
        Ok(self.load(a, 2) as u16)
    }
    fn read_u32(&self, a: u32) -> Result<u32, FrameError> {
        Ok(self.load(a, 4))
    }
    fn write_u8(&mut self, a: u32, v: u8) -> Result<(), FrameError> {
        self.put(a, 1, v.into());
        Ok(())
    }
    fn write_u16(&mut self, a: u32, v: u16) -> Result<(), FrameError> {
        self.put(a, 2, v.into());
        Ok(())
    }
}
pub(crate) fn num(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
