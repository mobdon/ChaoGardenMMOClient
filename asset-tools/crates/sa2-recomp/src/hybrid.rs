//! Disc code, Steam values: a module built from the disc REL (its layout,
//! relocations and code match GSNE8P's main.dol) with Steam's section bytes
//! copied in wherever they line up.
//!
//! - A code section the same size in both: Steam's bytes, all of them.
//! - A data section: the two are aligned and Steam's bytes replace the
//!   disc's where a run corresponds (same length between matching
//!   stretches); insertions and deletions are listed. Even at the same size:
//!   stg44D's Steam data drops 8 bytes and adds 8 later, and copied whole it
//!   put 559 words 8 bytes off (Mad Space crashed loading).
//! - A code section Steam rebuilt: the disc's code is kept.
//!
//! Relocated fields hold zero in both files and OSLink fills them, so a
//! difference there is never a difference in the running game.
use crate::rel::Rel;

/// Bytes compared to resynchronise after a difference.
const WINDOW: usize = 16;
/// Longest same-length replacement looked for before treating a difference
/// as an insertion or deletion.
const MAX_REPLACE: usize = 256;
/// How far ahead (in each file) an insertion or deletion is looked for.
const MAX_SHIFT: usize = 4096;

#[derive(Debug, Default)]
pub struct Hybrid {
    pub bytes: Vec<u8>,
    /// Bytes now holding Steam's value instead of the disc's.
    pub patched: usize,
    /// What could not be matched, one line each.
    pub unmatched: Vec<String>,
}

/// Build the hybrid of a disc REL and its Steam counterpart.
pub fn hybrid(disc: &Rel, steam: &Rel) -> Hybrid {
    let mut out = Hybrid { bytes: disc.bytes.clone(), ..Default::default() };
    for (i, (d, s)) in disc.sections.iter().zip(&steam.sections).enumerate() {
        if d.offset == 0 || s.offset == 0 || d.size == 0 {
            continue; // .bss or absent
        }
        let (Some(dr), Some(sr)) = (
            disc.bytes.get(d.offset as usize..(d.offset + d.size) as usize),
            steam.bytes.get(s.offset as usize..(s.offset + s.size) as usize),
        ) else {
            out.unmatched.push(format!("section {i}: outside its file"));
            continue;
        };
        let base = d.offset as usize;
        if d.size == s.size && (d.exec || s.exec) {
            for (k, (a, b)) in dr.iter().zip(sr).enumerate() {
                if a != b {
                    out.bytes[base + k] = *b;
                    out.patched += 1;
                }
            }
        } else if d.exec || s.exec {
            out.unmatched.push(format!(
                "section {i} (code): {:#x} bytes on disc, {:#x} on Steam; disc code kept",
                d.size, s.size
            ));
        } else {
            let (patches, gaps) = align(dr, sr);
            for (doff, soff, len) in patches {
                for k in 0..len {
                    if out.bytes[base + doff + k] != sr[soff + k] {
                        out.bytes[base + doff + k] = sr[soff + k];
                        out.patched += 1;
                    }
                }
            }
            for (doff, dlen, soff, slen) in gaps {
                out.unmatched.push(format!(
                    "section {i} (data): disc {doff:#x}+{dlen:#x} vs Steam {soff:#x}+{slen:#x}"
                ));
            }
        }
    }
    let (n, bytes) = constants(disc, steam, &mut out.bytes);
    if n > 0 {
        out.patched += bytes;
        out.unmatched.push(format!("{n} constants take Steam's value (its code loads another one there)"));
    }
    out
}

