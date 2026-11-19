# ASOE Tiny Chao Garden memory game

Run the Rust player-turn game with `cargo run --example tiny_game -- 12345`. The decimal argument is the native RNG seed. Memorize the displayed cards, press Enter, then use w/a/s/d and f to move and select. Seven pairs win the round. Three mismatches lose it. The terminal advances through noninteractive animation frames automatically and reports local points. It uses the original dealt board directly, skipping the Chao's pre-turn movement.

## Mechanics and source

For ASOE revision 0 (SHA-256 afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020), the verified compressed Tiny Chao Garden program starts at ROM offset 0x9BE78. The addresses below are offsets into that program after decompression to 0x02000000. The reference assembly is chao_garden/asm/code.s from SAT-R/sa2 commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba. Its functions sub_0200a554 and the cases starting at _0200B6E4 correspond to the US deal and player-turn routines at different addresses.

| US offset | Behavior |
| --- | --- |
| 0x1114, 0x8C44 | Advance the native 32-bit RNG; draw a row then a column and retry occupied cells. Deal cards 0–13 on a 6×5 grid. |
| 0x8E10, 0x9E2C | Start the cursor at (2,2). Poll the four fresh direction presses in up/down/left/right order, wrap the cursor and accept a fresh A press on a distinct, occupied cell. |
| 0x9F94, 0xA2BA | Open each card over 30 frames and compare the card IDs divided by two. |
| 0x9FE0, 0xA058 | Hold a mismatch for 84 frames, close the cards, and end the round at three mistakes. |
| 0xA0A0, 0xA1E0, 0xA268 | Remove a pair, fly its score to the counter and count it. Edge cards contribute one point, inner cards three and the two center cells five. Seven pairs win. |
| 0xA354, 0x8128 | The result screen adds 30, 20 and 10 in sequence for a perfect game, 20 and 10 for one miss, or 10 for two misses. The local counter saturates at 99. |

The Rust API separates the initial deal from player input. `deal(seed)` returns the initial board and RNG state. `MemoryRound::from_board(board)` accepts a validated post-preview board with exactly one of each native card ID. `step(held_keys)` consumes one GBA frame's held-key mask. State includes both flip counters, pair count, misses, local score, cursor, phase and outcome. Result points are a local display calculation.

## Native comparisons

Four seeds (0, 1, 12345 and 0xFFFFFFFF) were run through the US deal routine in Unicorn. Rust reproduced all 30 cells and the final native RNG word for each. In the native deal, only card OAM position and animation helpers were stubbed.

Four deterministic key streams drove the native player frame dispatcher at 0x90E8. The oracle supplied held and rising input at 0x03003830, started in interactive mode 6 with the seed-12345 deal, and captured state at 0xA338 after the flip updates each frame. Audio start and card-sprite animation calls returned immediately. The original comparisons, timers, removal, score and transition code ran unchanged.

| Input run | Frames checked | Matched pairs | Mistakes | Final display points |
| --- | ---: | ---: | ---: | ---: |
| Perfect | 1,171 | 7 | 0 | 92 |
| Three wrong pairs | 517 | 0 | 3 | 0 |
| One miss, then seven correct | 1,347 | 7 | 1 | 62 |
| Two misses, then seven correct | 1,523 | 7 | 2 | 42 |

For every frame, the regression test hashes the 30 board bytes, cursor, native subphase, outer game mode, matched-pair count, mistake count, point counter, both flip counters and timer. Each digest matches its independent native run. The original result-screen counter also ran from 32 points for all three winning cases and returned 92, 62 and 42, with its visual transition declared ready. Run `cargo test --test tiny_game` to reproduce comparisons from the recorded digests without a ROM.

This covers the initial deal and interactive matching round. Pre-play Chao movement, start/pause screens, native graphics and audio, Garden launch/return, and authenticated server settlement remain separate work. The example provides no path to a save or account balance.
