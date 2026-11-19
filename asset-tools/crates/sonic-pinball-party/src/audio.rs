//! A3VE's pointer-only m4a-compatible song catalog.
//!
//! This module validates metadata and pointer bounds. It does not interpret
//! sequence opcodes, follow control flow, synthesize instruments, or play audio.

use serde::Serialize;
use std::fmt;

pub const SONG_TABLE_OFFSET: usize = 0x0f_9f74;
pub const SONG_COUNT: usize = 200;
pub const SONG_TABLE_BYTES: usize = SONG_COUNT * 8;
const ROM_BASE: u32 = 0x0800_0000;
const MAX_TRACKS: u8 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioError {
    pub offset: usize,
    pub reason: &'static str,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "audio metadata at {:#x}: {}", self.offset, self.reason)
    }
}

impl std::error::Error for AudioError {}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrackPointer {
    pub pointer_offset: usize,
    pub rom_offset: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Song {
    pub id: usize,
    pub table_entry_offset: usize,
    pub header_offset: usize,
    pub header_bytes: usize,
    pub player_index: u16,
    pub player_group: u16,
    pub track_count: u8,
    pub block_count: u8,
    pub priority: u8,
    pub reverb: u8,
    pub voicegroup_offset: Option<usize>,
    pub tracks: Vec<TrackPointer>,
}

fn bounded(rom: &[u8], offset: usize, length: usize) -> Result<&[u8], AudioError> {
    let end = offset.checked_add(length).ok_or(AudioError {
        offset,
        reason: "range overflow",
    })?;
    rom.get(offset..end).ok_or(AudioError {
        offset,
        reason: "truncated range",
    })
}

fn u32_at(rom: &[u8], offset: usize) -> Result<u32, AudioError> {
    let bytes = bounded(rom, offset, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

fn pointer(rom: &[u8], offset: usize, target_bytes: usize) -> Result<usize, AudioError> {
    let value = u32_at(rom, offset)?;
    let relative = value.checked_sub(ROM_BASE).ok_or(AudioError {
        offset,
        reason: "pointer below ROM base",
    })? as usize;
    bounded(rom, relative, target_bytes).map_err(|_| AudioError {
        offset,
        reason: "pointer target outside ROM",
    })?;
    Ok(relative)
}

/// Read the 200 version-specific 8-byte table entries and bounded song headers.
///
/// Call only after authenticating the A3VE source identity. An all-zero 8-byte
/// header is an empty song. Voicegroup bounds cover its first 12-byte instrument
/// record only; the complete bank extent and track stream lengths remain unknown.
pub fn parse_songs(rom: &[u8]) -> Result<Vec<Song>, AudioError> {
    bounded(rom, SONG_TABLE_OFFSET, SONG_TABLE_BYTES)?;
    let mut songs = Vec::with_capacity(SONG_COUNT);
    for id in 0..SONG_COUNT {
        let entry = SONG_TABLE_OFFSET + id * 8;
        let header_offset = pointer(rom, entry, 8)?;
        if header_offset & 3 != 0 {
            return Err(AudioError {
                offset: entry,
                reason: "unaligned song header",
            });
        }
        let header = bounded(rom, header_offset, 8)?;
        let track_count = header[0];
        if track_count > MAX_TRACKS {
            return Err(AudioError {
                offset: header_offset,
                reason: "track count exceeds limit",
            });
        }
        if header[1] != 0 {
            return Err(AudioError {
                offset: header_offset + 1,
                reason: "block-based song header unsupported",
            });
        }
        if track_count == 0 && header.iter().any(|value| *value != 0) {
            return Err(AudioError {
                offset: header_offset,
                reason: "nonzero empty song header",
            });
        }
        let header_bytes = 8 + usize::from(track_count) * 4;
        bounded(rom, header_offset, header_bytes)?;
        let voicegroup_offset = if track_count == 0 {
            None
        } else {
            let bank = pointer(rom, header_offset + 4, 12)?;
            if bank & 3 != 0 {
                return Err(AudioError {
                    offset: header_offset + 4,
                    reason: "unaligned voicegroup",
                });
            }
            Some(bank)
        };
        let mut tracks = Vec::with_capacity(usize::from(track_count));
        for track in 0..usize::from(track_count) {
            let pointer_offset = header_offset + 8 + track * 4;
            tracks.push(TrackPointer {
                pointer_offset,
                rom_offset: pointer(rom, pointer_offset, 1)?,
            });
        }
        let metadata = bounded(rom, entry + 4, 4)?;
        songs.push(Song {
            id,
            table_entry_offset: entry,
            header_offset,
            header_bytes,
            player_index: u16::from_le_bytes(metadata[0..2].try_into().expect("two bytes")),
            player_group: u16::from_le_bytes(metadata[2..4].try_into().expect("two bytes")),
            track_count,
            block_count: header[1],
            priority: header[2],
            reverb: header[3],
            voicegroup_offset,
            tracks,
        });
    }
    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(rom: &mut [u8], offset: usize, value: u32) {
        rom[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture() -> Vec<u8> {
        let mut rom = vec![0; SONG_TABLE_OFFSET + SONG_TABLE_BYTES];
        for id in 0..SONG_COUNT {
            put(&mut rom, SONG_TABLE_OFFSET + id * 8, ROM_BASE + 0x100);
        }
        rom
    }

    #[test]
    fn preserves_empty_entries_and_nonempty_header_pointers() {
        let mut rom = fixture();
        put(&mut rom, SONG_TABLE_OFFSET + 8, ROM_BASE + 0x140);
        rom[0x140..0x144].copy_from_slice(&[2, 0, 20, 128]);
        put(&mut rom, 0x144, ROM_BASE + 0x200);
        put(&mut rom, 0x148, ROM_BASE + 0x180);
        put(&mut rom, 0x14c, ROM_BASE + 0x181);
        put(&mut rom, SONG_TABLE_OFFSET + 12, 0x0003_0002);
        let songs = parse_songs(&rom).unwrap();
        assert_eq!(songs.len(), 200);
        assert!(songs[0].tracks.is_empty());
        assert_eq!(songs[0].voicegroup_offset, None);
        assert_eq!(songs[1].voicegroup_offset, Some(0x200));
        assert_eq!(songs[1].tracks[1].rom_offset, 0x181);
        assert_eq!(songs[1].tracks[1].pointer_offset, 0x14c);
        assert_eq!(songs[1].header_bytes, 16);
        assert_eq!(songs[1].player_index, 2);
        assert_eq!(songs[1].player_group, 3);
    }

    #[test]
    fn rejects_truncated_table_and_invalid_header_pointer() {
        let mut rom = fixture();
        assert!(parse_songs(&rom[..rom.len() - 1]).is_err());
        put(&mut rom, SONG_TABLE_OFFSET, 0xffff_ffff);
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "pointer target outside ROM"
        );
        put(&mut rom, SONG_TABLE_OFFSET, 0x0200_0000);
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "pointer below ROM base"
        );
    }

    #[test]
    fn rejects_unsupported_counts_empty_headers_and_block_headers() {
        let mut rom = fixture();
        rom[0x100] = 33;
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "track count exceeds limit"
        );
        rom[0x100] = 0;
        rom[0x102] = 1;
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "nonzero empty song header"
        );
        rom[0x101] = 1;
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "block-based song header unsupported"
        );
    }

