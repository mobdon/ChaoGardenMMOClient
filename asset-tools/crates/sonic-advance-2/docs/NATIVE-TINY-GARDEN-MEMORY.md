# Embedded Tiny Garden: initial Chao Memory deal

The Rust port recreates the starting five-row, six-column Memory board in the Tiny Chao Garden embedded in the USA Sonic Advance 2 ROM. The standalone round continues through reveal, moving cards, input, scoring and results. Fourteen physical cards represent seven pairs; sixteen cells remain empty. For each card the original advances the Garden LCG twice, takes high bits of the two outputs separately to select a row and column, and retries occupied cells. Retrying consumes both draws. Substituting a shuffled array would change the board, the random state available for subsequent gameplay, and often the number of draws. The native face ID is the physical card ID divided by two.

Sources are the locally verified Garden multiboot (SHA-256 84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca, 191,340 bytes) and SAT-R/sa2 2dd3f17d30cd3c156e508aac0017620ae485b9ba, chao_garden/src/game.c and chao_garden/asm/code.s. Embedded Thumb 0200151C/1528 read and write the seed; 02001534 returns the next 15 bits; 02001554 takes a fresh draw modulo a nonzero argument. 0200A554 clears and populates the board and calls 0200A45C to set the initial card art. The USA ROM verifies the three exact code ranges; no source imagery or ROM data is distributed.

tests/native_garden_memory_oracle.py executes those actual Thumb instructions, including the board placement retry loop, compiler modulo routine and OAM helper, with Unicorn. Only BIOS Div for the separate bounded RNG routine is supplied as a host service; all tested operands are nonnegative with nonzero divisors. The oracle retains two fingerprints, not boards, seeds or pixels. Rust matched 73,728 draws across 4,096 seeds and 4,096 complete deals. Each deal comparison includes all 30 cells, the final RNG word, and the original initial position and OBJ tile/bank for each of its 14 cards. The Rust test independently checks that every board contains each physical card once and each face twice.

Run cargo test --test source_native_garden_behavior -- --include-ignored with A2NE_ROM pointing at the verified local USA ROM. Regenerate the source-byte fingerprints by running the oracle with the locally extracted multiboot and Unicorn available on the Python path.

The module accepts a caller-supplied seed. Its public round view omits private card IDs and RNG state. The deal fingerprint covers only initialization; verification of the playable round follows.


## Playable round and original Thumb cross-check

The crate owns native_garden_memory.rs; it compiles with no external checkout or service. Its frame implementation originates from the existing Rust translation at native_memory.rs, SHA-256 1b0acca6efa4d3ae8daaeed13fe2eab7039f5df1e44f92700cd1d3bea21d8a2f. This port binds it to the locally verified A2NE payload and Garden RNG. It corrects an entry timing error: full original callback 0200A8A4 leaves 48 in word 030019E4 when case 3 calls the board initializer and enters case 4. Starting the reveal at zero delays the shuffle by 48 frames. The normal post-instruction reveal ends after 222 frames, not 270.

Create A2neMemorySource from VerifiedRom::garden_bytes(), then begin_round(seed), step one native frame with NativeButtons { held, pressed }, inspect view(), and use finished() only when the results have ended. Direction edges wrap the cursor on the 6×5 board; A selects, empty cells and selecting the same physical card twice do nothing. Each selected card flips over 30 frames. Correct pairs score the sum of the two board cell values (1 outer, 3 inner, 5 center); mismatches hold for 84 frames before flipping back, and the third mistake ends the round. Clearing the board with 0/1/2 mistakes stages respectively 30+20+10, 20+10, or 10 extra rings; the displayed total caps at 99. The owned port returns a cartridge-local outcome. The original bank-write endpoint is separate.

tests/native_memory_round_oracle.py runs the original multiboot instructions from Memory initialization (0200A8A4) through the card deal, reveal, shuffle, manual selection, result phases and return request for 02001BF8. It supplies legal A and D-pad press/release edges, reading the board only to choose a repeatable test strategy. The emulator stubs audio calls and BIOS video copy/decompression; the oracle does not establish pixel or audio equality. The trace retains button edges, public occupancy, score, phase and cursor, with no private card IDs. It checks the original transient result and the in-memory native bank endpoint, starting from zero; it does not write a save.

| Seed | Original callback frames | Post-instruction frames compared | Original outcome |
|---:|---:|---:|---|
| 0 | 2,023 | 1,857 | Seven pairs, no mistakes: 18 + 60 = 78 |
| 7 | 2,051 | 1,885 | Seven pairs, one mistake: 28 + 30 = 58 |
| 19 | 2,139 | 1,973 | Seven pairs, two mistakes: 28 + 10 = 38 |
| 91 | 1,734 | 1,568 | One pair, three mistakes: 4 |

The Rust integration fixture checks phase, occupied cells, selection cursor, mistakes, matched pairs, rings and the cartridge RNG state on 7,283 frames. It compares final win/loss and the local ring result. Regenerate with python tests/native_memory_round_oracle.py MULTIBOOT IGNORED_TRACE and run cargo test --test source_native_garden_round -- --ignored with A2NE_ROM and A2NE_MEMORY_TRACE pointing to the corresponding local files. Trace SHA-256: 3343e233f861bdac182662ae7a17cd4bf5f464f34bdeb73a4a8dc65f273b7546. The gameplay port begins after the instructions; the initial instructions, pause menu and hardware presentation are not ported by this module.
