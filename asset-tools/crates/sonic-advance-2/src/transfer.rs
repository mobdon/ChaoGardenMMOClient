//! Read-only JoyBus transfer framing, independently implemented from format facts.
//!
//! Evidence: SAT-R/sa2 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
//! `chao_garden/src/joy_bus.c`, sub_02000b0c/sub_02000b64 and
//! sub_02000988/sub_02000a40. This is not a hardware protocol emulator.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;

/// The subsequent native copy targets save[0x58..0x858]. The hardware code
/// trusts its peer; this inspection tool rejects lengths beyond that storage.
pub const MAX_TRANSFER_BYTES: usize = 0x800;
pub const NATIVE_SEND_SAVE_OFFSET: usize = 0x58;
pub const NATIVE_RECEIVE_IWRAM_ADDRESS: u32 = 0x0300_3350;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    UnsupportedCommand(u8),
    PayloadTooLarge { declared: usize, limit: usize },
    WrongPayloadLength { expected: usize, actual: usize },
    WrongWordLength { actual: usize },
    UnexpectedSequence { expected: u16, actual: u16 },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TransferError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    ToGarden,
    FromGarden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCommand {
    pub raw_word: u32,
    pub opcode: u8,
    /// Preserved without guessing whether this byte is a checksum or flags.
    pub opaque_byte: u8,
    pub direction: Direction,
    pub halfword_count: u16,
    pub payload_bytes: usize,
}

