# Native Sonic airborne action boundary

`native_sonic.rs` adds Sonic's no-target airborne A action, buffered Forward Thrust initialization, and native Sonic input history. Collision, animation execution, sound playback, effects and complete callback scheduling remain host services. These operations do not grant MMO progression or alter a save.

## Source and ROM evidence

The reference is SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. `catalog/native-sonic-parity.json` records exact raw Git blob identities, byte lengths and SHA-256 hashes for the seven C/header inputs. The important action source is `src/game/sa2/stage/player_callbacks.c` (57,918 bytes; Git blob `43fccd46371979251beeea2ca0f73886d0a17fac`; SHA-256 `a486de9f4c2bbf8b6c3f74fbe45325a65f386f099ac21f716e58a72963928038`). Input history comes from `src/game/sa2/stage/input_buffer.c` (6,141 bytes; Git blob `08b5f240283c64b6b085b48389686f7dcfa54cc6`; SHA-256 `33b4fe55d431f24ee2e385a126cdd5dfa59095f0ff946a4311f05babc6e1896c`).

`SonicInputBuffer::from_rom` follows Sonic's pointer at ROM `0xC871D4`, through table `0xC87198`, to two 14-byte native patterns at `0xD5254` and `0xD5262`, ending with native pointer sentinel `0xFFFFFFFF`. The actual ROM patterns match the pinned C array bytes. Their SHA-256 hashes are `b66532f76f8e540e15338c39417cd1f05f3064e054fa7721cb965a8cf53a78eb` and `0355164edbc0c8d8c0350c44d1ea0a3e9bb26905922a3f436524d71f6c8320c4`. Data is loaded from the fingerprinted local A2NE ROM; the public implementation contains no extracted pattern data.

## Native actions

An A edge during an eligible Sonic jump with no homing target sets `SOME_ATTACK` (`0x20000000`) and character state 18, requests `CreateSonicAmySkidAttackEffect(..., 1)` and sound 113. The original `Player_TryMidAirAction` deliberately returns FALSE through this branch, so the current jump callback continues, including jump-release handling, gravity, position and collision. Repeated A edges do not create another shield while the attack bit remains set. Character state 18 maps to Sonic animation 17, initial variant zero; character state and animation ID are different namespaces.

Forward Thrust is separate from A. The native input buffer recognizes a short right/right or left/left directional edge sequence while `unk70` permits combos, setting `unk71` to one. The Jumping/Uncurl callback checks `Player_Sonic_TryForwardThrust` before the ordinary A/B action dispatcher. It can therefore take precedence over A and the existing shield bit.

Forward Thrust first resolves facing from held directions when `FLIP_WITH_MOVE_DIR` is set; simultaneous left/right ends facing right. It performs the native transition cancellation, enters the air, clears spin attack, changes offsets to 6/14 through the native rotated position adjustment, selects character state 80, and adds signed 576 (`Q(2.25)`) to existing horizontal velocity according to facing. The signed 16-bit result wraps; it is not a fixed replacement speed. It zeros vertical speed, rotation and the represented transition fields, clears animation completion, requests sound 112 and immediately calls `Player_Uncurl`. State 80 maps to Sonic animation 19, initial variant zero. A preexisting native flag `0x20000` additionally requests stopping sound 281.

The module preserves the native Uncurl air-input condition: `unk6E != 1 || airSpeedY > 0`. Uncurl does not perform Jumping's jump-release cap. The added `PlayerState.unk6e` and `Callback::Uncurl` represent these explicit native states; they are not save-layout fields.

## Host hooks

1. Construct one `SonicInputBuffer` from the verified ROM for the native engine session. After `PlayerState::handle_input`, call `handle_frame` before water/transition/callback processing, matching `Task_PlayerMain`.
2. In eligible normal-stage Jumping, call `try_no_target_air_action` before jump release. The host must establish that no native homing target exists and separately handle/reject unsupported attack-button actions. The API does not invent target data or silently replace a homing attack with a shield.
3. For `AirAction::InstaShield`, emit sound/effect requests and continue the current callback. `returns_early()` is false.
4. For `ForwardThrust`, emit the requested sound/optional sound stop, select Uncurl and execute its body immediately. `returns_early()` is true. Do not also finish the interrupted Jumping body.
5. Uncurl runs native presentation-state adjustment first; when `uncurl_accepts_air_input` is true, it applies air input, then checks special actions if the native jump-state bit is present. It then runs drag, native bounds handling, gravity, position, rotation and landing probes. Forward Thrust's cancellation removes the jump-state bit, so it cannot repeat itself on every frame.
6. Feed states 18/80 into their actual animation scripts. An embedded sprite animation change does not itself change the player's character state. Preserve native first-frame/termination behavior and landing dispatch.

The input buffer keeps the four-entry input ring, 32 saturating duration records, input/dead-state suppression, reverse pattern traversal and native decrement/acceptance order. `reset_counters` mirrors `InitNewInputCounters`: it clears only the first 31 records, preserves the final record and four-frame ring, and resets the current counter index. The reader bounds patterns to 1–16 steps and at most eight pointers; another character is rejected before mutation. A generic buffer for the other characters has not been ported.

## Original-C parity and reproduction

`tests/native_sonic_oracle.py` checks every reference's raw Git blob identity, verifies the two pattern byte ranges against the actual ROM, then compiles the original transition/offset helper, Sonic action routines, full midair dispatcher and input-buffer handlers. Other-character branches and homing-target calls are inert and excluded by the case generator. Native Uncurl's call is an inert boundary reporting its invocation and air-input predicate; this is not complete Uncurl physics or whole-frame parity.

The recorded action boundary has 4,096 cases × 22 signed words, 360,448 bytes, SHA-256 `4ae268cfa4cba77bc54911a5f21afb94748a423d14ec9925ae59484e65a5a2fb`. Cases include full signed speed ranges, rotated offsets, facing combinations, gravity, existing attack flags and buffered-action priority. The input boundary has 8,192 frames × 71 bytes, 581,632 bytes, SHA-256 `555122d5346fb35dcd208ce89d4b1ac305b35dc105485740bbd7b71cf6ae3f0a`. It includes successful actual-ROM combos, random histories, ignored/dead input and partial counter resets. Five focused unit tests cover the named action/input edge cases and malformed bounds.

From the isolated worktree root:

```powershell
python asset-tools/crates/sonic-advance-2/tests/native_sonic_oracle.py `
  --references ../../research/sat-r-native `
  --rom ../../gba/A2NE-f2b7a3329401.gba `
  --zig ../../tools/zig/zig-x86_64-windows-0.17.1/zig.exe `
  --local ../../a2ne-native-sonic-oracle `
  --report asset-tools/crates/sonic-advance-2/catalog/native-sonic-parity.json
$env:A2NE_ROM = (Resolve-Path '../../gba/A2NE-f2b7a3329401.gba').Path
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --lib native_sonic
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --test source_native_sonic -- --include-ignored
```

Original effect-task rendering, homing target acquisition/attacks, airborne B, grounded B/spindash, other characters and their side effects remain separate work. Native sound/effect requests identify required original assets; they do not prove an effect was rendered or audio was synthesized. The reference licensing and upstream contribution limits described in `NATIVE-PLAYER.md` also apply. Original source/generated C/executables, ROM and native data stay under ignored local paths.
