use sonic_advance_assets::{
    binary,
    tiny_game::{deal, MemoryRound, Outcome, Phase, A, DOWN, EMPTY, RIGHT},
};
#[test]
fn deals_match_four_executions_of_the_us_thumb_routine() {
    for (seed, expected, final_seed) in [
        (
            0,
            "09050c080300ffffffffffff02ffffff0dffffffffff0107ff0a0bff0604",
            1194991756,
        ),
        (
            1,
            "ffff080a06ffff0400050b07ff02ffffffff09ffffffffffff0cff03010d",
            2058258651,
        ),
        (
            12345,
            "ffffffff0b04ffff050a0dff00ff0c01060207ffffff08ff09ffffff03ff",
            2539649755,
        ),
        (
            u32::MAX,
            "050cff0b03ffffffff0907ffffffff040106ffffffff020aff0dffff0008",
            314311085,
        ),
    ] {
        let (board, last) = deal(seed);
        let hex = board.map(|v| format!("{v:02x}")).concat();
        assert_eq!((hex.as_str(), last), (expected, final_seed));
    }
}
#[test]
fn rejects_malformed_boards_and_filters_held_keys() {
    assert!(MemoryRound::from_board([EMPTY; 30]).is_err());
    let (mut board, _) = deal(0);
    let at = board.iter().position(|&v| v == 0).unwrap();
    board[at] = 1;
    assert!(MemoryRound::from_board(board).is_err());
    board[at] = 14;
    assert!(MemoryRound::from_board(board).is_err());
    let mut round = MemoryRound::from_board(deal(12345).0).unwrap();
    round.step(RIGHT);
    round.step(RIGHT);
    assert_eq!(round.cursor(), [3, 2]);
    round.step(0);
    round.step(RIGHT);
    assert_eq!(round.cursor(), [4, 2]);
    assert_eq!(round.phase(), Phase::First);
}
fn replay(targets: &[u8]) -> (usize, String, Option<u8>) {
    let (board, _) = deal(12345);
    let mut game = MemoryRound::from_board(board).unwrap();
    let mut keys = Vec::new();
    let (mut x, mut y) = (2, 2);
    for pair in targets.as_chunks::<2>().0 {
        for &card in pair {
            let cell = board.iter().position(|&v| v == card).unwrap();
            while x != cell % 6 {
                keys.extend([RIGHT, 0]);
                x = (x + 1) % 6;
            }
            while y != cell / 6 {
                keys.extend([DOWN, 0]);
                y = (y + 1) % 5;
            }
            keys.extend([A, 0]);
        }
        keys.extend([0; 160]);
    }
    let mut trace = Vec::new();
    let mut frames = 0;
    for held in keys {
        game.step(held);
        frames += 1;
        trace.extend_from_slice(game.board());
        trace.extend(game.cursor());
        let mode = match game.outcome() {
            Outcome::Playing => 6,
            Outcome::Won => 7,
            Outcome::Lost => 8,
        };
        trace.extend([
            game.phase() as u8,
            mode,
            game.pairs(),
            game.misses(),
            game.points(),
        ]);
        trace.extend(game.flip_frames().map(|v| v as u8));
        trace.push(game.timer());
        if game.outcome() != Outcome::Playing {
            break;
        }
    }
    (frames, binary::sha256(&trace), game.result_points())
}
#[test]
fn every_frame_matches_the_original_player_phase_through_win_or_loss() {
    let perfect: Vec<u8> = (0..14).collect();
    let mut one_miss = vec![0, 2];
    one_miss.extend(&perfect);
    let mut two_misses = vec![0, 2, 0, 2];
    two_misses.extend(&perfect);
    for (targets, count, digest, points) in [
        (
            &perfect[..],
            1171,
            "68dff21272f00958138655dd54e0de6109b7aab5294d4298b8e8c6990ac8cde8",
            92,
        ),
        (
            &[0, 2, 0, 2, 0, 2],
            517,
            "b67a60ab8b397bb3a036c1f7f4080ee8352b697a222aedc3889cf9731bfbdd21",
            0,
        ),
        (
            &one_miss,
            1347,
            "06b80040c81c0df86561dd7ecb42ec6adb2b0ea3d068ff70e11dbe1479757d11",
            62,
        ),
        (
            &two_misses,
            1523,
            "af4466e7de9b843b00cd3da96b8dc9c75da9b0e7608cd159e0b2b74d67faa420",
            42,
        ),
    ] {
        let (frames, hash, result) = replay(targets);
        assert_eq!(
            (frames, hash.as_str(), result),
            (count, digest, Some(points))
        );
    }
}
