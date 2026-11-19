# Full-game Rust port status

The target is the entire A3VE game, including all tables, party modes, menus,
audio, saves, transfers and embedded Tiny Garden/Chao Ball. This source-port
milestone is incomplete. No complete retail playthrough has passed, and no
completion percentage is inferred from the number of routines or tests.

The exact local cartridge is 8,388,608 bytes, SHA-256
`5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b`.
[engine-sources.json](../engine-sources.json) lists exact source offsets,
lengths and hashes from reference fixtures. It includes resources, partial
ports and dependency boundaries; it is an evidence registry, not a list of
completed game functions. `spp-game verify` checks all its spans after checking
the complete cartridge identity. No native files leave ignored local storage.

## Implemented behavior and limits

| Area | Rust source work | Remaining full-game gate |
| --- | --- | --- |
| Main input/callbacks | Native input initialization, edges, grouped/per-key repeat; ten-slot callback stack with original truncation and retained inactive storage | Screen dispatchers, complete menus, options, language and mode selection |
| Startup | Original `0x81E8` callback wrapper, including writes before calls and next-callback replacement | Intro routines `0x6EF4`, `0x7840`, `0x8168` and subsequent screen callbacks; strict `run` stops at the first missing call |
| Ball | Original initialization/trail reset, fixed-point integration, coordinate projection prefix and velocity limit | Launch/plunger, nudge/tilt, draw gates, drains, bonuses, extra balls and all lifecycle transitions |
| Flippers | Native initialization, geometry lookup, input/angle updates, side/proximity rules and source sound requests | Complete table integration and playthrough evidence; initializer writes outside the bounded prefix remain explicit outputs |
| Nudge/lifecycle | Original directional nudge/timers/decay and native curve; lifecycle router and source state machines for codes 0/1/2/3, including source launch velocity and nested ball reset | Called table, sound/UI and graphics helpers, complete drain/bonus/game-over paths and retail playthroughs; nudge routine alone establishes no accumulated tilt penalty |
| Collision | Map probing, alternating-axis sweep, materials, dynamic regions, flipper masks and collision response/kicks with explicit BIOS math boundary | Native table/object initialization, all moving objects and full input-trace/playthrough parity; inspect collision fixture scopes for synthetic versus native resource tests |
| Frame controller | Original normal/multiball control flow and four substeps with substep byte `0,1,2,3`; shared ball/flipper state prefix | Unported external calls stop execution; wrapper parity does not establish scoring, camera or table callbacks |
| OAM/palettes | Native submission queue, sorted descriptor compositor, alternate tree compositor, frame reset/affine defaults and typed DMA commands; source palette blending/cache/dirty banks | Whole GBA layer/priority/blend composition, all affine transforms, transfer scheduling and original screen/animation paths |
| Native presentation | Verified original 256×512 board and four 16×16 ball OBJ pixel frames; finite Windows 240×160 presentation/input host | Full original camera, sprite scheduling, other layers, visible menus/gameplay and interactive visual acceptance |
| Sprite animation | Native initializer, bounded command dispatcher and eight frame/wait/loop/end handlers | Remaining handler IDs, real screen scripts, scheduling and complete animation/presentation parity |
| Intro interpolation | Native 32-byte context constructor and integer stepping/update with real OAM submission; constructor's opaque uninitialized stack word is an explicit input | Intro resource loading, complete init/resume/update callbacks, all 39 context instances and final screen playback |
| Sound | Separate main-game 135-entry map, conditional/forced routing, channel-2 wrapper; native MPlayOpen/Start/Stop and track/channel release state | Track sequence execution, instrument banks, PSG backend, mixer, timing and audible playback |
| Math | Original software division and native quarter-wave sine/cosine | Authentic BIOS is not supplied. Strict backend errors; opt-in integer numerical reference is separately labeled and tested |
| Table rules | Source call targets and numeric table/layer identifiers preserved | Sonic, NiGHTS, Samba and all special modes: scoring, missions, bosses, progression, unlocks and multiplayer rules |
| Saves/transfer | Original asset work's bounded opaque Garden record/flash inspection | Main save layout/writes, complete transfer protocol, atomic progression and native save round trips |
| Tiny Garden/Chao Ball | Exact embedded payload, resource and opaque-save catalog | Original care/AI, items, menus, minigames including Chao Ball, rewards and complete garden playthroughs |