impl TransferCommand {
    /// Command low byte selects receive (2A) or send (22); its upper two
    /// bytes encode a big-endian halfword count. Command 26 is a send-start
    /// control word rather than a second independent length declaration.
    pub fn parse(raw_word: u32) -> Result<Self, TransferError> {
        let opcode = raw_word as u8;
        let direction = match opcode {
            0x2a => Direction::ToGarden,
            0x22 => Direction::FromGarden,
            other => return Err(TransferError::UnsupportedCommand(other)),
        };
        let halfword_count = ((raw_word >> 16) as u16).swap_bytes();
        let payload_bytes = usize::from(halfword_count) * 2;
        if payload_bytes > MAX_TRANSFER_BYTES {
            return Err(TransferError::PayloadTooLarge {
                declared: payload_bytes,
                limit: MAX_TRANSFER_BYTES,
            });
        }
        Ok(Self {
            raw_word,
            opcode,
            opaque_byte: (raw_word >> 8) as u8,
            direction,
            halfword_count,
            payload_bytes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferWord {
    pub raw_word: u32,
    /// The low halfword carries the byte-swapped sequence number.
    pub sequence: u16,
    /// The upper halfword stores two payload bytes in little-endian order.
    pub data: [u8; 2],
}

impl TransferWord {
    pub fn parse(raw_word: u32) -> Self {
        Self {
            raw_word,
            sequence: (raw_word as u16).swap_bytes(),
            data: [(raw_word >> 16) as u8, (raw_word >> 24) as u8],
        }
    }

    /// Local transcript files represent each hardware word little-endian.
    pub fn from_le_bytes(bytes: &[u8]) -> Result<Self, TransferError> {
        let raw: [u8; 4] = bytes
            .try_into()
            .map_err(|_| TransferError::WrongWordLength {
                actual: bytes.len(),
            })?;
        Ok(Self::parse(u32::from_le_bytes(raw)))
    }
}

#[derive(Debug)]
pub struct TransferPayload<'a> {
    pub command: TransferCommand,
    raw: &'a [u8],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadReport {
    pub command: TransferCommand,
    pub sha256: String,
    pub native_save_offset: usize,
    pub interpretation: &'static str,
}

impl<'a> TransferPayload<'a> {
    pub fn parse(command: TransferCommand, raw: &'a [u8]) -> Result<Self, TransferError> {
        // Revalidate even if a caller constructed this public report directly.
        let command = TransferCommand::parse(command.raw_word)?;
        if raw.len() != command.payload_bytes {
            return Err(TransferError::WrongPayloadLength {
                expected: command.payload_bytes,
                actual: raw.len(),
            });
        }
        Ok(Self { command, raw })
    }

    pub fn raw_bytes(&self) -> &'a [u8] {
        self.raw
    }

    pub fn report(&self) -> PayloadReport {
        PayloadReport {
            command: self.command,
            sha256: format!("{:x}", Sha256::digest(self.raw)),
            native_save_offset: NATIVE_SEND_SAVE_OFFSET,
            interpretation: "opaque native save-tail data; checksum validity does not authorize MMO gains",
        }
    }
}

/// Decode a complete, already deduplicated payload-word list. Hardware
/// retries, control words and interrupt state belong to a transcript audit;
/// this helper deliberately requires one sequential word per halfword.
pub fn decode_sequential_words(
    command: TransferCommand,
    words: &[u32],
) -> Result<Vec<u8>, TransferError> {
    let command = TransferCommand::parse(command.raw_word)?;
    if words.len() != usize::from(command.halfword_count) {
        return Err(TransferError::WrongPayloadLength {
            expected: usize::from(command.halfword_count),
            actual: words.len(),
        });
    }
    let mut bytes = Vec::with_capacity(command.payload_bytes);
    for (index, raw_word) in words.iter().enumerate() {
        let word = TransferWord::parse(*raw_word);
        let expected = index as u16;
        if word.sequence != expected {
            return Err(TransferError::UnexpectedSequence {
                expected,
                actual: word.sequence,
            });
        }
        bytes.extend_from_slice(&word.data);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_endianness_unknown_byte_and_storage_bounds() {
        let command = TransferCommand::parse(0x0004_a52a).unwrap();
        assert_eq!(command.direction, Direction::ToGarden);
        assert_eq!(command.halfword_count, 0x400);
        assert_eq!(command.payload_bytes, 0x800);
        assert_eq!(command.opaque_byte, 0xa5);
        assert_eq!(
            TransferCommand::parse(0x0004_0022).unwrap().direction,
            Direction::FromGarden
        );
        assert!(matches!(
            TransferCommand::parse(0x0104_002a),
            Err(TransferError::PayloadTooLarge { .. })
        ));
        assert!(TransferCommand::parse(0x26).is_err());
    }

    #[test]
    fn word_preserves_opaque_data_and_swaps_sequence_only() {
        let word = TransferWord::parse(0xbbaa_3412);
        assert_eq!(word.sequence, 0x1234);
        assert_eq!(word.data, [0xaa, 0xbb]);
        assert_eq!(
            TransferWord::from_le_bytes(&[0x12, 0x34, 0xaa, 0xbb]).unwrap(),
            word
        );
        for length in [0, 1, 2, 3, 5] {
            assert!(TransferWord::from_le_bytes(&vec![0; length]).is_err());
        }
    }

    #[test]
    fn payload_length_is_exact_and_bytes_survive() {
        let command = TransferCommand::parse(0x0200_002a).unwrap();
        let bytes = [0x1c, 0xff, 0xa7, 0];
        let payload = TransferPayload::parse(command, &bytes).unwrap();
        assert_eq!(payload.raw_bytes(), &bytes);
        assert_eq!(payload.report().command.payload_bytes, 4);
        assert!(TransferPayload::parse(command, &bytes[..3]).is_err());
        assert!(TransferPayload::parse(command, &[0; 5]).is_err());
    }

    #[test]
    fn sequential_decoder_rejects_missing_reordered_and_repeated_words() {
        let command = TransferCommand::parse(0x0200_002a).unwrap();
        assert_eq!(
            decode_sequential_words(command, &[0xbbaa_0000, 0xddcc_0100]).unwrap(),
            [0xaa, 0xbb, 0xcc, 0xdd]
        );
        assert!(decode_sequential_words(command, &[0xbbaa_0000]).is_err());
        assert!(decode_sequential_words(command, &[0xbbaa_0000, 0xddcc_0000]).is_err());
        assert!(decode_sequential_words(command, &[0xbbaa_0100, 0xddcc_0000]).is_err());
    }
}
