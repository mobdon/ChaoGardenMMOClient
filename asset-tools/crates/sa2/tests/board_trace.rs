use sa2_asset_tools::board::{BoardPlayer, BoardTrigger};
use sha2::{Digest, Sha256};

// Keep the sequence synchronized with the C driver in STAGE-BOARD.md.
#[test]
fn compares_board_callbacks_with_compiled_reference() {
    let mut out = Vec::new();
    for ang in [0, 1, 0xfffe, 0xffff] {
        for radius in [-11.0, -10.0, -9.0, -5.0, 0.0] {
            for two in [false, true] {
                for bits in 0..4 {
                    let mut trigger = BoardTrigger::new([1.0, 2.0, 3.0], radius, ang);
                    for frame in 0..12 {
                        let players = std::array::from_fn(|slot| {
                            let mode = [0x4b, 0x4c, 0x52, 0x53][(frame + slot) % 4];
                            let position = match (frame + slot) % 4 {
                                0 => [4.0, 6.0, 3.0],
                                1 => [1.0, 2.0, 3.0],
                                2 => [1.0, 2.0, 4.0],
                                _ => [1.0, 2.0, 3.5],
                            };
                            (bits & (1 << slot) != 0).then_some(BoardPlayer { position, mode })
                        });
                        for (slot, input) in trigger.tick(players, two, frame == 0 || frame == 7) {
                            out.extend([slot, input, 0]);
                        }
                        out.push(0xff);
                    }
                }
            }
        }
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(out)),
        "95a5f893a068fb53175e7603d91c10f764e21e0d2d7d1dc22821f79a431a9d34"
    );
}
