//! Music-side effects of City Escape's stage prolog, BgmStart and SET ADX task.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "function", content = "argument")]
pub enum MusicCall {
    #[serde(rename = "Bgm_SetVolume")]
    SetVolume(u32),
    #[serde(rename = "StgPlayMusic")]
    StagePlay(&'static str),
    #[serde(rename = "BGM_Play")]
    Play(&'static str),
    #[serde(rename = "fn_80011DF4")]
    Fn80011df4,
    #[serde(rename = "fn_80011FB0")]
    Fn80011fb0(&'static str),
}

const ESCAPE: &str = "c_escap1.adx";
const TRUCK: &str = "c_escap2.adx";
const NO_INTRO: &str = "c_escap3.adx";

#[derive(Clone, Debug)]
pub struct AdxTrigger {
    pub mode: u8,
}

impl AdxTrigger {
    pub fn spawn(range_out: bool, short_city_2p: bool) -> Option<Self> {
        (!range_out && !short_city_2p).then_some(Self { mode: 0 })
    }

    pub fn tick(
        &mut self,
        range_out: bool,
        player_in_sphere: bool,
        game_state: i32,
    ) -> Vec<MusicCall> {
        let mut calls = Vec::new();
        if range_out {
            return calls;
        }
        if player_in_sphere && matches!(self.mode, 0 | 2 | 4) {
            self.mode = 1;
            calls.extend([MusicCall::Fn80011df4, MusicCall::Fn80011fb0(TRUCK)]);
        }
        match game_state {
            16 if self.mode != 1 => {
                calls.push(MusicCall::Play(NO_INTRO));
                self.mode = 4;
            }
            13 => self.mode = 2,
            9 if self.mode != 1 => self.mode = 3,
            _ => {}
        }
        calls
    }

    pub fn destroy(self, game_state: i32, predicate_8001cc18: bool) -> Vec<MusicCall> {
        match if game_state == 7 { 2 } else { self.mode } {
            2 => vec![MusicCall::Play(ESCAPE)],
            1 | 3 | 4 => vec![MusicCall::Play(if predicate_8001cc18 {
                ESCAPE
            } else {
                NO_INTRO
            })],
            _ => Vec::new(),
        }
    }
}

/// The first argument is lbl_801CC168.unk_0x0 in the reference.
pub fn prolog(stage_variant: i32, two_player: bool) -> (Option<BgmStart>, Vec<MusicCall>) {
    if stage_variant == 0 && two_player {
        (Some(BgmStart::default()), Vec::new())
    } else {
        let song = if stage_variant == 0 {
            ESCAPE
        } else {
            "t9_sonic.adx"
        };
        (
            None,
            vec![MusicCall::SetVolume(0), MusicCall::StagePlay(song)],
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct BgmStart {
    pub counter: i32,
}

impl BgmStart {
    pub fn tick(&mut self, game_state: i32, player_modes: [u8; 2]) -> Vec<MusicCall> {
        if game_state == 7 || (game_state == 16 && player_modes == [54, 54]) {
            self.counter = 0;
            return vec![MusicCall::Fn80011df4];
        }
        if game_state != 16 {
            return Vec::new();
        }
        let before = self.counter;
        self.counter = self.counter.wrapping_add(1);
        if before == 9 {
            vec![MusicCall::SetVolume(0), MusicCall::StagePlay(ESCAPE)]
        } else if self.counter < 9 {
            vec![MusicCall::Fn80011df4]
        } else {
            Vec::new()
        }
    }
}
