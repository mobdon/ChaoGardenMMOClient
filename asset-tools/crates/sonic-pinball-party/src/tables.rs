//! Version-specific main pinball visual resource descriptors.
//!
//! Column roles are established by pointer grouping and independent tile preview.
//! Record-to-table/stage names and physics are not inferred from this catalog.

use serde::Serialize;
use std::fmt;

pub const TABLE_DESCRIPTOR_OFFSET: usize = 0x0a_c78c;
pub const TABLE_DESCRIPTOR_COUNT: usize = 23;
pub const TABLE_DESCRIPTOR_BYTES: usize = TABLE_DESCRIPTOR_COUNT * 7 * 4;
pub const SUPPLEMENTAL_LZ_OFFSET: usize = 0x0a_ca10;
pub const SUPPLEMENTAL_PALETTE_OFFSET: usize = 0x0a_ca2c;
const ROM_BASE: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError {
    pub offset: usize,
    pub reason: &'static str,
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table metadata at {:#x}: {}", self.offset, self.reason)
    }
}

impl std::error::Error for TableError {}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResourcePointer {
    pub pointer_offset: usize,
    pub rom_offset: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InferredAllocation {
    pub rom_offset: usize,
    pub bytes: usize,
    /// This is adjacent-resource spacing, not a traced native copy size.
    pub basis: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TableDescriptor {
    pub id: usize,
    pub descriptor_offset: usize,
    /// The original seven pointers, preserving column order and source locations.
    pub columns: Vec<ResourcePointer>,
    /// Records 0..7 have four variant pointers in column 0; later records are direct.
    pub primary_tiles: Vec<ResourcePointer>,
    /// Corresponding 256-entry BGR555 banks from column 2 (four variants or direct).
    pub primary_palettes: Vec<ResourcePointer>,
    pub inferred_tile_allocations: Vec<InferredAllocation>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TableResources {
    pub descriptor_offset: usize,
    pub descriptor_bytes: usize,
    pub descriptors: Vec<TableDescriptor>,
    pub supplemental_lz: Vec<ResourcePointer>,
    pub supplemental_palettes: Vec<ResourcePointer>,
}

fn range(rom: &[u8], offset: usize, length: usize) -> Result<&[u8], TableError> {
    let end = offset.checked_add(length).ok_or(TableError {
        offset,
        reason: "range overflow",
    })?;
    rom.get(offset..end).ok_or(TableError {
        offset,
        reason: "truncated range",
    })
}

fn pointer(rom: &[u8], offset: usize, target_bytes: usize) -> Result<ResourcePointer, TableError> {
    let encoded = range(rom, offset, 4)?;
    let value = u32::from_le_bytes(encoded.try_into().expect("four bytes"));
    let rom_offset = value.checked_sub(ROM_BASE).ok_or(TableError {
        offset,
        reason: "pointer below ROM base",
    })? as usize;
    range(rom, rom_offset, target_bytes).map_err(|_| TableError {
        offset,
        reason: "pointer target outside ROM",
    })?;
    Ok(ResourcePointer {
        pointer_offset: offset,
        rom_offset,
    })
}

fn pointers(
    rom: &[u8],
    offset: usize,
    count: usize,
    target_bytes: usize,
) -> Result<Vec<ResourcePointer>, TableError> {
    range(rom, offset, count * 4)?;
    (0..count)
        .map(|i| pointer(rom, offset + i * 4, target_bytes))
        .collect()
}

/// Read exactly 23 x 7 descriptors, seven supplemental LZ pointers and fourteen
/// supplemental palettes. Native identity must be verified by the caller first.
pub fn parse_tables(rom: &[u8]) -> Result<TableResources, TableError> {
    range(rom, TABLE_DESCRIPTOR_OFFSET, TABLE_DESCRIPTOR_BYTES)?;
    let mut descriptors = Vec::with_capacity(TABLE_DESCRIPTOR_COUNT);
    for id in 0..TABLE_DESCRIPTOR_COUNT {
        let descriptor_offset = TABLE_DESCRIPTOR_OFFSET + id * 28;
        let columns = pointers(rom, descriptor_offset, 7, 1)?;
        let (primary_tiles, primary_palettes) = if id < 8 {
            (
                pointers(rom, columns[0].rom_offset, 4, 1)?,
                pointers(rom, columns[2].rom_offset, 4, 512)?,
            )
        } else {
            range(rom, columns[2].rom_offset, 512)?;
            (vec![columns[0].clone()], vec![columns[2].clone()])
        };
        // Column 4 is a directly referenced 256-entry palette in every record.
        range(rom, columns[4].rom_offset, 512)?;
        let mut inferred_tile_allocations = Vec::with_capacity(primary_tiles.len());
        for (variant, tile) in primary_tiles.iter().enumerate() {
            let (end, basis) = if id < 8 {
                (
                    primary_palettes[variant].rom_offset,
                    "spacing to corresponding palette; native copy extent untraced",
                )
            } else {
                (
                    columns[1].rom_offset,
                    "spacing to primary tilemap; native copy extent untraced",
                )
            };
            let bytes = end.checked_sub(tile.rom_offset).ok_or(TableError {
                offset: tile.pointer_offset,
                reason: "inferred tile allocation reversed",
            })?;
            if bytes == 0 || bytes % 32 != 0 || bytes > 128 * 1024 {
                return Err(TableError {
                    offset: tile.pointer_offset,
                    reason: "inferred tile allocation outside supported bounds",
                });
            }
            range(rom, tile.rom_offset, bytes)?;
            inferred_tile_allocations.push(InferredAllocation {
                rom_offset: tile.rom_offset,
                bytes,
                basis,
            });
        }
        descriptors.push(TableDescriptor {
            id,
            descriptor_offset,
            columns,
            primary_tiles,
            primary_palettes,
            inferred_tile_allocations,
        });
    }
    let supplemental_lz = pointers(rom, SUPPLEMENTAL_LZ_OFFSET, 7, 4)?;
    for item in &supplemental_lz {
        if rom[item.rom_offset] != 0x10 {
            return Err(TableError {
                offset: item.rom_offset,
                reason: "supplemental resource is not LZ77",
            });
        }
    }
    let supplemental_palettes = pointers(rom, SUPPLEMENTAL_PALETTE_OFFSET, 14, 512)?;
    Ok(TableResources {
        descriptor_offset: TABLE_DESCRIPTOR_OFFSET,
        descriptor_bytes: TABLE_DESCRIPTOR_BYTES,
        descriptors,
        supplemental_lz,
        supplemental_palettes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(rom: &mut [u8], offset: usize, target: usize) {
        rom[offset..offset + 4].copy_from_slice(&(ROM_BASE + target as u32).to_le_bytes());
    }

    fn fixture() -> Vec<u8> {
        let mut rom = vec![0; SUPPLEMENTAL_PALETTE_OFFSET + 14 * 4];
        for id in 0..TABLE_DESCRIPTOR_COUNT {
            let offset = TABLE_DESCRIPTOR_OFFSET + id * 28;
            for column in 0..7 {
                put(&mut rom, offset + column * 4, 0x800);
            }
            if id < 8 {
                put(&mut rom, offset, 0x100);
                put(&mut rom, offset + 8, 0x140);
            } else {
                put(&mut rom, offset, 0x200);
                put(&mut rom, offset + 4, 0x400);
                put(&mut rom, offset + 8, 0x400);
            }
        }
        for variant in 0..4 {
            put(&mut rom, 0x100 + variant * 4, 0x200);
            put(&mut rom, 0x140 + variant * 4, 0x400);
        }
        for index in 0..7 {
            put(&mut rom, SUPPLEMENTAL_LZ_OFFSET + index * 4, 0xa00);
        }
        rom[0xa00] = 0x10;
        for index in 0..14 {
            put(&mut rom, SUPPLEMENTAL_PALETTE_OFFSET + index * 4, 0x400);
        }
        rom
    }

    #[test]
    fn preserves_source_pointer_locations_and_indirection() {
        let resources = parse_tables(&fixture()).unwrap();
        assert_eq!(resources.descriptors.len(), 23);
        assert_eq!(resources.descriptors[0].primary_tiles.len(), 4);
        assert_eq!(
            resources.descriptors[0].primary_tiles[2].pointer_offset,
            0x108
        );
        assert_eq!(resources.descriptors[8].primary_tiles.len(), 1);
        assert_eq!(
            resources.descriptors[0].inferred_tile_allocations[0].bytes,
            512
        );
        assert_eq!(resources.supplemental_lz.len(), 7);
        assert_eq!(resources.supplemental_palettes.len(), 14);
    }

    #[test]
    fn rejects_truncation_and_invalid_nested_pointers() {
        let mut rom = fixture();
        assert!(parse_tables(&rom[..rom.len() - 1]).is_err());
        rom[0x100..0x104].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        assert_eq!(
            parse_tables(&rom).unwrap_err().reason,
            "pointer target outside ROM"
        );
        rom[0x100..0x104].copy_from_slice(&0x0200_0000u32.to_le_bytes());
        assert_eq!(
            parse_tables(&rom).unwrap_err().reason,
            "pointer below ROM base"
        );
    }

    #[test]
    fn rejects_palette_truncation_and_reversed_allocation() {
        let mut rom = fixture();
        let last = rom.len() - 1;
        put(&mut rom, SUPPLEMENTAL_PALETTE_OFFSET, last);
        assert!(parse_tables(&rom).is_err());
        let mut rom = fixture();
        put(&mut rom, 0x100, 0x600);
        assert_eq!(
            parse_tables(&rom).unwrap_err().reason,
            "inferred tile allocation reversed"
        );
    }

    #[test]
    fn rejects_oversized_allocations_and_invalid_lz_marker() {
        let mut rom = fixture();
        put(&mut rom, 0x140, 0x20800);
        assert!(parse_tables(&rom).is_err());
        let mut rom = fixture();
        rom[0xa00] = 0x20;
        assert_eq!(
            parse_tables(&rom).unwrap_err().reason,
            "supplemental resource is not LZ77"
        );
    }
}
