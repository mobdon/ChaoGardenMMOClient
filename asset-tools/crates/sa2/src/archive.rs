//! Bounded readers; no archive entry name is ever used as a filesystem path.
//! Layout references: X-Hax/SA3D.Archival PRS.cs and X-Hax/sa_tools
//! Libraries/ArchiveLib/{Puyo,PAK}.cs. See docs/TEXTURE-ARCHIVE-REFERENCES.md.
use crate::{Error, Result};
use serde::Serialize;

fn span(data: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    data.get(
        at..at
            .checked_add(len)
            .ok_or_else(|| Error::invalid("range overflow"))?,
    )
    .ok_or_else(|| Error::invalid("truncated archive"))
}
fn le32(data: &[u8], at: usize) -> Result<usize> {
    let b = span(data, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}
fn be16(data: &[u8], at: usize) -> Result<u16> {
    let b = span(data, at, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

/// LSB-first control bits, literals and overlapping 8/13-bit backreferences.
/// A terminator is required. Trailing alignment bytes are permitted.
pub fn decompress_prs(input: &[u8], max_output: usize) -> Result<Vec<u8>> {
    struct Bits<'a> {
        input: &'a [u8],
        at: usize,
        control: u8,
        left: u8,
    }
    impl Bits<'_> {
        fn byte(&mut self) -> Result<u8> {
            let b = *self
                .input
                .get(self.at)
                .ok_or_else(|| Error::invalid("truncated PRS"))?;
            self.at += 1;
            Ok(b)
        }
        fn bit(&mut self) -> Result<bool> {
            if self.left == 0 {
                self.control = self.byte()?;
                self.left = 8;
            }
            let bit = self.control & 1 != 0;
            self.control >>= 1;
            self.left -= 1;
            Ok(bit)
        }
    }
    let mut bits = Bits {
        input,
        at: 0,
        control: 0,
        left: 0,
    };
    let mut out = Vec::new();
    loop {
        if bits.bit()? {
            if out.len() == max_output {
                return Err(Error::invalid("PRS output limit"));
            }
            out.push(bits.byte()?);
            continue;
        }
        let (distance, length) = if bits.bit()? {
            let word = u16::from_le_bytes([bits.byte()?, bits.byte()?]);
            if word == 0 {
                return Ok(out);
            }
            let length = if word & 7 == 0 {
                bits.byte()? as usize + 1
            } else {
                (word & 7) as usize + 2
            };
            (8192 - (word as usize >> 3), length)
        } else {
            let length = ((usize::from(bits.bit()?) << 1) | usize::from(bits.bit()?)) + 2;
            (256 - bits.byte()? as usize, length)
        };
        if distance > out.len() {
            return Err(Error::invalid("PRS backreference before output"));
        }
        if length > max_output.saturating_sub(out.len()) {
            return Err(Error::invalid("PRS output limit"));
        }
        for _ in 0..length {
            let b = out[out.len() - distance];
            out.push(b);
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GvmEntry<'a> {
    pub index: u16,
    pub name: String,
    pub global_index: Option<u32>,
    pub offset: usize,
    pub length: usize,
    #[serde(skip)]
    pub data: &'a [u8],
}

/// Reads the GVM entry table and exact GVRT chunk ranges. Known metadata and
/// zero alignment padding are skipped; arbitrary byte scanning is prohibited.
pub fn read_gvm(data: &[u8]) -> Result<Vec<GvmEntry<'_>>> {
    if span(data, 0, 4)? != b"GVMH" {
        return Err(Error::invalid("expected GVMH"));
    }
    let flags = be16(data, 8)?;
    let count = be16(data, 10)? as usize;
    let stride = 2
        + if flags & 1 != 0 { 28 } else { 0 }
        + if flags & 2 != 0 { 2 } else { 0 }
        + if flags & 4 != 0 { 2 } else { 0 }
        + if flags & 8 != 0 { 4 } else { 0 };
    let table_end = 12 + count * stride;
    span(data, 12, count * stride)?;
    let mut at = le32(data, 4)?
        .checked_add(8)
        .ok_or_else(|| Error::invalid("GVM header overflow"))?;
    if at < table_end || at > data.len() {
        return Err(Error::invalid("GVM table/data overlap"));
    }
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        loop {
            if span(data, at, 4)? == b"GVRT" {
                break;
            }
            if data[at] == 0 {
                at += 1;
                continue;
            }
            let magic = span(data, at, 4)?;
            if matches!(
                magic,
                b"MDLN" | b"COMM" | b"CONV" | b"IMGC" | b"PVMI" | b"PVPN" | b"PVPL" | b"PVRI"
            ) {
                let len = le32(data, at + 4)?;
                span(
                    data,
                    at,
                    len.checked_add(8)
                        .ok_or_else(|| Error::invalid("GVM chunk overflow"))?,
                )?;
                at += len + 8;
            } else {
                return Err(Error::invalid("unknown GVM metadata chunk"));
            }
        }
        let length = le32(data, at + 4)?
            .checked_add(8)
            .ok_or_else(|| Error::invalid("GVM texture overflow"))?;
        if length < 16 {
            return Err(Error::invalid("short GVRT chunk"));
        }
        let chunk = span(data, at, length)?;
        let row = 12 + i * stride;
        let name = if flags & 1 != 0 {
            let b = span(data, row + 2, 28)?;
            let n = b.iter().position(|b| *b == 0).unwrap_or(b.len());
            // Names are Shift-JIS on both GameCube and Steam (one Chao race bank uses full-width letters).
            let (name, _, malformed) = encoding_rs::SHIFT_JIS.decode(&b[..n]);
            if malformed {
                return Err(Error::invalid("malformed Shift-JIS GVM name"));
            }
            name.into_owned()
        } else {
            format!("{i:03}")
        };
        let global_index = if flags & 8 != 0 {
            let b = span(data, row + stride - 4, 4)?;
            Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        } else {
            None
        };
        entries.push(GvmEntry {
            index: be16(data, row)?,
            name,
            global_index,
            offset: at,
            length,
            data: chunk,
        });
        at += length;
    }
    Ok(entries)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PakEntry<'a> {
    pub name: String,
    pub long_path: String,
    pub offset: usize,
    pub length: usize,
    /// Preserved metadata; not assumed to be a compression flag.
    pub second_length: u32,
    #[serde(skip)]
    pub data: &'a [u8],
}

pub fn read_pak(data: &[u8]) -> Result<Vec<PakEntry<'_>>> {
    if span(data, 0, 4)? != b"\x01pak" {
        return Err(Error::invalid("expected SA2 PC PAK"));
    }
    let count = le32(data, 0x39)?;
    if count > data.len().saturating_sub(0x3d) / 16 {
        return Err(Error::invalid("PAK entry count exceeds table"));
    }
    let mut at = 0x3d;
    let mut records = Vec::with_capacity(count);
    fn string(data: &[u8], at: &mut usize) -> Result<String> {
        let len = le32(data, *at)?;
        *at += 4;
        let raw = span(data, *at, len)?;
        *at += len;
        String::from_utf8(raw.to_vec()).map_err(|_| Error::invalid("non-UTF8 PAK name"))
    }
    for _ in 0..count {
        let long = string(data, &mut at)?;
        let name = string(data, &mut at)?;
        let length = le32(data, at)?;
        let second = le32(data, at + 4)? as u32;
        at += 8;
        records.push((name, long, length, second));
    }
    let mut entries = Vec::with_capacity(count);
    for (name, long_path, length, second_length) in records {
        let bytes = span(data, at, length)?;
        entries.push(PakEntry {
            name,
            long_path,
            length,
            second_length,
            offset: at,
            data: bytes,
        });
        at += length;
    }
    Ok(entries)
}

