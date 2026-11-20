use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_garden_behavior::{GardenRng, MemoryDeal},
};
use std::num::NonZeroU16;
#[test]
fn tiny_garden_rng_and_memory_deals_match_a2ne_thumb() {
    let mut rng_out = Vec::new();
    let mut deal_out = Vec::new();
    for i in 0..4096u32 {
        let seed = i.wrapping_mul(0x9e3779b9);
        let mut rng = GardenRng::new(seed);
        for limit in [1, 2, 5, 6, 7, 255, 256, 32768, 65535] {
            rng_out.extend(rng.next_u15().to_le_bytes());
            rng_out.extend(rng.below(NonZeroU16::new(limit).unwrap()).to_le_bytes());
            rng_out.extend(rng.state().to_le_bytes());
        }
        let mut rng = GardenRng::new(seed);
        let deal = MemoryDeal::new(&mut rng).unwrap();
        let mut at = [None; 14];
        let mut pairs = [0; 7];
        for row in 0..5 {
            for col in 0..6 {
                let card = deal.card(row, col);
                deal_out.push(card.unwrap_or(255));
                if let Some(n) = card {
                    assert!(at[usize::from(n)].replace((row, col)).is_none());
                    pairs[usize::from(deal.face(row, col).unwrap())] += 1;
                }
            }
        }
        assert_eq!(pairs, [2; 7]);
        assert_eq!(deal.face(5, 0), None);
        assert_eq!(deal.card(0, 6), None);
        deal_out.extend(rng.state().to_le_bytes());
        for (card, pos) in at.into_iter().enumerate() {
            let (row, col) = pos.unwrap();
            deal_out.extend([row * 32, col * 32]);
            let tile = 0x7f + (card as u16 / 2) * 16;
            let bank = (2 + card as u16 / 4) << 12;
            deal_out.extend((tile | bank).to_le_bytes());
        }
    }
    assert_eq!(
        gba::sha256(&rng_out),
        "3299bc70b775d7fdc8cc496d60846ddd1c9508293fa94f632c1c716114da0d3c"
    );
    assert_eq!(
        gba::sha256(&deal_out),
        "8cb590bbc5e9c776719fb031382b88f5dd3752a6e3fd26fbb895acbd2f2a0995"
    );
}
#[test]
#[ignore = "requires verified local A2NE_ROM"]
fn tiny_garden_memory_original_instruction_ranges() {
    let source = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&source).unwrap();
    let b = rom.garden_bytes();
    for (start, end, hash) in [
        (
            0x151c,
            0x1588,
            "173a419e990f9b0fece26492aa6f99763d47a24a007290c498f1abe81cdc2df1",
        ),
        (
            0xa45c,
            0xa554,
            "a792e938ee0756261a86fd7f471ecdf240763efbb9e8fc13b4047f47983541e0",
        ),
        (
            0xa554,
            0xa638,
            "843ddb6e4a8c62181907914b9119f815b2c89b401ba52a6cf9736b7b1311c915",
        ),
    ] {
        assert_eq!(gba::sha256(&b[start..end]), hash);
    }
}
