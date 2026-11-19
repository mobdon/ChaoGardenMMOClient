# Native normal-stage life cycle

`native_lifecycle.rs` implements normal, pre-goal, single-player camera bounds, the stage clock, native death setup/movement/wait and life-loss decisions. It reads actual checkpoint destinations from the verified A2NE ROM. Complete stage task recreation, intro/game-over screens, background/audio state, boss/goal region shifts, time-attack modes and multiplayer remain explicit host boundaries.

## Pinned source and data

The reference is SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Exact raw Git blob identities, lengths and SHA-256 hashes are recorded in `catalog/native-lifecycle-parity.json` for player/stage code, native constants, spawn positions and checkpoint interactable source. The stage source is 41,283 bytes, Git blob `e42773050528c39ecf3493784ddf0957f25deee4`, SHA-256 `3b0291d8b44e32b95ea4cc2520aae2929295185a7faefd5fa25e27776dd9a566`.

The original `gCheckpointPositions` table matches the unique actual ROM range `0xD63FC..0xD650C`: 34 pairs of signed 32-bit coordinates, 272 bytes, SHA-256 `09271b8e951e3761d544beaff8581ba547cb0e31922919a3554694b96820dd3b`. `checkpoint_position` reads course-act entries and checks that they fit the player's signed 16-bit checkpoint fields. No coordinate table is embedded in the public Rust source.

## Bounds and death ordering

Call `normal_bounds` at the original `sub_80232D0` boundary before position advancement. It uses the camera's current min/max limits, including narrowed limits supplied by native entities/scripts. The player center is clamped to the inclusive Q24.8 interval `[Q(min), Q(max)-1]`. Horizontal clipping zeros horizontal and ground velocity; it does not kill the player. Vertical clipping on the safe side zeros vertical and ground velocity.

Normal gravity kills at `worldY >= Q(camera.maxY)-1`; inverted gravity kills at `worldY <= Q(camera.minY)`. The death boundary sets the native dead flag and upward impulse (`-1248`, or `-672` in water) and puts the center at that boundary. The special native flag `0x80000000` suppresses death and permits clipping even alongside the dead flag. An already-dead player without that flag bypasses this bounds operation. Unsupported goal-region shifts and malformed/unrepresentable bounds fail explicitly.

At the **end** of `Task_PlayerMain`, after this frame's animation/presentation pass, `finish_player_main` enters the dying task. It selects character state 22 (animation 29), zeros horizontal air speed, changes vertical speed below `-1024` to `-512`, or positive vertical speed to zero, sets invulnerability to two, clears represented item/standing flags, locks both camera axes, sets the timer-stop/act-start flag and selects sprite priority one with unit scale. It returns sound 119 (life lost) or 157 (drowned). Background-music restoration/stops, object-pointer clearing and unrepresented full-player fields are separate host work.

On each later dying-task tick, the native offscreen test occurs **before** gravity and position. Under normal gravity, the raw threshold is `Q(camera.y) + Q(240)-1`; inverted gravity uses `Q(camera.y-80)`. If the player has not left the screen, native gravity and position run, then the host updates death animation. Once outside, the hidden bit is set and a 60-count wait starts. Waiting ticks perform no player animation. Sixty calls decrement 60 to zero; the next invocation resolves the death. These are distinct native task invocations.

## Clock, lives and restart

The native stage task runs at priority `0xFF00`, after the camera, checkpoint/entity and player tasks. `tick_stage_clock` increments stage time even while the act-start flag pauses the course timer. An active timer advances checkpoint time and course time by one. Normal time expires only above 36,000 frames, not at equality. Reversed time uses native unsigned subtraction followed by a signed-positive check. Expiry sets act-start and, unless native time-limit disabling suppresses death, clears represented item effects, supplies the native death impulse, marks the player dead and requests sound 149. Because the timer runs after player main, its newly set dead flag is processed by the next player-main invocation.

