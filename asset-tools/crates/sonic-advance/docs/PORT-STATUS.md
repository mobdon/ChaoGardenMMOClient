# Sonic Advance native Rust port status

User-directed continuation, November 18, 2026: implement the actual game in Rust
beyond asset extraction, and publish the source to GitHub. The whole-game port
remains in progress. This branch adds a runnable first-stage testbed using Rust
mechanics and original source-local presentation. It is not ROM instruction
execution, a complete retail engine, or a rollout into the MMO service.

The isolated branch is `codex/sonic-advance-native-runtime`, based on the exact
asset-tooling PR #4 head `c5ccc2533162c0ad34262dfd4438c3848713e97b`.
Changes remain under `asset-tools/crates/sonic-advance/`. Shared schemas,
other games' crates, server histories, live previews and saves are untouched.
Astra completed the runtime integration, export failure handling and tests.

## Implemented in this pass

- Source-gated loading of all 18 foreground/collision graphs, native signed
  height/angle profiles, palettes, tiles, backgrounds and foreground occlusion.
- A bounded native sprite animation state machine and normal OAM composition,
  including Sonic Idle/Walk/Run/Spin and the native ring animation/effect.
- Signed fixed-point SA1 dry movement constants, ground acceleration/braking,
  friction, slope contribution, air control, jump release and gravity, with an
  explicitly upright collision adapter. Spawn and sine samples come from ASOE.
- All 18 RLE ring layouts, live-body pickup rectangles and ephemeral collection.
- A Rust 240x160 framebuffer, optional native desktop window, keyboard adapter,
  deterministic replay and local-only trace/image/hash receipts.

Detailed evidence and limits: [stage](STAGE-EVIDENCE.md),
[sprite](SPRITE-EVIDENCE.md), [player](PLAYER-EVIDENCE.md),
[rings](RING-EVIDENCE.md), [runtime](RUNTIME-EVIDENCE.md).

## Run locally

Activate this project's existing Rust toolchain if necessary. From the isolated
worktree, set `SONIC_ADVANCE_ASOE_ROM` to the private supplied ROM. No ROM or
converted assets are downloaded by the game tool.

```powershell
cargo run --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --features native-window --bin sonic-advance-native -- play $env:SONIC_ADVANCE_ASOE_ROM
```

Controls: arrows or A/D to move, Space or Z to jump, P to pause, R to restart the
local practice run, Escape to close. Restart discards only that local run; there
is no profile/save writer. The window reports unsupported paths rather than
silently substituting another mechanic.

```powershell
cargo run --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --bin sonic-advance-native -- replay $env:SONIC_ADVANCE_ASOE_ROM .local 600
```

The output directory must already exist and resolve to a directory named
`.local`. Replay emits local PPM/JSON artifacts using derived names and rejects
existing files. These original-image artifacts remain ignored and private.

## Remaining whole-game gates

| Native subsystem | Current status and next required proof |
| --- | --- |
| Terrain and player traversal | Upright dry subset runs. Port all four collision sensors, full loops, layer triggers, rolling/spindash, water and character-specific states; compare retail frame traces. A parsed stage graph is not a completed playable stage. |
| Entities and stage progression | Rings are loaded/collected locally. Enemies, springs, platforms, hazards, checkpoints, bosses, goals, death/lives and stage/act transitions remain. |
| Scheduler, menus and effects | A bounded local 60 Hz runner exists. Original task order, title/options/character select, camera triggers, palette/tile animation and remaining graphics modes are unfinished. |
| Audio | Native animation sound events are represented. Port sequence/voice/sample traversal, synthesis/mixing and playback with audio parity. |
| Saving and transfers | No native save writer/import authority exists. Trace exact ASOE save lifecycle, corruption/recovery and JoyBus semantics before local compatibility work. |
| Tiny Garden and minigames | The embedded program is fingerprinted. Its full native art graph, behavior/care/shop/minigame logic and Chaos type fallback remain separate unported systems. |
| Other modes and characters | Multiplayer/link, special stages, remaining characters and all their abilities need independent execution and fidelity evidence. |
| MMO/WASM integration | No new network contract or service reward path is provided. Any future browser shared mechanics must use the agreed Rust/WASM boundary; canonical progression stays in the separate Rust authority. |

The current testbed can block on geometry requiring unported collision modes.
It does not complete an act or establish retail playthrough parity. Native
window creation/build and framebuffer output were checked; complete interactive
keyboard acceptance remains open. No merge, deployment or live migration is
authorized by this source publication.
