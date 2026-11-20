//! The generic stage objects every GameCube stage REL carries its own copy
//! of (object list rows 0 to 23 and the effect and message tasks they
//! start), ported once and shared by all stages.
//!
//! The reference copy is stg42D.rel (GSNE8P); [`routines`] names each ported
//! routine by its section-1 offset there, with the length and the SHA-256 of
//! its bytes with every relocated field zeroed ([`masked_text`]). The local
//! test `tests/stage_objects_shared.rs` finds the same masked bytes in the
//! other stage RELs, which is the evidence that one port serves them all.
//! Engine services stay outside, behind [`host::ObjectHost`].
pub mod big_the_cat;
pub mod colli_prim;
pub mod containers;
pub mod effects;
pub mod hintbox;
pub mod host;
pub mod itembox;
pub mod light_modmod;
pub mod messenger;
pub mod player_contact;
pub mod ring;
pub mod schbox;
pub mod skull;
pub mod udreel_stoplsd;

use crate::{Error, Result};

/// A ported routine: its stg42D.rel section-1 offset, length and the
/// SHA-256 of its relocation-masked bytes.
#[derive(Debug, Clone, Copy)]
pub struct Routine {
    pub name: &'static str,
    pub stg42d: u32,
    pub len: u32,
    pub masked_sha256: &'static str,
}

/// Every ported routine, module by module. Each object module lists its own
/// routines in a `ROUTINES` table.
pub const MODULES: &[(&str, &[Routine])] = &[
    ("colli_prim", colli_prim::ROUTINES),
    ("big_the_cat", big_the_cat::ROUTINES),
    ("containers", containers::ROUTINES),
    ("effects", effects::ROUTINES),
    ("hintbox", hintbox::ROUTINES),
    ("itembox", itembox::ROUTINES),
    ("messenger", messenger::ROUTINES),
    ("ring", ring::ROUTINES),
    ("schbox", schbox::ROUTINES),
    ("skull", skull::ROUTINES),
    ("skull (local calls)", skull::BL_MASKED_ROUTINES),
    ("udreel_stoplsd", udreel_stoplsd::ROUTINES),
    (
        "udreel_stoplsd (local calls)",
        udreel_stoplsd::BL_MASKED_ROUTINES,
    ),
    ("light_modmod", light_modmod::ROUTINES),
    ("player_contact", player_contact::ROUTINES),
];

/// Every ported routine.
pub fn routines() -> impl Iterator<Item = &'static Routine> {
    MODULES.iter().flat_map(|(_, r)| r.iter())
}

fn word(d: &[u8], at: usize) -> Result<u32> {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error::invalid("REL header out of range"))
}

/// Section 1 (the code) of a GameCube REL with every relocated field
/// zeroed: the 16-bit halves of ADDR16_LO/HI/HA, the whole word of ADDR32
/// and the displacement of REL24 branches. Identical routines in two
/// modules then have identical bytes.
pub fn masked_text(rel: &[u8]) -> Result<Vec<u8>> {
    let count = word(rel, 0xc)? as usize;
    let info = word(rel, 0x10)? as usize;
    if !(2..=64).contains(&count) {
        return Err(Error::invalid("REL section count"));
    }
    let off = (word(rel, info + 8)? & !1) as usize;
    let len = word(rel, info + 12)? as usize;
    let mut text = rel
        .get(
            off..off
                .checked_add(len)
                .ok_or_else(|| Error::invalid("REL text"))?,
        )
        .ok_or_else(|| Error::invalid("REL text out of range"))?
        .to_vec();
    let (imp, imp_len) = (word(rel, 0x28)? as usize, word(rel, 0x2c)? as usize);
    for k in 0..imp_len / 8 {
        let mut at = word(rel, imp + 8 * k + 4)? as usize;
        let (mut section, mut cursor) = (0u8, 0usize);
        for _ in 0..1_000_000 {
            let r = rel
                .get(at..at + 8)
                .ok_or_else(|| Error::invalid("REL relocation out of range"))?;
            at += 8;
            let (delta, kind, s) = (u16::from_be_bytes([r[0], r[1]]) as usize, r[2], r[3]);
            if kind == 203 {
                break;
            }
            cursor += delta;
            if kind == 202 {
                (section, cursor) = (s, 0);
                continue;
            }
            if section != 1 || kind == 201 {
                continue;
            }
            let (from, mask): (usize, &[u8]) = match kind {
                1 => (cursor, &[0, 0, 0, 0]),
                4..=6 => (cursor, &[0, 0]),
                10 => (cursor, &[]),
                _ => continue,
            };
            if kind == 10 {
                if let Some(b) = text.get_mut(cursor..cursor + 4) {
                    let ins = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) & 0xfc00_0003;
                    b.copy_from_slice(&ins.to_be_bytes());
                }
            } else if let Some(b) = text.get_mut(from..from + mask.len()) {
                b.copy_from_slice(mask);
            }
        }
    }
    Ok(text)
}

/// [`masked_text`] with the displacement of every `b`/`bl` (primary opcode
/// 18) zeroed as well. A REL resolves its calls within section 1 when it is
/// linked, without a relocation, so a routine that calls a module-local
/// helper (fabsf at +0x84, sqrtf at +0x74CC, the stage's land and effect
/// helpers) has a different displacement in every stage whose code before
/// the helper differs. With those masked, the shared test finds the same
/// routine across the stage RELs. The recorded hashes stay over
/// [`masked_text`].
pub fn branch_masked_text(rel: &[u8]) -> Result<Vec<u8>> {
    let mut text = masked_text(rel)?;
    for w in text.as_chunks_mut::<4>().0 {
        let ins = u32::from_be_bytes(*w);
        if ins >> 26 == 18 {
            *w = (ins & 0xfc00_0003).to_be_bytes();
        }
    }
    Ok(text)
}
