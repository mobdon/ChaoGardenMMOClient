use serde_json::{json, Value};
use sonic_advance_assets::{
    catalog,
    garden::GardenEvidence,
    garden_screen,
    tiny_game::{self, MemoryRound, Outcome},
    tiny_rps::Round,
    Result,
};

pub struct Garden {
    frame: u32,
    previous: u16,
    selected: usize,
    activity: Activity,
    pixels: Vec<u8>,
}
enum Activity {
    Home,
    Preview(MemoryRound),
    Memory(MemoryRound),
    Hands(Round),
}
impl Garden {
    pub fn load(rom: &[u8], held: u16) -> Result<Self> {
        let image = catalog::garden(rom)?;
        let bg = garden_screen::background(&GardenEvidence::parse(&image.bytes)?)?;
        let mut pixels = vec![0u8; 240 * 160 * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        for (i, c) in bg.into_iter().enumerate() {
            let j = 4 * (i / 176 * 240 + i % 176);
            for k in 0..3 {
                let v = ((c >> (k * 5)) & 31) as u8;
                pixels[j + k] = (v << 3) | (v >> 2);
            }
        }
        for obj in garden_screen::entry_objects(&GardenEvidence::parse(&image.bytes)?)?
            .iter()
            .rev()
        {
            for (i, &color) in obj.colors.iter().enumerate() {
                if color == 0xffff {
                    continue;
                }
                let j = 4 * ((obj.y + i / obj.width) * 240 + obj.x + i % obj.width);
                for k in 0..3 {
                    let v = ((color >> (k * 5)) & 31) as u8;
                    pixels[j + k] = (v << 3) | (v >> 2);
                }
            }
        }
        Ok(Self {
            frame: 0,
            previous: held,
            selected: 0,
            activity: Activity::Home,
            pixels,
        })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    // False returns to the parent menu. B is a local practice exit.
    pub fn step(&mut self, rom: &[u8], held: u16) -> Result<bool> {
        self.frame = self.frame.wrapping_add(1);
        let fresh = held & !self.previous;
        self.previous = held;
        if fresh & 2 != 0 {
            if matches!(self.activity, Activity::Home) {
                return Ok(false);
            }
            self.activity = Activity::Home;
            return Ok(true);
        }
        match &mut self.activity {
            Activity::Home => {
                if fresh & 0xf0 != 0 {
                    self.selected ^= 1;
                }
                if fresh & 1 != 0 {
                    self.activity = if self.selected == 0 {
                        Activity::Preview(MemoryRound::from_board(tiny_game::deal(self.frame).0)?)
                    } else {
                        Activity::Hands(Round::from_rom(rom, self.frame)?)
                    };
                }
            }
            Activity::Preview(_) => {
                if fresh & 1 != 0 {
                    if let Activity::Preview(round) =
                        std::mem::replace(&mut self.activity, Activity::Home)
                    {
                        self.activity = Activity::Memory(round);
                    }
                }
            }
            Activity::Memory(round) => {
                if round.outcome() == Outcome::Playing {
                    // Confirming the preview must not also flip a card on the next tick.
                    round.step((held & !1) | (fresh & 1));
                } else if fresh & 1 != 0 {
                    self.activity = Activity::Home;
                }
            }
            Activity::Hands(round) => {
                if round.end().is_none() {
                    round.step((held & !1) | (fresh & 1));
                } else if fresh & 1 != 0 {
                    self.activity = Activity::Home;
                }
            }
        }
        Ok(true)
    }
    pub fn snapshot(&self) -> Value {
        let (name, view, hint) = match &self.activity {
            Activity::Home => (
                "Garden",
                format!(
                    "{} Memory\n{} Rock-Paper-Scissors",
                    if self.selected == 0 { ">" } else { " " },
                    if self.selected == 1 { ">" } else { " " }
                ),
                "Arrows choose a practice game. Z opens it. X returns to the main menu.",
            ),
            Activity::Preview(r) | Activity::Memory(r) => {
                let preview = matches!(self.activity, Activity::Preview(_));
                let cursor = r.cursor();
                let mut rows = Vec::new();
                for y in 0..5 {
                    let mut row = String::new();
                    for x in 0..6 {
                        let i = y * 6 + x;
                        let card = r.board()[i];
                        let face = if card == tiny_game::EMPTY {
                            '·'
                        } else {
                            (if preview {
                                Some(card / 2)
                            } else {
                                r.visible_face(i)
                            })
                            .map_or('#', |n| char::from(b'A' + n))
                        };
                        if cursor == [x as u8, y as u8] {
                            row.push_str(&format!("[{face}]"));
                        } else {
                            row.push_str(&format!(" {face} "));
                        }
                    }
                    rows.push(row);
                }
                rows.push(format!("Turn {:?}", r.phase()));
                rows.push(format!(
                    "Pairs {}/7   Misses {}/3   Points {}",
                    r.pairs(),
                    r.misses(),
                    r.result_points().unwrap_or(r.points())
                ));
                (
                    "Memory",
                    rows.join("\n"),
                    if preview {
                        "Memorize the pairs. Z starts the round."
                    } else if r.outcome() == Outcome::Playing {
                        "Arrows move. Z flips a card. X leaves practice."
                    } else {
                        "Round over. Z returns to the Garden."
                    },
                )
            }
            Activity::Hands(r) => {
                let targets = r
                    .targets()
                    .iter()
                    .filter_map(|t| t.position().map(|(x, y)| format!("{:?} ({x},{y})", t.hand)))
                    .collect::<Vec<_>>();
                let lanes = r
                    .lanes()
                    .iter()
                    .enumerate()
                    .map(|(i, l)| {
                        format!("{}{:?}", if i == r.selected() { ">" } else { " " }, l.hand)
                    })
                    .collect::<Vec<_>>();
                (
                    "Rock-Paper-Scissors",
                    format!(
                        "{}\nTargets: {}\nTime {}   Lives {}   Points {}{}",
                        lanes.join("  "),
                        targets.join(", "),
                        r.time().0,
                        r.lives(),
                        r.result_points().unwrap_or(r.points()),
                        r.end().map_or(String::new(), |e| format!("\n{e:?}"))
                    ),
                    if r.end().is_some() {
                        "Round stopped. Z returns to the Garden."
                    } else {
                        "Left and Right choose a lane. Z fires. X leaves practice."
                    },
                )
            }
        };
        json!({"scene":"tinyGarden","activity":name,"frame":self.frame,"view":view,"hint":hint})
    }
}
