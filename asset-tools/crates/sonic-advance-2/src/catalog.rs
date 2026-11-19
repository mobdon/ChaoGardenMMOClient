//! Metadata-only manifest. Overlapping views are intentional, never coverage totals.
use crate::{Error, VerifiedRom, gba, graphics, tables};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub source: String,
    pub offset: usize,
    pub length: usize,
    pub category: String,
    pub format: String,
    pub evidence: String,
    pub status: String,
    pub notes: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedReport {
    pub format: String,
    pub bytes: usize,
    pub units: usize,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReport {
    #[serde(flatten)]
    pub resource: Resource,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded: Option<DecodedReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub tool: &'static str,
    pub reference_commit: &'static str,
    pub behavior: &'static str,
    pub rom_sha1: String,
    pub rom_sha256: String,
    pub garden_sha1: String,
    pub garden_sha256: String,
    pub garden_compressed_bytes: usize,
    pub resources: Vec<ResourceReport>,
    pub unresolved: Vec<&'static str>,
}

fn hex(text: &str) -> Result<usize, Error> {
    usize::from_str_radix(text.trim_start_matches("0x"), 16)
        .map_err(|_| Error("Invalid catalog hex number".into()))
}

pub fn resources() -> Result<Vec<Resource>, Error> {
    let mut result = vec![Resource {
        id: "rom.header".into(), source: "rom".into(), offset: 0, length: gba::GBA_HEADER_LEN,
        category: "identity".into(), format: "opaque".into(), evidence: "GBA cartridge header and pinned full-ROM fingerprints".into(),
        status: "verified".into(), notes: "Header checksum does not identify a retail revision without the full ROM hash".into(),
    }, Resource {
        id: "rom.tiny-garden.lz77".into(), source: "rom".into(), offset: gba::REFERENCE_GARDEN_ROM_OFFSET, length: 155_273,
        category: "multiboot".into(), format: "lz77".into(), evidence: "Dedicated mb_chao_garden.sha1 at pinned SAT-R/sa2 commit; exact local payload proof".into(),
        status: "decoded".into(), notes: "ROM offsets and decompressed EWRAM offsets are separate address spaces".into(),
    }];
    for line in include_str!("../catalog/garden-ranges.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5 {
            return Err(Error("Invalid garden range metadata".into()));
        }
        result.push(Resource {
            id: format!("garden.range.{}", fields[0]),
            source: fields[1].into(),
            offset: hex(fields[2])?,
            length: hex(fields[3])?,
            category: "unclassified".into(),
            format: "opaque".into(),
            evidence: fields[4].into(),
            status: "opaque".into(),
            notes: "Pinned assembly range boundary, not a claim of asset identity or reachability"
                .into(),
        });
    }
    let semantic: Vec<Resource> = serde_json::from_str(include_str!("../catalog/resources.json"))
        .map_err(|e| Error(e.to_string()))?;
    result.extend(semantic);
    let mut ids = BTreeSet::new();
    for item in &result {
        if !ids.insert(&item.id)
            || !item
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(Error(format!(
                "Invalid or duplicate resource ID: {}",
                item.id
            )));
        }
    }
    Ok(result)
}

