//! A3VE MPlayStart, MPlayStop and trackStop translated to Rust.
//!
//! This ports player/track/channel state transitions. PSG oscillator stops are
//! emitted in source order for a hardware backend; sequence execution, voices,
//! mixing and audible playback are separate, unfinished systems.
use std::fmt;

pub const SIGNATURE: u32 = 0x6873_6d53;
pub const PLAYER_BYTES: usize = 0x40;
pub const TRACK_BYTES: usize = 0x50;
pub const CHANNEL_BYTES: usize = 0x40;
pub const SOUND_INFO_PREFIX: usize = 0x30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MPlayError {
    TrackStorage { required: usize, available: usize },
    HeaderTracks { required: usize, available: usize },
    TooManyChannels(usize),
    DuplicateChannel(u32),
    MissingChannel(u32),
    ChannelCycle(u32),
    TrackIndex(usize),
}
impl fmt::Display for MPlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MPlayError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub address: u32,
    pub bytes: [u8; CHANNEL_BYTES],
}

/// Header fields consumed by ROM 0x5F170; track streams remain native IDs.
#[derive(Debug, Clone)]
pub struct SongHeader {
    pub address: u32,
    pub track_count: u8,
    pub priority: u8,
    pub reverb: u8,
    pub voice_group: u32,
    pub tracks: Vec<u32>,
}
impl SongHeader {
    /// Load only the explicitly bounded fields consumed by MPlayStart. The
    /// native 135-entry request map is separate from the 200-entry catalog.
    pub fn load(rom: &[u8], address: u32) -> Result<Self, String> {
        crate::catalog::validate_rom(rom).map_err(|e| e.to_string())?;
        Self::parse_verified(rom, address)
    }
    fn parse_verified(rom: &[u8], address: u32) -> Result<Self, String> {
        let offset = crate::binary::pointer_offset(address, 0x08000000, rom.len())
            .map_err(|e| e.to_string())?;
        let prefix = crate::binary::span(rom, offset, 8).map_err(|e| e.to_string())?;
        let count = usize::from(prefix[0]);
        let header = crate::binary::span(rom, offset, 8 + count * 4).map_err(|e| e.to_string())?;
        let voice_group = word(header, 4);
        if voice_group != 0 {
            let bank = crate::binary::pointer_offset(voice_group, 0x08000000, rom.len())
                .map_err(|e| e.to_string())?;
            crate::binary::span(rom, bank, 12).map_err(|e| e.to_string())?;
        }
        let mut tracks = Vec::with_capacity(count);
        for index in 0..count {
            let pointer = word(header, 8 + index * 4);
            let target = crate::binary::pointer_offset(pointer, 0x08000000, rom.len())
                .map_err(|e| e.to_string())?;
            crate::binary::span(rom, target, 1).map_err(|e| e.to_string())?;
            tracks.push(pointer);
        }
        Ok(Self {
            address,
            track_count: prefix[0],
            priority: prefix[2],
            reverb: prefix[3],
            voice_group,
            tracks,
        })
    }
}