/// Constants Steam changed in its code: the disc code loads constant A where
/// Steam's matching instruction loads B (e.g. CartD: disc 150.0, Steam 100.0).
/// Every disc reference to A is matched to Steam's code by the instructions
/// around it (relocated fields masked); if all agree on one Steam constant
/// with another value, A takes that value. Returns (constants, bytes) patched.
fn constants(disc: &Rel, steam: &Rel, out: &mut [u8]) -> (usize, usize) {
    use std::collections::{HashMap, HashSet};
    const CTX: u32 = 6; // instructions each side
    let (Some((dt, dtext)), Some((st, stext))) = (disc.text(), steam.text()) else { return (0, 0) };
    // Only float-constant loads: the low half (ADDR16_LO) on an lfs, or on an
    // addi followed by `lfs 0(that register)`. A reference to a table or
    // struct start could point at a layout Steam changed, not a value.
    let site = |r: &crate::rel::Reloc, rel: &Rel, t: u32| {
        let Some(code) = rel.sections.get(t as usize) else { return false };
        let at = r.offset & !3;
        let w = rel.word(code, at);
        let next = rel.word(code, at + 4);
        let addi_then_lfs = w >> 26 == 14 && next >> 26 == 48 && (next >> 16) & 31 == (w >> 21) & 31 && next & 0xFFFF == 0;
        r.section as u32 == t
            && r.module == rel.id
            && r.kind == 4
            && (w >> 26 == 48 || addi_then_lfs)
            && rel.sections.get(r.target_section as usize).is_some_and(|s| !s.exec && s.offset != 0)
    };
    let context = |rel: &Rel, sec: &crate::rel::RelSection, masked: &HashSet<u32>, at: u32| -> Option<Vec<u32>> {
        let first = at.checked_sub(4 * CTX)?;
        (0..=2 * CTX)
            .map(|k| first + 4 * k)
            .map(|o| (o + 4 <= sec.size).then(|| rel.word(sec, o) & if masked.contains(&o) { 0xFFFF_0000 } else { !0 }))
            .collect()
    };
    let dmask: HashSet<u32> = disc.relocs.iter().filter(|r| r.section as u32 == dt).map(|r| r.offset & !3).collect();
    let smask: HashSet<u32> = steam.relocs.iter().filter(|r| r.section as u32 == st).map(|r| r.offset & !3).collect();
    let starget: HashMap<u32, (u8, u32)> =
        steam.relocs.iter().filter(|r| site(r, steam, st)).map(|r| (r.offset & !3, (r.target_section, r.addend))).collect();
    let mut index: HashMap<Vec<u32>, Vec<u32>> = HashMap::new();
    for &o in starget.keys() {
        if let Some(c) = context(steam, stext, &smask, o) {
            index.entry(c).or_default().push(o);
        }
    }
    // disc constant -> the Steam constants its references load (None: unmatched)
    let mut map: HashMap<(u8, u32), Vec<Option<(u8, u32)>>> = HashMap::new();
    for r in disc.relocs.iter().filter(|r| site(r, disc, dt)) {
        let at = r.offset & !3;
        let found = context(disc, dtext, &dmask, at)
            .and_then(|c| index.get(&c))
            .filter(|v| v.len() == 1)
            .and_then(|v| starget.get(&v[0]).copied());
        map.entry((r.target_section, r.addend)).or_default().push(found);
    }
    let (mut n, mut bytes) = (0, 0);
    for ((ds, da), uses) in map {
        let Some(Some(first)) = uses.first().copied() else { continue };
        if uses.iter().any(|u| *u != Some(first)) {
            continue;
        }
        let (ss, sa) = first;
        let (Some(dsec), Some(ssec)) = (disc.sections.get(ds as usize), steam.sections.get(ss as usize)) else { continue };
        let (dat, sat) = ((dsec.offset + da) as usize, (ssec.offset + sa) as usize);
        let (Some(dv), Some(sv)) = (out.get(dat..dat + 4).map(<[u8]>::to_vec), steam.bytes.get(sat..sat + 4)) else { continue };
        if dv != sv && da + 4 <= dsec.size && sa + 4 <= ssec.size {
            out[dat..dat + 4].copy_from_slice(sv);
            n += 1;
            bytes += dv.iter().zip(sv).filter(|(a, b)| a != b).count();
        }
    }
    (n, bytes)
}