/// One 60-byte record of a Steam PAK's `.inf`: the GVM slot this texture
/// replaces. Records are in GVM slot order; in the installed packs their names
/// and global indices match the GVM entries (122 packs checked 2026-11-20).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PakSlot {
    pub name: String,
    pub global_index: u32,
    /// Size of the original GameCube texture, not of the DDS.
    pub width: u32,
    pub height: u32,
    /// All eight little-endian words after the name, unchanged.
    pub raw: [u32; 8],
}

pub fn read_pak_inf(data: &[u8]) -> Result<Vec<PakSlot>> {
    if !data.len().is_multiple_of(60) || data.len() / 60 > 65_536 {
        return Err(Error::invalid("PAK .inf is not whole 60-byte records"));
    }
    data.as_chunks::<60>()
        .0
        .iter()
        .map(|r| {
            let n = r[..28].iter().position(|b| *b == 0).unwrap_or(28);
            let (name, _, bad) = encoding_rs::SHIFT_JIS.decode(&r[..n]);
            if bad {
                return Err(Error::invalid("malformed PAK .inf name"));
            }
            let raw: [u32; 8] = std::array::from_fn(|i| {
                let w = &r[28 + i * 4..32 + i * 4];
                u32::from_le_bytes([w[0], w[1], w[2], w[3]])
            });
            Ok(PakSlot {
                name: name.into_owned(),
                global_index: raw[0],
                width: raw[4],
                height: raw[5],
                raw,
            })
        })
        .collect()
}

