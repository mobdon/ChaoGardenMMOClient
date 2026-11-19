//! Lossless table and m4a descriptor inspection; no sound synthesis or motion VM.
use crate::{Error, bounded_range};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionRecord {
    pub first_word: u16,
    pub fields: [u8; 6],
}

/// The garden's eight-byte frame records retain unnamed fields verbatim.
/// Not every arbitrary eight-byte data range is a motion script.
pub fn motion_records(bytes: &[u8], max_records: usize) -> Result<Vec<MotionRecord>, Error> {
    if !bytes.len().is_multiple_of(8) || bytes.len() / 8 > max_records.min(4096) {
        return Err(Error("Invalid or oversized eight-byte motion table".into()));
    }
    Ok(bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| MotionRecord {
            first_word: u16::from_le_bytes([b[0], b[1]]),
            fields: b[2..].try_into().expect("six bytes"),
        })
        .collect())
}

pub fn pointer_offset(pointer: u32, base: u32, source_len: usize) -> Result<usize, Error> {
    let offset = pointer
        .checked_sub(base)
        .ok_or_else(|| Error("Pointer below source base".into()))? as usize;
    if offset >= source_len {
        return Err(Error("Pointer outside source".into()));
    }
    Ok(offset)
}

#[derive(Debug, Serialize)]
pub struct PointerReference {
    pub pointer: u32,
    pub offset: Option<usize>,
}

/// Indexing proves pointer starts are inside the fingerprinted source, not
/// the lengths, command grammar or reachability of their target resources.
pub fn pointer_table(
    bytes: &[u8],
    base: u32,
    source_len: usize,
) -> Result<Vec<PointerReference>, Error> {
    if !bytes.len().is_multiple_of(4) || bytes.len() / 4 > 16384 {
        return Err(Error("Invalid or oversized pointer table".into()));
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| {
            let pointer = u32::from_le_bytes(*word);
            Ok(PointerReference {
                pointer,
                offset: if pointer == 0 {
                    None
                } else {
                    Some(pointer_offset(pointer, base, source_len)?)
                },
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongHeader {
    pub track_count: u8,
    pub block_count: u8,
    pub priority: u8,
    pub reverb: u8,
    pub voicegroup_pointer: u32,
    pub track_pointers: Vec<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongTableEntry {
    pub pointer: u32,
    pub opaque_tail_words: [u16; 2],
    pub header: Option<SongHeader>,
}

/// Inspect bounded eight-byte m4a song-table rows and referenced headers.
/// Four zero bytes identify the native dummy header; it has no voicegroup.
pub fn song_table(
    source: &[u8],
    offset: usize,
    length: usize,
    base: u32,
) -> Result<Vec<SongTableEntry>, Error> {
    if !length.is_multiple_of(8) || length / 8 > 1024 {
        return Err(Error("Invalid or oversized m4a song table".into()));
    }
    let raw = bounded_range(source, offset, length)?;
    let mut rows = Vec::with_capacity(length / 8);
    for row in raw.as_chunks::<8>().0.iter() {
        let pointer = u32::from_le_bytes(row[..4].try_into().expect("four bytes"));
        let target = pointer_offset(pointer, base, source.len())?;
        let prefix = bounded_range(source, target, 4)?;
        let header = if prefix == [0, 0, 0, 0] {
            None
        } else {
            Some(song_header(source, target, base)?)
        };
        rows.push(SongTableEntry {
            pointer,
            opaque_tail_words: [
                u16::from_le_bytes([row[4], row[5]]),
                u16::from_le_bytes([row[6], row[7]]),
            ],
            header,
        });
    }
    Ok(rows)
}

/// Standard m4a song header: four control bytes, voicegroup, then tracks.
/// Voicegroup/track pointer starts are checked. Target lengths, sequence
/// commands, instruments and sample contents remain unknown.
pub fn song_header(source: &[u8], offset: usize, base: u32) -> Result<SongHeader, Error> {
    let header = bounded_range(source, offset, 8)?;
    let tracks = header[0];
    if tracks == 0 || tracks > 16 {
        return Err(Error("Invalid m4a track count (expected 1..16)".into()));
    }
    let voicegroup_pointer = u32::from_le_bytes(header[4..8].try_into().expect("four bytes"));
    pointer_offset(voicegroup_pointer, base, source.len())?;
    let track_start = offset
        .checked_add(8)
        .ok_or_else(|| Error("Song offset overflow".into()))?;
    let pointers = bounded_range(source, track_start, tracks as usize * 4)?;
    let mut track_pointers = Vec::with_capacity(tracks as usize);
    for bytes in pointers.as_chunks::<4>().0.iter() {
        let pointer = u32::from_le_bytes(*bytes);
        pointer_offset(pointer, base, source.len())?;
        track_pointers.push(pointer);
    }
    Ok(SongHeader {
        track_count: tracks,
        block_count: header[1],
        priority: header[2],
        reverb: header[3],
        voicegroup_pointer,
        track_pointers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motions_preserve_every_field_and_reject_partial_records() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8];
        let row = &motion_records(&bytes, 1).unwrap()[0];
        assert_eq!(row.first_word, 0x201);
        assert_eq!(row.fields, [3, 4, 5, 6, 7, 8]);
        assert!(motion_records(&bytes[..7], 1).is_err());
        assert!(motion_records(&bytes, 0).is_err());
    }
    #[test]
    fn song_header_checks_address_space_and_truncation() {
        let mut bytes = vec![0; 32];
        bytes[0] = 1;
        bytes[4..8].copy_from_slice(&0x02000010u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0x02000018u32.to_le_bytes());
        assert_eq!(
            song_header(&bytes, 0, 0x02000000).unwrap().track_pointers,
            [0x02000018]
        );
        assert!(song_header(&bytes[..11], 0, 0x02000000).is_err());
        bytes[8..12].copy_from_slice(&0x08000018u32.to_le_bytes());
        assert!(song_header(&bytes, 0, 0x02000000).is_err());
        assert!(pointer_offset(0x01ffffff, 0x02000000, 32).is_err());
        assert!(pointer_offset(0x02000020, 0x02000000, 32).is_err());
    }

    #[test]
    fn dummy_song_has_no_invented_voicegroup() {
        let mut source = vec![0; 12];
        source[..4].copy_from_slice(&0x02000008u32.to_le_bytes());
        let rows = song_table(&source, 0, 8, 0x02000000).unwrap();
        assert!(rows[0].header.is_none());
        assert!(song_table(&source, 0, 7, 0x02000000).is_err());
        source[..4].copy_from_slice(&0x0200000bu32.to_le_bytes());
        assert!(song_table(&source, 0, 8, 0x02000000).is_err());
    }

    #[test]
    fn pointer_tables_preserve_addresses_and_reject_wrong_domains() {
        let word = 0x08000018u32.to_le_bytes();
        let table = pointer_table(&word, 0x08000000, 32).unwrap();
        assert_eq!(table[0].pointer, 0x08000018);
        assert_eq!(table[0].offset, Some(24));
        assert_eq!(
            pointer_table(&[0; 4], 0x08000000, 32).unwrap()[0].offset,
            None
        );
        assert!(pointer_table(&word[..3], 0x08000000, 32).is_err());
        assert!(pointer_table(&word, 0x02000000, 32).is_err());
    }
}
