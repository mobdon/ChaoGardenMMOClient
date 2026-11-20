//! Adjacent A3VE sequence commands and native track finish/unlink.
//! Exact entry IDs are used; no opcode-to-handler table is inferred here.
use super::{
    mplay::{Channel, CHANNEL_BYTES, TRACK_BYTES},
    sequence::{SequenceData, SequenceError},
};

pub const UNLINK_OFFSET: usize = 0x5e148;
pub const UNLINK_LENGTH: usize = 0x20;
pub const UNLINK_SHA256: &str = "c3d23903d0b87a7b19fcd84aa0234c674e3014a9a2835853727defeaad3422dc";
pub const FINISH_OFFSET: usize = 0x5e168;
pub const FINISH_LENGTH: usize = 0x30;
pub const FINISH_SHA256: &str = "767cb12459588832366a031b8088826493705f8746b882354bb470db7bdb62de";
pub const REGISTER_BASE: u32 = 0x04000060;
const MAX_RECORDS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Data(SequenceError),
    RegisterWrite { address: u32, value: u8 },
    Address(u32),
    Overlap { first: u32, second: u32 },
    TooManyRecords { tracks: usize, channels: usize },
    MissingTrack(u32),
    MissingChannel(u32),
    ChannelCycle(u32),
    UnportedEntry(u32),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<SequenceError> for Error {
    fn from(value: SequenceError) -> Self {
        Self::Data(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Handler {
    Set18IfChanged,
    Centered0c,
    RegisterByte,
}
impl Handler {
    pub fn from_entry(entry: u32) -> Result<Self, Error> {
        match entry {
            0x5e318 => Ok(Self::Set18IfChanged),
            0x5e330 => Ok(Self::Centered0c),
            0x5e344 => Ok(Self::RegisterByte),
            _ => Err(Error::UnportedEntry(entry)),
        }
    }
}
pub trait RegisterIo {
    /// This is the actual native byte write, not a request to be discarded.
    /// A completed implementation must supply the corresponding device effect.
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Error>;
}
pub struct RejectRegisterIo;
impl RegisterIo for RejectRegisterIo {
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Error> {
        Err(Error::RegisterWrite { address, value })
    }
}
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed native fields"),
    )
}
fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn add(address: u32, offset: u32) -> Result<u32, Error> {
    address
        .checked_add(offset)
        .ok_or(Error::Data(SequenceError::AddressOverflow(address)))
}
fn byte(data: &dyn SequenceData, address: u32) -> Result<u8, Error> {
    data.read(address, 1)
        .filter(|bytes| bytes.len() == 1)
        .map(|bytes| bytes[0])
        .ok_or(Error::Data(SequenceError::MissingBytes {
            address,
            length: 1,
        }))
}
fn protected(data: &dyn SequenceData, address: u32) -> Result<u8, Error> {
    let value = byte(data, address)?;
    // Exact A3VE guard/literal at 0x5e1b2/0x5e1c8: the already-loaded value
    // is cleared for source addresses below 0x02000000.
    Ok(if address < 0x02000000 { 0 } else { value })
}

/// Translate one command using immutable bounded command bytes. Input errors
/// and rejected device writes leave the track unchanged. No player fields are
/// written by these three native entries.
pub fn apply(
    handler: Handler,
    data: &dyn SequenceData,
    track: &mut [u8; TRACK_BYTES],
    io: &mut dyn RegisterIo,
) -> Result<(), Error> {
    let mut next = *track;
    let cursor = word(&next, 0x40);
    if matches!(handler, Handler::RegisterByte) {
        let index = byte(data, cursor)?; // Native index read is unprotected.
        let source = add(cursor, 1)?;
        let value = protected(data, source)?;
        put(&mut next, 0x40, add(source, 1)?);
        io.write_byte(REGISTER_BASE + u32::from(index), value)?;
    } else {
        let value = protected(data, cursor)?;
        put(&mut next, 0x40, add(cursor, 1)?);
        match handler {
            Handler::Set18IfChanged if next[0x18] != value => {
                next[0x18] = value;
                next[0] |= 0x0f;
            }
            Handler::Set18IfChanged => {}
            Handler::Centered0c => {
                next[0x0c] = value.wrapping_sub(0x40);
                next[0] |= 0x0c;
            }
            Handler::RegisterByte => unreachable!("separate native device branch"),
        }
    }
    *track = next;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub address: u32,
    pub bytes: [u8; TRACK_BYTES],
}
/// Explicit snapshots of every addressed record. Owners and backlinks may name
/// different supplied tracks/channels; missing/cyclic/overlapping RAM is an
/// error, never an allocation or a fabricated zero record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelGraph {
    pub tracks: Vec<Track>,
    pub channels: Vec<Channel>,
}
impl ChannelGraph {
    fn track_index(&self, address: u32) -> Result<usize, Error> {
        self.tracks
            .iter()
            .position(|track| track.address == address)
            .ok_or(Error::MissingTrack(address))
    }
    fn channel_index(&self, address: u32) -> Result<usize, Error> {
        self.channels
            .iter()
            .position(|channel| channel.address == address)
            .ok_or(Error::MissingChannel(address))
    }
    fn validate(&self) -> Result<(), Error> {
        if self.tracks.len() > MAX_RECORDS || self.channels.len() > MAX_RECORDS {
            return Err(Error::TooManyRecords {
                tracks: self.tracks.len(),
                channels: self.channels.len(),
            });
        }
        let mut extents = Vec::with_capacity(self.tracks.len() + self.channels.len());
        for (address, size) in self
            .tracks
            .iter()
            .map(|record| (record.address, TRACK_BYTES))
            .chain(
                self.channels
                    .iter()
                    .map(|record| (record.address, CHANNEL_BYTES)),
            )
        {
            let end = address
                .checked_add(size as u32)
                .ok_or(Error::Address(address))?;
            if address & 3 != 0
                || !((0x02000000..0x02040000).contains(&address) && end <= 0x02040000
                    || (0x03000000..0x03008000).contains(&address) && end <= 0x03008000)
            {
                return Err(Error::Address(address));
            }
            for &(old, old_end) in &extents {
                if address < old_end && old < end {
                    return Err(Error::Overlap {
                        first: old,
                        second: address,
                    });
                }
            }
            extents.push((address, end));
        }
        Ok(())
    }
    fn unlink_inner(&mut self, current: usize) -> Result<(), Error> {
        let owner = word(&self.channels[current].bytes, 0x2c);
        if owner == 0 {
            return Ok(());
        }
        let next = word(&self.channels[current].bytes, 0x34);
        let previous = word(&self.channels[current].bytes, 0x30);
        if previous != 0 {
            let index = self.channel_index(previous)?;
            put(&mut self.channels[index].bytes, 0x34, next);
        } else {
            let index = self.track_index(owner)?;
            put(&mut self.tracks[index].bytes, 0x20, next);
        }
        if next != 0 {
            let index = self.channel_index(next)?;
            put(&mut self.channels[index].bytes, 0x30, previous);
        }
        put(&mut self.channels[current].bytes, 0x2c, 0);
        Ok(())
    }
    /// Complete native 0x5e148. Prev/next fields of the detached node survive.
    pub fn unlink_channel(&mut self, address: u32) -> Result<(), Error> {
        self.validate()?;
        let mut staged = self.clone();
        let index = staged.channel_index(address)?;
        staged.unlink_inner(index)?;
        *self = staged;
        Ok(())
    }
    /// Complete native 0x5e168, including 0x5e148 in original visit order.
    /// This differs from trackStop 0x5e610: it requests channel release with bit
    /// 0x40, unlinks owners/backlinks, then clears only the track flag byte.
    /// No oscillator, mixer, note-duration or audible-release effect is claimed.
    pub fn finish_track(&mut self, address: u32) -> Result<(), Error> {
        self.validate()?;
        let mut staged = self.clone();
        let track = staged.track_index(address)?;
        let mut current = word(&staged.tracks[track].bytes, 0x20);
        let mut visited = Vec::new();
        while current != 0 {
            if visited.contains(&current) {
                return Err(Error::ChannelCycle(current));
            }
            visited.push(current);
            let index = staged.channel_index(current)?;
            let flags = staged.channels[index].bytes[0];
            if flags & 0xc7 != 0 {
                staged.channels[index].bytes[0] = flags | 0x40;
            }
            staged.unlink_inner(index)?;
            // The native routine reloads this after unlink; do not replace it
            // with a precomputed chain when backlinks can modify a record.
            current = word(&staged.channels[index].bytes, 0x34);
        }
        staged.tracks[track].bytes[0] = 0;
        *self = staged;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn bytes<const N: usize>(value: &Value) -> [u8; N] {
        let hex = value.as_str().unwrap();
        assert_eq!(hex.len(), N * 2);
        std::array::from_fn(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
    }
    struct Data {
        address: u32,
        command: [u8; 8],
    }
    impl SequenceData for Data {
        fn read(&self, address: u32, length: usize) -> Option<&[u8]> {
            let index = address.checked_sub(self.address)? as usize;
            self.command.get(index..index.checked_add(length)?)
        }
    }
    #[derive(Default)]
    struct CapturedRegisterWrites(Vec<[u32; 3]>);
    impl RegisterIo for CapturedRegisterWrites {
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Error> {
            // Matches the oracle's observed original byte store. This test
            // capture does not implement a GBA device or oscillator.
            self.0.push([address, 1, u32::from(value)]);
            Ok(())
        }
    }
    #[test]
    fn adjacent_commands_match_384_original_instruction_cases() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/sequence-extended-vectors.json"))
                .unwrap();
        let cases = vectors["commands"].as_array().unwrap();
        assert_eq!(cases.len(), 384);
        for (index, case) in cases.iter().enumerate() {
            let data = Data {
                address: case["cursor"].as_u64().unwrap() as u32,
                command: bytes(&case["command"]),
            };
            let mut track = bytes(&case["track"]);
            let mut io = CapturedRegisterWrites::default();
            apply(
                Handler::from_entry(case["entry"].as_u64().unwrap() as u32).unwrap(),
                &data,
                &mut track,
                &mut io,
            )
            .unwrap();
            assert_eq!(
                track,
                bytes(&case["after"]),
                "complete native track case {index}"
            );
            let expected = case["device_writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| std::array::from_fn(|i| row[i].as_u64().unwrap() as u32))
                .collect::<Vec<[u32; 3]>>();
            assert_eq!(io.0, expected, "native device-write boundary case {index}");
        }
    }
    fn tracks(value: &Value) -> Vec<Track> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|record| Track {
                address: record["address"].as_u64().unwrap() as u32,
                bytes: bytes(&record["bytes"]),
            })
            .collect()
    }
    fn channels(value: &Value) -> Vec<Channel> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|record| Channel {
                address: record["address"].as_u64().unwrap() as u32,
                bytes: bytes(&record["bytes"]),
            })
            .collect()
    }
    #[test]
    fn unlink_and_finish_match_512_complete_original_record_snapshots() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/sequence-extended-vectors.json"))
                .unwrap();
        let cases = vectors["graphs"].as_array().unwrap();
        assert_eq!(cases.len(), 512);
        for (index, case) in cases.iter().enumerate() {
            let mut graph = ChannelGraph {
                tracks: tracks(&case["tracks"]),
                channels: channels(&case["channels"]),
            };
            let argument = case["argument"].as_u64().unwrap() as u32;
            match case["entry"].as_u64().unwrap() {
                0x5e148 => graph.unlink_channel(argument).unwrap(),
                0x5e168 => graph.finish_track(argument).unwrap(),
                _ => panic!("unexpected original graph entry"),
            }
            assert_eq!(
                graph,
                ChannelGraph {
                    tracks: tracks(&case["after_tracks"]),
                    channels: channels(&case["after_channels"])
                },
                "native graph case {index}"
            );
        }
    }
    #[test]
    fn unavailable_device_missing_data_and_overflow_are_atomic() {
        let data = Data {
            address: 0x02008000,
            command: [2, 255, 0, 0, 0, 0, 0, 0],
        };
        let mut track = [0x55; TRACK_BYTES];
        put(&mut track, 0x40, data.address);
        let before = track;
        assert_eq!(
            apply(
                Handler::RegisterByte,
                &data,
                &mut track,
                &mut RejectRegisterIo
            ),
            Err(Error::RegisterWrite {
                address: REGISTER_BASE + 2,
                value: 255
            })
        );
        assert_eq!(track, before);
        put(&mut track, 0x40, 0x02008008);
        let before = track;
        assert!(matches!(
            apply(
                Handler::Centered0c,
                &data,
                &mut track,
                &mut RejectRegisterIo
            ),
            Err(Error::Data(SequenceError::MissingBytes { .. }))
        ));
        assert_eq!(track, before);
        let end = Data {
            address: u32::MAX,
            command: [1; 8],
        };
        put(&mut track, 0x40, u32::MAX);
        let before = track;
        assert!(matches!(
            apply(
                Handler::Set18IfChanged,
                &end,
                &mut track,
                &mut RejectRegisterIo
            ),
            Err(Error::Data(SequenceError::AddressOverflow(u32::MAX)))
        ));
        assert_eq!(track, before);
        assert!(
            Handler::from_entry(0x5e35c).is_err(),
            "adjacent runtime is not a guessed sequence handler"
        );
    }
    #[test]
    fn missing_owners_cycles_and_aliases_fail_without_changing_records() {
        let track = Track {
            address: 0x02002000,
            bytes: [0; TRACK_BYTES],
        };
        let channel = Channel {
            address: 0x02003000,
            bytes: [0; CHANNEL_BYTES],
        };
        let mut graph = ChannelGraph {
            tracks: vec![track],
            channels: vec![channel],
        };
        put(&mut graph.tracks[0].bytes, 0x20, 0x02003000);
        graph.channels[0].bytes[0] = 1;
        put(&mut graph.channels[0].bytes, 0x2c, 0x02002100);
        let before = graph.clone();
        assert_eq!(
            graph.finish_track(0x02002000),
            Err(Error::MissingTrack(0x02002100))
        );
        assert_eq!(graph, before);
        put(&mut graph.channels[0].bytes, 0x2c, 0x02002000);
        put(&mut graph.channels[0].bytes, 0x34, 0x02003000);
        let before = graph.clone();
        assert_eq!(
            graph.finish_track(0x02002000),
            Err(Error::ChannelCycle(0x02003000))
        );
        assert_eq!(graph, before);
        graph.channels[0].address = graph.tracks[0].address;
        let before = graph.clone();
        assert!(matches!(
            graph.unlink_channel(0x02002000),
            Err(Error::Overlap { .. })
        ));
        assert_eq!(graph, before);
        graph.channels[0].address = 0x02003000;
        put(&mut graph.channels[0].bytes, 0x2c, 0);
        put(&mut graph.channels[0].bytes, 0x30, 0xffffffff);
        put(&mut graph.channels[0].bytes, 0x34, 0xffffffff);
        let before = graph.clone();
        graph.unlink_channel(0x02003000).unwrap();
        assert_eq!(
            graph, before,
            "unowned native node never dereferences its dormant links"
        );
    }
}
