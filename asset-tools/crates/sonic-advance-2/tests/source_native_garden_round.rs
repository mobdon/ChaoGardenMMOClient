use sonic_advance_2_assets::native_garden_memory::{
    A2neMemorySource, NativeButtons, NativeMemoryPhase as Phase,
};
#[test]
#[ignore = "requires A2NE_ROM and ignored native Memory trace from native_memory_round_oracle.py"]
fn four_legal_memory_rounds_match_unmodified_thumb() {
    let rom = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = sonic_advance_2_assets::VerifiedRom::parse(&rom).unwrap();
    let trace = std::fs::read(std::env::var_os("A2NE_MEMORY_TRACE").unwrap()).unwrap();
    assert_eq!(
        sonic_advance_2_assets::gba::sha256(&trace),
        "3343e233f861bdac182662ae7a17cd4bf5f464f34bdeb73a4a8dc65f273b7546"
    );
    let src = A2neMemorySource::from_verified_payload(rom.garden_bytes()).unwrap();
    let mut at = 0;
    let w = |p| u32::from_le_bytes(trace[p..p + 4].try_into().unwrap());
    let mut total = 0;
    while at < trace.len() {
        let seed = w(at);
        let frames = w(at + 4);
        at += 8;
        let mut round = src.begin_round(seed).unwrap();
        for frame in 0..frames {
            let data = &trace[at..at + 16];
            at += 16;
            let key = u16::from(data[0]);
            round
                .step(NativeButtons {
                    pressed: key,
                    held: key,
                })
                .unwrap();
            assert_eq!(
                round.rng_seed(),
                u32::from_le_bytes(data[12..16].try_into().unwrap()),
                "seed {seed} frame {frame} RNG"
            );
            let view = round.view();
            let expected = match data[1] {
                4 => Phase::Reveal,
                5 => Phase::Shuffle,
                6 => Phase::Matching,
                7..=9 => Phase::Result,
                0 => Phase::Ended,
                _ => unreachable!(),
            };
            assert_eq!(view.phase, expected, "seed {seed} frame {frame} phase");
            let occupied = view
                .cells
                .iter()
                .enumerate()
                .fold(0u32, |v, (i, c)| v | (u32::from(c.occupied) << i));
            assert_eq!(
                occupied,
                u32::from_le_bytes(data[8..12].try_into().unwrap()),
                "seed {seed} frame {frame} occupied"
            );
            if view.phase == Phase::Matching {
                assert_eq!(
                    view.cursor,
                    Some((data[3], data[4])),
                    "seed {seed} frame {frame} cursor"
                );
            }
            assert_eq!(
                [view.mistakes, view.matched_pairs, view.rings],
                [data[5], data[6], data[7]],
                "seed {seed} frame {frame} score"
            );
        }
        let (selection, all, bank) = (w(at), w(at + 4), w(at + 8));
        at += 12;
        let f = round.finished().unwrap();
        assert_eq!(u32::from(f.rings), bank);
        assert_eq!(f.won, seed != 91);
        println!(
            "seed {seed}: {all} original callback frames, selection starts at {selection}, {frames} compared, won={}, rings={bank}",
            f.won
        );
        total += frames;
    }
    println!("{total} compared frames");
}
