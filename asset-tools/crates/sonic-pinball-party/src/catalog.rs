//! Metadata-only catalog with physical and semantic evidence kept separate.
use crate::{
    binary::{self, Error},
    sha1, sha256,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ROM_LEN: usize = 0x800000;
pub const ROM_SHA1: &str = "92b5c19cb4dee8fdfc15a8abdc699822d5373042";
pub const ROM_SHA256: &str = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b";
pub const GARDEN_OFFSET: usize = 0xd0970;
pub const GARDEN_STORED_LEN: usize = 152_552;
pub const GARDEN_LEN: usize = 188_716;
pub const GARDEN_SHA256: &str = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";
pub const SCAN_BUDGET: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub game_code: String,
    pub revision: u8,
    pub length: usize,
    pub sha1: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    /// rom or garden; garden offsets are decoded EWRAM image offsets.
    pub space: String,
    pub offset: usize,
    pub offset_hex: String,
    pub stored_length: usize,
    pub stored_sha256: String,
    pub encoding: String,
    pub decoded_length: usize,
    pub decoded_sha256: String,
    /// Aligned literal references within the same space; not proof of use.
    pub pointer_references: Vec<usize>,
    pub status: String,
    pub category: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub source: Source,
    pub scan_policy: String,
    pub assets: Vec<Asset>,
}

/// No source-specific offset is accessed until the complete source is matched.
pub fn validate_rom(rom: &[u8]) -> Result<Source, Error> {
    if rom.len() != ROM_LEN {
        return Err(Error::Identity);
    }
    let h = binary::span(rom, 0xa0, 0x20)?;
    let complement = h[..=0x1c]
        .iter()
        .fold(0u8, |s, b| s.wrapping_sub(*b))
        .wrapping_sub(0x19);
    if &h[..12] != b"SONICPINBALL"
        || &h[12..16] != b"A3VE"
        || h[0x12] != 0x96
        || h[0x1c] != 0
        || complement != h[0x1d]
        || sha256(rom) != ROM_SHA256
    {
        return Err(Error::Identity);
    }
    Ok(Source {
        game_code: "A3VE".into(),
        revision: 0,
        length: rom.len(),
        sha1: sha1(rom),
        sha256: ROM_SHA256.into(),
    })
}

pub fn extract_garden(rom: &[u8]) -> Result<binary::Decoded, Error> {
    validate_rom(rom)?;
    let stored = binary::span(rom, GARDEN_OFFSET, GARDEN_STORED_LEN)?;
    let decoded = binary::decode(stored, GARDEN_LEN)?;
    if decoded.consumed != GARDEN_STORED_LEN
        || decoded.bytes.len() != GARDEN_LEN
        || sha256(&decoded.bytes) != GARDEN_SHA256
    {
        return Err(Error::HashMismatch("tiny-garden".into()));
    }
    Ok(decoded)
}

fn pointer_index(bytes: &[u8], base: u32) -> BTreeMap<usize, Vec<usize>> {
    let mut result: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for offset in (0..bytes.len().saturating_sub(3)).step_by(4) {
        let pointer =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("bounded word"));
        if let Ok(target) = binary::pointer_offset(pointer, base, bytes.len()) {
            result.entry(target).or_default().push(offset);
        }
    }
    result
}

fn scan_space(
    bytes: &[u8],
    space: &str,
    base: u32,
    budget: &mut usize,
) -> Result<Vec<Asset>, Error> {
    let pointers = pointer_index(bytes, base);
    let mut result = Vec::new();
    for offset in (0xc0..bytes.len().saturating_sub(3)).step_by(4) {
        if !matches!(bytes[offset], 0x10 | 0x30) {
            continue;
        }
        let length = binary::declared_size(&bytes[offset..])?;
        if !(32..=binary::MAX_DECODED).contains(&length) {
            continue;
        }
        *budget = budget.checked_sub(length).ok_or(Error::ScanBudget)?;
        if let Ok(decoded) = binary::decode(&bytes[offset..], length) {
            if result.len() >= 8192 {
                return Err(Error::ScanBudget);
            }
            let stored = binary::span(bytes, offset, decoded.consumed)?;
            result.push(Asset {
                id: format!("{space}-{:06x}", offset), space: space.into(), offset, offset_hex: format!("0x{offset:06X}"),
                stored_length: stored.len(), stored_sha256: sha256(stored), encoding: if bytes[offset] == 0x10 { "gba-lz77-10" } else { "gba-rle-30" }.into(),
                decoded_length: decoded.bytes.len(), decoded_sha256: sha256(&decoded.bytes),
                pointer_references: pointers.get(&offset).cloned().unwrap_or_default(),
                status: "structural-candidate".into(), category: "unknown".into(),
                evidence: "Strict bounded decoding only; alignment/literal pointers do not establish resource semantics, reachability or independent ownership. Candidates may overlap.".into(),
            });
        }
    }
    Ok(result)
}

