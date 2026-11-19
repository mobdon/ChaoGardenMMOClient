# Sonic Advance 2 native Rust port

The Rust native port has started with executable engine, player, sprite and
course terrain subsystems. The existing asset catalog remains a separate
foundation. This is not a completed retail game, a ROM emulator or an MMO
adaptation. No production server, canonical Chao, credentials, live save,
browser authority or device protocol is changed by these modules.

Native source facts are pinned to SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. The verified A2NE cartridge and
embedded garden hash gates still apply to every ROM-backed resource. New Rust
modules implement behavior independently; downloaded C/assembly, generated C
oracle sources, executables, original assets and runtime images stay ignored
under `.local`.

## Executable runtime API

`native_runtime.rs` is independent of ROM execution and the MMO service.

| API | Implemented native behavior | Evidence |
| --- | --- | --- |
| `InputState::default` | Zero input/counters; per-key first intervals 20 and continued intervals 8 | `src/core.c` engine initialization and `GetInput` |
| `poll_keyinput(register, playback)` | Active-low ten-bit hardware input, separate physical input, optional recorder word, pressed/released masks and ten repeat counters | `src/core.c` `GetInput`; `include/gba/io_reg.h` button masks |
| `poll_held(held)` / `frame()` | Host input binding and copyable `InputFrame` with `current`, `physical`, `pressed`, `released`, `repeated`, `counters` | Same native state transitions; host adapter supplies the hardware-equivalent word |
| `NativeRng::seeded(a,b)` / `next_u16()` | Two-word wrapping recurrence `previous + current + 0x37119371`, returning the upper halfword | `src/game/math.c` `Random` / `SeedRng` |
| `PseudoRng::seeded(state)` / `next_u32()` | Separate wrapping LCG multiplier `0x196225`, increment `0x3C6EF35F` | `include/core.h` `PSEUDO_RANDOM_32` |
| `q24_8_*` helpers | Native signed 32-bit integer conversion, arithmetic shift, fraction, wrapping multiply/negated multiply and division | `include/global.h` `Q_24_8`, `I`, `Q_MUL`, `Q_MUL_NEG`, `Q_DIV2` |
| `TaskScheduler<T>` | Native priority/list dispatch, equal-priority FIFO, parent capture, inactive/protected/flags-800 behavior, callback replacement, destructors, half-open priority destruction | `src/task.c`, `include/task.h` |

The input reader preserves the native first-poll detail: counters initially
contain zero, so holding a button on the very first poll starts its continued
interval immediately. An initial released frame sets the first interval.
Physical input is captured before demo playback replaces game input. Recorder
words retain all sixteen bits, while repeat testing covers the ten physical
keys. `previous` models the separate native `gPrevInput`; game-side edits to
`current` do not rewrite edge history. Recorder tape storage and record/playback
mode transitions remain separate work.

Both native RNGs are deterministic game algorithms. Neither provides secret
generation, credentials, server entropy or legitimate progression authority.
Fixed-point multiplication uses a wrapping 32-bit product before arithmetic
right shift, preserving ARM arithmetic rather than using floating point or an
unbounded intermediate. `q24_8_div` models the ordinary `Q_DIV2` arithmetic;
zero divisors return an explicit host error. BIOS exception behavior and the
signed minimum/-1 corner are not established as retail BIOS parity.

## Scheduler storage and execution boundary

Create a task with `create(priority, flags, data, main, destructor)`. Main and
destructor callbacks are typed function pointers receiving the scheduler and
`TaskId`. Use `data`/`data_mut`, `metadata`, `set_main`, `set_flags`, `destroy`,
`destroy_in_priority_range` and `execute(engine_flags, max_dispatches)`.

The native capacity is 128 slots including two sentinels, leaving 126 game
tasks. Lower priorities execute first; creation inserts after existing equal
priorities. The next task is captured before each callback. A task inserted
between the current callback and its captured next task waits until a later
frame; a task inserted farther ahead can run in the same frame. Destroying
the pending next task updates that pending pointer. Destructors execute before
unlinking and can create or destroy other tasks.

`TASK_INACTIVE` is 0x1, `TASK_DESTROY_DISABLED` is 0x2, and task flag 0x4 allows
execution during engine flag 0x800. That engine flag is retained by its exact
source name; it is not silently equated with every pause condition. Destruction
uses the native half-open interval `[lower, upper)`.

Typed, generation-checked host handles replace native raw IWRAM addresses.
Task data is caller-owned Rust data, with no claim of native heap addresses,
allocator fragmentation, physical memory clearing, DMA or unknown task debug
fields. The host rejects stale handles, recursive execution/destruction,
exhausted slots and unsupported priority 0xFFFF. Retail creation at the tail's
0xFFFF priority does not link a normal task; the port rejects that orphan case.
A caller-supplied dispatch budget stops runaway callbacks. These are explicit
host storage/error boundaries; their error paths are not retail behavior.
Callback panics and arbitrary reentrant destruction are outside the supported
native-equivalent callback domain.

