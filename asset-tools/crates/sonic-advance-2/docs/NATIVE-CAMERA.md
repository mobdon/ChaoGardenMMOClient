# Native A2NE normal-stage camera

`native_camera.rs` implements the retail 240×160 normal-stage camera's integer state and follow behavior in Rust. It is one native execution boundary, alongside the player primitives and terrain readers. It is not a full stage renderer, full game port, or replacement for the MMO's camera and authority.

## Source identity

The reference is SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`:

| Reference | Identity |
| --- | --- |
| `src/game/shared/stage/camera.c` | 44,392 bytes; Git blob SHA-1 `499bdcdf4df6d8f5f779e499a5fdaff71268c5aa`; SHA-256 `feea71aeb35722c9977241d06e96e48482b1b217dd74d18fa8e52a630157c713` |
| `include/game/shared/stage/camera.h` | 5,606 bytes; Git blob SHA-1 `85c3075383ca2be6695a7f4d324e31e3c0fd8133` |

The implementation follows the normal-stage scalar initialization in `InitCamera`, the nonboss branch of `UpdateCamera`, and the two camera task entry points. Native background rendering, register writes, boss/extra-stage branches and multiplayer spectator selection are outside this boundary. Local reference files and generated oracle code/executables remain ignored under `.local`.

## Behavior and host order

`NativeCamera::normal_stage` initializes the native anchor to `max(playerPixelX - 120, 0)` / `max(playerPixelY - 84, 0)`. The stage bounds must come from the verified native collision header. Initialization does not perform an extra final-bound clamp that the native scalar initialization omits.

The native camera task runs at priority `0xF00`, before the player task at `0x3000`. Its first invocation only arms the update task; `tick_task` returns `updated: false` and preserves that delay. Subsequent invocations follow the player's state from before the current player update. A host that advances the player first or immediately follows on the initial task invocation changes native timing. `update` exposes the follow operation directly for source parity and callers that already manage task state.

The horizontal target uses a signed velocity filter `(airSpeedX + previousLead * 15) >> 4`, then adds `lead >> 5` pixels. Horizontal movement honors its deadzone and a Q24.8 step limit. Boost activation later in the player task calls `on_boost_activation`, setting that limit to raw `1024` (`Q(4)`); camera updates recover it by raw `32` per update toward `4096` (`Q(16)`).

The vertical target approaches `spriteOffsetY - 4` by five pixels per update, with the target offset negated under inverted gravity. The airborne vertical deadzone grows by four to 24 pixels and contracts by four to zero on the ground; the native Knuckles state-nine exception is retained. The initial vertical step limit is 12 pixels. Target delay, independent axis locks, shift/look offsets and supplied shake offsets retain their native roles. The camera does not produce those native script inputs itself.

The preliminary position clamp reserves one more edge pixel than the final clamp; both are retained. Shake is applied after final bounds. Native `dx` and `dy` are **previous position minus new position**, in whole pixels. They are not the player's Q24.8 displacement.

Malformed stage bounds, negative step/deadzone limits and vertical deadzones outside `0..=24` fail before a camera step mutates state. These errors are a Rust host boundary for invalid inputs, not a claim that the retail game validates arbitrary camera structures. Integer storage operations preserve native signed widths and explicit wrapping where needed.

## Original-C parity

`tests/native_camera_oracle.py` consumes the exact local C source bytes and rejects a mismatching raw Git blob identity. It extracts the original initialization scalar block and full `UpdateCamera` function, compiles them with the installed Zig C compiler, and emits state fingerprints. Render/background calls are inert host stubs, with boss/extra-stage selection excluded. No copied C function body or generated executable is published.

The checked report `catalog/native-camera-parity.json` records 4,096 deterministic cases with 23 signed state words per case: 241 initialization-only cases and 3,855 normal-stage updates. Cases vary stage dimensions, player position/velocity, airborne state, character, gravity, targets, delays, locks, look/shifts, step limits, deadzones, body offset and shake. The 376,832-byte result has SHA-256 `14074bfc0010b47a75f30fc709177f1eea0f59821d12759362bda5cfe64081d7`. The ordinary Rust integration test recomputes the same fingerprint without requiring a ROM or reference download.

Four focused unit tests additionally cover first-task timing/spawn anchor, signed lead/delayed targets, incremental airborne/boost recovery, and rejection before mutation. This evidence verifies the represented scalar boundary; it does not establish C parity of a complete game frame or rendering pipeline.

From the isolated worktree root, with the local research/tool paths adjusted as needed:

```powershell
python asset-tools/crates/sonic-advance-2/tests/native_camera_oracle.py `
  --source ../../research/sat-r-native/src_game_shared_stage_camera.c `
  --zig ../../tools/zig/zig-x86_64-windows-0.17.1/zig.exe `
  --local ../../a2ne-native-camera-oracle `
  --report asset-tools/crates/sonic-advance-2/catalog/native-camera-parity.json
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --lib native_camera
cargo test --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --test source_native_camera
cargo clippy --locked --manifest-path asset-tools/crates/sonic-advance-2/Cargo.toml --lib -- -D warnings
```

## Remaining native work

Boss and extra-stage cameras, multiplayer/dead-player spectator paths, water/background and parallax callbacks, camera script producers, display registers, tile rendering and their task lifetimes remain separate work. The stage host must explicitly choose the represented normal-stage mode. No browser service contract, progression, rewards, save mutation or runtime rollout is introduced here.

The pinned reference tree has no top-level or main-game-code license; licenses for bundled tools do not grant rights to the game source. This Rust module records functional behavior independently and publishes only test tooling and fingerprints. See `NATIVE-PLAYER.md` for the shared provenance and upstream-contribution limits.
