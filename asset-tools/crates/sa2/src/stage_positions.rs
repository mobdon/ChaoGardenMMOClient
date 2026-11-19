//! Native PC character start, end, two-player intro and mission end rows.
use crate::{object_table, Error, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PositionKind {
    Start,
    End,
    TwoPlayerIntro,
    MissionEnd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Variant {
    Single,
    Player1,
    Player2,
    Mission2,
    Mission3,
}
#[derive(Debug, Serialize)]
pub struct Pose {
    pub variant: Variant,
    pub yaw: u16,
    pub position: [f32; 3],
}
#[derive(Debug, Serialize)]
pub struct PositionRow {
    pub character: &'static str,
    pub kind: PositionKind,
    pub stage: u16,
    pub row_va: u32,
    pub unknown: Option<u16>,
    pub poses: Vec<Pose>,
}

use PositionKind::{End, MissionEnd, Start, TwoPlayerIntro};
const TABLES: &[(&str, PositionKind, u32, usize)] = &[
    ("Sonic", Start, 0x1746F20, 21),
    ("Shadow", Start, 0x17472C0, 18),
    ("Knuckles", Start, 0x17475D8, 18),
    ("Rouge", Start, 0x17478F0, 17),
    ("Tails", Start, 0x1747BE8, 2),
    ("MechEggman", Start, 0x1747C40, 22),
    ("MechTails", Start, 0x1748008, 20),
    ("SuperSonic", Start, 0x1748378, 3),
    ("SuperShadow", Start, 0x1748400, 3),
    ("Sonic", End, 0x1748488, 22),
    ("Shadow", End, 0x1748850, 19),
    ("Knuckles", End, 0x1748B98, 18),
    ("Rouge", End, 0x1748EB0, 18),
    ("MechEggman", End, 0x17491C8, 21),
    ("MechTails", End, 0x1749568, 19),
    ("SuperSonic", End, 0x17498B0, 3),
    ("SuperShadow", End, 0x1749938, 3),
    ("Sonic", TwoPlayerIntro, 0x17499C0, 12),
    ("Shadow", TwoPlayerIntro, 0x1749B40, 12),
    ("Knuckles", TwoPlayerIntro, 0x1749CC0, 12),
    ("Rouge", TwoPlayerIntro, 0x1749E40, 12),
    ("MechEggman", TwoPlayerIntro, 0x1749FC0, 11),
    ("MechTails", TwoPlayerIntro, 0x174A120, 11),
    ("Sonic", MissionEnd, 0x174A280, 9),
    ("Shadow", MissionEnd, 0x174A3A0, 6),
    ("Knuckles", MissionEnd, 0x174A460, 7),
    ("Rouge", MissionEnd, 0x174A540, 6),
    ("MechEggman", MissionEnd, 0x174A600, 7),
    ("MechTails", MissionEnd, 0x174A6E0, 6),
];

/// Returns each available character and context; absent rows remain absent.
pub fn for_stage(exe: &[u8], stage: u16) -> Result<Vec<PositionRow>> {
    Ok(read_rows(exe)?
        .into_iter()
        .filter(|r| r.stage == stage)
        .collect())
}

pub fn read_rows(exe: &[u8]) -> Result<Vec<PositionRow>> {
    let image = object_table::verify_image(exe)?;
    let mut rows = Vec::new();
    for &(character, kind, va, count) in TABLES {
        let variants: &[Variant] = match kind {
            Start | End => &[Variant::Single, Variant::Player1, Variant::Player2],
            TwoPlayerIntro => &[Variant::Player1, Variant::Player2],
            MissionEnd => &[Variant::Mission2, Variant::Mission3],
        };
        let stride = 8 + 12 * variants.len();
        let at = image.va_to_offset(va, stride * count)?;
        let bytes = exe
            .get(at..at + stride * count)
            .ok_or_else(|| Error::invalid("truncated character position table"))?;
        for (i, r) in bytes.chunks_exact(stride).enumerate() {
            let word = |off: usize| u16::from_le_bytes(r[off..off + 2].try_into().unwrap());
            let level = word(0);
            if (level == 91) != (i + 1 == count) {
                return Err(Error::invalid(format!(
                    "unexpected character position sentinel at {va:#x} row {i}"
                )));
            }
            if level == 91 {
                continue;
            }
            let mut poses = Vec::with_capacity(variants.len());
            for (slot, &variant) in variants.iter().enumerate() {
                let position = std::array::from_fn(|axis| {
                    let start = 8 + slot * 12 + axis * 4;
                    f32::from_le_bytes(r[start..start + 4].try_into().unwrap())
                });
                if !position.iter().all(|v| v.is_finite()) {
                    return Err(Error::invalid("nonfinite native character position"));
                }
                poses.push(Pose {
                    variant,
                    yaw: word(2 + slot * 2),
                    position,
                });
            }
            rows.push(PositionRow {
                character,
                kind,
                stage: level,
                row_va: va + (i * stride) as u32,
                unknown: (variants.len() == 2).then(|| word(6)),
                poses,
            });
        }
    }
    Ok(rows)
}