After the offscreen wait, ordinary `HandleLifeLost` clears rings and special rings, decrements the native unsigned-byte lives count once, then requests stage recreation if lives remain or a zero-lives game-over screen otherwise. A timeout follows `HandleDeath`: it decrements lives and requests a **time-up game-over screen even when lives remain**, or the zero-lives screen when none remain. The ordinary ring-clearing branch is not executed for timeout. `DeathTick` exposes these decisions; the host must perform the correct ring clear and stage/screen transition. Ring one-up rules remain in `native_rings.rs`; synchronize the same lives byte.

`reinitialize_player` is permitted only after `RestartStage`. It resets the represented player state at the preserved checkpoint and reconstructs the native camera, including its first-task arming delay. `InitializePlayer` restores checkpoint/course time from the saved checkpoint time, except that pre-restart course time at or above 36,000 clears the checkpoint time first. `CreateGameStage` preserves global stage time, checkpoint coordinates and checkpoint progress; a new `GameStageStart` is a different operation. Restart restores native input lock and act-start/intro flags. The movement harness must label any immediate intro bypass explicitly; returning a new player/camera does not execute original task teardown, entity/ring restoration, intro or game-over UI.

## Checkpoint activation

The actual checkpoint entity task runs at priority `0x2010`, before player `0x3000`. Once active and onscreen, its native activation checks that the player is neither dead nor under interactable override and has crossed the entity's X coordinate. There is no Y-overlap test. It saves the stage's table destination and current global checkpoint time, increments the checkpoint/boss index from zero and selects the native activated visual/sound. A full entity adapter should apply its validated event to `checkpoint`/`checkpoint_reached`; `cross_checkpoint` is a small optional guard for a host owning that activation boundary. It must never create a checkpoint without a native map entity.

## Original-C parity

`tests/native_lifecycle_oracle.py` rejects any raw Git blob mismatch, verifies the checkpoint table against the supplied ROM, and compiles the original normal bounds function, original single-player timer tail, original scalar player-death setup, `DeadPlayerLeftScreen`, `Task_PlayerDied`, `Task_PlayerHandleDeath`, `HandleLifeLost` and the SA2 `HandleDeath`. Display/audio calls and real task destruction/recreation are inert host boundaries. Death cases execute complete movement/wait/resolution sequences with a bounded 512-tick ceiling; no timeout is accepted as success.

Each group has 4,096 cases × 23 signed state words (376,832 bytes):

| Group | SHA-256 |
| --- | --- |
| Normal bounds | `953fab60f377fe761bf00b57992c15743799338af9f9af1696e918d854591475` |
| Stage clock | `4547023c206082e0a82701560296bbcf354d37306f17204f10a1f2cbd108059b` |
| Death through life-loss resolution | `6ff0ce19d94c2458da2ee6973c376b002b9f87aad33507ac8fa7f943f194fc60` |

All three Rust fingerprints and the opt-in actual-ROM checkpoint test pass. Four focused unit tests cover narrowed bounds, no mutation on malformed limits, exact wait timing, ordinary checkpoint restart and strict timeout/game-over behavior. This proves the represented boundaries; it is not full game-frame, stage recreation or screen/audio parity.

From the isolated worktree root:

```powershell
python asset-tools/crates/sonic-advance-2/tests/native_lifecycle_oracle.py `
  --references ../../research/sat-r-native `
  --rom ../../gba/A2NE-f2b7a3329401.gba `
  --zig ../../tools/zig/zig-x86_64-windows-0.17.1/zig.exe `
  --local ../../a2ne-native-lifecycle-oracle `
  --report asset-tools/crates/sonic-advance-2/catalog/native-lifecycle-parity.json
$env:A2NE_ROM = (Resolve-Path '../../gba/A2NE-f2b7a3329401.gba').Path
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --lib native_lifecycle
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --test source_native_lifecycle -- --include-ignored
```

The shared reference licensing and upstream contribution limits in `NATIVE-PLAYER.md` apply. Original C, generated C/executables, ROM, saves and assets stay ignored/local. No live service, canonical save or browser preview is replaced by these local native-engine checks.
