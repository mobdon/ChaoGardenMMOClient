//! Static character animation tables in the pinned Steam executable.
use crate::{manifest, model::Reader, object_table, Error, Result};
use serde::Serialize;

// SA2Variables.h declarations, also mapped by SA Tools Characters.ini.
const TABLES: &[(&str, u32, usize)] = &[
    ("Tails", 0xA0D608, 118),
    ("MechEggman", 0xA0E1F0, 139),
    ("MechTails", 0xA0EAA0, 190),
    ("ChaoWalker", 0xA0F680, 198),
    ("DarkChaoWalker", 0xA102E0, 202),
    ("Eggman", 0xA12750, 164),
    ("Knuckles", 0xA13320, 217),
    ("Tikal", 0xA140B0, 260),
    ("Rouge", 0xA150F0, 217),
    ("Chaos", 0xA15E80, 272),
    ("Sonic", 0xA17C18, 203),
    ("Shadow", 0xA188C8, 249),
    ("Amy", 0xA19858, 249),
    ("MetalSonic", 0xA1A7E8, 249),
    ("SuperSonic", 0x170D228, 110),
    ("SuperShadow", 0x170D908, 117),
];

#[derive(Debug, Serialize)]
pub struct AnimationRow {
    pub source_va: u32,
    pub motion: i16,
    pub model: i16,
    pub mode: i16,
    pub next: i16,
    pub transition_speed: f32,
    pub animation_speed: f32,
}

#[derive(Debug, Serialize)]
pub struct AnimationTable {
    pub character: &'static str,
    pub source_va: u32,
    pub sha256: String,
    pub rows: Vec<AnimationRow>,
}

fn decode(bytes: &[u8], va: u32) -> Result<AnimationRow> {
    let r = Reader { data: bytes };
    Ok(AnimationRow {
        source_va: va,
        motion: r.u16(0)? as i16,
        model: r.u16(2)? as i16,
        mode: r.u16(4)? as i16,
        next: r.u16(6)? as i16,
        transition_speed: r.f32(8)?,
        animation_speed: r.f32(12)?,
    })
}

pub fn row(exe: &[u8], character: &str, action: usize) -> Result<AnimationRow> {
    let &(_, base, count) = TABLES
        .iter()
        .find(|t| t.0 == character)
        .ok_or_else(|| Error::invalid("unknown animation table"))?;
    if action >= count {
        return Err(Error::invalid("action outside character animation table"));
    }
    let va = base + (action * 16) as u32;
    let at = object_table::verify_image(exe)?.va_to_offset(va, 16)?;
    decode(&exe[at..at + 16], va)
}

pub fn read(exe: &[u8]) -> Result<Vec<AnimationTable>> {
    let image = object_table::verify_image(exe)?;
    TABLES
        .iter()
        .map(|&(character, source_va, count)| {
            let len = count * 16;
            let at = image.va_to_offset(source_va, len)?;
            let bytes = &exe[at..at + len];
            let rows = bytes
                .chunks_exact(16)
                .enumerate()
                .map(|(i, b)| decode(b, source_va + (i * 16) as u32))
                .collect::<Result<_>>()?;
            Ok(AnimationTable {
                character,
                source_va,
                sha256: manifest::sha256(bytes),
                rows,
            })
        })
        .collect()
}
