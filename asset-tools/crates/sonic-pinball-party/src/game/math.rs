//! Integer math used by the native A3VE ball helpers.
//!
//! Division and table trigonometry have supplied-ROM execution parity. Sqrt and
//! ArcTan2 are BIOS services: their optional reference implementation is checked
//! separately against a primary emulator reference, not an authentic BIOS dump.

use crate::{catalog, sha256};
use std::fmt;

pub const TRIG_TABLE_OFFSET: usize = 0x0a_f022;
pub const TRIG_TABLE_LENGTH: usize = 2050;
pub const TRIG_TABLE_SHA256: &str =
    "9c9e60ec4bd598b1551293dc50521c9b4d011cb3822e53865d16647cb01ac008";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathError {
    SourceIdentity,
    TrigTableHash,
    BiosUnavailable { service: u8 },
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MathError {}

/// Native ROM 0x608CC signed quotient, with truncation toward zero. The local
/// division-by-zero handler at 0x60960 is a no-op and the caller returns zero.
/// MIN/-1 wraps to MIN on ARM; widening makes that behavior explicit in Rust.
pub fn signed_divide(numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        0
    } else {
        (i64::from(numerator) / i64::from(denominator)) as i32
    }
}

/// Runtime-only native resource. No table samples are embedded in the source.
#[derive(Clone)]
pub struct TrigTable {
    samples: [u16; 1025],
}

impl fmt::Debug for TrigTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrigTable")
            .field("entry_count", &self.samples.len())
            .field("sha256", &TRIG_TABLE_SHA256)
            .finish()
    }
}

impl TrigTable {
    pub fn from_rom(rom: &[u8]) -> Result<Self, MathError> {
        catalog::validate_rom(rom).map_err(|_| MathError::SourceIdentity)?;
        let bytes = &rom[TRIG_TABLE_OFFSET..TRIG_TABLE_OFFSET + TRIG_TABLE_LENGTH];
        if sha256(bytes) != TRIG_TABLE_SHA256 {
            return Err(MathError::TrigTableHash);
        }
        let mut samples = [0; 1025];
        for (sample, pair) in samples.iter_mut().zip(bytes.as_chunks::<2>().0) {
            *sample = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Ok(Self { samples })
    }

    /// Native 0x2DF90: the low u16 turn angle is quantized to 4096 steps;
    /// quarter-wave samples scale to 20000. The low four angle bits are ignored.
    pub fn sine(&self, angle: u32) -> i16 {
        let mut index = usize::from(angle as u16) >> 4;
        let negative = index > 2047;
        if negative {
            index -= 2048;
        }
        if index > 1023 {
            index = 2048 - index;
        }
        let value = self.samples[index] as i16;
        if negative {
            value.wrapping_neg()
        } else {
            value
        }
    }

    /// Native 0x2DFE0 adds a wrapping quarter-turn before calling 0x2DF90.
    pub fn cosine(&self, angle: u32) -> i16 {
        self.sine(u32::from((angle as u16).wrapping_add(0x4000)))
    }
}

/// Dependencies reached by ROM wrappers 0x5FEF4(SWI8) and 0x5FEAC(SWI10).
/// Returning scalar values does not model BIOS timing or undocumented registers.
pub trait BiosMath {
    fn sqrt(&mut self, value: u32) -> Result<u16, MathError>;
    /// The A3VE caller supplies sign-extended i16 coordinates to BIOS ArcTan2.
    fn atan2(&mut self, x: i16, y: i16) -> Result<u16, MathError>;
}

#[derive(Debug, Default)]
pub struct RejectBiosMath;

impl BiosMath for RejectBiosMath {
    fn sqrt(&mut self, _value: u32) -> Result<u16, MathError> {
        Err(MathError::BiosUnavailable { service: 8 })
    }
    fn atan2(&mut self, _x: i16, _y: i16) -> Result<u16, MathError> {
        Err(MathError::BiosUnavailable { service: 10 })
    }
}

/// Optional numerical BIOS reference, implemented independently from the
/// primary documented polynomial/quadrant specification. It is not native BIOS
/// machine-code parity and is deliberately distinct from RejectBiosMath.
/// Reference provenance and independent checks are in tests/math-vectors.json.
#[derive(Debug, Default)]
pub struct ReferenceBiosMath;

impl BiosMath for ReferenceBiosMath {
    fn sqrt(&mut self, value: u32) -> Result<u16, MathError> {
        Ok(integer_sqrt(value))
    }
    fn atan2(&mut self, x: i16, y: i16) -> Result<u16, MathError> {
        Ok(reference_atan2(x, y))
    }
}

/// Floor sqrt for every u32, with exactly16 bounded candidate checks.
pub fn integer_sqrt(value: u32) -> u16 {
    let mut root = 0u32;
    for shift in (0..16).rev() {
        let candidate = root | (1 << shift);
        if u64::from(candidate) * u64::from(candidate) <= u64::from(value) {
            root = candidate;
        }
    }
    root as u16
}

fn reference_atan(ratio: i32) -> i32 {
    // ratio is bounded to [-0x4000,0x4000] by octant reduction. Integer shifts
    // and truncating division reproduce the numerical specification exactly.
    let squared = -((ratio * ratio) >> 14);
    let polynomial = [0x390, 0x91c, 0xfb6, 0x16aa, 0x2081, 0x3651, 0xa2f9]
        .into_iter()
        .fold(0xa9, |value, coefficient| {
            ((value * squared) >> 14) + coefficient
        });
    (ratio * polynomial) >> 16
}

fn reference_atan2(x: i16, y: i16) -> u16 {
    let x = i32::from(x);
    let y = i32::from(y);
    if y == 0 {
        return if x < 0 { 0x8000 } else { 0 };
    }
    if x == 0 {
        return if y < 0 { 0xc000 } else { 0x4000 };
    }
    // One octant comparison covers all quadrants; the lower-left diagonal uses
    // the vertical branch, matching the primary reference's strict tie rule.
    let horizontal = x.abs() > y.abs() || (x.abs() == y.abs() && !(x < 0 && y < 0));
    let angle = if horizontal {
        let quadrant = if x < 0 {
            0x8000
        } else if y < 0 {
            0x10000
        } else {
            0
        };
        quadrant + reference_atan((y << 14) / x)
    } else {
        let axis = if y < 0 { 0xc000 } else { 0x4000 };
        axis - reference_atan((x << 14) / y)
    };
    angle as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn native_division_vectors_include_zero_and_wrapping_overflow() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/math-vectors.json")).unwrap();
        for row in data["nativeDivision"].as_array().unwrap() {
            assert_eq!(
                signed_divide(
                    row[0].as_i64().unwrap() as i32,
                    row[1].as_i64().unwrap() as i32
                ),
                row[2].as_i64().unwrap() as i32
            );
        }
    }

