//! What the game asks the sound hardware to do, as a queue of commands.
//!
//! Game code (ported or recompiled) calls [`Sound`]; a frontend drains the
//! queue once a frame and plays it. The browser frontend hands the commands to
//! `asset-tools/vgmstream-wasm/web/sa2-audio.js`, which decodes the player's own
//! Steam files with vgmstream. Names are Steam file names, never game data.

/// One request from the game.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundCmd {
    /// Start background music from `resource/gd_PC/ADX/<file>`, looping at its own points.
    PlayBgm {
        file: String,
    },
    StopBgm,
    /// A sound effect: bank folder `MLT/<bank>` (or `MPB/`, `event/MLT/`), file `<part>.csb`, waveform
    /// subsong from 1 (vgmstream order; `sa2::csb` maps cue names to waveforms).
    PlaySe {
        bank: String,
        part: u32,
        subsong: u32,
    },
    /// A voice line: entry `index` of `event_adx[_e].afs`.
    PlayVoice {
        index: u32,
        english: bool,
    },
    /// 0.0..=1.0 for each bus.
    SetVolume {
        bus: Bus,
        volume: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Master,
    Bgm,
    Se,
    Voice,
}

/// The game's side: push commands; the frontend takes them with [`Sound::drain`].
#[derive(Debug, Default)]
pub struct Sound {
    queue: Vec<SoundCmd>,
}

impl Sound {
    pub fn play_bgm(&mut self, file: &str) {
        self.queue.push(SoundCmd::PlayBgm {
            file: file.to_owned(),
        });
    }
    pub fn stop_bgm(&mut self) {
        self.queue.push(SoundCmd::StopBgm);
    }
    pub fn play_se(&mut self, bank: &str, part: u32, subsong: u32) {
        self.queue.push(SoundCmd::PlaySe {
            bank: bank.to_owned(),
            part,
            subsong,
        });
    }
    pub fn play_voice(&mut self, index: u32, english: bool) {
        self.queue.push(SoundCmd::PlayVoice { index, english });
    }
    pub fn set_volume(&mut self, bus: Bus, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.queue.push(SoundCmd::SetVolume { bus, volume });
    }
    /// Everything requested since the last call, in order.
    pub fn drain(&mut self) -> Vec<SoundCmd> {
        std::mem::take(&mut self.queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_in_order_and_clamps_volume() {
        let mut s = Sound::default();
        s.play_bgm("BOSS_02A.ADX");
        s.play_se("se_kart", 0, 3);
        s.set_volume(Bus::Bgm, 2.0);
        s.set_volume(Bus::Se, f32::NAN);
        let got = s.drain();
        assert_eq!(
            got[0],
            SoundCmd::PlayBgm {
                file: "BOSS_02A.ADX".into()
            }
        );
        assert_eq!(
            got[1],
            SoundCmd::PlaySe {
                bank: "se_kart".into(),
                part: 0,
                subsong: 3
            }
        );
        assert_eq!(
            got[2],
            SoundCmd::SetVolume {
                bus: Bus::Bgm,
                volume: 1.0
            }
        );
        assert_eq!(
            got[3],
            SoundCmd::SetVolume {
                bus: Bus::Se,
                volume: 0.0
            }
        );
        assert!(s.drain().is_empty());
    }
}