/// Preload once after source authentication; requests need no repeated hashing.
/// This retains the request map without assuming the 200-entry catalog's IDs.
pub struct MPlayBank {
    pub routing: super::sound::SoundBank,
    pub songs: Vec<Option<SongHeader>>,
}
impl MPlayBank {
    pub fn load(rom: &[u8]) -> Result<Self, String> {
        let routing = super::sound::SoundBank::load(rom)?;
        let songs = routing
            .headers
            .iter()
            .map(|&p| {
                if p == 0 {
                    Ok(None)
                } else {
                    SongHeader::parse_verified(rom, p).map(Some)
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { routing, songs })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MPlayEffects {
    /// Native calls through soundInfo+0x2C, argument0 = channel.type & 7.
    /// Returning these does not mean a PSG register or oscillator was changed.
    pub psg_stops: Vec<u8>,
    pub accepted: bool,
}

/// Snapshots preserve every unmodified field. Counts and linked lists are
/// bounded before writes; invalid/cyclic RAM is rejected atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MPlayState {
    pub player: [u8; PLAYER_BYTES],
    pub tracks: Vec<[u8; TRACK_BYTES]>,
    pub channels: Vec<Channel>,
    pub sound_info: [u8; SOUND_INFO_PREFIX],
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed layout"))
}
fn put_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_half(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

impl MPlayState {
    /// ROM 0x5F0F8, including native Clear64 at 0x5E130. Count truncates to u8,
    /// zero does nothing, and nonzero counts clamp to 16. Existing sound-info
    /// callback links become player+0x38/+0x3C; pointers remain opaque Rust IDs.
    pub fn open(
        &mut self,
        player_address: u32,
        track_address: u32,
        count: u32,
    ) -> Result<MPlayEffects, MPlayError> {
        let count = usize::from((count as u8).min(16));
        if count == 0 || word(&self.sound_info, 0) != SIGNATURE {
            return Ok(MPlayEffects::default());
        }
        if self.tracks.len() < count {
            return Err(MPlayError::TrackStorage {
                required: count,
                available: self.tracks.len(),
            });
        }
        put_word(&mut self.sound_info, 0, SIGNATURE + 1);
        self.player.fill(0);
        put_word(&mut self.player, 0x2c, track_address);
        self.player[8] = count as u8;
        put_word(&mut self.player, 4, 0x80000000);
        for track in &mut self.tracks[..count] {
            track[0] = 0;
        }
        let prior_callback = word(&self.sound_info, 0x20);
        if prior_callback != 0 {
            put_word(&mut self.player, 0x38, prior_callback);
            put_word(&mut self.player, 0x3c, word(&self.sound_info, 0x24));
            put_word(&mut self.sound_info, 0x20, 0);
        }
        put_word(&mut self.sound_info, 0x24, player_address);
        put_word(&mut self.sound_info, 0x20, 0x0805e3a9);
        put_word(&mut self.sound_info, 0, SIGNATURE);
        put_word(&mut self.player, 0x34, SIGNATURE);
        Ok(MPlayEffects {
            accepted: true,
            ..Default::default()
        })
    }
    fn capacity(&self) -> Result<usize, MPlayError> {
        let count = usize::from(self.player[8]);
        if self.tracks.len() < count {
            return Err(MPlayError::TrackStorage {
                required: count,
                available: self.tracks.len(),
            });
        }
        Ok(count)
    }
    fn validate_channels(&self, tracks: usize) -> Result<(), MPlayError> {
        // A work budget, not a claim about native channel allocation geometry.
        if self.channels.len() > 256 {
            return Err(MPlayError::TooManyChannels(self.channels.len()));
        }
        for (index, channel) in self.channels.iter().enumerate() {
            if self.channels[..index]
                .iter()
                .any(|c| c.address == channel.address)
            {
                return Err(MPlayError::DuplicateChannel(channel.address));
            }
        }
        for track in &self.tracks[..tracks] {
            self.validate_chain(track)?;
        }
        Ok(())
    }
    fn validate_chain(&self, track: &[u8; TRACK_BYTES]) -> Result<(), MPlayError> {
        if track[0] & 0x80 == 0 {
            return Ok(());
        }
        let mut address = word(track, 0x20);
        let mut seen = Vec::new();
        while address != 0 {
            if seen.contains(&address) {
                return Err(MPlayError::ChannelCycle(address));
            }
            seen.push(address);
            let channel = self
                .channels
                .iter()
                .find(|c| c.address == address)
                .ok_or(MPlayError::MissingChannel(address))?;
            address = word(&channel.bytes, 0x34);
        }
        Ok(())
    }
    fn stop_track_validated(&mut self, index: usize, effects: &mut MPlayEffects) {
        if self.tracks[index][0] & 0x80 == 0 {
            return;
        }
        let mut address = word(&self.tracks[index], 0x20);
        while address != 0 {
            let channel = self
                .channels
                .iter_mut()
                .find(|c| c.address == address)
                .expect("validated channel chain");
            if channel.bytes[0] != 0 {
                let kind = channel.bytes[1] & 7;
                if kind != 0 {
                    effects.psg_stops.push(kind);
                }
                channel.bytes[0] = 0;
            }
            put_word(&mut channel.bytes, 0x2c, 0);
            address = word(&channel.bytes, 0x34);
        }
        put_word(&mut self.tracks[index], 0x20, 0);
    }
    /// ROM 0x5E610. Inactive tracks retain their channel-link field verbatim.
    pub fn stop_track(&mut self, index: usize) -> Result<MPlayEffects, MPlayError> {
        let track = self
            .tracks
            .get(index)
            .ok_or(MPlayError::TrackIndex(index))?;
        self.validate_channels(0)?;
        self.validate_chain(track)?;
        let mut effects = MPlayEffects {
            accepted: true,
            ..Default::default()
        };
        self.stop_track_validated(index, &mut effects);
        Ok(effects)
    }
    /// ROM 0x5F170. Source priority guards, temporary signature lock, tempo
    /// resets, retained unknown fields and inactive-track quirks are preserved.
    pub fn start(&mut self, header: &SongHeader) -> Result<MPlayEffects, MPlayError> {
        let mut effects = MPlayEffects::default();
        if word(&self.player, 0x34) != SIGNATURE {
            return Ok(effects);
        }
        if self.player[0xb] != 0 {
            let first_started = if word(&self.player, 0) != 0 {
                self.tracks.first().ok_or(MPlayError::TrackStorage {
                    required: 1,
                    available: 0,
                })?[0]
                    & 0x40
                    != 0
            } else {
                false
            };
            let status = word(&self.player, 4);
            if (first_started || (status as u16 != 0 && (status as i32) >= 0))
                && self.player[9] > header.priority
            {
                return Ok(effects);
            }
        }
        let capacity = self.capacity()?;
        let active = usize::from(header.track_count).min(capacity);
        if header.tracks.len() < active {
            return Err(MPlayError::HeaderTracks {
                required: active,
                available: header.tracks.len(),
            });
        }
        self.validate_channels(capacity)?;
        put_word(&mut self.player, 0x34, SIGNATURE + 1);
        put_word(&mut self.player, 4, 0);
        put_word(&mut self.player, 0, header.address);
        put_word(&mut self.player, 0x30, header.voice_group);
        self.player[9] = header.priority;
        put_word(&mut self.player, 0xc, 0);
        put_half(&mut self.player, 0x1c, 150);
        put_half(&mut self.player, 0x20, 150);
        put_half(&mut self.player, 0x1e, 256);
        put_half(&mut self.player, 0x22, 0);
        put_half(&mut self.player, 0x24, 0);
        for index in 0..capacity {
            self.stop_track_validated(index, &mut effects);
            if index < active {
                self.tracks[index][0] = 0xc0;
                put_word(&mut self.tracks[index], 0x20, 0);
                put_word(&mut self.tracks[index], 0x40, header.tracks[index]);
            } else {
                self.tracks[index][0] = 0;
            }
        }
        // MPlayStart passes a u8 into SoundMode (0x5EF54), so only the low
        // reverb branch can execute; frequency/PSG mode branches are not ported.
        if header.reverb & 0x80 != 0 && word(&self.sound_info, 0) == SIGNATURE {
            self.sound_info[5] = header.reverb & 0x7f;
        }
        put_word(&mut self.player, 0x34, SIGNATURE);
        effects.accepted = true;
        Ok(effects)
    }
    /// ROM 0x5F254. Track flags survive; native trackStop clears channel links.
    pub fn stop(&mut self) -> Result<MPlayEffects, MPlayError> {
        let mut effects = MPlayEffects::default();
        if word(&self.player, 0x34) != SIGNATURE {
            return Ok(effects);
        }
        let count = self.capacity()?;
        self.validate_channels(count)?;
        put_word(&mut self.player, 0x34, SIGNATURE + 1);
        let status = word(&self.player, 4) | 0x8000_0000;
        put_word(&mut self.player, 4, status);
        for index in 0..count {
            self.stop_track_validated(index, &mut effects);
        }
        put_word(&mut self.player, 0x34, SIGNATURE);
        effects.accepted = true;
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes<const N: usize>(v: &serde_json::Value) -> [u8; N] {
        let text = v.as_str().unwrap();
        assert_eq!(text.len(), N * 2);
        std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
    }
    fn snapshot(v: &serde_json::Value) -> MPlayState {
        MPlayState {
            player: bytes(&v["player"]),
            tracks: v["tracks"].as_array().unwrap().iter().map(bytes).collect(),
            channels: v["channels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| Channel {
                    address: c["address"].as_u64().unwrap() as u32,
                    bytes: bytes(&c["bytes"]),
                })
                .collect(),
            sound_info: bytes(&v["soundInfo"]),
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_player_and_track_state_vectors() {
        let fixture: serde_json::Value =
            serde_json::from_str(vectors!("mplay-vectors.json")).unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let mut state = snapshot(&case["before"]);
            let effects = match case["routine"].as_str().unwrap() {
                "open" => state
                    .open(
                        case["playerAddress"].as_u64().unwrap() as u32,
                        case["trackAddress"].as_u64().unwrap() as u32,
                        case["count"].as_u64().unwrap() as u32,
                    )
                    .unwrap(),
                "start" => {
                    let h = &case["header"];
                    state
                        .start(&SongHeader {
                            address: h["address"].as_u64().unwrap() as u32,
                            track_count: h["count"].as_u64().unwrap() as u8,
                            priority: h["priority"].as_u64().unwrap() as u8,
                            reverb: h["reverb"].as_u64().unwrap() as u8,
                            voice_group: h["voiceGroup"].as_u64().unwrap() as u32,
                            tracks: h["tracks"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|p| p.as_u64().unwrap() as u32)
                                .collect(),
                        })
                        .unwrap()
                }
                "stop" => state.stop().unwrap(),
                "trackStop" => state
                    .stop_track(case["index"].as_u64().unwrap() as usize)
                    .unwrap(),
                _ => panic!("unknown fixture routine"),
            };
            let mut packed = state.player.to_vec();
            for track in &state.tracks {
                packed.extend_from_slice(track);
            }
            for channel in &state.channels {
                packed.extend_from_slice(&channel.address.to_le_bytes());
                packed.extend_from_slice(&channel.bytes);
            }
            packed.extend_from_slice(&state.sound_info);
            assert_eq!(
                crate::sha256(&packed),
                case["afterSha256"].as_str().unwrap(),
                "complete native RAM case {index}"
            );
            assert_eq!(
                serde_json::to_value(effects.psg_stops).unwrap(),
                case["psgStops"],
                "native PSG boundary case {index}"
            );
        }
    }
    #[test]
    fn corrupt_channel_graphs_and_counts_fail_before_mutation() {
        let mut state = MPlayState {
            player: [0; PLAYER_BYTES],
            tracks: vec![[0; TRACK_BYTES]],
            channels: vec![Channel {
                address: 0x2001000,
                bytes: [0; CHANNEL_BYTES],
            }],
            sound_info: [0; SOUND_INFO_PREFIX],
        };
        put_word(&mut state.player, 0x34, SIGNATURE);
        state.player[8] = 1;
        state.tracks[0][0] = 0x80;
        put_word(&mut state.tracks[0], 0x20, 0x2001000);
        put_word(&mut state.channels[0].bytes, 0x34, 0x2001000);
        let before = state.clone();
        assert_eq!(state.stop(), Err(MPlayError::ChannelCycle(0x2001000)));
        assert_eq!(state, before);
        state.player[8] = 255;
        let before = state.clone();
        assert!(matches!(state.stop(), Err(MPlayError::TrackStorage { .. })));
        assert_eq!(state, before);
    }
    #[test]
    fn channel_work_budget_precedes_link_traversal() {
        let mut state = MPlayState {
            player: [0; PLAYER_BYTES],
            tracks: vec![[0; TRACK_BYTES]],
            channels: (0..257)
                .map(|index| Channel {
                    address: 0x02001000 + index * 0x40,
                    bytes: [0; CHANNEL_BYTES],
                })
                .collect(),
            sound_info: [0; SOUND_INFO_PREFIX],
        };
        state.tracks[0][0] = 0x80;
        put_word(&mut state.tracks[0], 0x20, 0xffffffff);
        let before = state.clone();
        assert_eq!(state.stop_track(0), Err(MPlayError::TooManyChannels(257)));
        assert_eq!(state, before);
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; every main-game sound-map header"]
    fn loads_all_native_request_headers_without_assuming_catalog_index_mapping() {
        use std::io::Read;
        let mut rom = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("Set SPP_ROM"))
            .unwrap()
            .take((crate::catalog::ROM_LEN + 1) as u64)
            .read_to_end(&mut rom)
            .unwrap();
        let bank = MPlayBank::load(&rom).unwrap();
        assert_eq!(bank.songs.len(), super::super::sound::SONG_MAP_COUNT);
        for (address, header) in bank.routing.headers.iter().zip(&bank.songs) {
            assert_eq!(*address == 0, header.is_none());
            if let Some(header) = header {
                assert_eq!(header.address, *address);
            }
        }
    }
}
