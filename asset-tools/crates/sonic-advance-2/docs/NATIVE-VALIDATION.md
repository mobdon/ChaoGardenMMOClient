# Native Rust execution validation

Validated locally on 2026-11-18 with Rust 1.100 and the existing locked
dependencies. All 64 Rust tests passed: 51 ordinary tests and 13 opt-in source
tests. Strict Clippy for all targets, rustfmt and Git whitespace checks passed.
No production service, live preview or real save was changed.

| Check | Executed evidence | Public fingerprint record |
| --- | --- | --- |
| Runtime | Compiled pinned original C: 72 input frames, 192 outputs from each RNG, seven fixed-point cases and 16 task callback events | `catalog/native-runtime-parity.json` |
| Player primitives | 8,192 cases over 16 primitive boundaries, 30 words/case, original C and ROM sine table | `catalog/native-player-parity.json` |
| Terrain | 4,096 query sets: vertical/horizontal samples, short probes and extended probes with original C; ten ROM ranges checked, six matching pinned native Git blobs | `catalog/native-stage-parity.json`, `catalog/native-stage-resources.json` |
| Collision | 8,192 original-C cases, 18 words/case, no unsupported cases in the selected domain | `catalog/native-collision-parity.json` |
| Animation | Three independently decoded original PNGs match normalized five-bit RGBA; 50 native animation/variant combinations across five characters execute for 9,000 updates | `catalog/native-sprite-parity.json` |
| Integrated movement | 227 frames exercise walking, jump variants 0/1/2, landing, braking, turning and idle; fixed state/pixel regression plus tapped/held impulse-frame checks | `catalog/native-harness-replay.json` |
| Host | Ephemeral loopback session rejects unsupported inputs without ticking; reset restores exact original state and pixels | `tests/source_native_host.rs` |
| Browser | Fresh isolated headless Chrome: nonblank canvas, enabled controls, native right-input movement and reset while running returning to paused frame zero | Local browser observation; no published asset/screenshot |
| Existing assets/saves/transfer | Original independent source hashes, typed re-encoding, three map renders, garden templates/CRC and bounded synthetic save/transfer inspection still pass | Existing asset parity metadata and `docs/VALIDATION.md` |

The integrated replay is an observed Rust regression, not an independent oracle
for the entire original game loop. Player primitive oracles stub their named
host boundaries; collision oracles use original terrain with the documented
normal-gravity domain. Broad animation execution is not independent pixel
parity for all 50 combinations. The guides record each boundary precisely.

Full source-backed suite, after generating the local oracles through
[runtime/collision](NATIVE-PORT.md), [player](NATIVE-PLAYER.md) and
[sprite](NATIVE-ANIMATION.md) instructions:

```powershell
$env:A2NE_ROM = '<exact local A2NE ROM>'
$env:A2NE_RUNTIME_ORACLE = '<local runtime-oracle.json>'
$env:A2NE_COLLISION_ORACLE = '<local collision results.bin>'
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --all-targets -- --include-ignored
cargo clippy --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --check
```

Terrain oracle reproduction additionally uses `A2NE_TERRAIN_C` pointing to the
exact pinned `src/game/shared/stage/terrain_collision.c` Git blob
`a62362ae875a5f54eb6a1c08343967e93071a210`, `A2NE_ZIG` pointing to local Zig,
and `A2NE_ROM`, then runs `python tests/native_stage_oracle.py` from this crate.
The helper checks the source identity and publishes fingerprints only. A local
copy with an extra trailing newline fails that identity check.

Only this crate's Rust/host source, independent verification harnesses,
documentation and metadata are proposed for GitHub. ROMs, extracted images,
original C, generated C/executables, oracle vectors, browser profiles and visual
proof remain ignored/local. The [coverage matrix](NATIVE-PORT.md#full-native-game-coverage-boundary)
lists the substantial work still required for a full retail-game port.