Audio-main servicing between native task dispatches and hardware interrupt
ordering are not implemented by the pure scheduler. Its dispatch count includes
the native dummy sentinel callbacks when those callbacks are eligible.

## Actual C-source execution parity

`tests/native_runtime_oracle.py` reads six exact local reference files, checks
their Git blob hashes and extracts the original `GetInput`, `Random`,
`SeedRng`, arithmetic/LCG macros and task implementation. It writes those
originals only into the selected ignored output directory, compiles them with
local Zig C, executes them, and records an ignored JSON oracle. The public
helper contains the independent host test harness, not a copied native
implementation.

The oracle binds physical input, recorder words, task storage and audio calls
to test stubs. Task logic uses the reference's portable pointer/storage branch;
it does not prove GBA allocator or interrupt parity. C compilation uses explicit
32-bit types, `-fwrapv`, disabled strict aliasing and a little-endian host check
to preserve the selected ARM arithmetic observations. This C executable is a
test oracle, not the Rust game runtime.

From the original workstation checkout after loading its local toolchain:

```powershell
. ./scripts/use-rust.ps1
$crateRoot = '.local/worktrees/sonic-advance-2-native/asset-tools/crates/sonic-advance-2'
$python = 'C:/Users/Admin/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'
& $python "$crateRoot/tests/native_runtime_oracle.py" `
  --reference-dir .local/research/sat-r-runtime `
  --output-dir .local/gba/native-runtime-oracle `
  --zig .local/tools/zig/zig-x86_64-windows-0.17.1/zig.exe
$env:A2NE_RUNTIME_ORACLE = (Resolve-Path .local/gba/native-runtime-oracle/runtime-oracle.json).Path
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --lib native_runtime::
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --test native_runtime_source -- --ignored
```

For a clean checkout, use its crate-relative path and provide the local pinned
reference snapshot/compiler explicitly. Required flattened reference names and
Git blob hashes are recorded in the helper and
`catalog/native-runtime-parity.json`. The optional helper `--summary` argument
regenerates that public metadata-only summary. It contains counts and the
canonical JSON digest, with no source code or native data vectors.

Executed parity covers 72 input frames, 192 two-word RNG outputs, 192 LCG
outputs, seven signed fixed-point cases and sixteen task callback events.
The task scenarios include equal priorities, inactive/protected tasks,
flags-800 dispatch, priority-range destruction, creation during dispatch,
destroying the pending next task and self-destruction. Native task counts also
match. Three focused runtime unit tests cover additional limits, stale-handle
reuse, repeat timing and arithmetic boundaries. The ignored source test checks
the committed oracle digest before comparing every Rust result.

## Normal-gravity player collision

`native_collision.rs` consumes `TerrainProbes`; the ROM-backed `NativeStage`
implements that interface. Calls preserve native pixel sensors and Q24.8
position corrections rather than approximating terrain with drawn rectangles.

| API | Implemented native behavior | Source boundary |
| --- | --- | --- |
| `jump_clearance` | Two-sensor clearance at `rotation + 128`, including the native sensor-angle fields | `sub_8022F58`; the caller compares its distance with four pixels |
| `ground_wall` | Ground-speed-dependent wall probe, speed cancellation and correction before position advancement | `sub_802302C` / `sub_8023128` |
| `ground_follow` | All four attachment quadrants, native sensor order, retained rotation fields, penetration cutoff and speed-dependent follow limit after movement | `sub_8022D6C` dispatch and `sub_80228C0` / `sub_80229EC` / `sub_8022B18` / `sub_8022C44` |
| `air_collision` | Integer angle direction dispatch, both side sensors, ceiling/floor correction and native surface attachment/speed conversion | `sub_8022190` and its four branches |
| `native_arctan2` | Signed integer polynomial angle service | Pinned portable `libagbsyscall` reference; actual retail BIOS hardware parity is not claimed |

Landing clears the native in-air/spin/direction-flip flags, restores the native
sprite offsets, resets the defeat-score index and schedules the ground callback.
The later full `Player_TouchGround` callback is a separate lifecycle step: this
collision API does not itself execute the complete next-frame callback graph.
`CollisionReport` exposes correction deltas, blocked sides, ceiling contact,
landing and leaving-ground events for the host.

The supported context is normal gravity without water, object attachment or the
special native 0x200/0x800000 collision states. Floor, both walls and ceiling
attachment now use the original routines, including the signed rounding at
rotation-quadrant boundaries and the original upward steep-surface landing.
Callers must supply the normal-gravity context; this API has no inverted-gravity
parameter. Errors leave player state unchanged. Water-running and object-bound
motion remain separate unimplemented services.

After ground following, the caller must execute the native
`PLAYERFN_UPDATE_UNK2A` step: decrement a nonzero control-lock timer, or detach
from nonfloor rotations below absolute ground speed 480, set speed to zero and
lock controls for thirty frames. If in-air is then set, schedule `Player_Jumping`.
This callback ordering is distinct from terrain correction and is already
represented by the local runner. Full character movement/callback parity still
requires its own integration evidence.

The source oracle compiles the exact pinned player and terrain functions from
local references and probes the verified cartridge's actual Leaf Forest Act 1
height, rotation, metatile, map and flag bytes. Raw Git blob hashes are checked
before decoding, without source normalization. Original C, compiled binaries
and result vectors remain in the ignored output directory. Public metadata in
`catalog/native-collision-parity.json` records the three source hashes, seed,
case count, result size and SHA256.

```powershell
$env:A2NE_ROM = (Resolve-Path .local/gba/A2NE-f2b7a3329401.gba).Path
& $python "$crateRoot/tests/native_collision_oracle.py" `
  --rom $env:A2NE_ROM `
  --reference-dir .local/research/sat-r-runtime `
  --output-dir .local/gba/native-collision-oracle `
  --summary "$crateRoot/catalog/native-collision-parity.json" `
  --zig .local/tools/zig/zig-x86_64-windows-0.17.1/zig.exe
$env:A2NE_COLLISION_ORACLE = (Resolve-Path .local/gba/native-collision-oracle/results.bin).Path
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --lib native_collision::
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --test source_native_collision -- --ignored
```

