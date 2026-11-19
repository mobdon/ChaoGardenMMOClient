//! Physical 44-byte two-player demo records from the installed PC raw .bin files.
use crate::{Error, Result};
use serde::Serialize;

pub const RECORD_BYTES: usize = 44;
pub const MAX_RAW_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DemoPlayer {
    pub held: u32,
    pub pressed: u32,
    pub transformed_x: i16,
    pub transformed_y: i16,
    pub stick_pushed: u8,
    pub joy_angle: i32,
    /// IEEE 754 bits, preserving negative zero, infinities and NaN payloads.
    pub magnitude_bits: u32,
}

impl DemoPlayer {
    pub fn magnitude(&self) -> f32 {
        f32::from_bits(self.magnitude_bits)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DemoFrame {
    pub players: [DemoPlayer; 2],
    pub unknown: [u8; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct Demo<'a> {
    records: &'a [u8],
}

pub fn read_raw(bytes: &[u8]) -> Result<Demo<'_>> {
    if bytes.is_empty() || bytes.len() > MAX_RAW_BYTES || !bytes.len().is_multiple_of(RECORD_BYTES)
    {
        return Err(Error::invalid(
            "raw demo requires complete 44-byte records and at most 16 MiB",
        ));
    }
    Ok(Demo { records: bytes })
}

impl<'a> Demo<'a> {
    pub fn len(&self) -> usize {
        self.records.len() / RECORD_BYTES
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn frame(&self, index: usize) -> Option<DemoFrame> {
        if index >= self.len() {
            return None;
        }
        let start = index * RECORD_BYTES;
        let b = &self.records[start..start + RECORD_BYTES];
        let word = |offset: usize| u32::from_be_bytes(b[offset..offset + 4].try_into().unwrap());
        let short = |offset: usize| i16::from_be_bytes(b[offset..offset + 2].try_into().unwrap());
        let players = std::array::from_fn(|slot| DemoPlayer {
            held: word(slot * 4),
            pressed: word(8 + slot * 4),
            transformed_x: short(16 + slot * 2),
            transformed_y: short(20 + slot * 2),
            stick_pushed: b[24 + slot],
            joy_angle: word(28 + slot * 8) as i32,
            magnitude_bits: word(32 + slot * 8),
        });
        Some(DemoFrame {
            players,
            unknown: [b[26], b[27]],
        })
    }
    pub fn cursor(&self) -> DemoCursor<'a> {
        DemoCursor {
            demo: *self,
            next: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DemoCursor<'a> {
    demo: Demo<'a>,
    next: usize,
}

impl DemoCursor<'_> {
    pub fn position(&self) -> usize {
        self.next
    }
    /// The physical one-past-end position is valid and yields no record.
    pub fn seek(&mut self, index: usize) -> Result<()> {
        if index > self.demo.len() {
            return Err(Error::invalid("demo seek past physical record count"));
        }
        self.next = index;
        Ok(())
    }
}
impl Iterator for DemoCursor<'_> {
    type Item = DemoFrame;
    fn next(&mut self) -> Option<Self::Item> {
        let frame = self.demo.frame(self.next)?;
        self.next += 1;
        Some(frame)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.demo.len() - self.next;
        (left, Some(left))
    }
}
impl ExactSizeIterator for DemoCursor<'_> {}
impl std::iter::FusedIterator for DemoCursor<'_> {}
