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
the complete cartridge identity. Garden spans are checked against the separately authenticated decoded payload, never against a guessed cartridge offset. No native files leave ignored local storage.

## Implemented behavior and limits

| Area | Rust source work | Remaining full-game gate |
| --- | --- | --- |
| Main input/callbacks | Native input initialization, edges, grouped/per-key repeat; ten-slot callback stack; physical-memory input/stack adapter; title and mode-selection wrappers | Complete menu rendering/child screens, options and whole boot flow |
| Startup | Original `0x81E8` wrapper; `0x6EF4`/`0x7840`/`0x8168` init/resume/update, 39 contexts with exact destination permutation, bounded native type10 resources and installed auxiliary callbacks, `0x6825` animation sequence and title through mode-selection transition `0x0801CF4D` | Remaining boot/menu calls and complete memory/hardware binding; the minimal CLI `run` still stops at its unbound `0x6EF4` entry |
| Ball | Original initialization/trail reset, fixed-point integration, coordinate projection prefix and velocity limit | Launch/plunger, nudge/tilt, draw gates, drains, bonuses, extra balls and all lifecycle transitions |
| Flippers | Native initialization, geometry lookup, input/angle updates, side/proximity rules and source sound requests | Complete table integration and playthrough evidence; initializer writes outside the bounded prefix remain explicit outputs |
| Nudge/lifecycle | Original directional nudge/timers/decay and native curve; lifecycle router and source state machines for codes 0/1/2/3, including source launch velocity and nested ball reset | Called table, sound/UI and graphics helpers, complete drain/bonus/game-over paths and retail playthroughs; nudge routine alone establishes no accumulated tilt penalty |
| Collision | Map probing, alternating-axis sweep, materials, dynamic regions, flipper masks and collision response/kicks with explicit BIOS math boundary | Native table/object initialization, all moving objects and full input-trace/playthrough parity; inspect collision fixture scopes for synthetic versus native resource tests |
| Frame controller | Original normal/multiball control flow and four substeps with substep byte `0,1,2,3`; shared ball/flipper state; lifecycle, events and message scheduler adapters; table callback/update wrappers and five bootstrap variants | Unported external calls stop execution; wrapper parity does not establish complete scoring, camera or table callbacks |
| OAM/palettes | Native submission queue, sorted descriptor compositor, alternate tree compositor, frame reset, native affine matrix writer and typed DMA commands; shared-memory graphics adapter; source palette blending/cache/dirty banks | Whole GBA layer/priority/blend composition, transfer scheduling and original screen/animation paths |
| Native presentation | Original board/ball rasters; native descriptor/glyph message setup, all ten reveal/scroll/fade modes, queued duration transitions, score HUD formatting, board display setup and original display-register transactions; finite Windows host | Full original camera, layer/hardware behavior, sprite scheduling, visible menus/gameplay and interactive visual acceptance; score display is not score-award logic |
| Sprite animation | Native initializer, bounded command dispatcher and eight frame/wait/loop/end handlers | Remaining handler IDs, real screen scripts, scheduling and complete animation/presentation parity |
| Intro interpolation | Native 32-byte context constructor/stepping, 39 initialized contexts, seven installed callback IDs and direct completion helper; authentic compressed-resource hashes; constructor's opaque stack word is an explicit input | Final intro/title screen playback, remaining resource/hardware/audio execution and boot integration |
| Sound | Separate main-game 135-entry map, routing and MPlay lifecycle; 16 native command leaves, full sequencer tick, instrument/note allocation, PCM/PSG pitch, four-channel PSG register updates, unlink/finish graphs and DMA gate transactions | PCM mixer/output, DMA timing and audible playback; command-byte MMIO capture is not device execution |
| Math | Original software division and native quarter-wave sine/cosine | Authentic BIOS is not supplied. Strict backend errors; opt-in integer numerical reference is separately labeled and tested |
| Table rules | Source region/layer event routers, native action/message queues, phase/multiball setup and six map pointer bindings; native u64 score awards and five contact-handler groups; camera/visibility object placement; numeric IDs retained | Sonic, NiGHTS, Samba and all special modes: complete scoring, missions, bosses, progression, unlocks and multiplayer rules |
| Saves/transfer | Bounded opaque Garden record/flash reader; actual native six-sector loader, initializer, rotating/retrying writer; bounded JoyBus endpoint/IRQ/controller, full synthetic receive-to-save transaction and once-only write parity | Main-game save layout, real native save fixtures, physical transfer, device persistence and canonical progression integration |
| Tiny Garden/Chao Ball | Independent payload/layout; all 64 care states and Chao initializer, 27 home/part/cursor/HUD callbacks, shop/inventory, puzzle tasks/rules/results/rewards, queues/snapshots/OAM, Garden-specific audio routing; joined 600 home and 2,160 board frames match native instructions | Full boot and native devices, hatching/effect/pause/item continuations, other minigame, mixer/output, complete interactive sessions and canonical service round trip |

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
intercept intro dependencies; separate intro tests execute the original wrappers,
constructors and lookups. Their resource-loading seam uses synthetic same-length
bytes; independent authentic hashes verify the actual decoded resources. The
auxiliary sequence executes 326 native ticks before the separately ported `0x6825` animation. Its own authentic resource sequence runs 387 ticks through the title transition. Title parity reaches the actual mode-menu ID after 254 ticks. These tests retain declared external call boundaries.
Frame tests separate intercepted wrapper calls
from actual native flipper/integrator execution. Palette tests implement only
the stated BIOS halfword-copy hook. Sound routing tests intercept player calls;
the separate player-lifecycle tests execute those routines and intercept only
the PSG oscillator callback. Controlled BIOS outputs prove surrounding native
math/collision code and call order, not authentic BIOS execution. Sound DMA tests
model only the specified SWI0B zero-word fill. Display tests intercept VBlank and
compare register transactions; they do not emulate interrupts or a PPU. Native
VRAM address-window writes may cross the backing boundary; a production backend
must apply the original mode-dependent hardware behavior.

