//! Bounded PE32 data inspection. No DLL loading, relocation or execution.
//! References: Microsoft PE/COFF specification and the local SA Tools 786
//! PC_SA2 mapping files. Mapped model addresses are catalog evidence only.

use crate::{Error, Result};
use serde::Serialize;
use std::collections::BTreeSet;

fn bytes(input: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    input
        .get(
            at..at
                .checked_add(size)
                .ok_or_else(|| Error::invalid("PE range overflow"))?,
        )
        .ok_or_else(|| Error::invalid("truncated PE range"))
}
fn u16le(input: &[u8], at: usize) -> Result<u16> {
    let b = bytes(input, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn u32le(input: &[u8], at: usize) -> Result<u32> {
    let b = bytes(input, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeSection {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeExport {
    pub name: String,
    pub ordinal: u32,
    pub rva: u32,
    pub file_offset: Option<usize>,
    pub forwarded_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeImage {
    pub image_base: u32,
    pub header_size: u32,
    pub sections: Vec<PeSection>,
    pub exports: Vec<PeExport>,
}

impl PeImage {
    /// Map a complete contiguous source span; reject virtual-only tails and
    /// ambiguous overlapping sections. Callers still check against input bytes.
    pub fn rva_to_offset(&self, rva: u32, size: usize) -> Result<usize> {
        let end = u64::from(rva)
            .checked_add(size as u64)
            .ok_or_else(|| Error::invalid("PE RVA span overflow"))?;
        if end > u64::from(u32::MAX) + 1 {
            return Err(Error::invalid("PE RVA exceeds address space"));
        }
        let mut mapped = None;
        if rva < self.header_size && end <= u64::from(self.header_size) {
            mapped = Some(rva as usize);
        }
        for section in &self.sections {
            let base = u64::from(section.virtual_address);
            let raw_end = base + u64::from(section.raw_size);
            if u64::from(rva) >= base && end <= raw_end {
                if mapped.is_some() {
                    return Err(Error::invalid("ambiguous PE RVA mapping"));
                }
                mapped = Some(section.raw_offset as usize + (u64::from(rva) - base) as usize);
            }
        }
        mapped.ok_or_else(|| Error::invalid("PE RVA has no complete file-backed span"))
    }

    pub fn va_to_offset(&self, va: u32, size: usize) -> Result<usize> {
        self.rva_to_offset(
            va.checked_sub(self.image_base)
                .ok_or_else(|| Error::invalid("PE VA below image base"))?,
            size,
        )
    }

    pub(crate) fn c_string(&self, input: &[u8], rva: u32) -> Result<String> {
        let mut output = Vec::new();
        for index in 0..512u32 {
            let at = self.rva_to_offset(
                rva.checked_add(index)
                    .ok_or_else(|| Error::invalid("PE name RVA overflow"))?,
                1,
            )?;
            let byte = bytes(input, at, 1)?[0];
            if byte == 0 {
                return String::from_utf8(output)
                    .map_err(|_| Error::invalid("non-UTF8 PE export name"));
            }
            if !byte.is_ascii() {
                return Err(Error::invalid("non-ASCII PE export name"));
            }
            output.push(byte);
        }
        Err(Error::invalid("unterminated or oversized PE export name"))
    }
}

pub fn read_pe(input: &[u8]) -> Result<PeImage> {
    if bytes(input, 0, 2)? != b"MZ" {
        return Err(Error::invalid("not a DOS/PE image"));
    }
    let pe = u32le(input, 0x3c)? as usize;
    if bytes(input, pe, 4)? != b"PE\0\0" {
        return Err(Error::invalid("PE signature missing"));
    }
    let coff = pe
        .checked_add(4)
        .ok_or_else(|| Error::invalid("PE header overflow"))?;
    bytes(input, coff, 20)?;
    if u16le(input, coff)? != 0x14c {
        return Err(Error::invalid(
            "unsupported PE machine; SA2 reader requires i386",
        ));
    }
    let count = usize::from(u16le(input, coff + 2)?);
    let optional_size = usize::from(u16le(input, coff + 16)?);
    let optional = coff + 20;
    if count == 0 || count > 96 || optional_size < 104 {
        return Err(Error::invalid(
            "invalid PE32 section count or optional header",
        ));
    }
    bytes(input, optional, optional_size)?;
    if u16le(input, optional)? != 0x10b {
        return Err(Error::invalid("unsupported PE format; PE32 required"));
    }
    let header_size = u32le(input, optional + 60)?;
    bytes(input, 0, header_size as usize)?;
    let mut image = PeImage {
        image_base: u32le(input, optional + 28)?,
        header_size,
        sections: Vec::new(),
        exports: Vec::new(),
    };
    let section_table = optional + optional_size;
    bytes(input, section_table, count * 40)?;
    if (header_size as usize) < section_table + count * 40 {
        return Err(Error::invalid("PE sections outside declared headers"));
    }
    for index in 0..count {
        let at = section_table + index * 40;
        let name_bytes = bytes(input, at, 8)?;
        let name = String::from_utf8_lossy(
            &name_bytes[..name_bytes.iter().position(|b| *b == 0).unwrap_or(8)],
        )
        .into_owned();
        let section = PeSection {
            name,
            virtual_size: u32le(input, at + 8)?,
            virtual_address: u32le(input, at + 12)?,
            raw_size: u32le(input, at + 16)?,
            raw_offset: u32le(input, at + 20)?,
        };
        bytes(
            input,
            section.raw_offset as usize,
            section.raw_size as usize,
        )?;
        if u64::from(section.virtual_address) + u64::from(section.raw_size)
            > u64::from(u32::MAX) + 1
        {
            return Err(Error::invalid("PE section RVA overflow"));
        }
        image.sections.push(section);
    }
    if u32le(input, optional + 92)? == 0 {
        return Ok(image);
    }
    let export_rva = u32le(input, optional + 96)?;
    let export_size = u32le(input, optional + 100)?;
    if export_rva == 0 && export_size == 0 {
        return Ok(image);
    }
    if export_size < 40 {
        return Err(Error::invalid("truncated PE export directory"));
    }
    let export_end = export_rva
        .checked_add(export_size)
        .ok_or_else(|| Error::invalid("PE export directory overflow"))?;
    let directory = image.rva_to_offset(export_rva, export_size as usize)?;
    bytes(input, directory, export_size as usize)?;
    let ordinal_base = u32le(input, directory + 16)?;
    let function_count = u32le(input, directory + 20)? as usize;
    let name_count = u32le(input, directory + 24)? as usize;
    if function_count > 1_000_000 || name_count > 1_000_000 {
        return Err(Error::invalid("PE export count limit exceeded"));
    }
    let functions = image.rva_to_offset(u32le(input, directory + 28)?, function_count * 4)?;
    let names = image.rva_to_offset(u32le(input, directory + 32)?, name_count * 4)?;
    let ordinals = image.rva_to_offset(u32le(input, directory + 36)?, name_count * 2)?;
    bytes(input, functions, function_count * 4)?;
    bytes(input, names, name_count * 4)?;
    bytes(input, ordinals, name_count * 2)?;
    let mut seen_names = BTreeSet::new();
    let mut export_string_bytes = 0usize;
    for index in 0..name_count {
        let name = image.c_string(input, u32le(input, names + index * 4)?)?;
        // Each name is retained by both the output vector and uniqueness set.
        // A million suffix/alias exports must not amplify a small input into
        // unbounded cloned strings. Forwarder strings share this budget below.
        export_string_bytes = export_string_bytes
            .checked_add(name.len() * 2)
            .filter(|size| *size <= 64 * 1024 * 1024)
            .ok_or_else(|| Error::invalid("PE export string allocation budget exceeded"))?;
        if !seen_names.insert(name.clone()) {
            return Err(Error::invalid("duplicate PE export name"));
        }
        let ordinal_index = u16le(input, ordinals + index * 2)? as usize;
        if ordinal_index >= function_count {
            return Err(Error::invalid("PE export ordinal outside function table"));
        }
        let rva = u32le(input, functions + ordinal_index * 4)?;
        let forwarded_to = if (export_rva..export_end).contains(&rva) {
            Some(image.c_string(input, rva)?)
        } else {
            None
        };
        export_string_bytes = export_string_bytes
            .checked_add(forwarded_to.as_ref().map_or(0, String::len))
            .filter(|size| *size <= 64 * 1024 * 1024)
            .ok_or_else(|| Error::invalid("PE export string allocation budget exceeded"))?;
        let file_offset = if rva == 0 || forwarded_to.is_some() {
            None
        } else {
            Some(image.rva_to_offset(rva, 1)?)
        };
        let ordinal = ordinal_base
            .checked_add(ordinal_index as u32)
            .ok_or_else(|| Error::invalid("PE ordinal overflow"))?;
        image.exports.push(PeExport {
            name,
            ordinal,
            rva,
            file_offset,
            forwarded_to,
        });
    }
    Ok(image)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaoModelCandidate {
    pub id: String,
    pub array_index: usize,
    pub pointer: u32,
    pub file_offset: Option<usize>,
}

/// The seven explicitly mapped PC array members from Chao_DLL.ini. A mapped
/// pointer establishes availability, not converted geometry or entitlement.
pub fn chao_candidates(input: &[u8], image: &PeImage) -> Result<Vec<ChaoModelCandidate>> {
    let root = image
        .exports
        .iter()
        .find(|e| e.name == "AL_RootObject")
        .ok_or_else(|| Error::invalid("AL_RootObject export not found"))?;
    if root.forwarded_to.is_some() {
        return Err(Error::invalid("AL_RootObject is a forwarded export"));
    }
    let candidates = [
        ("child", 0),
        ("neutral-chaos", 108),
        ("hero-chaos", 114),
        ("dark-chaos", 120),
        ("tails", 126),
        ("knuckles", 132),
        ("amy", 138),
    ];
    let mut result = Vec::new();
    for (id, index) in candidates {
        let rva = root
            .rva
            .checked_add(index as u32 * 4)
            .ok_or_else(|| Error::invalid("Chao array RVA overflow"))?;
        let offset = image.rva_to_offset(rva, 4)?;
        let pointer = u32le(input, offset)?;
        let file_offset = if pointer == 0 {
            None
        } else {
            Some(image.va_to_offset(pointer, 52)?)
        };
        result.push(ChaoModelCandidate {
            id: id.into(),
            array_index: index,
            pointer,
            file_offset,
        });
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransporterPart {
    pub id: String,
    pub name: String,
    /// SA Tools mapping address, intentionally not labeled RVA or file offset.
    pub mapping_address: u32,
    pub texture_list: String,
}

/// Validate the four official GBA-machine model sections against a supplied
/// local maintainer mapping. Do not apply GameCube offsets to a PC executable.
pub fn read_transporter_mapping(config: &str) -> Result<Vec<TransporterPart>> {
    let expected = [
        ("button", "Chao Transporter Button"),
        ("shutter", "Chao Transporter GBA Version Shutter"),
        ("casing", "Chao Transporter GBA Version Casing"),
        ("base", "Chao Transporter GBA Version Base"),
    ];
    let mut parts = Vec::new();
    for (id, name) in expected {
        let mut in_section = false;
        let mut found = false;
        let mut model_type = None;
        let mut address = None;
        let mut texture = None;
        for raw_line in config.lines() {
            let line = raw_line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_section = &line[1..line.len() - 1] == name;
                if in_section {
                    if found {
                        return Err(Error::invalid("duplicate Transporter mapping section"));
                    }
                    found = true;
                }
                continue;
            }
            if !in_section {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let target = match key.trim() {
                    "type" => &mut model_type,
                    "address" => &mut address,
                    "texture" => &mut texture,
                    _ => continue,
                };
                if target.replace(value.trim().to_owned()).is_some() {
                    return Err(Error::invalid("duplicate Transporter mapping field"));
                }
            }
        }
        if model_type.as_deref() != Some("chunkmodel") || texture.as_deref() != Some("AL_OBJECT") {
            return Err(Error::invalid(format!(
                "missing or changed Transporter model mapping: {name}"
            )));
        }
        let mapping_address = u32::from_str_radix(
            address
                .as_deref()
                .ok_or_else(|| Error::invalid("missing Transporter address"))?,
            16,
        )
        .map_err(|_| Error::invalid("invalid Transporter hex mapping address"))?;
        parts.push(TransporterPart {
            id: id.into(),
            name: name.into(),
            mapping_address,
            texture_list: "AL_OBJECT".into(),
        });
    }
    Ok(parts)
}

/// UI archive discovery hints established by scripts/extract-chao-ui.js.
/// Individual image roles remain evidence attached to actual decoded entries.
pub const CHAO_UI_ARCHIVES: [&str; 6] = [
    "al_tex_common.prs",
    "al_tex_chao.prs",
    "al_tex_ent_common_en.prs",
    "al_tex_odekake_menu_en.prs",
    "al_tex_cave_menu_en.prs",
    "al_common_tex.prs",
];

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut b = vec![0; 1024];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x14cu16.to_le_bytes());
        b[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&224u16.to_le_bytes());
        b[0x98..0x9a].copy_from_slice(&0x10bu16.to_le_bytes());
        b[0xb4..0xb8].copy_from_slice(&0x400000u32.to_le_bytes());
        b[0xd4..0xd8].copy_from_slice(&512u32.to_le_bytes());
        b[0xf4..0xf8].copy_from_slice(&1u32.to_le_bytes());
        b[0xf8..0xfc].copy_from_slice(&0x1000u32.to_le_bytes());
        b[0xfc..0x100].copy_from_slice(&128u32.to_le_bytes());
        b[0x180..0x184].copy_from_slice(&1024u32.to_le_bytes());
        b[0x184..0x188].copy_from_slice(&0x1000u32.to_le_bytes());
        b[0x188..0x18c].copy_from_slice(&512u32.to_le_bytes());
        b[0x18c..0x190].copy_from_slice(&512u32.to_le_bytes());
        b[532..536].copy_from_slice(&1u32.to_le_bytes());
        b[536..540].copy_from_slice(&1u32.to_le_bytes());
        for (at, val) in [
            (540, 0x1028u32),
            (544, 0x102c),
            (548, 0x1030),
            (552, 0x1100),
            (556, 0x1032),
        ] {
            b[at..at + 4].copy_from_slice(&val.to_le_bytes());
        }
        b[562..566].copy_from_slice(b"test");
        b
    }
    #[test]
    fn pe_maps_raw_sections_and_rejects_virtual_tails() {
        let image = read_pe(&sample()).unwrap();
        assert_eq!(image.exports[0].name, "test");
        assert_eq!(image.exports[0].file_offset, Some(768));
        assert_eq!(image.va_to_offset(0x401100, 52).unwrap(), 768);
        assert!(image.rva_to_offset(0x1200, 1).is_err());
        assert!(image.rva_to_offset(0x11ff, 2).is_err());
        assert!(image.rva_to_offset(u32::MAX, 2).is_err());
    }
    #[test]
    fn pe_rejects_truncation_and_invalid_ordinal() {
        let b = sample();
        for end in 0..b.len() {
            assert!(read_pe(&b[..end]).is_err());
        }
        let mut b = sample();
        b[560..562].copy_from_slice(&1u16.to_le_bytes());
        assert!(read_pe(&b).is_err());
    }
    #[test]
    fn pe_preserves_aliased_addresses() {
        let mut b = sample();
        b[536..540].copy_from_slice(&2u32.to_le_bytes());
        b[544..548].copy_from_slice(&0x1040u32.to_le_bytes());
        b[548..552].copy_from_slice(&0x1048u32.to_le_bytes());
        b[576..580].copy_from_slice(&0x1032u32.to_le_bytes());
        b[580..584].copy_from_slice(&0x1050u32.to_le_bytes());
        b[592..598].copy_from_slice(b"alias\0");
        let image = read_pe(&b).unwrap();
        assert_eq!(image.exports.len(), 2);
        assert_eq!(image.exports[0].rva, image.exports[1].rva);
    }
    #[test]
    fn transporter_requires_all_mapped_parts() {
        assert!(read_transporter_mapping(
            "[Chao Transporter Button]\ntype=chunkmodel\naddress=E56784\ntexture=AL_OBJECT"
        )
        .is_err());
    }
}