    #[test]
    fn separate_reference_bios_vectors_match_primary_numerical_reference() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/math-vectors.json")).unwrap();
        let mut reference = ReferenceBiosMath;
        for row in data["referenceBiosSqrt"].as_array().unwrap() {
            assert_eq!(
                reference.sqrt(row[0].as_u64().unwrap() as u32).unwrap(),
                row[1].as_u64().unwrap() as u16
            );
        }
        for row in data["referenceBiosAtan2"].as_array().unwrap() {
            assert_eq!(
                reference
                    .atan2(
                        row[0].as_i64().unwrap() as i16,
                        row[1].as_i64().unwrap() as i16
                    )
                    .unwrap(),
                row[2].as_u64().unwrap() as u16
            );
        }
    }

    #[test]
    fn unavailable_bios_is_an_explicit_error_and_invalid_rom_is_rejected() {
        assert_eq!(
            RejectBiosMath.sqrt(1),
            Err(MathError::BiosUnavailable { service: 8 })
        );
        assert_eq!(
            RejectBiosMath.atan2(1, 1),
            Err(MathError::BiosUnavailable { service: 10 })
        );
        assert!(matches!(
            TrigTable::from_rom(&[]),
            Err(MathError::SourceIdentity)
        ));
    }

    #[test]
    fn synthetic_trig_table_checks_reflection_and_input_truncation() {
        let mut samples = [0; 1025];
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = index as u16;
        }
        let table = TrigTable { samples };
        assert_eq!(table.sine(0x4000), 1024);
        assert_eq!(table.sine(0xc000), -1024);
        assert_eq!(table.sine(0x1001f), 1);
        assert_eq!(table.cosine(0xffff_c000), 0);
        assert_eq!(table.sine(0xffff_ffff), -1);
    }

    #[test]
    #[ignore = "Requires local SPP_ROM for native trigonometry resources"]
    fn source_trig_vectors_match_original_rom_execution() {
        let path = std::env::var_os("SPP_ROM").expect("SPP_ROM required");
        let rom = std::fs::read(path).unwrap();
        let table = TrigTable::from_rom(&rom).unwrap();
        let data: Value =
            serde_json::from_str(include_str!("../../tests/math-vectors.json")).unwrap();
        for row in data["nativeTrig"].as_array().unwrap() {
            let angle = row[0].as_u64().unwrap() as u32;
            assert_eq!(table.sine(angle), row[1].as_i64().unwrap() as i16);
            assert_eq!(table.cosine(angle), row[2].as_i64().unwrap() as i16);
        }
    }
}
