# Tiny Garden execution checkpoint — 2026-11-18

**The full Tiny Garden end-to-end requirement has not passed.** The strict
all-system audit attempts original entry paths and exits with failure when
acceptance is incomplete. A callback, synthetic flash transaction or unit-test
result cannot establish a full game, a physical device test or a canonical
service round trip.

## Joined Rust execution

`tests/garden_runtime_reference.py` runs original A3VE instructions for the same
explicit inputs used by `garden_runtime`'s Rust test. No game function is
intercepted in these joined scenarios. Bounded BIOS copy/decode and numerical
primitives are explicit reference seams. Unicorn is an offline oracle only;
the Rust runtime never executes ROM or decoded-payload instructions.

| Scenario | Executed coverage | Evidence and limit |
| --- | --- | --- |
| Home initialization → frame callback | 10 scenarios × 60 frames = 600; five languages, Chao present/absent | Original graphics, task creation, Chao parts/care, cursor, HUD, shop, transfer maintenance, palette/background queues, sorting and OAM composition; whole EWRAM, IWRAM, MMIO, palette, VRAM and OAM hashes match. |
| Chao Ball setup → frame callback | 12 scenarios × 180 frames = 2,160; four rounds × three input profiles | Native task creation/removal, movement/shoot inputs, rules, score display, register/background queues, sprite composition and sound routing; same complete workspace hashes match. |

These are direct home/board callback runs. Initial RAM is explicit synthetic
state plus the authenticated local payload, with no loaded native save or booted
audio engine. Source song routing executes against that same inactive player
state. The home frame counter is the supplied snapshot value. VBlank timing,
the main-loop mixer, real input hardware and visible/audible output are outside
these comparisons. The board scenarios cover finite input traces, not complete
winning/losing sessions for every round.

## Component comparisons

| Rust module | Native fixture | Scope |
| --- | --- | --- |
| `garden_services` | 940 cases | Task walk/unlink, source-order callback mutation, background flush/queue, sprite lists/sort/OAM, PRNG, task/affine snapshots; malformed counts/links fail bounded. |
| `garden_home` | 48 cases | Boot ordering, IRQ data-copy/MMIO transactions, home installation and original graphics; save, JoyBus/audio initialization and song start have declared component seams. |
| `garden_care` | 664 scenarios / 1,406 ticks | All 64 action IDs, 52 distinct internal targets, initializer, component allocation, timers, motion, water, feeding/signed effects, localized dialog tiles and source call order. |
| `garden_home_tasks` | 770 scenarios / 1,377 ticks | 27 callbacks: original body/part poses, objects, cursor/hit boxes, petting, HUD, item removal and localized display paths. |
| `garden_shop` | 108 scenarios in two profiles | All three purchase categories, stock/eligibility/ring subtraction, cancellation, labels and inventory contact; original and synthetic source tables. |
| `garden_puzzle_objects` | 347 scenarios in two profiles | Shot flight/contact, native group threshold, flips/collapse/recheck, result banners, ring cap and next-round/return callbacks; two resolving chains match removal at exactly 47 and 84 frames. |
| `garden_save` | 53 cases | Original six-sector scan/selected reread, new-record initialization, generation/RNG/rings/CRC, rotating writes/retries, interruption cases, fresh-RAM reload and opaque-tail preservation against synthetic flash. |
| `garden_transfer` | 165 scenarios in two profiles | Original endpoint, ten-state IRQ dispatch, receive/send, communication UI and localized tiles; ordered register width/address/value, RAM, calls and flash hashes. |
| `garden_audio` | 278 cases | Independently authenticated four players/37 songs; native start/stop/resume/fade and channel unlink, with explicit dynamic PSG/SoundMode boundaries. |
| `garden_audio_init` | 119 cases | Native sound/PSG setup, callback and code-data copies, SoundMode, DMA pause/resume and sound-main wrapper; exact RAM/register transactions, explicit frequency and mixer dependencies. All 37 full initializer cases use the authenticated local payload. |

The transfer integration case receives all 2,048 bytes, runs the controller and
actual Rust/native save writer, and produces the same checksum-valid 2,136-byte
synthetic flash record. A subsequent controller call does not save twice.
Out-of-window lengths and corrupt cursors stop before writing adjacent records.
Bus inputs and flash operations are supplied by the test. No physical console,
cable timing, installed-game session or real user save was exercised.

Each fixture declares its own intercepted calls. Component parity does not
mean that those dependencies execute within the same test. Exact source spans,
domain labels and SHA-256 values are in `engine-sources.json`; Garden offsets
are decoded-payload offsets and never inherited from Advance or the main ROM
engine. Native data stays local; fixtures retain hashes and synthetic inputs.

## Strict all-system audit

Run from this crate with the exact local A3VE cartridge:

```powershell
cargo run --offline --bin spp-game -- garden-audit $env:SPP_ROM
```

The audit uses fresh independent snapshots for boot, home/care, Chao Ball, the
other native minigame, load, write, audio and transfer. It prints completed call
counts and the first missing address for each path. Its nonzero exit is an
intentional failed acceptance result. Completed transfer initialization and
maintenance do not count as a peer exchange. It opens no native save and makes
no service request.

The original Thumb boot path `0x168 → 0xC4 → 0x248` currently stops at
**flash identification `0x02012028`**. Load/write also require native flash
timer/read/program functions and sound/wait dependencies. The other minigame
starts at **`0x0200A8A4`**, which remains unported. The main audio path through
`0x10660 → 0xF938` needs this payload's sequencer/mixer/sample closure and output.
The initializer `0x105E8` reaches strict frequency/timer/scanline helper
`0x02010AE0`; the sound-main wrapper reaches strict mixer `0x0200F938`.
Main-loop VBlank `0x11B04` requires a real platform contract.

Other known home branches still include egg hatching `0x3F68`, notifications
`0x86B4`, water/sleep effect children `0x8A2C/0x6798/0x68D8/0x69F8`, and the
pause-menu successor `0x21F4`. Special-item handoff can also reach the unported
`0x8DEC` continuation. Unknown entries remain strict errors. Complete
home-to-shop-to-care-to-minigame-to-save sessions and every success/failure path
still need execution coverage after those branches and devices are connected.

This is an isolated source checkpoint. No client preview, runtime deployment,
canonical Chao, credentials, inventory, rings or server state changed. The
canonical Rust service round trip remains unverified. Native JoyBus and local
save records do not define a new MMO protocol or authorize progression imports.

## Reproduction

The checkpoint passed 222 Rust tests with local native inputs enabled, formatting
and strict Clippy. The Python unit suites ran 44 tests with one optional test
skipped. The reviewed oracle driver covers 64 instruction/reference fixtures
plus one explicitly metadata-only systems map, and 593 authenticated source
spans. These counts do not establish end-to-end acceptance.

Set `SPP_ROM` and `A3VE_ROM` to the authenticated local cartridge. Set
`SPP_GARDEN` and `SPP_GARDEN_PAYLOAD` to its separately decoded, authenticated
Garden program for the optional local-loader checks. Local Python Unicorn and
the independent numerical reference are research dependencies only.

```powershell
cargo test --offline -- --include-ignored
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
python tests/engine_sources.py --rom $env:SPP_ROM
python tests/check_source_port.py --rom $env:SPP_ROM --bios-reference-tool PATH_TO_LOCAL_REFERENCE
python -m unittest discover -s tests -p '*_reference.py' -q
python -m unittest discover -s tests -p reference_parity.py -q
```