/// Returns a deterministic decoded stream; raw bytes always remain separately available.
/// Palette RGBA is fully opaque, and tile indices do not imply a sprite/OAM assembly.
pub fn decode_resource(
    rom: &VerifiedRom<'_>,
    item: &Resource,
) -> Result<Option<(Vec<u8>, usize, String)>, Error> {
    let raw = rom.range(&item.source, item.offset, item.length)?;
    let graphics_error = |e: graphics::GraphicsError| Error(e.to_string());
    let result = match item.format.as_str() {
        "opaque" => return Ok(None),
        "lz77" => {
            let decoded =
                gba::decode_lz77(raw, 4 * 1024 * 1024).map_err(|e| Error(e.to_string()))?;
            let length = decoded.bytes.len();
            (decoded.bytes, length, "lz77-bytes".into())
        }
        "palette-bgr555" => {
            let colors = graphics::parse_palette(raw, 256).map_err(graphics_error)?;
            let mut rgba = Vec::with_capacity(colors.len() * 4);
            for color in &colors {
                rgba.extend_from_slice(&color.rgb8());
                rgba.push(255);
            }
            (rgba, colors.len(), "rgba8-opaque".into())
        }
        "tiles-4bpp" | "tiles-8bpp" => {
            let tiles = if item.format == "tiles-4bpp" {
                graphics::decode_4bpp_tiles(raw, 16384)
            } else {
                graphics::decode_8bpp_tiles(raw, 16384)
            }
            .map_err(graphics_error)?;
            let count = tiles.len();
            (
                tiles.into_iter().flatten().collect(),
                count,
                "tile-major-indices8".into(),
            )
        }
        "motion-8" => {
            let rows = tables::motion_records(raw, 4096)?;
            let count = rows.len();
            (
                serde_json::to_vec(&rows).map_err(|e| Error(e.to_string()))?,
                count,
                "motion-fields-json".into(),
            )
        }
        "text-map" => {
            if !raw.len().is_multiple_of(2) || raw.len() / 2 > 65536 {
                return Err(Error("Invalid or oversized text-map word stream".into()));
            }
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Word {
                raw: u16,
                tile_index: u16,
                horizontal_flip: bool,
                vertical_flip: bool,
                palette_bank: u8,
            }
            let rows: Vec<_> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| {
                    let entry = graphics::TilemapEntry::from_word(u16::from_le_bytes([b[0], b[1]]));
                    Word {
                        raw: entry.raw,
                        tile_index: entry.tile_index,
                        horizontal_flip: entry.horizontal_flip,
                        vertical_flip: entry.vertical_flip,
                        palette_bank: entry.palette_bank,
                    }
                })
                .collect();
            (
                serde_json::to_vec(&rows).map_err(|e| Error(e.to_string()))?,
                rows.len(),
                "text-map-words-json".into(),
            )
        }
        "m4a-song-table" => {
            let base = if item.source == "garden" {
                0x02000000
            } else {
                0x08000000
            };
            let rows = tables::song_table(
                rom.source_bytes(&item.source)?,
                item.offset,
                item.length,
                base,
            )?;
            (
                serde_json::to_vec(&rows).map_err(|e| Error(e.to_string()))?,
                rows.len(),
                "m4a-song-table-json".into(),
            )
        }
        "pointer-table" => {
            let source = rom.source_bytes(&item.source)?;
            let base = if item.source == "garden" {
                0x02000000
            } else {
                0x08000000
            };
            let rows = tables::pointer_table(raw, base, source.len())?;
            (
                serde_json::to_vec(&rows).map_err(|e| Error(e.to_string()))?,
                rows.len(),
                "pointer-offsets-json".into(),
            )
        }
        other => {
            return Err(Error(format!(
                "Unsupported resource format {other} for {}",
                item.id
            )));
        }
    };
    Ok(Some(result))
}

pub fn manifest(rom: &VerifiedRom<'_>) -> Result<Manifest, Error> {
    let mut reports = Vec::new();
    for item in resources()? {
        let raw = rom.range(&item.source, item.offset, item.length)?;
        let decoded = decode_resource(rom, &item)?.map(|(bytes, units, format)| DecodedReport {
            format,
            bytes: bytes.len(),
            units,
            sha256: gba::sha256(&bytes),
        });
        reports.push(ResourceReport {
            sha256: gba::sha256(raw),
            resource: item,
            decoded,
        });
    }
    Ok(Manifest {
        schema_version: 1,
        tool: "sonic-advance-2-assets/0.1.0",
        reference_commit: crate::REFERENCE_COMMIT,
        behavior: "Native A2NE data inspection; no MMO progression, gameplay emulation or unused-content claim",
        rom_sha1: gba::sha1(rom.rom_bytes()),
        rom_sha256: gba::sha256(rom.rom_bytes()),
        garden_sha1: gba::sha1(rom.garden_bytes()),
        garden_sha256: gba::sha256(rom.garden_bytes()),
        garden_compressed_bytes: rom.compressed_len,
        resources: reports,
        unresolved: vec![
            "Main-game reachability and exhaustive asset boundaries require further tracing",
            "Garden sprite/OAM assembly, animation control flow, text encoding and some table fields remain unknown",
            "Audio sequences, instruments and samples are not synthesized",
            "Native save compatibility does not authenticate MMO ownership or legitimate gains",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_unique_ids_and_bounded_source_ranges() {
        let items = resources().unwrap();
        assert!(items.len() >= 158);
        for item in items {
            let source_len = match item.source.as_str() {
                "rom" => crate::ROM_LEN,
                "garden" => gba::REFERENCE_GARDEN_LEN,
                _ => panic!("unknown source"),
            };
            assert!(item.length > 0);
            assert!(
                item.offset.checked_add(item.length).unwrap() <= source_len,
                "{}",
                item.id
            );
        }
    }
}
