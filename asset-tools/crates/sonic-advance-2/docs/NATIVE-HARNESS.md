# Run the initial native Rust game slice locally

`a2ne-native` connects the verified cartridge's Leaf Forest Act 1 terrain,
original Sonic sprite scripts/pixels, native fixed-point movement, directional
terrain probes and collision responses. Rust executes every simulation frame.
The browser binds physical keys and displays a Rust-produced 240x160 RGBA frame.
It contains no movement, collision, animation or reward simulation.

This is an incomplete game port. The runner supports left/right movement,
jumping with variable height, normal-gravity floor landing, braking and turning.
Other input bits are rejected before mutation. A second jump edge in midair,
wall/ceiling attachments, water/object states, unintegrated character states and
stage-edge/death callbacks stop the session with their specific limitation.
Reset creates a fresh in-memory session. No save or MMO service is read/written.

From a checkout with Rust available and the exact owner-supplied ROM local:

```powershell
$crate = 'asset-tools/crates/sonic-advance-2/Cargo.toml'
$rom = (Resolve-Path '.local/gba/A2NE-f2b7a3329401.gba').Path
cargo run --locked --release --manifest-path $crate --bin a2ne-native -- serve $rom
```

Open the printed loopback URL, select Run, then use left/right and Z or Space
for native A/jump. Pause and Reset are inspection-host controls. Optional final
port argument `0` selects an available ephemeral port. The default is 42872.
Only `127.0.0.1` is bound. This binary is a separate local research runner, not
the canonical Chao Garden service, an MMO protocol or a runtime rollout.

For deterministic execution without a browser:

```powershell
cargo run --locked --manifest-path $crate --bin a2ne-native -- replay $rom `
  asset-tools/crates/sonic-advance-2/examples/leaf-forest-inputs.json
```

Replay JSON contains only `segments`, each with an integer `keys` word and a
positive `frames` count. It accepts at most 4,096 segments/36,000 frames and
256 KiB of input. Keys are native left=32, right=16, A=1. Output is metadata,
state transitions and the final pixel SHA-256. The program never writes an
image, ROM, save, extracted resource or other local asset. The browser receives
composed viewport pixels over loopback; no source-file route is exposed.

## Native execution and deliberate host bindings

The native physics update precedes player input and callbacks. A new jump
request selects the next frame's InitJump callback. InitJump applies its
impulse and executes airborne physics in that same frame. A one-frame tap
therefore releases/caps the impulse before that frame's gravity and position
update. Original jump animation completion changes variant 0 to 1 and collision
offsets to 6/9; falling near the floor selects variant 2 through the native
extended probe. Ground shape restoration uses the original sprite-offset
correction. Native walk variants and signed-shift animation rates are retained.

The launch skips the retail intro/countdown and explicitly releases input.
Ring count remains zero; collectibles, enemies, objects, stage events, combat,
death and progression are not integrated. Native input buffer/water/action
dispatch outside the documented domain and the complete task graph remain
unfinished. The runner's small callback dispatcher is not full retail loop
parity. Its frame counter counts fixed simulation frames, independent of the
wall clock. Browser request pacing targets 60 Hz for local inspection; it is
not trusted time or progression. One host process has one in-memory session.

The camera follows the player using a host clamp. Terrain renders both original
foreground maps and original palette-zero clear color. The compositor then
places original non-affine sprite pixels using native dimensions/origin and
translation. Standing and jumping sprites flip horizontally when facing right
and shift one pixel right when facing left. In the pinned
`src/game/shared/stage/player.c`, lines 6087–6109, the walking sprite uses an
affine matrix whose rotation is the player's rotation shifted left two bits.
The host draws that sprite upright. Native slope rotation, camera modes/parallax,
complete BG/OBJ priority, transient effects and UI are pending. No substitute
sprite, handmade effect or replacement art is generated.

Native jump/brake/boost requests and animation sound IDs are exposed as events;
the runner has no audio sequencer/mixer or particle renderer. The events are
not evidence of audible or visual effect parity.

## Verification boundary

The source tests compare 4,096 terrain-query sets, 8,192 player primitive cases
and 8,192 collision cases against compiled pinned original C, with separate
input/math/RNG/scheduler checks. Original sprite images have independent pixel
checks. See [runtime/coverage](NATIVE-PORT.md), [player](NATIVE-PLAYER.md) and
[animation](NATIVE-ANIMATION.md) for fingerprints, reproduction and limitations.
Those comparisons establish the routines exercised, not a retail playthrough.

`source_native_jump_order` checks the original same-frame release/impulse
ordering using the verified stage. `source_native_host` launches an ephemeral
host and verifies exact reset state/pixels, one-frame input advancement and
unchanged state after an unsupported input. The replay integration checks
movement, jump variants/landing, turning and deterministic final state/pixels.
A fresh Chrome profile drew the initial canvas, accepted rightward input and
returned to frame zero paused after Reset.

Normal Rust tests need no ROM or C compiler. Source-backed tests are opt-in:

```powershell
$env:A2NE_ROM = $rom
cargo test --locked --manifest-path $crate --test source_native_animation `
  --test source_native_stage --test source_native_player --test source_native_jump_order `
  --test source_native_game --test source_native_host -- --ignored
```

The player test uses the checked-in digest from the original C oracle.
Runtime and collision source tests also require locally generated oracle
files; their guides provide exact generation steps.
All originals, generated executables, oracle vectors, browser profiles and
visual proof remain ignored/local. Public files contain Rust, the bounded host
adapter, independent test harnesses, metadata/fingerprints and documentation.
