# ASOE native ring-counter HUD

Verified November 18, 2026. `hud.rs` reads the original ASOE ring icon and
normal/red number glyphs, then composites this selected SA1 HUD element at the
reference screen coordinates. It supplies no font fallback, ring balance,
gameplay reward, save mutation or clock. The complete native stage HUD, title
screen and menu state machines remain separate work.

The only accepted ROM is the 8 MiB USA ASOE revision-zero source with SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
`Hud::load_asoe` verifies that fingerprint before applying any offset and checks
the already cataloged UI table roots. Original pixel data is read locally; no
ROM, icon/glyph pixels or downloaded implementation text is included in Git.

## Source resources and representation

ASOE UI table `0x680A38` has 65 rows of 28 bytes. Row zero names the font bank and
palette; row five names the ring icon. The source caller loads eleven 8x16 glyphs
per digit bank: ten digits plus a colon. Each 4bpp glyph comprises two consecutive
8x8 tiles. The icon is one 32x32 object comprising sixteen row-major 8x8 tiles.
Source index zero is transparent; other palette entries use BGR555 bit replication.

| Native resource | ASOE ROM offset | Bytes | SHA-256 |
| --- | --- | --- | --- |
| Font bank root | `0x658078` | 8,192 | Full-bank hash remains in the existing catalog. |
| Normal digits: font tile 32 onward | `0x658478` | 704 | `8f79fe746f1fd6c4142b40383761787d84708ebd450a404a47d737b5e7b27013` |
| Red digits: font tile 224 onward | `0x659C78` | 704 | `af60786628d9293cbf003042ab869b16feff01f2b4cd6dd05c1ca40e2afdf69d` |
| Ring icon | `0x65A318` | 512 | `e92c5c2d1dbb9adb2c007528f6d33d3759e28a36face04cac0171d1fccc566b2` |
| Shared 16-color palette | `0x65A078` | 32 | `45cdc5144f5dc133f8f4f1837814db8bb14991d91faad7cf513b9000ed249ff6` |
| Zero-warning glyph selection data | `0x658070` | 4 | `16ea092497732dd0726d532ab41c0699b85dd2cf85c2a3d144ad54468a70ab14` |

The palette named by the ring-icon row at `0x65A518` exactly matches the font
palette. The reference loads the font palette into OBJ palette bank six and
reuses it for this icon and the digits. This overlay represents the resulting
colors directly rather than reproducing VRAM copies or palette-bank scheduling.

## Layout and warning behavior

The pinned SA1 stage-UI caller and UI-object renderer establish these rules:

- Ring icon: top-left `(-2,-1)`, size 32x32. Negative screen coordinates clip
  the original object at the top and left of the framebuffer.
- Digits: size 8x16, top `y=5`, horizontal spacing eight pixels. A one-digit
  count starts at `x=9`, two digits at `x=5`, three digits at `x=1`.
- Displayed counts saturate at 999. Leading zeroes are omitted; zero itself
  remains one digit. The presentation never changes the supplied count.
- At zero rings, a 32-tick warning cycle selects normal glyphs for eight ticks,
  red glyphs for sixteen ticks, then normal glyphs for eight ticks. The warning
  phase advances only while the native ring display processes a zero count.

`counter_layout(rings,zero_phase)` exposes those selected layout rules.
`advance_zero_phase(phase,rings,ticks)` advances the explicit phase without
rendering. The phase must be below 32. Draw the current phase before advancing
it; multiple renders of the same simulation tick must not advance the warning.
Nonzero ring counts preserve its previous phase, so a future ring-loss path
must not substitute unrestricted global time for the warning state.

```rust
let hud = Hud::load_asoe(&local_rom)?;
hud.render_into(&mut rgba, 240, 160, practice_ring_count, zero_phase)?;
```

`Hud::decode` accepts exact-sized raw banks separately for synthetic tests. Public
rendering rejects invalid dimensions, buffers and phases before changing the
destination. Pixel work is capped at 1,048,576 framebuffer pixels. It overlays
source pixels on the supplied frame and preserves destination pixels beneath
transparent source indices; it allocates no per-frame image.

## Independent output evidence

An independent local Python oracle reads packed source bytes directly. It
reconstructs the tile order, palette expansion, native clipping and selected
counter layout without importing the Rust decoders. These hashes cover a
240x160 RGBA buffer initialized to transparent zero, with only this HUD element
composited. Actual stage/player frames have different hashes.

| Count / warning phase | RGBA SHA-256 |
| --- | --- |
| `0 / 0` | `321b4c6c98fd586d74f5f78a0a81c75afd1feb8568524628ed7247e665da74de` |
| `0 / 8` | `04d3e13ff746cabdc25ce6ac74605acf823eab453d44cc136dbdb645488d8ef3` |
| `0 / 24` | `321b4c6c98fd586d74f5f78a0a81c75afd1feb8568524628ed7247e665da74de` |
| `7 / 15` | `e5b291db99512ff4870a8597a0956c152b427e5d490a3299a3290a214e0eb869` |
| `42 / 15` | `f68dc2c5342daab029714955e62b08967867fb645b0b0913fc62e32fe25b8db4` |
| `291 / 15` | `6a95e7eaaf0f6806cd380439e5ec42ad3145058f371fabc79c696f0035c87b57` |
| `999 / 15` and `1000 / 15` | `b6f502940f6fa144ce2e96b6bf8c8fda6c10c23bb217f669149b1449a4d045f9` |

`tests/hud.rs` checks all these outputs in an opt-in local-source test. Synthetic
tests exercise decimal bounds, phase cadence, glyph/icon tile ordering, clipped
screen placement, transparent overlay, truncated/wrong-sized input banks and
unchanged destination buffers on rejected render requests. Source PNGs and the
oracle's image outputs stay in ignored local research storage.

## Primary reference and remaining gaps

Research is pinned to SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Exact reference text identities were
checked against the pinned tree's Git blob hashes. Selected SHA-256 values:

| Pinned reference path | SHA-256 |
| --- | --- |
| `src/game/sa1/stage/stage_ui.c` | `0c3367c891c1db02e9ad5cd3e9ea4f5bca50feb0a066af5d33200aadb19032cf` |
| `src/game/sa1/stage/ui_rendering.c` | `ae17ffc2de1238b07cbe7dbc6f2fa7edbba83d18143ab00f8ecc9fc962671c6e` |
| `include/game/sa1/stage/stage_ui.h` | `be427c195cdb85181f7445781ff898cb27012ac670cab55c10cdf303ae4bb184` |
| `include/game/sa1/stage/ui_rendering.h` | `d743452951dcccdde11fc76b876c043d893068ea0848c7c2ba2e13ac416a3b9f` |

The source supplies the UI selection/layout rules; the ASOE ROM supplies the
separately verified pixels and palette. This does not establish native ASOE
execution parity, precise native task initialization, OAM arbitration/priority,
palette-update ordering, timer/score/lives behavior, audio warnings or a complete
HUD/menu implementation. Other regions' offsets are not reused. No external
implementation or artwork license grant is assumed by recording format facts.

See [NATIVE-RUNTIME.md](NATIVE-RUNTIME.md) for the standalone stage boundary and
remaining game-port gates, and [VALIDATION.md](VALIDATION.md) for the actual
integrated native/WASM/browser checks.