/// A PAK's DDS textures in GVM slot order, paired by the `.inf` names
/// (DDS entry names are the slot name plus `.dds`, compared without case).
pub fn pak_textures_in_slot_order<'a>(
    entries: &'a [PakEntry<'a>],
) -> Result<Vec<(PakSlot, &'a [u8])>> {
    let mut infs = entries
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().ends_with(".inf"));
    let inf = infs
        .next()
        .ok_or_else(|| Error::invalid("PAK has no .inf slot list"))?;
    if infs.next().is_some() {
        return Err(Error::invalid("PAK has more than one .inf"));
    }
    read_pak_inf(inf.data)?
        .into_iter()
        .map(|slot| {
            // Entry names carry the pack's folder (`al_tex_common\ad_common.dds`).
            let want = format!("{}.dds", slot.name).to_ascii_lowercase();
            let dds = entries
                .iter()
                .find(|e| {
                    let file = e.name.rsplit(['\\', '/']).next().unwrap_or(&e.name);
                    file.to_ascii_lowercase() == want
                })
                .ok_or_else(|| Error::invalid(format!("PAK has no DDS for slot {}", slot.name)))?;
            Ok((slot, dds.data))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prs_literal_termination_and_bounds() {
        assert_eq!(
            decompress_prs(&[0b1011, b'A', b'B', 0, 0], 2).unwrap(),
            b"AB"
        );
        assert!(decompress_prs(&[0b1011, b'A', b'B', 0, 0], 1).is_err());
        assert!(decompress_prs(&[1, b'A'], 9).is_err());
        assert!(decompress_prs(&[0, 0], 9).is_err());
    }
    #[test]
    fn prs_overlapping_short_backreference() {
        // literal A; short copy length 5/distance 1; long terminator.
        assert_eq!(
            decompress_prs(&[0b01011001, b'A', 255, 0, 0], 6).unwrap(),
            b"AAAAAA"
        );
    }
    #[test]
    fn archive_truncation_never_panics() {
        for n in 0..80 {
            assert!(read_gvm(&vec![0; n]).is_err());
            assert!(read_pak(&vec![0; n]).is_err());
        }
    }
    fn one_texture_gvm(name: &[u8]) -> Vec<u8> {
        let mut b = b"GVMH".to_vec();
        b.extend(34u32.to_le_bytes());
        b.extend(1u16.to_be_bytes()); // names only
        b.extend(1u16.to_be_bytes());
        b.extend(0u16.to_be_bytes());
        let mut field = [0u8; 28];
        field[..name.len()].copy_from_slice(name);
        b.extend(field);
        b.extend(b"GVRT");
        b.extend(8u32.to_le_bytes());
        b.extend([0; 8]);
        b
    }
    fn inf_record(name: &str, gbix: u32, w: u32, h: u32) -> Vec<u8> {
        let mut r = vec![0u8; 28];
        r[..name.len()].copy_from_slice(name.as_bytes());
        for v in [gbix, 5, 0, 5, w, h, w * h * 2, 0x8000_0000] {
            r.extend(v.to_le_bytes());
        }
        r
    }
    fn entry<'a>(name: &str, data: &'a [u8]) -> PakEntry<'a> {
        PakEntry {
            name: name.into(),
            long_path: String::new(),
            offset: 0,
            length: data.len(),
            second_length: 0,
            data,
        }
    }
    #[test]
    fn pak_slots_follow_the_inf_order() {
        let inf = [
            inf_record("Zeta", 7, 64, 32),
            inf_record("alpha", 9, 16, 16),
        ]
        .concat();
        let entries = [
            entry("pack\\alpha.dds", b"A"),
            entry("pack.inf", &inf),
            entry("zeta.DDS", b"Z"),
        ];
        let slots = pak_textures_in_slot_order(&entries).unwrap();
        assert_eq!(slots[0].0.name, "Zeta");
        assert_eq!(
            (slots[0].0.global_index, slots[0].0.width, slots[0].0.height),
            (7, 64, 32)
        );
        assert_eq!(slots[0].1, b"Z");
        assert_eq!(slots[1].1, b"A");
        assert!(read_pak_inf(&inf[..59]).is_err());
        assert!(pak_textures_in_slot_order(&entries[..2]).is_err()); // no DDS for Zeta
        assert!(pak_textures_in_slot_order(&entries[2..]).is_err()); // no .inf
    }
    #[test]
    fn gvm_names_are_shift_jis() {
        // Synthetic name with full-width letters, as in a retail Chao race texture bank.
        let entries_data = one_texture_gvm(b"tex_\x82\x83\x82\x92");
        let entries = read_gvm(&entries_data).unwrap();
        assert_eq!(entries[0].name, "tex_\u{ff43}\u{ff52}");
        assert_eq!(
            read_gvm(&one_texture_gvm(b"plain")).unwrap()[0].name,
            "plain"
        );
        assert!(read_gvm(&one_texture_gvm(b"bad\x82")).is_err());
    }
    #[test]
    fn pak_preserves_hostile_names_as_metadata_only() {
        let mut b = vec![0; 0x3d];
        b[..4].copy_from_slice(b"\x01pak");
        b[0x39..0x3d].copy_from_slice(&1u32.to_le_bytes());
        for name in [b"../outside".as_slice(), b"C:\\absolute".as_slice()] {
            b.extend((name.len() as u32).to_le_bytes());
            b.extend(name);
        }
        b.extend(3u32.to_le_bytes());
        b.extend(7u32.to_le_bytes());
        b.extend(b"xyz");
        let entries = read_pak(&b).unwrap();
        assert_eq!(entries[0].name, "C:\\absolute");
        assert_eq!(entries[0].long_path, "../outside");
        assert_eq!(entries[0].second_length, 7);
        assert_eq!(entries[0].data, b"xyz");
        b.pop();
        assert!(read_pak(&b).is_err());
    }
}

pub fn texture_bank_report(prs: &[u8], count: usize) -> Result<serde_json::Value> {
    let data = crate::archive::decompress_prs(prs, 16 * 1024 * 1024)?;
    let entries = crate::archive::read_gvm(&data)?;
    if entries.len() != count {
        return Err(Error::invalid(
            "GVM count disagrees with native texture list",
        ));
    }
    let mut rows = Vec::new();
    for e in entries {
        let image = crate::texture::decode_gvr(e.data, None)?;
        rows.push(serde_json::json!({"index":e.index,"name":e.name,"globalIndex":e.global_index,"width":image.width,"height":image.height,
            "rgbaSha256":crate::manifest::sha256(&image.rgba),"nonopaquePixels":image.rgba.as_chunks::<4>().0.iter().filter(|p|p[3]!=255).count()}));
    }
    Ok(serde_json::json!({"textures":rows,"archiveSha256":crate::manifest::sha256(prs)}))
}
