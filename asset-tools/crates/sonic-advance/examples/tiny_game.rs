use sonic_advance_assets::tiny_game::{
    deal, MemoryRound, Outcome, Phase, A, DOWN, EMPTY, LEFT, RIGHT, UP,
};
use std::{
    error::Error,
    io::{self, BufRead, Write},
};

fn draw(round: &MemoryRound, preview: bool) {
    let cursor = round.cursor();
    for y in 0..5 {
        for x in 0..6 {
            let cell = 6 * y + x;
            let card = round.board()[cell];
            let face = if card == EMPTY {
                '.'
            } else if preview {
                char::from(b'A' + card / 2)
            } else {
                round
                    .visible_face(cell)
                    .map_or('#', |v| char::from(b'A' + v))
            };
            if cursor == [x as u8, y as u8] {
                print!("[{face}]")
            } else {
                print!(" {face} ")
            }
        }
        println!();
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let seed = std::env::args()
        .nth(1)
        .map(|v| v.parse::<u32>())
        .transpose()?
        .unwrap_or(12345);
    let (board, _) = deal(seed);
    let mut round = MemoryRound::from_board(board)?;
    let mut lines = io::stdin().lock().lines();
    println!("Memory cards · seed {seed}. Memorize the pairs, then press Enter.");
    draw(&round, true);
    if lines.next().transpose()?.is_none() {
        return Ok(());
    }
    println!("Use w/a/s/d to move and f to flip. Type a sequence or q to quit.");
    loop {
        draw(&round, false);
        println!(
            "Pairs {}/7 · misses {}/3 · points {}",
            round.pairs(),
            round.misses(),
            round.points()
        );
        if let Some(points) = round.result_points() {
            println!("{:?} · final points {points}", round.outcome());
            break;
        }
        print!("> ");
        io::stdout().flush()?;
        let Some(input) = lines.next().transpose()? else {
            break;
        };
        for ch in input.chars() {
            let key = match ch {
                'w' => UP,
                's' => DOWN,
                'a' => LEFT,
                'd' => RIGHT,
                'f' => A,
                'q' => return Ok(()),
                _ => continue,
            };
            round.step(key);
            round.step(0);
            if round.phase() == Phase::Reveal {
                draw(&round, false);
            }
            while round.outcome() == Outcome::Playing
                && !matches!(round.phase(), Phase::First | Phase::Second)
            {
                round.step(0);
            }
            if round.outcome() != Outcome::Playing {
                break;
            }
        }
    }
    Ok(())
}