fn raw_asset(
    bytes: &[u8],
    space: &str,
    offset: usize,
    length: usize,
    id: &str,
    category: &str,
    evidence: &str,
) -> Result<Asset, Error> {
    let stored = binary::span(bytes, offset, length)?;
    Ok(Asset {
        id: id.into(),
        space: space.into(),
        offset,
        offset_hex: format!("0x{offset:06X}"),
        stored_length: length,
        stored_sha256: sha256(stored),
        encoding: "raw".into(),
        decoded_length: length,
        decoded_sha256: sha256(stored),
        pointer_references: Vec::new(),
        status: "mapped-span".into(),
        category: category.into(),
        evidence: evidence.into(),
    })
}

pub fn build_catalog(rom: &[u8]) -> Result<Catalog, Error> {
    let source = validate_rom(rom)?;
    let garden = extract_garden(rom)?;
    let mut budget = SCAN_BUDGET;
    let mut assets = scan_space(rom, "rom", 0x08000000, &mut budget)?;
    assets.extend(scan_space(
        &garden.bytes,
        "garden",
        0x02000000,
        &mut budget,
    )?);
    if let Some(payload) = assets
        .iter_mut()
        .find(|a| a.space == "rom" && a.offset == GARDEN_OFFSET)
    {
        payload.id = "tiny-garden-program".into();
        payload.status = "verified-container".into();
        payload.category = "tiny-garden".into();
        payload.evidence = "A3VE payload; ARM entry word EA000006, EWRAM-addressed literal references, nested resource streams. Loader references and independent parity recorded in docs/CATALOG.md.".into();
    }
    for (offset, length, id, category, evidence) in [
        (0xac78c, 23 * 28, "visual-resource-descriptors", "pinball-table-visuals", "23 records of seven ROM pointers. Row0 fields0/1/2 independently render a Sonic board; other row names/column meanings are not assigned."),
        (0xaca10, 7 * 4, "supplemental-compressed-pointers", "pinball-table-visuals", "Seven aligned ROM pointers to strict LZ77 streams."),
        (0xaca2c, 14 * 4, "supplemental-palette-pointers", "palettes", "Fourteen palette pointers; referenced targets separated by 512 bytes."),
        (0xf9f74, 200 * 8, "song-table", "audio", "200 eight-byte header/player records; bounded header/track-start validation; no sequence execution."),
        (0x6ebea0, 270 * 4, "audio-debug-label-pointers", "audio", "270 label pointers. Not a direct 200-song index mapping."),
        (0xade1c, 300, "table-debug-labels", "ui", "Contiguous debug menu label span; names do not establish resource row identities."),
        (0xcb78c, 10, "main-save-marker", "saves", "Marker referenced by ROM literal words 0x48ED0/0x4916C/0x4962C; main save layout/checksum still unknown."),
        (0x2c9860, 14, "flash-library-marker", "saves", "Flash library identifier only; no inference of main save field layout."),
        (0xf5d58, 512, "main-crc16-table", "saves-transfer", "All 256 words independently match reflected polynomial0x8408; transfer framing remains unverified."),
        (0x1d19c, 4, "garden-loader-pointer", "tiny-garden", "Literal ROM pointer0x080D0970 independently identifies A3VE payload location."),
        (0x31c5fc, 0xd000, "sonic-board-tile-allocation", "tiles", "Raw4bpp allocation ending at palette0x3295FC. Extent inferred from adjacent palette target; map uses tiles0..831; not every allocated tile is proven consumed."),
        (0x3295fc, 512, "sonic-board-palettes", "palettes", "Sixteen BGR555 palette banks, descriptor0 variant0 column2; render paired with ROM3345FC map and31C5FC tiles."),
    ] { assets.push(raw_asset(rom, "rom", offset, length, id, category, evidence)?); }
    for (offset, length, id, category, evidence) in [
        (0x1f4e0, 512, "garden-crc16-table", "saves", "All256 CRC table words match polynomial0x8408; payload0x189C points here."),
        (0x22b68, 512, "garden-bg-palettes", "palettes", "BG palette source pointer at payload0x1D20."),
        (0x23e50, 512, "garden-obj-palettes", "palettes", "OBJ palette source pointer at payload0x1D50."),
        (0x1606c, 224, "garden-minigame-obj-palettes", "chao-ball-candidate", "Source pointer0xD4F4 paired with OBJ palette destination0x05000220 at0xD4F8; Chao Ball dispatch linkage still unverified."),
        (0x17074, 416, "garden-minigame-bg-palettes", "chao-ball-candidate", "Source pointer0xD494 in unique minigame loader; semantic dispatch linkage still unverified."),
        (0x2b0, 12, "garden-save-literals", "saves", "A3VE literal pool: marker0x1965, record length0x858, CRC input length0x856."),
        (0x185c, 0x44, "garden-crc-routine", "saves", "CRC implementation span including table pointer at0x189C; initFFFF/final inversion traced locally."),
        (0x248, 0x164, "garden-save-loader", "saves", "A3VE native loader: sectors10..15, marker/CRC checks, first valid then strictly greater unsigned generation; opaque record body."),
        (0x3ac, 0x90, "garden-save-writer", "saves", "Native save writer code provenance only; Rust tool never writes native saves."),
        (0x43c, 0x44, "garden-save-initializer", "saves", "Native save initializer provenance; no generated progression or field interpretation."),
        (0x480, 0x110, "garden-joybus-initializer", "transfer", "Hardware JoyBus register literals at538/53C/540/544 and protocol magic at580/588. Packet layout, handshake and commit order unknown."),
        (0x1d68, 0x98, "garden-palette-selector", "sprites", "Display palette index compare13 at1D88; <=13 uses24050+index*32; higher uses caller RAM palette030015B0. Index-to-Chao-type mapping unknown."),
        (0x3a90, 0x68, "garden-minigame-selector", "chao-ball-candidate", "Object ID4 and secondary flag select Thumb entryD295 orA8A5. Semantic Chao Ball label remains unconfirmed by runtime."),
        (0xd294, 0x270, "garden-minigame-loader", "chao-ball-candidate", "Thumb loader referenced at3AD4 as0200D295. Exact BG/OBJ tile and palette literals mapped separately."),
        (0x24050, 0x1c0, "garden-chao-palette-banks", "palettes", "Fourteen32-byte ROM palette banks; selector index is not proven equal to native Chao type code."),
        (0x24210, 0x180, "garden-secondary-palette-banks", "palettes", "Twelve32-byte palette banks referenced at payload1DF8; unassigned display variant semantics."),
    ] { assets.push(raw_asset(&garden.bytes, "garden", offset, length, id, category, evidence)?); }
    for asset in &mut assets {
        if asset.space == "garden" {
            match asset.offset {
                0x1614c => {
                    asset.category = "chao-ball-candidate".into();
                    asset.status = "verified-obj-tiles".into();
                    asset.evidence = "Unique A3VE minigame OBJ loader literal0xD4FC; destination0x06010000 at0xD500. Minigame identity/animation assembly remain unverified.".into();
                }
                0x17214 => {
                    asset.category = "chao-ball-candidate".into();
                    asset.status = "verified-bg-tiles".into();
                    asset.evidence = "Unique A3VE minigame BG loader literal0xD498; destination0x06000040 at0xD49C. Minigame identity remains unverified.".into();
                }
                _ => {}
            }
        } else if asset.offset == 0x3345fc {
            asset.category = "pinball-table-tilemap".into();
            asset.status = "verified-render".into();
            asset.evidence = "Descriptor0 column1; 32x64 text map independently renders Sonic board using4bpp allocation31C5FC and palette3295FC.".into();
        }
    }
    let songs = crate::audio::parse_songs(rom).map_err(|_| Error::Range)?;
    let mut headers = std::collections::BTreeSet::new();
    for song in songs {
        if headers.insert(song.header_offset) {
            assets.push(raw_asset(rom, "rom", song.header_offset, song.header_bytes, &format!("song-header-{:06x}", song.header_offset), "audio", "Header length8+trackCount*4; track starts/voicegroup bounds validated, no opcode stream length or playback claim.")?);
        }
    }
    let tables = crate::tables::parse_tables(rom).map_err(|_| Error::Range)?;
    for descriptor in &tables.descriptors {
        for (variant, allocation) in descriptor.inferred_tile_allocations.iter().enumerate() {
            let mut asset = raw_asset(
                rom,
                "rom",
                allocation.rom_offset,
                allocation.bytes,
                &format!("board-{:02}-variant-{variant}-tiles", descriptor.id),
                "tiles",
                allocation.basis,
            )?;
            asset.status = "inferred-allocation".into();
            assets.push(asset);
        }
        for (variant, palette) in descriptor.primary_palettes.iter().enumerate() {
            assets.push(raw_asset(rom, "rom", palette.rom_offset, 512, &format!("board-{:02}-variant-{variant}-palettes", descriptor.id), "palettes", "Descriptor primary palette pointer, 256 BGR555 entries. Row and variant names remain unassigned.")?);
        }
        assets.push(raw_asset(rom, "rom", descriptor.columns[4].rom_offset, 512, &format!("board-{:02}-secondary-palettes", descriptor.id), "palettes", "Descriptor column4 direct palette pointer, 256 BGR555 entries; layer semantics untraced.")?);
        if descriptor.id < 8 {
            for column in [0, 2] {
                assets.push(raw_asset(rom, "rom", descriptor.columns[column].rom_offset, 16, &format!("board-{:02}-column-{column}-variants", descriptor.id), "pinball-table-visuals", "Four ROM pointers in first eight descriptors, retained as indices; no stage/physics identity inference.")?);
            }
        }
    }
    for (i, palette) in tables.supplemental_palettes.iter().enumerate() {
        assets.push(raw_asset(
            rom,
            "rom",
            palette.rom_offset,
            512,
            &format!("supplemental-palettes-{i:02}"),
            "palettes",
            "Supplemental palette pointer, 256 BGR555 entries.",
        )?);
    }
    let rom_pointers = pointer_index(rom, 0x08000000);
    let garden_pointers = pointer_index(&garden.bytes, 0x02000000);
    for asset in &mut assets {
        asset.pointer_references = (if asset.space == "rom" {
            &rom_pointers
        } else {
            &garden_pointers
        })
        .get(&asset.offset)
        .cloned()
        .unwrap_or_default();
    }
    Ok(Catalog { schema_version: 1, source, scan_policy: "4-byte aligned offsets from 0xC0; tags 0x10/0x30; declared length 32..262144; strict full-token decode; aggregate declared-work cap 536870912; <=8192 candidates per space; no semantic inference from shape".into(), assets })
}