    #[test]
    fn rejects_out_of_bounds_track_and_voicegroup() {
        let mut rom = fixture();
        rom[0x100..0x104].copy_from_slice(&[1, 0, 0, 128]);
        let end = rom.len() as u32;
        put(&mut rom, 0x104, ROM_BASE + end - 8);
        put(&mut rom, 0x108, ROM_BASE + 0x180);
        assert!(parse_songs(&rom).is_err());
        put(&mut rom, 0x104, ROM_BASE + 0x200);
        put(&mut rom, 0x108, ROM_BASE + end);
        assert!(parse_songs(&rom).is_err());
    }

    #[test]
    fn rejects_truncated_track_array_and_unaligned_structures() {
        let mut rom = fixture();
        let end = rom.len() as u32;
        put(&mut rom, SONG_TABLE_OFFSET, ROM_BASE + end - 8);
        let last = rom.len() - 8;
        rom[last] = 32;
        assert!(parse_songs(&rom).is_err());
        put(&mut rom, SONG_TABLE_OFFSET, ROM_BASE + 0x101);
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "unaligned song header"
        );
        put(&mut rom, SONG_TABLE_OFFSET, ROM_BASE + 0x100);
        rom[0x100..0x104].copy_from_slice(&[1, 0, 0, 128]);
        put(&mut rom, 0x104, ROM_BASE + 0x201);
        assert_eq!(
            parse_songs(&rom).unwrap_err().reason,
            "unaligned voicegroup"
        );
    }
}
