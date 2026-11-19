# Native A2NE player primitive boundary

`native_player.rs` executes native player integer behavior in Rust. It is a partial native game-code port, separate from the existing asset readers and the MMO's custom authoritative movement. It does not execute ARM instructions or delegate gameplay to an emulator.

Reference: SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, especially `src/game/shared/stage/player.c`, `include/constants/sa2/move_states.h`, `include/constants/sa2/char_states.h`, `include/game/shared/parameters/characters.h` and `include/trig.h`. Game variants are checked explicitly: the similarly named SA1 jump function is not the A2NE jump implementation.

## Executable behavior

The module represents character identity, signed Q24.8 world positions, signed Q8.8 air/ground velocities, move flags, rotation, held/new inputs, physics, selected callback and the movement fields needed by collision. `new_at` uses the represented native initialization fields with default A/B/R controls; a native stage-start host releases the initial input lock. It does not initialize the entire retail player object or save record.

Native operations include input edges/locking; ring-dependent boost threshold, boost state, acceleration and speed limits; ground acceleration, braking and friction; air input preserving momentum above top speed; the original unsigned-Y-conditioned air drag; gravity, fall cap and inverted-gravity position arithmetic; slope acceleration and ground-to-air velocity conversion; signed rotation relaxation; delayed jump callback selection, jump initialization, variable jump height; and sprite-offset position adjustment.

The source uses 8 fractional bits. Normal/water gravity is 42/12 raw units; the alternate rising-gravity operation halves those values. Jump force is 1248/672, release caps are -768/-384, and downward fall speed is capped at 3840. Ring thresholds select acceleration 8/12/14/16/18 with deceleration 64. Rust explicitly wraps the original integer storage operations rather than relying on build-mode overflow behavior. Sine/cosine values are supplied from the verified native Q2.14 table shifted right by six; no floating-point trigonometry replaces that table.

## Host ordering and limits

Native `Task_PlayerMain` updates boost threshold → boost state → physics → walk selection → input handling → input-buffer/water/transition handling → the selected player callback → presentation updates and timers. The host must preserve that ordering when integrating more of the original loop.

For `Player_Idle`, the original sequence tries actions/jump first; otherwise ground input and the wall probe run, followed by slope acceleration, camera/boundary handling, ground-to-air conversion, position advancement, ground contact probes and the slow-slope/lock transition. `ground_input` exposes the scalar part before its required wall probe; it does not silently replace the probe with a flat floor.

For a jump, `Player_TryJump` requires a new jump-button edge and directional clearance greater than three, then selects `Player_InitJump` for the next callback execution. Initialization adds the native rotation-relative impulse and enters `Player_Jumping`. That callback checks special mid-air actions and jump release; performs presentation-state changes; applies air input, drag, camera bounds, gravity and position; relaxes rotation; then handles terrain/object landing. The primitives do not claim that every one of those host services is implemented.

Rolling, spindash, special character actions, attacks, homing targets, water lifecycle, camera transitions, bosses, scripted interactables, object standing, death/results, multiplayer and all reward/save effects remain separate work. An enum value for a callback is not an implementation of its complete body. Unsupported contexts must stay explicit in the host.

## Native presentation relationship

The shared-state table maps Sonic Idle/Walk/Jump-slow/Jump-fast/Brake states 0/9/10/11/7 to matching animation IDs and initial variant zero. Walk always uses animation 9 with variant `walk_anim`: absolute ground speed at most 320, 640, 1024, 2304 and 2560 selects variants 0 through 4; larger speeds select 5. Native walk animation speed is `abs((signed_speed >> 5) + (signed_speed >> 6))`, clamped to sprite rates 0.5 through 8.0. Signed shifts precede the absolute value.

Jump animations 10/11 start at variant zero. In `sub_80246DC`, animation completion selects variant one, sets spin attack and changes offsets to 6/9. While falling, variant one changes to two when the original extended floor probe is within 32 pixels. A complete host must supply that probe and animation completion signal; permanently replaying variant zero is not native jump presentation.

## Original-C parity

Five focused synthetic tests cover edge cases. The opt-in actual-ROM test additionally compares 8,192 cases across 16 native primitive boundaries against compiled original C and the actual ROM sine table. It compares 30 state/event words per case, totaling 983,040 bytes. `catalog/native-player-parity.json` contains the fingerprints and exact scope.

`tests/native_player_oracle.py` reads four local pinned reference files and rejects any Git blob identity mismatch. It compiles selected original C functions and macro bodies with the installed Zig C compiler, using typed host-boundary stubs. The ground wall probe, effects and next jump callback are inert in this oracle because the test compares the scalar primitive boundary. This is not whole-frame, full-stage or full-game C parity. Generated C, sine bytes and executables remain under `.local`; none is embedded in the public test source.

Example from the combined checkout (replace tool paths as needed):

```powershell
python asset-tools/crates/sonic-advance-2/tests/native_player_oracle.py `
  --references .local/research/sat-r-native `
  --rom .local/gba/A2NE-f2b7a3329401.gba `
  --zig .local/tools/zig/zig-x86_64-windows-0.17.1/zig.exe `
  --local .local/a2ne-native-player-oracle `
  --report asset-tools/crates/sonic-advance-2/catalog/native-player-parity.json
$env:A2NE_ROM = (Resolve-Path '.local/gba/A2NE-f2b7a3329401.gba').Path
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --test source_native_player -- --ignored
```

The ignored reference files use flattened names matching their pinned repository paths, for example `src_game_shared_stage_player.c`. Normal builds/tests do not fetch references or native data.

## Reference licensing and remaining inventory

The pinned repository tree contains licenses for particular tools and bundled external libraries, but no top-level or main-game-code license. Those tool-specific licenses are not evidence of a general game-code grant. The published Rust implementation records functional behavior independently; this test consumes user-local research inputs and publishes fingerprints only. No upstream C implementation, SDK code, original art, ROM, save or generated native executable is vendored. The upstream README's no-LLM contribution policy also means this work should not be submitted to that upstream project.

The reference's C player module alone is 10,225 lines/305,227 bytes and terrain collision is 978 lines, with further character callbacks, entities, stage logic, renderer, audio, saves and multiboot games. Those source sizes are inventory measures, not Rust port completion percentages. The upstream README reports a complete C build with approximately sixty nonmatching functions and over six hundred unnamed functions; that does not establish a complete Rust port. Remaining coverage must be tracked by executed native subsystems and parity boundaries rather than inferred from asset availability.
