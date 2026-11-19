//! Authored mission-rank thresholds in the verified Steam executable.
use crate::{object_table, Error, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "unit", content = "dThroughA", rename_all = "camelCase")]
pub enum Thresholds {
    StoredScore([i16; 4]),
    Seconds([u16; 4]),
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankRow {
    pub row_rva: u32,
    pub level: u16,
    pub mission: u8,
    pub course: Option<&'static str>,
    pub thresholds: Thresholds,
}
pub fn read(exe: &[u8]) -> Result<Vec<RankRow>> {
    let pe = object_table::verify_image(exe)?;
    let mut rows = vec![];
    for (base, count, mission, score, course) in [
        (0x133ad40, 30, 1, true, None),
        (0x133b0d0, 34, 2, false, None),
        (0x133b208, 30, 3, false, None),
        (0x133ae70, 30, 4, true, None),
        (0x133afa0, 30, 5, true, None),
        (0x133b378, 5, 0, false, Some("Route 101")),
        (0x133b3a5, 5, 0, false, Some("Route 280")),
    ] {
        let stride = if score { 10 } else { 9 };
        let at = pe.rva_to_offset(base, (count * stride) as usize)?;
        for i in 0..count {
            let pos = at + (i * stride) as usize;
            let b = &exe[pos..pos + stride as usize];
            let (level, thresholds) = if score {
                (
                    u16::from_le_bytes([b[0], b[1]]),
                    Thresholds::StoredScore(std::array::from_fn(|j| {
                        i16::from_le_bytes([b[2 + j * 2], b[3 + j * 2]])
                    })),
                )
            } else {
                if b[1..].as_chunks::<2>().0.iter().any(|p| p[1] >= 60) {
                    return Err(Error::invalid("invalid rank seconds"));
                }
                (
                    u16::from(b[0]),
                    Thresholds::Seconds(std::array::from_fn(|j| {
                        u16::from(b[1 + j * 2]) * 60 + u16::from(b[2 + j * 2])
                    })),
                )
            };
            rows.push(RankRow {
                row_rva: base + i * stride,
                level,
                mission: if course.is_some() {
                    (i + 1) as u8
                } else {
                    mission
                },
                course,
                thresholds,
            });
        }
    }
    Ok(rows)
}