Numeric source table IDs are retained without assuming they map directly to
menu names or to another routine's IDs. In particular, a selector's no-rebind
path is not evidence for a new dedicated map. No collision geometry is inferred
from rendered background pixels.

## What parity means here

Ordinary Rust tests use synthetic state and committed output vectors. The
Python reference tools execute the exact local A3VE instructions under bounded
Unicorn runs; Capstone is a research aid. Neither is linked into the Rust game.
Vectors contain synthetic scalar state, opaque native addresses and output
hashes, never ROM instructions, native art/audio or real saves.

Reference boundaries are explicit in each fixture and script. Startup tests
intercept intro dependencies. Frame tests separate intercepted wrapper calls
from actual native flipper/integrator execution. Palette tests implement only
the stated BIOS halfword-copy hook. Sound routing tests intercept player calls;
the separate player-lifecycle tests execute those routines and intercept only
the PSG oscillator callback. Controlled BIOS outputs prove surrounding native
math/collision code and call order, not authentic BIOS execution.

`ReferenceBiosMath` has separate integer numerical vectors checked against the
primary mGBA 0.10.3 mathematical reference, source SHA-256
`8885962e1ca2d8c79a7f13a5d1e074bb2b39704a64aa164f76e1506868c8759a`.
This does not prove BIOS cycle timing, register clobbers or machine-code parity.
Unknown/corrupt counts, pointers, links, decode lengths and work budgets fail
at bounded Rust boundaries rather than reproducing native out-of-bounds reads
or infinite loops. Native wrapping and truncation are retained for supported
inputs. Unknown fields are preserved.

## Reproduction

This milestone passed 102 Rust tests with the local ROM checks enabled, 26
Python unit tests, regeneration/comparison of all 23 engine oracle fixtures,
all 113 registered source-span hashes, formatting and strict Clippy. These
counts are validation evidence for the scoped ports above, not full-game
completion. Native collision coverage includes 1,000 complete dispatches using
the cartridge's own maps/masks, with the BIOS seam explicitly recorded.

From this crate, activate the local Rust toolchain if needed, set `SPP_ROM` and
`A3VE_ROM` to the same exact local cartridge, then run:

```powershell
cargo test --locked -- --include-ignored
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
python tests/engine_sources.py --rom $env:SPP_ROM
python tests/check_source_port.py --rom $env:SPP_ROM
python tests/game_graphics_reference.py --rom $env:SPP_ROM
python tests/mplay_reference.py --rom $env:SPP_ROM
python tests/sound_reference.py --rom $env:SPP_ROM
```

Native instruction oracle regeneration additionally requires local Python
Unicorn; its modules and any primary numerical reference stay outside Git.
Each script's `--help` describes its regeneration/check options. Regenerate
fixtures in scratch storage first, compare them, and review any changed source
span or boundary. `engine_sources.py --generate` refreshes metadata only.

The Windows host completed a finite 3,600-callback smoke run, reporting 2,705
successful presentations and a clean frame-budget exit. A one-frame run also
completed. Automated screenshot capture timed out while restoring its window;
visual/input acceptance remains unverified. Headless rendering checks exact
RGBA fingerprints and reports the number of frames actually rendered.

## Completion and integration gates

A complete port must boot through menus, run every table and party mode,
exercise success/failure/drain/game-over paths, and complete Tiny Garden/Chao
Ball with native presentation and audio. Deterministic original input traces
must match state, events, score, frame output, audio timing and save/transfer
results. Unknown routines must not be skipped or replaced with guessed rules.

This crate does not modify the client preview, canonical Chao, server, account,
inventory, rings or saves. There is no new service API or service round-trip
claim. Future MMO integration must use the separately agreed Rust authority
and prove the canonical service round trip. Source publication authorizes no
runtime rollout or migration.