`table_calls::TableCalls` composes completed lifecycle, table-action, bootstrap,
collision-binding, object-placement, scoring/contact and message routines on one supplied memory image. Unknown
dependencies remain synchronous errors through its backend. Board initializer
and hardware/palette services are separate explicit adapters. Queue consumption
calls `0x2FF40` directly, preserving later requests and the active duration;
immediate `0x3046C` requests retain their distinct queue-flush behavior.

`ReferenceBiosMath` has separate integer numerical vectors checked against the
primary mGBA 0.10.3 mathematical reference, source SHA-256
`8885962e1ca2d8c79a7f13a5d1e074bb2b39704a64aa164f76e1506868c8759a`.
This does not prove BIOS cycle timing, register clobbers or machine-code parity.
Unknown/corrupt counts, pointers, links, decode lengths and work budgets fail
at bounded Rust boundaries rather than reproducing native out-of-bounds reads
or infinite loops. Native wrapping and truncation are retained for supported
inputs. Unknown fields are preserved.

## Reproduction

The explicit oracle driver controls registry membership; unregistered scratch
fixtures cannot change publication metadata. The systems map explicitly has no
instruction-parity or end-to-end acceptance claim. Counts are scoped evidence,
never a completion percentage. See [GARDEN-VALIDATION.md](GARDEN-VALIDATION.md)
for the joined frame traces, component scopes, device seams and reproduction.
The current checkpoint passed 222 Rust tests, 44 Python unit tests (one optional
test skipped), formatting and strict Clippy; the oracle driver verifies 64
instruction/reference fixtures, one metadata-only systems map and 593 source
spans.

The previous `0x02001420` frame barrier is now ported. Native home and board
callbacks compose through their task, care, shop, graphics and song-routing
paths on one shared bounded memory image. The real boot path still stops at
**`0x02012028`**, flash identification. `garden-audit` attempts eight system
entry paths and returns failure while full acceptance is incomplete. BIOS
numerical/copy primitives and synthetic MMIO latches do not establish authentic
BIOS, hardware timing or visible/audible output. Direct callback traces hold
the supplied native frame counter fixed; the complete main loop and trusted
time are separate gates. The other minigame, interactive continuations, mixer,
devices and service integration remain unfinished.

From this crate, activate the local Rust toolchain if needed, set `SPP_ROM` and
`A3VE_ROM` to the same exact local cartridge, then run:

```powershell
cargo test --locked -- --include-ignored
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
python tests/engine_sources.py --rom $env:SPP_ROM
python tests/check_source_port.py --rom $env:SPP_ROM
cargo run --offline --bin spp-game -- garden-audit $env:SPP_ROM
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
