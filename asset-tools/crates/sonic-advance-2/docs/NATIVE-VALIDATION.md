# Native Rust execution validation

Validated locally on 2026-11-18 with Rust 1.100 and the existing locked
dependencies. Ordinary and all owner-supplied ROM/oracle tests were run together. Strict Clippy for all targets, rustfmt and Git whitespace checks passed.
No production service, live preview or real save was changed.

| Check | Executed evidence | Public fingerprint record |
| --- | --- | --- |
| Runtime | Compiled pinned original C: 72 input frames, 192 outputs from each RNG, seven fixed-point cases and 16 task callback events | `catalog/native-runtime-parity.json` |
| Player primitives | 8,192 cases over 16 primitive boundaries, 30 words/case, original C and ROM sine table | `catalog/native-player-parity.json` |
| Terrain | 4,096 query sets: vertical/horizontal samples, short probes and extended probes with original C; ten ROM ranges checked, six matching pinned native Git blobs | `catalog/native-stage-parity.json`, `catalog/native-stage-resources.json` |
| Collision | 32,768 original-C cases covering all four attachment quadrants, 18 words/case, no unsupported cases in the selected domain | `catalog/native-collision-parity.json` |
| Animation | Three independently decoded original PNGs match normalized five-bit RGBA; 50 native animation/variant combinations across five characters execute for 9,000 updates | `catalog/native-sprite-parity.json` |
| Course maps | 31 slots/150 named banks matched pinned source; animated tile and edge probes; 8 full Leaf Forest scenes compared to an independent raw tile reader | `catalog/native-course-terrain.json` |
| Camera, affine and Sonic | 4,096 camera cases, 4,096 native Thumb transform cases, 360 walker pixel cases, 4,096 Sonic action cases, 8,192 input-history frames | [Camera](NATIVE-CAMERA.md), [affine](NATIVE-AFFINE.md), [Sonic](NATIVE-SONIC.md) |
| Objects and stage lifecycle | Native ring layouts and overlaps; real layer triggers; 19,208 spring contacts/4,096 launches; visible/silent checkpoint behavior and 12,288 original-C lifecycle cases | [Rings](NATIVE-RINGS.md), [entities](NATIVE-ENTITIES.md), [springs](NATIVE-SPRINGS.md), [checkpoints](NATIVE-CHECKPOINTS.md), [lifecycle](NATIVE-LIFECYCLE.md) |
| HUD | Seven independent direct-OAM full-frame pixel hashes using original cartridge graphics and palette banks: clock, zero/overflow rings, lives, score, speed and time warning | `catalog/native-hud-parity.json`, `tests/native_hud_oracle.py` |
| Campaign save | 256 native profile cases, 1,024 header selections and 64 full in-memory session comparisons; CLI leaves input unchanged | [Campaign save](NATIVE-CAMPAIGN-SAVE.md) |
| Combat | 392 authored enemy records; original-C Mon/Buzzer callbacks, 4,096 damage cases, 201,243 targeting cases, 6,144 initializers and 266,240 unobstructed callback frames | [Enemies](NATIVE-ENEMIES.md), [combat](NATIVE-COMBAT.md) |
| Scattered rings | 880 original task ticks and 28,160 slot snapshots; actual stage checks for collision, timers, pool bounds and sprite frames | [Scatter](NATIVE-SCATTER.md) |
| Goals and results | 4,096 goal/score cases, 512 bonus cases, 16,384 player controller cases and 83,200 results frames compared to pinned C; 8,253 UI submissions and 258 fade samples. Controlled-state integration from actual finish coordinates in all 14 acts checks victory animation, course clock freeze, tally, native flash readback, and next act/boss selection | [Goal](NATIVE-GOAL.md), [results presentation](NATIVE-RESULTS-UI.md) |
| Audio | 34 cues, including all 14 act themes, compared over 34,507 note events; one-shot and loop boundaries from original sequences; waveform fidelity remains unproven | [Audio](NATIVE-AUDIO.md) |
| Integrated movement | 14 normal acts run for 1,800 right-input frames each, including real spring and pit/retry routes. The 227-frame replay collects three rings and exercises walking, jump variants 0/1/2, landing, braking, turning and idle; fixed state/pixel regression plus tapped/held impulse-frame checks | `catalog/native-harness-replay.json` |
| Host | Ephemeral loopback session rejects unsupported inputs without ticking; WAV read returns stereo samples without ticking; reset restores exact state and pixels | `tests/source_native_host.rs` |
| Browser | Fresh isolated headless Chrome: Music Plant act 2 auditions cue 18 through Web Audio while paused; right-input movement and reset to frame zero on the same act; no runtime exceptions | Local browser observation; no published asset/screenshot |
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
$env:A2NE_AFFINE_ORACLE = '<local transforms.bin>'
$env:A2NE_CAMPAIGN_SAVE_ORACLE = '<local native-campaign-save-oracle directory>'
$env:A2NE_GOAL_ORACLE = '<local native-goal-oracle directory>'
$env:A2NE_GOAL_PLAYER_ORACLE = '<local native-goal-player-oracle/results.bin>'
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
