//! Native sound-request routing. The m4a sequencer/mixer is a separate port.
use crate::{binary, catalog};
use serde::Serialize;

pub const PLAYERS_OFFSET: usize = 0x6dd41c;
pub const SONG_MAP_OFFSET: usize = 0x6dd42c;
pub const SONG_MAP_COUNT: usize = 135;

pub struct SoundBank {
    pub players: [u32; 4],
    pub headers: [u32; SONG_MAP_COUNT],
}

impl SoundBank {
    pub fn load(rom: &[u8]) -> Result<Self, String> {
        catalog::validate_rom(rom).map_err(|e| e.to_string())?;
        let players = std::array::from_fn(|i| {
            binary::u32_le(rom, PLAYERS_OFFSET + i * 4).expect("version-gated source range")
        });
        for &p in &players {
            if !((0x02000000..=0x0203fff8).contains(&p) || (0x03000000..=0x03007ff8).contains(&p)) {
                return Err("sound player pointer outside native work RAM".into());
            }
        }
        let headers = std::array::from_fn(|i| {
            binary::u32_le(rom, SONG_MAP_OFFSET + i * 4).expect("version-gated source range")
        });
        for &p in &headers {
            if p != 0 {
                let off =
                    binary::pointer_offset(p, 0x08000000, rom.len()).map_err(|e| e.to_string())?;
                binary::span(rom, off, 8).map_err(|e| e.to_string())?;
            }
        }
        Ok(Self { players, headers })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SoundCall {
    Start { player: u32, header: u32 },
    Stop { player: u32 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundRouting {
    /// Original four halfwords at030007C8; values remain native sound-map IDs.
    pub current: [u16; 4],
    /// Snapshot of each playerinfo halfword+4. Set by the real audio engine,
    /// never inferred from a request or replaced by a made-up playback timer.
    pub player_status: [u16; 4],
}

impl SoundRouting {
    /// ROM 0x38D4: the flipper's forced channel-2 request, with native constant
    /// return value 2 even when the song index is rejected by ROM 0x384C.
    pub fn force_channel2(&mut self, bank: &SoundBank, song: u32) -> (u32, Option<SoundCall>) {
        (2, self.request(bank, 2, song, true))
    }
    /// Port of ROM38FC: truncatechannel tou8, reject>3, testnativeu16status.
    pub fn is_playing(&self, channel: u32) -> bool {
        let channel = usize::from(channel as u8);
        channel < 4 && self.player_status[channel] != 0
    }

    /// ROM37D4 avoids restarting the same active sound; ROM384C forcesstart.
    /// External MPlayStart/MPlayStop calls are emitted in native order. Their
    /// sequencer/audio effects remain explicitly unimplemented by this module.
    pub fn request(
        &mut self,
        bank: &SoundBank,
        channel: u32,
        song: u32,
        force: bool,
    ) -> Option<SoundCall> {
        let channel = usize::from(channel as u8);
        let song = song as u16;
        if channel >= 4 || usize::from(song) >= SONG_MAP_COUNT {
            return None;
        }
        let header = bank.headers[usize::from(song)];
        let call = if header == 0 {
            Some(SoundCall::Stop {
                player: bank.players[channel],
            })
        } else if force || self.current[channel] != song || self.player_status[channel] == 0 {
            Some(SoundCall::Start {
                player: bank.players[channel],
                header,
            })
        } else {
            None
        };
        self.current[channel] = song;
        call
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_request_and_status_cases() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/sound-vectors.json")).unwrap();
        let bank = SoundBank {
            players: std::array::from_fn(|i| fixture["players"][i].as_u64().unwrap() as u32),
            headers: std::array::from_fn(|i| fixture["headers"][i].as_u64().unwrap() as u32),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let mut routing = SoundRouting {
                current: std::array::from_fn(|i| case["current"][i].as_u64().unwrap() as u16),
                player_status: std::array::from_fn(|i| case["status"][i].as_u64().unwrap() as u16),
            };
            let channel = case["channel"].as_u64().unwrap() as u32;
            let song = case["song"].as_u64().unwrap() as u32;
            if case["routine"] == "status" {
                assert_eq!(
                    routing.is_playing(channel),
                    case["result"].as_u64().unwrap() != 0
                );
            } else {
                let call = if case["routine"] == "force-channel2" {
                    let (result, call) = routing.force_channel2(&bank, song);
                    assert_eq!(u64::from(result), case["returnValue"].as_u64().unwrap());
                    call
                } else {
                    routing.request(&bank, channel, song, case["routine"] == "force")
                };
                let actual = call.map(|c| match c {
                    SoundCall::Start { player, header } => {
                        serde_json::json!(["start", player, header])
                    }
                    SoundCall::Stop { player } => serde_json::json!(["stop", player, 0]),
                });
                assert_eq!(serde_json::to_value(actual).unwrap(), case["call"]);
                assert_eq!(
                    serde_json::to_value(routing.current).unwrap(),
                    case["after"]
                );
            }
        }
    }
}
