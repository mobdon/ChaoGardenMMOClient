//! Deterministic US Tiny Chao Garden memory-card deal and player-turn logic.
use crate::{Error, Result};
use serde::Serialize;

pub const EMPTY: u8 = 0xff;
pub const A: u16 = 1;
pub const RIGHT: u16 = 0x10;
pub const LEFT: u16 = 0x20;
pub const UP: u16 = 0x40;
pub const DOWN: u16 = 0x80;
/// Native initial deal, before the Chao moves any cards during its preview.
pub fn deal(seed: u32) -> ([u8; 30], u32) {
    let mut state = seed;
    let mut board = [EMPTY; 30];
    let mut draw = || {
        state = state.wrapping_mul(0x41c64e6d).wrapping_add(0x3039);
        (state >> 23) & 0xff
    };
    for card in 0..14 {
        loop {
            let row = draw() % 5;
            let column = draw() % 6;
            let cell = &mut board[(row * 6 + column) as usize];
            if *cell == EMPTY {
                *cell = card;
                break;
            }
        }
    }
    (board, state)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Phase {
    First,
    Second,
    Reveal,
    Mismatch,
    Close,
    Remove,
    Fly,
    Score,
    Count,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Playing,
    Won,
    Lost,
}
/// State starts at the native interactive phase, after Chao's preview shuffle.
#[derive(Debug, Serialize)]
pub struct MemoryRound {
    board: [u8; 30],
    cursor: [u8; 2],
    phase: Phase,
    outcome: Outcome,
    picks: [u8; 2],
    flip: [i8; 2],
    timer: u8,
    pairs: u8,
    misses: u8,
    points: u8,
    #[serde(skip)]
    previous_keys: u16,
}
impl MemoryRound {
    pub fn from_board(board: [u8; 30]) -> Result<Self> {
        let mut seen = 0u16;
        for card in board.into_iter().filter(|v| *v != EMPTY) {
            if card >= 14 || seen & (1 << card) != 0 {
                return Err(Error("invalid or duplicate memory card".into()));
            }
            seen |= 1 << card;
        }
        if seen != 0x3fff {
            return Err(Error("expected all fourteen memory cards".into()));
        }
        Ok(Self {
            board,
            cursor: [2, 2],
            phase: Phase::First,
            outcome: Outcome::Playing,
            picks: [0; 2],
            flip: [0; 2],
            timer: 0,
            pairs: 0,
            misses: 0,
            points: 0,
            previous_keys: 0,
        })
    }
    pub fn board(&self) -> &[u8; 30] {
        &self.board
    }
    /// Visible card face during the first or second flip of an interactive turn.
    pub fn visible_face(&self, cell: usize) -> Option<u8> {
        let &card = self.board.get(cell)?;
        if card == EMPTY {
            return None;
        }
        let visible = match self.phase {
            Phase::Second => self.picks[0] as usize == cell,
            Phase::Reveal | Phase::Mismatch | Phase::Close | Phase::Remove => {
                self.picks.contains(&(cell as u8))
            }
            _ => false,
        };
        visible.then_some(card / 2)
    }
    pub fn cursor(&self) -> [u8; 2] {
        self.cursor
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub fn pairs(&self) -> u8 {
        self.pairs
    }
    pub fn misses(&self) -> u8 {
        self.misses
    }
    pub fn points(&self) -> u8 {
        self.points
    }
    /// Per-card opening/closing counter for the two flip animations.
    pub fn flip_frames(&self) -> [i8; 2] {
        self.flip
    }
    pub fn timer(&self) -> u8 {
        self.timer
    }
    /// Winning result-screen bonus after counting all remaining mistake markers.
    /// This is a local display value and does not award any account currency.
    pub fn result_points(&self) -> Option<u8> {
        match self.outcome {
            Outcome::Playing => None,
            Outcome::Lost => Some(self.points),
            Outcome::Won => Some((self.points + [60, 30, 10][self.misses as usize]).min(99)),
        }
    }
    /// Advance one frame using the GBA held-key mask. Only fresh presses move
    /// or select. Simultaneous directions run in the cartridge's order.
    pub fn step(&mut self, held: u16) {
        if self.outcome != Outcome::Playing {
            return;
        }
        let pressed = held & !self.previous_keys;
        self.previous_keys = held;
        match self.phase {
            Phase::First | Phase::Second => self.select(pressed),
            Phase::Reveal if self.flip[1] == 30 => {
                self.timer = 0;
                let [a, b] = self.picks.map(|p| self.board[p as usize]);
                self.phase = if a / 2 == b / 2 {
                    Phase::Remove
                } else {
                    Phase::Mismatch
                };
            }
            Phase::Mismatch => {
                self.timer += 1;
                if self.timer == 84 {
                    self.misses += 1;
                    self.phase = Phase::Close;
                }
            }
            Phase::Close if self.flip[0] == 0 => {
                if self.misses == 3 {
                    self.outcome = Outcome::Lost;
                    self.timer = 0;
                } else {
                    self.phase = Phase::First;
                }
            }
            Phase::Remove => {
                if self.timer == 0 {
                    for &cell in &self.picks {
                        self.board[cell as usize] = EMPTY;
                    }
                }
                self.timer += 1;
                if self.timer == 17 {
                    self.timer = 0;
                    self.phase = Phase::Fly;
                }
            }
            Phase::Fly => {
                self.timer += 1;
                if self.timer == 21 {
                    self.phase = Phase::Score;
                }
            }
            Phase::Score => {
                self.points += self.picks.map(card_points).iter().sum::<u8>();
                self.phase = Phase::Count;
            }
            Phase::Count => {
                self.pairs += 1;
                if self.pairs == 7 {
                    self.outcome = Outcome::Won;
                } else {
                    self.phase = Phase::First;
                }
            }
            _ => {}
        }
        match self.phase {
            Phase::Second => self.flip[0] = (self.flip[0] + 1).min(30),
            Phase::Reveal => {
                for n in &mut self.flip {
                    *n = (*n + 1).min(30);
                }
            }
            Phase::Close => {
                for n in &mut self.flip {
                    *n -= 1;
                }
            }
            _ => {}
        }
    }
    fn select(&mut self, pressed: u16) {
        let [mut x, mut y] = self.cursor;
        if pressed & UP != 0 {
            y = (y + 4) % 5;
        }
        if pressed & DOWN != 0 {
            y = (y + 1) % 5;
        }
        if pressed & LEFT != 0 {
            x = (x + 5) % 6;
        }
        if pressed & RIGHT != 0 {
            x = (x + 1) % 6;
        }
        self.cursor = [x, y];
        let cell = y * 6 + x;
        if pressed & A == 0 || self.board[cell as usize] == EMPTY {
            return;
        }
        if self.phase == Phase::First {
            self.picks[0] = cell;
            self.flip[0] = 0;
            self.phase = Phase::Second;
        } else if cell != self.picks[0] {
            self.picks[1] = cell;
            self.flip[1] = 0;
            self.phase = Phase::Reveal;
        }
    }
}
fn card_points(cell: u8) -> u8 {
    let (x, y) = (cell % 6, cell / 6);
    if (2..=3).contains(&x) && y == 2 {
        5
    } else if (1..=4).contains(&x) && (1..=3).contains(&y) {
        3
    } else {
        1
    }
}
