# US Tiny Garden rock–paper–scissors shooter

The US program uses three launch lanes and ten targets travelling around a rectangle. Each lane holds a hand. A winning shot knocks a target away for points, a tie clears the target without points, and a losing shot or missed target consumes a life. Clearing a wave adds ten seconds up to 99. The player starts with five lives and 30 seconds. Run the terminal game with `cargo run --example tiny_rps -- "$env:SONIC_ADVANCE_ASOE_ROM" 12345`. Type a/d to select a lane, f to fire, a period for one frame, or a decimal number to wait several frames. It reads only the local cartridge.

## Native source

Reference is the exact US ASOE revision 0 fingerprint recorded in binary.rs. The embedded Tiny program is decompressed at ROM offset 0x9BE78. All offsets below are relative to that program, loaded at GBA address 0x02000000. The shared RNG and surrounding game structure can also be found at SAT-R/sa2 commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba in chao_garden/src/game.c and chao_garden/asm/code.s. US instructions determine the behavior in this port.

* The active dispatcher at 0xB80C advances the background Chao's shared RNG use, target movement, all three launchers, fresh left/right/A presses, then the clock. Routines 0xA800 and 0xABE8 draw hands for targets and launchers. The collision half-width profile at 0x11F84 is read from the locally verified program.
* The original OBJ setup at 0xB6D6 loads its palette and tiles. The three descriptors at 0x11FB4 point to a closed fist, two extended fingers, and an open hand in that order. Collision dispatch at 0xAE30 independently confirms rock beats scissors, scissors beats paper, and paper beats rock.
* A shot climbs four pixels per frame. Targets travel two pixels per frame or four when one remains. Routine 0xA8EC retires a winning or tied target after the 30-frame hit counter. A win adds floor(wave_index/2) + 1 to the local counter, capped at 99. A tie adds zero. At 0xB14C, a losing shot spends a life after its animation. A shot missing all targets spends its life once it passes y = −32.
* The interactive phase ends at zero lives, or after time expires with hittable targets remaining. The result phase continues target updates before returning to the Garden. A separate original-code test initialized seven points, one pending wave-index-four winning hit and 99,994 local rings. The native result path took 81 frames with immediate input, reached ten points, and capped the synthetic in-memory ring total at 99,999. The Rust result calculation includes pending winning hits. The Rust port never writes a ring balance.

## Independent frame comparison

Six deterministic input streams executed the installed program's Thumb initializers and active dispatcher in Unicorn. The Rust integration test reconstructs those inputs and compares all 8,389 per-frame samples via six SHA-256 digests. Every sample includes the full RNG state, selected lane, lives, score, wave, timer, round status, all ten target states, counters, hands and path coordinates, and all three launcher states, counters, hands and active projectile heights.

| Seed | Frames | Ending | Local points at end of input |
| ---: | ---: | --- | ---: |
| 0 | 1,801 | Clock | 0 |
| 12,345 | 1,801 | Clock | 4 |
| 32 | 1,801 | Clock | 1 |
| 321 | 1,801 | Clock | 1 |
| 100 | 300 | Out of shots | 3 |
| 123 | 885 | Out of shots during second wave | 5 |

Run `cargo test --test tiny_rps -- --include-ignored` with the local cartridge environment variable set. The native comparison omits sound playback, sprite drawing and recoil vectors after collision; these have no influence on subsequent collision eligibility. Terminal output is a text representation, and pause menus and Garden service integration are outside this implementation.

One source edge is explicit. At 0xB2E6 the maximum random draw with certain candidate counts can index beyond the stack entries just written by the original launcher. If this occurs the Rust simulation reports UnresolvedReload. Resolving the stale stack byte requires additional call-stack evidence.