pub struct VerifiedRom<'a> {
    rom: &'a [u8],
    garden: binary::Decoded,
}

impl<'a> VerifiedRom<'a> {
    pub fn new(rom: &'a [u8]) -> Result<Self, Error> {
        Ok(Self {
            rom,
            garden: extract_garden(rom)?,
        })
    }

    pub fn garden(&self) -> &[u8] {
        &self.garden.bytes
    }

    pub fn extract(&self, asset: &Asset) -> Result<Vec<u8>, Error> {
        let source = match asset.space.as_str() {
            "rom" => self.rom,
            "garden" => &self.garden.bytes,
            _ => return Err(Error::Range),
        };
        let stored = binary::span(source, asset.offset, asset.stored_length)?;
        if sha256(stored) != asset.stored_sha256 {
            return Err(Error::HashMismatch(asset.id.clone()));
        }
        let output = match asset.encoding.as_str() {
            "raw" => stored.to_vec(),
            "gba-lz77-10" | "gba-rle-30" => {
                let expected_tag = if asset.encoding == "gba-lz77-10" {
                    0x10
                } else {
                    0x30
                };
                if stored.first() != Some(&expected_tag) {
                    return Err(Error::InvalidTag(stored.first().copied().unwrap_or(0)));
                }
                let d = binary::decode(stored, asset.decoded_length)?;
                if d.consumed != stored.len() {
                    return Err(Error::Range);
                }
                d.bytes
            }
            _ => return Err(Error::Range),
        };
        if output.len() != asset.decoded_length || sha256(&output) != asset.decoded_sha256 {
            return Err(Error::HashMismatch(asset.id.clone()));
        }
        Ok(output)
    }
}

pub fn extract_asset(rom: &[u8], asset: &Asset) -> Result<Vec<u8>, Error> {
    VerifiedRom::new(rom)?.extract(asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_other_games_before_offsets() {
        assert!(validate_rom(&[]).is_err());
        let mut fake = vec![0; ROM_LEN];
        fake[0xac..0xb0].copy_from_slice(b"A3VE");
        assert_eq!(validate_rom(&fake), Err(Error::Identity));
        assert_eq!(extract_garden(&fake), Err(Error::Identity));
    }
    #[test]
    fn scan_has_work_budget_and_no_semantic_claims() {
        let mut bytes = vec![0; 0xd0];
        bytes[0xc0..0xc6].copy_from_slice(&[0x30, 32, 0, 0, 0x9d, 7]);
        assert!(matches!(
            scan_space(&bytes, "synthetic", 0x02000000, &mut 31),
            Err(Error::ScanBudget)
        ));
        let assets = scan_space(&bytes, "synthetic", 0x02000000, &mut 32).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].category, "unknown");
        assert_eq!(assets[0].status, "structural-candidate");
    }
}
