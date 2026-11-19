//! Native MPlay command leaves. This is not a complete sequencer or mixer.
//! Entry IDs identify original routines; no Advance opcode layout is assumed.
use super::mplay::{PLAYER_BYTES, TRACK_BYTES};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceError {
    MissingBytes { address: u32, length: usize },
    AddressOverflow(u32),
    PatternDepth(u8),
    UnportedRoutine(u32),
}
impl std::fmt::Display for SequenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SequenceError {}

pub trait SequenceData {
    /// Bounded, non-aliasing command/instrument source bytes only. Native game
    /// streams are ROM resources. MMIO and self-modifying code are not modeled.
    fn read(&self, address: u32, length: usize) -> Option<&[u8]>;
}
#[derive(Debug, Clone, Copy)]
pub enum Handler {
    Goto,
    Pattern,
    Return,
    Repeat,
    Set1d,
    Tempo,
    Set0a,
    Voice,
    Set12,
    Centered14,
    Centered0e,
    Set0f,
    Set1b,
}
impl Handler {
    pub fn from_entry(entry: u32) -> Result<Self, SequenceError> {
        Ok(match entry {
            0x5e1d8 => Self::Goto,
            0x5e1f8 => Self::Pattern,
            0x5e214 => Self::Return,
            0x5e228 => Self::Repeat,
            0x5e258 => Self::Set1d,
            0x5e264 => Self::Tempo,
            0x5e278 => Self::Set0a,
            0x5e28c => Self::Voice,
            0x5e2bc => Self::Set12,
            0x5e2d0 => Self::Centered14,
            0x5e2e4 => Self::Centered0e,
            0x5e2f8 => Self::Set0f,
            0x5e30c => Self::Set1b,
            _ => return Err(SequenceError::UnportedRoutine(entry)),
        })
    }
}
fn word(b: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(b[offset..offset + 4].try_into().expect("fixed fields"))
}
fn put(b: &mut [u8], offset: usize, v: u32) {
    b[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}
fn add(address: u32, offset: u32) -> Result<u32, SequenceError> {
    address
        .checked_add(offset)
        .ok_or(SequenceError::AddressOverflow(address))
}
fn byte(data: &dyn SequenceData, address: u32) -> Result<u8, SequenceError> {
    data.read(address, 1)
        .filter(|b| b.len() == 1)
        .map(|b| b[0])
        .ok_or(SequenceError::MissingBytes { address, length: 1 })
}
fn protected(data: &dyn SequenceData, address: u32) -> Result<u8, SequenceError> {
    let value = byte(data, address)?;
    // Original 0x5E1B2 guard clears data loaded below native work RAM.
    Ok(if address < 0x02000000 { 0 } else { value })
}
fn next(data: &dyn SequenceData, track: &mut [u8; TRACK_BYTES]) -> Result<u8, SequenceError> {
    let address = word(track, 0x40);
    let value = protected(data, address)?;
    put(track, 0x40, add(address, 1)?);
    Ok(value)
}
fn jump(data: &dyn SequenceData, track: &mut [u8; TRACK_BYTES]) -> Result<(), SequenceError> {
    let address = word(track, 0x40);
    // Preserve the original high-to-low byte access order.
    let high = byte(data, add(address, 3)?)?;
    let middle = byte(data, add(address, 2)?)?;
    let low = byte(data, add(address, 1)?)?;
    let first = protected(data, address)?;
    put(track, 0x40, u32::from_le_bytes([first, low, middle, high]));
    Ok(())
}

/// Apply one translated command. Invalid source/depth errors are atomic Rust
/// boundaries; native valid-state wrapping/truncation and unknown bytes survive.
/// Pattern overflow calls unported 0x5E168 and stops explicitly.
pub fn apply(
    handler: Handler,
    data: &dyn SequenceData,
    player: &mut [u8; PLAYER_BYTES],
    track: &mut [u8; TRACK_BYTES],
) -> Result<(), SequenceError> {
    let mut p = *player;
    let mut t = *track;
    match handler {
        Handler::Goto => jump(data, &mut t)?,
        Handler::Pattern => {
            let depth = t[2];
            if depth >= 3 {
                return Err(SequenceError::UnportedRoutine(0x5e168));
            }
            let address = word(&t, 0x40);
            put(&mut t, 0x44 + usize::from(depth) * 4, add(address, 4)?);
            t[2] = depth + 1;
            jump(data, &mut t)?;
        }
        Handler::Return => {
            if t[2] > 3 {
                return Err(SequenceError::PatternDepth(t[2]));
            }
            if t[2] != 0 {
                t[2] -= 1;
                let address = word(&t, 0x44 + usize::from(t[2]) * 4);
                put(&mut t, 0x40, address);
            }
        }
        Handler::Repeat => {
            let address = word(&t, 0x40);
            if byte(data, address)? == 0 {
                put(&mut t, 0x40, add(address, 1)?);
                jump(data, &mut t)?;
            } else {
                let count = u32::from(t[3]) + 1;
                t[3] = count as u8;
                let limit = next(data, &mut t)?;
                if count >= u32::from(limit) {
                    t[3] = 0;
                    put(&mut t, 0x40, add(address, 5)?);
                } else {
                    jump(data, &mut t)?;
                }
            }
        }
        Handler::Tempo => {
            let value = u16::from(next(data, &mut t)?) * 2;
            p[0x1c..0x1e].copy_from_slice(&value.to_le_bytes());
            let scale = u16::from_le_bytes([p[0x1e], p[0x1f]]);
            let scaled = ((u32::from(value) * u32::from(scale)) >> 8) as u16;
            p[0x20..0x22].copy_from_slice(&scaled.to_le_bytes());
        }
        Handler::Voice => {
            let cursor = word(&t, 0x40);
            let index = byte(data, cursor)?;
            let base = add(word(&p, 0x30), u32::from(index) * 12)?;
            add(base, 11)?;
            let bytes = data.read(base, 12).filter(|b| b.len() == 12).ok_or(
                SequenceError::MissingBytes {
                    address: base,
                    length: 12,
                },
            )?;
            let voice = if base < 0x02000000 {
                [0; 12]
            } else {
                bytes.try_into().expect("exact instrument prefix")
            };
            put(&mut t, 0x40, add(cursor, 1)?);
            t[0x24..0x30].copy_from_slice(&voice);
        }
        _ => {
            let value = next(data, &mut t)?;
            let (offset, flags, center) = match handler {
                Handler::Set1d => (0x1d, 0, false),
                Handler::Set0a => (0x0a, 0x0c, false),
                Handler::Set12 => (0x12, 3, false),
                Handler::Centered14 => (0x14, 3, true),
                Handler::Centered0e => (0x0e, 0x0c, true),
                Handler::Set0f => (0x0f, 0x0c, false),
                Handler::Set1b => (0x1b, 0, false),
                _ => unreachable!(),
            };
            t[offset] = if center {
                value.wrapping_sub(0x40)
            } else {
                value
            };
            t[0] |= flags;
        }
    }
    *player = p;
    *track = t;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes<const N: usize>(v: &serde_json::Value) -> [u8; N] {
        let s = v.as_str().unwrap();
        assert_eq!(s.len(), N * 2);
        std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
    }
    struct Data {
        command_address: u32,
        command: [u8; 8],
        voice: [u8; 3072],
        voice_address: u32,
    }
    impl SequenceData for Data {
        fn read(&self, a: u32, n: usize) -> Option<&[u8]> {
            if let Some(i) = a.checked_sub(self.command_address) {
                if let Some(s) = self.command.get(i as usize..(i as usize).checked_add(n)?) {
                    return Some(s);
                }
            }
            let i = a.checked_sub(self.voice_address)? as usize;
            self.voice.get(i..i.checked_add(n)?)
        }
    }
    #[test]
    fn command_leaves_match_native_player_and_track_snapshots() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/sequence-vectors.json")).unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let seed = case["voiceSeed"].as_u64().unwrap() as u8;
            let data = Data {
                command_address: case["cursor"].as_u64().unwrap() as u32,
                command: bytes(&case["command"]),
                voice: std::array::from_fn(|i| seed.wrapping_add((i as u8).wrapping_mul(37))),
                voice_address: case["voiceAddress"].as_u64().unwrap() as u32,
            };
            let mut player = bytes(&case["player"]);
            let mut track = bytes(&case["track"]);
            apply(
                Handler::from_entry(case["entry"].as_u64().unwrap() as u32).unwrap(),
                &data,
                &mut player,
                &mut track,
            )
            .unwrap();
            assert_eq!(
                player,
                bytes(&case["afterPlayer"]),
                "native player case {index}"
            );
            assert_eq!(
                track,
                bytes(&case["afterTrack"]),
                "native track case {index}"
            );
        }
    }
    #[test]
    fn missing_data_and_invalid_depth_fail_before_mutation() {
        let data = Data {
            command_address: 0x02008000,
            command: [0; 8],
            voice: [0; 3072],
            voice_address: 0x0200a000,
        };
        let mut p = [0; PLAYER_BYTES];
        let mut t = [0; TRACK_BYTES];
        let before = t;
        assert!(apply(Handler::Goto, &data, &mut p, &mut t).is_err());
        assert_eq!(t, before);
        t[2] = 4;
        let before = t;
        assert_eq!(
            apply(Handler::Return, &data, &mut p, &mut t),
            Err(SequenceError::PatternDepth(4))
        );
        assert_eq!(t, before);
        assert_eq!(
            apply(Handler::Pattern, &data, &mut p, &mut t),
            Err(SequenceError::UnportedRoutine(0x5e168))
        );
        assert!(Handler::from_entry(0).is_err());
    }

    #[test]
    fn voice_rejects_wrapping_instrument_extent_before_read() {
        struct WrappedData;
        impl SequenceData for WrappedData {
            fn read(&self, address: u32, length: usize) -> Option<&[u8]> {
                match (address, length) {
                    (0x02008000, 1) => Some(&[0]),
                    (0xfffffff8, 12) => panic!("wrapping instrument read"),
                    _ => None,
                }
            }
        }
        let mut player = [0; PLAYER_BYTES];
        let mut track = [0; TRACK_BYTES];
        put(&mut player, 0x30, 0xfffffff8);
        put(&mut track, 0x40, 0x02008000);
        let before_player = player;
        let before_track = track;
        assert_eq!(
            apply(Handler::Voice, &WrappedData, &mut player, &mut track),
            Err(SequenceError::AddressOverflow(0xfffffff8))
        );
        assert_eq!(player, before_player);
        assert_eq!(track, before_track);
    }
}
