//! A3VE embedded-Garden song routing and player lifecycle.
//! Offsets address the independently authenticated Garden payload. Sequencing,
//! mixing and sound hardware are separate; accepting a hardware call is not
//! evidence of rendered audio or a canonical-service integration.
use super::frame::{FrameError, FrameMemory};

pub const SIGNATURE: u32 = 0x6873_6d53;
pub const SOUND_INFO_SLOT: u32 = 0x0300_7ff0;
pub const CLEAR_SLOT: u32 = 0x0300_71dc;
pub const CLEAR_TARGET: u32 = 0x0200_fd61;
pub const PLAYER_TABLE: u32 = 0x0201_9be0;
pub const SONG_TABLE: u32 = 0x0201_9c10;
pub const PLAYER_COUNT: usize = 4;
pub const SONG_COUNT: usize = 37;
pub const MAX_TRACKS: u8 = 16;
/// Native lists have no cycle guard. This adapter rejects cycles and caps work.
pub const MAX_CHANNELS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Player {
    pub context: u32,
    pub tracks: u32,
    pub capacity: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Song {
    pub header: u32,
    pub player: u16,
    /// Preserved record+6; these routing entry points do not consume it.
    pub native_tag: u16,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bank {
    players: [Player; PLAYER_COUNT],
    songs: [Song; SONG_COUNT],
}
impl Bank {
    /// Authenticates the whole A3VE Garden before following its own tables.
    /// Header/track/voice roots are bounded; this does not decode track code,
    /// instrument indirection or sample closure. The 37-record bound combines
    /// the ID0x24 caller at04C00 and the next zero-track header at019D38.
    pub fn load(payload: &[u8]) -> Result<Self, Error> {
        crate::garden::verify_payload_layout(payload).map_err(|_| Error::BankIdentity)?;
        let read32 = |at: usize| -> Result<u32, Error> {
            let bytes = payload.get(at..at + 4).ok_or(Error::BankLayout(at))?;
            Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
        };
        let read16 = |at: usize| -> Result<u16, Error> {
            let bytes = payload.get(at..at + 2).ok_or(Error::BankLayout(at))?;
            Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
        };
        let pointer = |address: u32, length: usize| -> Result<usize, Error> {
            let at = address
                .checked_sub(0x0200_0000)
                .ok_or(Error::Pointer(address))? as usize;
            payload
                .get(at..at.checked_add(length).ok_or(Error::Pointer(address))?)
                .ok_or(Error::Pointer(address))?;
            Ok(at)
        };
        let mut players = [Player {
            context: 0,
            tracks: 0,
            capacity: 0,
        }; PLAYER_COUNT];
        for (index, player) in players.iter_mut().enumerate() {
            let at = 0x19be0 + index * 12;
            let context = read32(at)?;
            let tracks = read32(at + 4)?;
            let capacity = payload[at + 8];
            if !(0x0300_0000..=0x0300_7fc0).contains(&context)
                || tracks < 0x0300_0000
                || tracks
                    .checked_add(u32::from(capacity) * 0x50)
                    .is_none_or(|end| end > 0x0300_8000)
                || capacity == 0
                || capacity > MAX_TRACKS
                || payload[at + 9..at + 12] != [0, 0, 0]
            {
                return Err(Error::BankLayout(at));
            }
            *player = Player {
                context,
                tracks,
                capacity,
            };
        }
        let mut songs = [Song {
            header: 0,
            player: 0,
            native_tag: 0,
        }; SONG_COUNT];
        for (index, song) in songs.iter_mut().enumerate() {
            let at = 0x19c10 + index * 8;
            let header = read32(at)?;
            let player = read16(at + 4)?;
            if usize::from(player) >= PLAYER_COUNT {
                return Err(Error::BankLayout(at));
            }
            let h = pointer(header, 8)?;
            let count = payload[h];
            if count > MAX_TRACKS {
                return Err(Error::BankLayout(h));
            }
            pointer(header, 8 + usize::from(count) * 4)?;
            if count != 0 {
                pointer(read32(h + 4)?, 12)?;
                for track in 0..usize::from(count) {
                    pointer(read32(h + 8 + track * 4)?, 1)?;
                }
            }
            *song = Song {
                header,
                player,
                native_tag: read16(at + 6)?,
            };
        }
        Ok(Self { players, songs })
    }
    pub fn players(&self) -> &[Player; PLAYER_COUNT] {
        &self.players
    }
    pub fn songs(&self) -> &[Song; SONG_COUNT] {
        &self.songs
    }
    fn song(&self, id: u32) -> Result<(Player, Song), Error> {
        let id = id as u16;
        let song = *self.songs.get(usize::from(id)).ok_or(Error::SongId(id))?;
        Ok((self.players[usize::from(song.player)], song))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    PsgStop { target: u32, channel_type: u8 },
    SoundMode { value: u8 },
    ClearPlayer { target: u32, player: u32 },
}
impl Call {
    pub fn target(&self) -> u32 {
        match *self {
            Self::PsgStop { target, .. } | Self::ClearPlayer { target, .. } => target,
            Self::SoundMode { .. } => 0x0201_0b85,
        }
    }
    pub fn arguments(&self) -> Vec<u32> {
        match *self {
            Self::PsgStop { channel_type, .. } => vec![u32::from(channel_type)],
            Self::SoundMode { value } => vec![u32::from(value)],
            Self::ClearPlayer { player, .. } => vec![player],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Memory(FrameError),
    BankIdentity,
    BankLayout(usize),
    Pointer(u32),
    SongId(u16),
    TrackCount(u8),
    ChannelCycle(u32),
    ChannelLimit,
    Entry(u32),
    Unported(Call),
    Backend { call: Call, reason: String },
}
impl From<FrameError> for Error {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub trait Calls {
    fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<(), Error>;
}
pub struct RejectCalls;
impl Calls for RejectCalls {
    fn call(&mut self, _: &mut dyn FrameMemory, call: Call) -> Result<(), Error> {
        Err(Error::Unported(call))
    }
}
fn word(memory: &mut dyn FrameMemory, address: u32, value: u32) -> Result<(), Error> {
    memory.write_u16(address, value as u16)?;
    memory.write_u16(address + 2, (value >> 16) as u16)?;
    Ok(())
}
fn count(memory: &dyn FrameMemory, player: u32) -> Result<u8, Error> {
    let value = memory.read_u8(player + 8)?;
    if value > MAX_TRACKS {
        return Err(Error::TrackCount(value));
    }
    Ok(value)
}
fn span(address: u32, length: u32) -> Result<(), Error> {
    address.checked_add(length).ok_or(Error::Pointer(address))?;
    Ok(())
}
/// Payload10240: unlink channels in source order, reloading next after PSG.
pub fn track_stop(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    track: u32,
) -> Result<(), Error> {
    span(track, 0x50)?;
    if memory.read_u8(track)? & 0x80 == 0 {
        return Ok(());
    }
    let mut channel = memory.read_u32(track + 0x20)?;
    let mut seen = std::collections::BTreeSet::new();
    while channel != 0 {
        span(channel, 0x40)?;
        if !seen.insert(channel) {
            return Err(Error::ChannelCycle(channel));
        }
        if seen.len() > MAX_CHANNELS {
            return Err(Error::ChannelLimit);
        }
        if memory.read_u8(channel)? != 0 {
            let channel_type = memory.read_u8(channel + 1)? & 7;
            if channel_type != 0 {
                let info = memory.read_u32(SOUND_INFO_SLOT)?;
                span(info, 0x30)?;
                let target = memory.read_u32(info + 0x2c)?;
                calls.call(
                    memory,
                    Call::PsgStop {
                        target,
                        channel_type,
                    },
                )?;
            }
            memory.write_u8(channel, 0)?;
        }
        word(memory, channel + 0x2c, 0)?;
        channel = memory.read_u32(channel + 0x34)?;
    }
    word(memory, track + 0x20, 0)
}
/// Payload10D28; only its independently traced FD60 clear target is internal.
pub fn open(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    player: u32,
    tracks: u32,
    capacity: u32,
) -> Result<(), Error> {
    let capacity = (capacity as u8).min(MAX_TRACKS);
    if capacity == 0 {
        return Ok(());
    }
    span(player, 0x40)?;
    span(tracks, u32::from(capacity) * 0x50)?;
    let info = memory.read_u32(SOUND_INFO_SLOT)?;
    span(info, 0x28)?;
    if memory.read_u32(info)? != SIGNATURE {
        return Ok(());
    }
    word(memory, info, SIGNATURE + 1)?;
    let target = memory.read_u32(CLEAR_SLOT)?;
    if target == CLEAR_TARGET {
        for offset in (0..0x40).step_by(4) {
            word(memory, player + offset, 0)?;
        }
    } else {
        calls.call(memory, Call::ClearPlayer { target, player })?;
    }
    word(memory, player + 0x2c, tracks)?;
    memory.write_u8(player + 8, capacity)?;
    word(memory, player + 4, 0x8000_0000)?;
    for index in 0..u32::from(capacity) {
        memory.write_u8(tracks + index * 0x50, 0)?;
    }
    let previous = memory.read_u32(info + 0x20)?;
    if previous != 0 {
        word(memory, player + 0x38, previous)?;
        let previous_context = memory.read_u32(info + 0x24)?;
        word(memory, player + 0x3c, previous_context)?;
        word(memory, info + 0x20, 0)?;
    }
    word(memory, info + 0x24, player)?;
    word(memory, info + 0x20, 0x0200_ffd9)?;
    word(memory, info, SIGNATURE)?;
    word(memory, player + 0x34, SIGNATURE)
}
/// Payload10DA0. Header and capacity reloads after calls are intentional.
pub fn start(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    player: u32,
    header: u32,
) -> Result<(), Error> {
    span(player, 0x40)?;
    if memory.read_u32(player + 0x34)? != SIGNATURE {
        return Ok(());
    }
    span(header, 0x48)?;
    let priority = memory.read_u8(header + 2)?;
    if memory.read_u8(player + 0xb)? != 0 {
        let active_track = memory.read_u32(player)? != 0 && {
            let tracks = memory.read_u32(player + 0x2c)?;
            memory.read_u8(tracks)? & 0x40 != 0
        };
        let status = memory.read_u32(player + 4)?;
        if (active_track || (status as u16 != 0 && (status as i32) >= 0))
            && memory.read_u8(player + 9)? > priority
        {
            return Ok(());
        }
    }
    count(memory, player)?;
    word(memory, player + 0x34, SIGNATURE + 1)?;
    word(memory, player + 4, 0)?;
    word(memory, player, header)?;
    let voices = memory.read_u32(header + 4)?;
    word(memory, player + 0x30, voices)?;
    memory.write_u8(player + 9, priority)?;
    word(memory, player + 0xc, 0)?;
    for (offset, value) in [(0x1c, 150), (0x20, 150), (0x1e, 256), (0x22, 0), (0x24, 0)] {
        memory.write_u16(player + offset, value)?;
    }
    let mut index = 0u8;
    let mut track = memory.read_u32(player + 0x2c)?;
    while index < memory.read_u8(header)? && index < count(memory, player)? {
        track_stop(memory, calls, track)?;
        memory.write_u8(track, 0xc0)?;
        word(memory, track + 0x20, 0)?;
        let script = memory.read_u32(header + 8 + u32::from(index) * 4)?;
        word(memory, track + 0x40, script)?;
        index += 1;
        track += 0x50;
    }
    while index < count(memory, player)? {
        track_stop(memory, calls, track)?;
        memory.write_u8(track, 0)?;
        index += 1;
        track += 0x50;
    }
    let reverb = memory.read_u8(header + 3)?;
    if reverb & 0x80 != 0 {
        calls.call(memory, Call::SoundMode { value: reverb })?;
    }
    word(memory, player + 0x34, SIGNATURE)
}
/// Payload10E84: unlike start, count and base are captured before callbacks.
pub fn stop(memory: &mut dyn FrameMemory, calls: &mut dyn Calls, player: u32) -> Result<(), Error> {
    span(player, 0x40)?;
    if memory.read_u32(player + 0x34)? != SIGNATURE {
        return Ok(());
    }
    let capacity = count(memory, player)?;
    word(memory, player + 0x34, SIGNATURE + 1)?;
    let status = memory.read_u32(player + 4)? | 0x8000_0000;
    word(memory, player + 4, status)?;
    let tracks = memory.read_u32(player + 0x2c)?;
    for index in 0..u32::from(capacity) {
        track_stop(memory, calls, tracks + index * 0x50)?;
    }
    word(memory, player + 0x34, SIGNATURE)
}
pub fn resume(memory: &mut dyn FrameMemory, player: u32) -> Result<(), Error> {
    span(player, 0x40)?;
    if memory.read_u32(player + 0x34)? == SIGNATURE {
        let status = memory.read_u32(player + 4)? & 0x7fff_ffff;
        word(memory, player + 4, status)?;
    }
    Ok(())
}
pub fn fade(memory: &mut dyn FrameMemory, player: u32, value: u32, kind: u16) -> Result<(), Error> {
    span(player, 0x40)?;
    if memory.read_u32(player + 0x34)? == SIGNATURE {
        memory.write_u16(player + 0x26, value as u16)?;
        memory.write_u16(player + 0x24, value as u16)?;
        memory.write_u16(player + 0x28, kind)?;
        if kind == 2 {
            resume(memory, player)?;
        }
    }
    Ok(())
}
/// Routes source entry points. Bank is immutable authenticated source metadata;
/// mutable player/header/channel data is always read from supplied live memory.
pub fn execute(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn Calls,
    bank: &Bank,
    entry: u32,
    args: [u32; 3],
) -> Result<(), Error> {
    match entry {
        0x1066c | 0x10698 | 0x106e4 | 0x10738 | 0x1076c => {
            let (player, song) = bank.song(args[0])?;
            let same = memory.read_u32(player.context)? == song.header;
            match entry {
                0x1066c => start(memory, calls, player.context, song.header),
                0x10698 | 0x106e4 => {
                    if !same {
                        return start(memory, calls, player.context, song.header);
                    }
                    let status = memory.read_u32(player.context + 4)?;
                    if status as u16 == 0 || (entry == 0x10698 && (status as i32) < 0) {
                        start(memory, calls, player.context, song.header)
                    } else if (status as i32) < 0 {
                        resume(memory, player.context)
                    } else {
                        Ok(())
                    }
                }
                0x10738 if same => stop(memory, calls, player.context),
                0x1076c if same => resume(memory, player.context),
                _ => Ok(()),
            }
        }
        0x107a0 | 0x107d8 => {
            for player in bank.players {
                if entry == 0x107a0 {
                    stop(memory, calls, player.context)?;
                } else {
                    resume(memory, player.context)?;
                }
            }
            Ok(())
        }
        0x105ac | 0x107cc => resume(memory, args[0]),
        0x105c8 | 0x10804 => fade(memory, args[0], args[1], 0x100),
        0x10814 => fade(memory, args[0], args[1], 0x101),
        0x10834 => fade(memory, args[0], args[1], 2),
        0x10d28 => open(memory, calls, args[0], args[1], args[2]),
        0x10da0 => start(memory, calls, args[0], args[1]),
        0x10e84 => stop(memory, calls, args[0]),
        0x10240 => track_stop(memory, calls, args[1]),
        _ => Err(Error::Entry(entry)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    const INFO: u32 = 0x0300_6100;
    const HEADER: u32 = 0x0203_0000;
    const CHANNEL: u32 = 0x0300_4000;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/garden-audio-vectors.json")).unwrap()
    }
    fn n(value: &Value, key: &str, default: u32) -> u32 {
        value
            .get(key)
            .and_then(Value::as_u64)
            .map_or(default, |v| v as u32)
    }
    fn bank(value: &Value) -> Bank {
        Bank {
            players: std::array::from_fn(|i| {
                let row = &value["players"][i];
                Player {
                    context: row[0].as_u64().unwrap() as u32,
                    tracks: row[1].as_u64().unwrap() as u32,
                    capacity: row[2].as_u64().unwrap() as u8,
                }
            }),
            songs: std::array::from_fn(|i| {
                let row = &value["songs"][i];
                Song {
                    header: row[0].as_u64().unwrap() as u32,
                    player: row[1].as_u64().unwrap() as u16,
                    native_tag: row[2].as_u64().unwrap() as u16,
                }
            }),
        }
    }
    struct Memory {
        ram: Vec<u8>,
        ewram: Vec<u8>,
    }
    impl Memory {
        fn get(&self, address: u32, width: usize) -> Result<&[u8], FrameError> {
            let (bytes, base) = if address >= 0x0300_0000 {
                (&self.ram, 0x0300_0000)
            } else {
                (&self.ewram, 0x0200_0000)
            };
            address
                .checked_sub(base)
                .and_then(|at| bytes.get(at as usize..(at as usize).checked_add(width)?))
                .ok_or(FrameError::MissingRead {
                    address,
                    width: width as u8,
                })
        }
        fn put(&mut self, address: u32, bytes: &[u8]) -> Result<(), FrameError> {
            let (memory, base) = if address >= 0x0300_0000 {
                (&mut self.ram, 0x0300_0000)
            } else {
                (&mut self.ewram, 0x0200_0000)
            };
            let at = address.checked_sub(base).ok_or(FrameError::MissingWrite {
                address,
                width: bytes.len() as u8,
            })? as usize;
            let target = memory
                .get_mut(at..at + bytes.len())
                .ok_or(FrameError::MissingWrite {
                    address,
                    width: bytes.len() as u8,
                })?;
            target.copy_from_slice(bytes);
            Ok(())
        }
        fn w(&mut self, address: u32, value: u32, width: usize) {
            self.put(address, &value.to_le_bytes()[..width]).unwrap();
        }
        fn new(source: &Value, case: &Value, bank: &Bank) -> Self {
            let seed = n(case, "seed", 83);
            let mut memory = Self {
                ram: (0..0x8000)
                    .map(|i| seed.wrapping_add(i * 37) as u8)
                    .collect(),
                ewram: vec![0; 0x40000],
            };
            for h in source["headers"].as_array().unwrap() {
                let address = n(h, "address", 0);
                memory.w(address, n(h, "count", 0), 1);
                memory.w(address + 2, n(h, "priority", 0), 1);
                memory.w(address + 3, n(h, "reverb", 0), 1);
                memory.w(address + 4, n(h, "voices", 0), 4);
                for (i, script) in h["tracks"].as_array().unwrap().iter().enumerate() {
                    memory.w(
                        address + 8 + i as u32 * 4,
                        script.as_u64().unwrap() as u32,
                        4,
                    );
                }
            }
            for player in bank.players {
                let header = if case["same"].as_bool().unwrap() {
                    bank.songs[n(case, "song", 11) as usize].header
                } else if n(case, "priorityGate", 0) != 0 {
                    HEADER
                } else {
                    0
                };
                memory.w(player.context, header, 4);
                memory.w(player.context + 4, n(case, "status", 0), 4);
                memory.w(player.context + 8, n(case, "capacity", 3), 1);
                memory.w(player.context + 9, n(case, "playerPriority", 2), 1);
                memory.w(player.context + 0xb, n(case, "priorityGate", 0), 1);
                memory.w(player.context + 0x2c, player.tracks, 4);
                memory.w(player.context + 0x34, n(case, "signature", SIGNATURE), 4);
                for index in 0..16 {
                    memory.w(player.tracks + index * 0x50, 0, 1);
                    memory.w(player.tracks + index * 0x50 + 0x20, 0, 4);
                }
            }
            memory.w(SOUND_INFO_SLOT, INFO, 4);
            memory.w(CLEAR_SLOT, n(case, "clearTarget", CLEAR_TARGET), 4);
            memory.w(INFO, n(case, "infoSignature", SIGNATURE), 4);
            memory.w(INFO + 0x20, n(case, "previous", 0x0200_ffd9), 4);
            memory.w(INFO + 0x24, 0x0300_7000, 4);
            memory.w(INFO + 0x2c, 0x0201_1ad9, 4);
            memory.w(HEADER, n(case, "headerCount", 3), 1);
            memory.w(HEADER + 2, n(case, "headerPriority", 4), 1);
            memory.w(HEADER + 3, n(case, "headerReverb", 0), 1);
            memory.w(HEADER + 4, 0x0201_949c, 4);
            for index in 0..16 {
                memory.w(HEADER + 8 + index * 4, 0x0201_d66c + index * 4, 4);
            }
            let tracks = bank.players[0].tracks;
            memory.w(tracks, n(case, "firstFlag", 0), 1);
            let channels = case["channels"].as_array().unwrap();
            if !channels.is_empty() {
                memory.w(tracks + 0x20, CHANNEL, 4);
            }
            for (index, channel) in channels.iter().enumerate() {
                let at = CHANNEL + index as u32 * 0x40;
                memory.w(at, n(channel, "active", 0), 1);
                memory.w(at + 1, n(channel, "kind", 0), 1);
                memory.w(at + 0x2c, tracks, 4);
                memory.w(
                    at + 0x34,
                    if index + 1 < channels.len() {
                        at + 0x40
                    } else {
                        0
                    },
                    4,
                );
            }
            memory
        }
    }
    impl FrameMemory for Memory {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            Ok(self.get(address, 1)?[0])
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes(
                self.get(address, 2)?.try_into().unwrap(),
            ))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes(
                self.get(address, 4)?.try_into().unwrap(),
            ))
        }
        fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            self.put(address, &[value])
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.put(address, &value.to_le_bytes())
        }
    }
    struct Backend<'a> {
        case: &'a Value,
        calls: Vec<Value>,
    }
    impl Calls for Backend<'_> {
        fn call(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<(), Error> {
            self.calls
                .push(json!({"target": call.target(), "args": call.arguments()}));
            let ordinal = self.calls.len() as u32;
            if let Some(mutations) = self.case["mutations"].as_array() {
                for mutation in mutations {
                    if n(mutation, "call", 0) == ordinal {
                        let address = n(mutation, "address", 0);
                        let value = n(mutation, "value", 0);
                        match n(mutation, "width", 4) {
                            1 => memory.write_u8(address, value as u8)?,
                            2 => memory.write_u16(address, value as u16)?,
                            4 => word(memory, address, value)?,
                            _ => panic!("fixture width"),
                        }
                    }
                }
            }
            if ordinal == n(self.case, "abortAtCall", 0) {
                return Err(Error::Backend {
                    call,
                    reason: "synthetic interruption".into(),
                });
            }
            Ok(())
        }
    }
    #[test]
    fn native_garden_routing_and_lifecycle_vectors() {
        let source = fixture();
        let bank = bank(&source);
        for case in source["cases"].as_array().unwrap() {
            let mut memory = Memory::new(&source, case, &bank);
            let mut calls = Backend {
                case,
                calls: vec![],
            };
            let args = std::array::from_fn(|i| case["args"][i].as_u64().unwrap() as u32);
            let result = execute(&mut memory, &mut calls, &bank, n(case, "entry", 0), args);
            assert_eq!(
                result.is_err(),
                case["aborted"].as_bool().unwrap(),
                "{}: {result:?}",
                case["name"]
            );
            assert_eq!(json!(calls.calls), case["calls"], "{}", case["name"]);
            assert_eq!(
                crate::sha256(&memory.ram[..0x7400]),
                case["ramSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                crate::sha256(memory.get(HEADER, 0x1000).unwrap()),
                case["scratchSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn strict_calls_and_bounded_malformed_state() {
        let source = fixture();
        let bank = bank(&source);
        let case = source["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "channels-10e84")
            .unwrap();
        let mut memory = Memory::new(&source, case, &bank);
        let player = bank.players[0];
        let error = stop(&mut memory, &mut RejectCalls, player.context).unwrap_err();
        assert!(matches!(
            error,
            Error::Unported(Call::PsgStop {
                channel_type: 2,
                ..
            })
        ));
        assert_eq!(
            memory.read_u32(player.context + 0x34).unwrap(),
            SIGNATURE + 1
        );
        assert_eq!(memory.read_u8(CHANNEL).unwrap(), 0);
        assert_eq!(memory.read_u8(CHANNEL + 0x80).unwrap(), 1);
        let mut memory = Memory::new(&source, case, &bank);
        memory.w(CHANNEL, 0, 1);
        memory.w(CHANNEL + 0x34, CHANNEL, 4);
        assert_eq!(
            track_stop(&mut memory, &mut RejectCalls, player.tracks),
            Err(Error::ChannelCycle(CHANNEL))
        );
        for id in [37, 255, 65535] {
            assert_eq!(
                execute(&mut memory, &mut RejectCalls, &bank, 0x1066c, [id, 0, 0]),
                Err(Error::SongId(id as u16))
            );
        }
        memory.w(player.context + 0x34, SIGNATURE, 4);
        memory.w(player.context + 8, 17, 1);
        assert_eq!(
            stop(&mut memory, &mut RejectCalls, player.context),
            Err(Error::TrackCount(17))
        );
        assert_eq!(
            stop(&mut memory, &mut RejectCalls, u32::MAX),
            Err(Error::Pointer(u32::MAX))
        );
        assert_eq!(Bank::load(&[]), Err(Error::BankIdentity));
        assert_eq!(
            Bank::load(&vec![0; crate::garden::PAYLOAD_LENGTH]),
            Err(Error::BankIdentity)
        );
    }
    #[test]
    fn optional_local_authenticated_bank() {
        if let Ok(path) = std::env::var("SPP_GARDEN") {
            let payload = std::fs::read(path).unwrap();
            let loaded = Bank::load(&payload).unwrap();
            assert_eq!(loaded, bank(&fixture()));
            let mut corrupt = payload.clone();
            corrupt[0x19c10] ^= 1;
            assert_eq!(Bank::load(&corrupt), Err(Error::BankIdentity));
            assert_eq!(
                Bank::load(&payload[..payload.len() - 1]),
                Err(Error::BankIdentity)
            );
        }
    }
}