All 32,768 seeded source cases match: 8,192 each for jump clearance, ground wall,
ground following and airborne collision. Ground inputs span all 256 rotation
values rather than a floor-only angle range. The Rust test verifies the local
2,359,296-byte oracle's committed SHA256
`e7ed7423c959eb1d4983bc6d18e5558ed06a98fc83a204f993db68b27412face`, then compares
eighteen words per case, including the independent integer angle result and
native state fields. Five focused Rust tests cover threshold behavior, shape
restoration, all four correction axes, exact quadrant boundaries, upward steep
attachment and transactional sensor/mode failures. That evidence establishes
the listed normal-gravity routines, not a complete level playthrough or every
collision state.

## Full native-game coverage boundary

| Subsystem | Current first-slice implementation | Work still required for a full game |
| --- | --- | --- |
| Main loop/input/tasks/RNG/math | Executable Rust input, logical task dispatch, both RNGs and selected Q24.8 helpers with C parity | Boot, interrupt/frame ordering, recorder lifecycle, complete math library, native allocator and global-state integration |
| Player | Sonic movement, jump, skid, four-direction terrain attachment, no-target Insta Shield/Forward Thrust and ordinary spring responses | Other playable characters, combat, full action graph and route parity |
| Terrain/stages | 31 terrain slots with animated foregrounds; 14 normal course acts boot from ROM spawn positions | Boss, special, final and multiplayer stage execution, stage-specific events, water and inverted-gravity orchestration |
| Sprite animation/presentation | Executable bounded animation commands, ordinary OBJ composition and native affine matrices/anchors/OBJ sampling from verified original resources; see `NATIVE-AFFINE.md` | Complete renderer integration, shared/global matrix services, mosaic/blend/window behavior, hardware-equivalent ordering, effects and broad gameplay-state selection |
| Camera/presentation loop | Normal camera, Leaf Forest BG0/BG3 parallax, priority-aware foregrounds, native HUD and affine walking | Other zone backgrounds and camera modes, native countdown, menus, hardware-wide OAM/DMA timing |
| Enemies/objects/bosses | Ordinary rings, layer switches, springs and visible/silent checkpoints use actual placements. Life loss, timeout and stage retry run in memory. | Remaining interactables, enemies, bosses, goal/results and full campaign playthrough |
| Audio | Song/voicegroup/track-start catalog inspection; animation sound commands can be events | Sequence interpreter, instruments/samples, mixing, sound-task timing and audible parity |
| Saves/transfer | Bounded garden save/transfer inspection plus campaign profile/sector codec, ten-sector selection/recovery and in-memory native write/reset lifecycle with exact C parity; see `NATIVE-CAMPAIGN-SAVE.md` | Real campaign-save fixture compatibility, flash hardware/error/timing parity, durable storage integration and complete transfer lifecycle |
| Tiny Garden/minigames | Embedded payload identified; selected assets/tables/save structures inspected | Complete original care/behavior/physics/scoring/reward execution and native transfer lifecycle; MMO adaptations remain separate |
| Multiplayer/link | No completed native link runtime | Native link/session synchronization, remote player behavior and deterministic multiplayer execution |
| Frontend/platform | Rust modules can execute locally without a GBA CPU emulator | Reviewed Rust/WASM/platform adapter, actual input/render/audio presentation and full user playthroughs |

Running a local Rust subsystem or drawing an original stage/sprite establishes
that specific executable slice. It does not establish a completed native game,
an installed Android app, a canonical MMO round trip, or permission to deploy
or migrate the live service.