/// Align two versions of a data section. Returns same-length replacements
/// `(disc offset, Steam offset, length)` and unmatched stretches
/// `(disc offset, disc length, Steam offset, Steam length)`.
pub fn align(d: &[u8], s: &[u8]) -> (Vec<(usize, usize, usize)>, Vec<(usize, usize, usize, usize)>) {
    // A run of one byte (zero padding) matches at any shift: it can't show
    // where the two versions line up again.
    let telling = |w: &[u8]| w.iter().any(|&x| x != w[0]);
    let eq = |i: usize, j: usize, n: usize| {
        i + n <= d.len() && j + n <= s.len() && d[i..i + n] == s[j..j + n] && telling(&d[i..i + n])
    };
    let (mut patches, mut gaps) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < d.len() && j < s.len() {
        if d[i] == s[j] {
            i += 1;
            j += 1;
            continue;
        }
        // A changed value: the same length, then the files agree again.
        let replace = (1..=MAX_REPLACE).find(|&k| eq(i + k, j + k, WINDOW));
        // An insertion or deletion: the nearest point where they agree again.
        // Looked for even when a replacement fits: a value removed in one
        // place and another added a little later also lines up again at the
        // same length, and would copy every value between them one slot off.
        let limit = replace.map_or(MAX_SHIFT, |k| k.min(MAX_SHIFT));
        let mut at = std::collections::HashMap::new();
        if let Some(room) = s.len().checked_sub(j + WINDOW) {
            for b in 0..=limit.min(room) {
                let w = &s[j + b..j + b + WINDOW];
                if telling(w) {
                    at.entry(w).or_insert(b);
                }
            }
        }
        let room = d.len().checked_sub(i + WINDOW);
        let shift = room
            .into_iter()
            .flat_map(|room| 0..=limit.min(room))
            .filter_map(|a| at.get(&d[i + a..i + a + WINDOW]).map(|&b| (a, b)))
            .filter(|&(a, b)| a != b)
            .min_by_key(|&(a, b)| a + b)
            // Bytes just before the resync that agree (the padding) belong to
            // the matched part, not the gap.
            .map(|(mut a, mut b)| {
                while a > 0 && b > 0 && d[i + a - 1] == s[j + b - 1] {
                    a -= 1;
                    b -= 1;
                }
                (a, b)
            });
        let best = match (replace, shift) {
            (Some(k), Some((a, b))) if a + b > k => Some((k, k)),
            (Some(k), None) => Some((k, k)),
            (_, shift) => shift,
        };
        match best {
            Some((a, b)) if a == b => patches.push((i, j, a)),
            Some((a, b)) => gaps.push((i, a, j, b)),
            None => {
                gaps.push((i, d.len() - i, j, s.len() - j));
                return (patches, gaps);
            }
        }
        let (a, b) = best.unwrap_or_default();
        i += a;
        j += b;
    }
    if i < d.len() || j < s.len() {
        gaps.push((i, d.len() - i, j, s.len() - j));
    }
    (patches, gaps)
}

#[cfg(test)]
mod tests {
    use super::align;

    #[test]
    fn changed_value_is_patched_and_insertion_is_a_gap() {
        let mut d: Vec<u8> = (0..200u8).collect();
        let mut s = d.clone();
        s[50] = 150; // a changed value (disc 50)
        s.splice(120..120, [7, 7, 7, 7]); // Steam added 4 bytes
        d[199] = 9;
        s[203] = 9;
        let (patches, gaps) = align(&d, &s);
        assert_eq!(patches, vec![(50, 50, 1)]);
        assert_eq!(gaps, vec![(120, 0, 120, 4)]);
    }

    #[test]
    fn zero_padding_does_not_hide_an_insertion() {
        // advertiseD section 4: the disc has 4 more bytes before a run of
        // zeros; lining up the zeros shifted every later constant by 4.
        let a: Vec<u8> = (1..=40).collect();
        let b: Vec<u8> = (101..=140).collect();
        let mut d = a.clone();
        d.extend([9, 9, 9, 8]);
        d.extend([0; 64]);
        d.extend(&b);
        let mut s = a.clone();
        s.extend([0; 64]);
        s.extend(&b);
        let (patches, gaps) = align(&d, &s);
        for (doff, soff, len) in patches {
            assert_eq!(d[doff..doff + len], s[soff..soff + len], "a patch changes the disc");
        }
        assert_eq!(gaps, vec![(40, 4, 40, 0)]);
    }


    #[test]
    fn a_removed_value_then_an_added_one_are_two_gaps() {
        // advertiseD section 4: Steam dropped the disc's 4.0 and added a
        // constant 0x34 bytes later. Copying the stretch between as same-length
        // "changed values" put every constant there one slot off (the memory
        // card screen's slide speed read 0.0 and the screen never took A).
        let a: Vec<u8> = (1..=40).collect();
        let m: Vec<u8> = (41..=80).collect();
        let b: Vec<u8> = (101..=140).collect();
        let mut d = a.clone();
        d.extend([0x40, 0x80, 0, 0]);
        d.extend(&m);
        d.extend(&b);
        let mut s = a.clone();
        s.extend(&m);
        s.extend([0x3d, 0xba, 0x2e, 0x8c]);
        s.extend(&b);
        let (patches, gaps) = align(&d, &s);
        assert!(patches.is_empty(), "{patches:?}");
        assert_eq!(gaps, vec![(40, 4, 40, 0), (84, 0, 80, 4)]);
    }
}
