use sonic_advance_assets::{
    binary,
    tiny_rps::{End, Hand, Hit, Round, FIRE, LEFT, RIGHT},
};
#[test]
fn hand_matrix() {
    let hands = [Hand::Rock, Hand::Scissors, Hand::Paper];
    for (i, &player) in hands.iter().enumerate() {
        assert_eq!(player.against(player), Hit::Tie);
        assert_eq!(player.against(hands[(i + 1) % 3]), Hit::Win);
        assert_eq!(player.against(hands[(i + 2) % 3]), Hit::Lose);
    }
}
fn sample(g: &Round) -> Vec<u8> {
    let mut data = g.seed().to_le_bytes().to_vec();
    data.extend([
        g.selected() as u8,
        g.lives(),
        g.points(),
        g.wave(),
        g.time().0,
        g.time().1,
        match g.end() {
            None => 0,
            Some(End::Time) => 1,
            Some(End::OutOfShots) => 2,
            Some(End::UnresolvedReload) => 3,
        },
    ]);
    for a in g.targets() {
        data.extend([a.phase, a.age, a.hand as u8]);
        data.extend(a.progress.to_le_bytes());
    }
    for a in g.lanes() {
        data.extend([a.phase, a.age, a.hand as u8]);
        data.extend((if a.phase <= 1 { a.y } else { 0 }).to_le_bytes());
    }
    data
}
#[test]
#[ignore = "requires exact private SONIC_ADVANCE_ASOE_ROM"]
fn matches_native_score_and_frame_traces() {
    let bytes = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    for (seed, mode, expected, points, frames, ending, wave) in [
        (
            0,
            0,
            "6d5b93a4122a984f5b0a25903565d7937e38ecc8314594977e2eefd721d802f5",
            0,
            1801,
            End::Time,
            0,
        ),
        (
            12345,
            1,
            "9b82afaec2704fe9ffb19d44ec67eba459e78940f115f1234ea5d40299d97426",
            4,
            1801,
            End::Time,
            0,
        ),
        (
            32,
            2,
            "7579bd16c2addfb17450864b3f95f2e4eec7554e86bbf95f88cb6756f75ee4ca",
            1,
            1801,
            End::Time,
            0,
        ),
        (
            321,
            1,
            "e4b8c3a00c2b4940a49a315e5ab366751f9c39bdb3d8e5a3a335d37ef91365c2",
            1,
            1801,
            End::Time,
            0,
        ),
        (
            100,
            3,
            "c76adb597a3e07505dd096bc8a8ec834afa52fefaf77f8a7d7ba44760c3bff96",
            3,
            300,
            End::OutOfShots,
            0,
        ),
        (
            123,
            3,
            "2a2545da35a8a0e3b22972eaaefb0635f669ea8b1a8815f77404dbe3783fabaa",
            5,
            885,
            End::OutOfShots,
            1,
        ),
    ] {
        let mut g = Round::from_rom(&bytes, seed).unwrap();
        let mut trace = Vec::new();
        for frame in 1..=frames {
            let (a, b, c) = match mode {
                1 => (7, 33, 49),
                2 => (53, 70, 113),
                _ => (4000, 4000, 4000),
            };
            let held = if mode == 3 {
                if frame % 2 != 0 {
                    0
                } else if let Some(idx) = g.lanes().iter().position(|l| l.phase == 0) {
                    let motion = match idx.cmp(&g.selected()) {
                        std::cmp::Ordering::Less => LEFT,
                        std::cmp::Ordering::Greater => RIGHT,
                        _ => 0,
                    };
                    motion | (u16::from(idx.abs_diff(g.selected()) <= 1) * FIRE)
                } else {
                    0
                }
            } else if mode == 0 {
                0
            } else {
                (u16::from(frame % a == 1) * FIRE)
                    | (u16::from(frame % b == 2) * RIGHT)
                    | (u16::from(frame % c == 3) * LEFT)
            };
            g.step(held);
            let data = sample(&g);

            trace.extend(data);
        }
        assert_eq!(g.end(), Some(ending));
        assert_eq!(g.wave(), wave);
        assert_eq!(g.points(), points);
        assert_eq!(binary::sha256(&trace), expected);
    }
}
