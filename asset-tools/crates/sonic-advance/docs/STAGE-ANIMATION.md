# Native Zone 1 stage animation and scroll evidence

Verified 2026-11-18 against the local 8 MiB ASOE revision-0 ROM. Its SHA-256 is
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
`Stage::load_asoe` requires this exact fingerprint. Native bytes and generated
images are not embedded in this repository.

## Primary source identity

The primary reconstruction is `SAT-R/sa2`, commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, using its `sa1` branches. Source
comments contain addresses for a different baserom layout; the ASOE offsets below
were resolved from the supplied ROM's pointers and validated independently.

| Pinned path | SHA-256 |
| --- | --- |
| `src/background.c` | `0b63bbbaa468cf04e83c8056176b87e1deca677714ce5eded5dbc350592ec952` |
| `src/game/sa1/stage/backgrounds/zone_1.inc.c` | `0e0b92cbe9ba95cffe458c9cce202a845f55905408fca0dbaba1ad1f2e5f6452` |
| `src/game/shared/stage/camera.c` | `feea71aeb35722c9977241d06e96e48482b1b217dd74d18fa8e52a630157c713` |
| `src/core.c` | `24cd4d98db80472077234f72663d5e30c4e4c768960065f011dae80a10cb8f50` |
| `data/sa1/maps/zone_1/act_2/tilemaps/fg/header.c` | `8955e9597b203ff752931d4a06fce4d53b958faa66793f1c152ce192738b927a` |
| `data/sa1/maps/zone_1/act_2/tilemaps/bg/header.c` | `bad6776503e0a74f8a718d6722a05282753e57eb65e9bbdc9ac85b550cbb5e61` |
| `src/data/sa1/tilemap_table.c` | `add9fc8f48adb84058a1756c0ed52701abf8ffb8cdc448a123fa9a014c8f6279` |

The native ordinary single-player path in `UpdateBgAnimationTiles` increments
its delay counter before comparison, updates every ten calls, increments then
wraps its four-phase counter, and writes 448 bytes to the beginning of the
foreground character bank. Phase zero uses the base-bank prefix; phases one to
three use appended blocks zero to two. The back header declares no separate
animation, but both foreground headers point at the same tile bank, so both
layers must observe that upload. `Stage::load_asoe` checks the actual tile
pointers, bank sizes, palette pointer and grid dimensions before sharing it.

The front grids reference no tile IDs 0–13. The back grids contain 339 such
placements in Act 1 and 318 in Act 2. Visible examples begin at world `(2920,256)`
and `(4296,152)` respectively. The tests include viewports `(2800,176)` and
`(4200,96)` that show the native changes at update boundaries 9/10/20/30/40.
The adjacent fourth appended 448-byte chunk is not selected by this ordinary
helper; its purpose and use in other modes remain unclassified.

## Exact ASOE layout

All offsets are file offsets. Both acts retain original 8×8 4bpp tiles,
12×12-tile metatiles, byte grid indices, palette-bank selection, flips and
index-zero transparency. No replacement art or handmade animation is used.

| Stage index | Front header | Back header | Background header | Metatile grid | Pixel size | Spawn entry / coordinates |
| --- | --- | --- | --- | --- | --- | --- |
| 0 / Neo Green Hill Act 1 | `0x4BF1B0` | `0x4BF1D4` | `0x62F474` | 136×14 | 13056×1344 | `0x4A6DEC` / `(104,444)` |
| 1 / Neo Green Hill Act 2 | `0x4D6468` | `0x4D648C` | `0x62F474` | 120×15 | 11520×1440 | `0x4A6DF0` / `(104,684)` |

The actual map table begins at `0x4A6C40`. Its first six pointers (24 bytes)
hash to `dd8aee3eee2dd3e4fa4a41c3edeb24241ce7680d0b4d8252885a00a535418b8b`.
Entries 2 and 5 both select the Act 1 background, as the primary table specifies;
the presence of a separate Act 2 background source declaration does not change
the runtime pointer. The shared background tiles are `0x62D274` / 6656 bytes,
map is `0x62EC74` / 2048 bytes, and its 28-byte header is `0x62F474`.

| Resource | File offset | Bytes | SHA-256 |
| --- | --- | --- | --- |
| Act 1 spawn | `0x4a6dec` | 4 | `1da348af7199a01f375f99fbfb29fc00259a22de1bc2a9cd625ef2789c8cf6dd` |
| Act 1 palette | `0x4a7470` | 512 | `749c529f84d653d9987737d23184ebd3b2705cdd2630bea1b951f6b96aa92927` |
| Act 1 tiles | `0x4a7670` | 31424 | `13586261d24a52ae77533910c3b09fbdf6a8fd13a87c54f5c10c444f5b3acc68` |
| Act 1 front header | `0x4bf1b0` | 36 | `d06374e55ad0e0b685d790e0bf6470aed5bd0552972bbb3a1ab1a118cca6fd04` |
| Act 1 front grid | `0x4be1d8` | 1904 | `ff06829ea0b281e7b059739ca627c64d3f3c711c8b235bfafb07a3eaba4d9d88` |
| Act 1 front metatiles (reachable prefix) | `0x4b1ab8` | 12960 | `df9fae8bbfae0303b60e29974a75333b1b1fcca224a2377955a4ea59d6d5b094` |
| Act 1 back header | `0x4bf1d4` | 36 | `2fc47b755d955bbddbb01ca20558df0146e413c904a239e1f624c93ba0d424eb` |
| Act 1 back grid | `0x4be948` | 1904 | `a7785dae502068ba8c2edcf3de6d96156b05e7cbd040a08b1498f3c2bbff6945` |
| Act 1 back metatiles (reachable prefix) | `0x4b1ab8` | 50976 | `ec205d8e21302c4d7f50ff0dd6e77f7dc30f375126bca2ecc3145c22f35968ee` |
| Act 1 animation phase 0 | `0x4a7670` | 448 | `510e73c253496bd5b4e96251469f2eb94e93a600b6fdd17c40354476bee6f807` |
| Act 1 animation phase 1 | `0x4af130` | 448 | `6ef75a5144126566f10db064a834270acd671db62889bd9142b918114d44a4c1` |
| Act 1 animation phase 2 | `0x4af2f0` | 448 | `f0d2dc26d8f1904b33796bcd85d3f6dc1a16675e2608645bc97ddb642f5632e6` |
| Act 1 animation phase 3 | `0x4af4b0` | 448 | `3cb4c236831ff8efd8680e46a7fc53e664e05998ae92321bb65b967316d8b61d` |
| Act 2 spawn | `0x4a6df0` | 4 | `a760b788550fb7b5072e16f2abff45529643c0a21f1a8a6d049dda2426e71138` |
| Act 2 palette | `0x4bf218` | 512 | `9bc70a2774dee1b0509e42649f115bd5738d3a42ee62401f15f53138ec609ece` |
| Act 2 tiles | `0x4bf418` | 31424 | `be2e385383557ff8a4064a25cb5fc31d9a27b27166f695f2718484092120fc64` |
| Act 2 front header | `0x4d6468` | 36 | `8e4f5c4d2dbd09eebcacb6aabe22d884714ad8ff3350808f4baeca6308eaa1a8` |
| Act 2 front grid | `0x4d5560` | 1800 | `4a162a6e1f854321ccaf9e358f1a289ec3e0895d3cc747eb05660500d26da952` |
| Act 2 front metatiles (reachable prefix) | `0x4c9860` | 14688 | `a6a07ffa5f5b6a018cd6b7b75f74b6db0a6aa561ef0787b284180411a614bb13` |
| Act 2 back header | `0x4d648c` | 36 | `93a8ee63b0863b80aa8e29d30e81d6d6c9fc95d9e5bf41fc3c0d72c2558ad6d3` |
| Act 2 back grid | `0x4d5c68` | 1800 | `d8aab171dcda6789bf8f6e31bf5880a542276f6805cf5950382644797c97d683` |
| Act 2 back metatiles (reachable prefix) | `0x4c9860` | 48384 | `5298e343a15226ecc0175a6aed1ddade53fb1fb3a06c3ce40642b1e0abadbb29` |
| Act 2 animation phase 0 | `0x4bf418` | 448 | `226f3006b1835a5a168f1b7ba31fbc53fdc3b1646a9ec50baaf5c379ffc96da0` |
| Act 2 animation phase 1 | `0x4c6ed8` | 448 | `6c6bd1f6e93883c2a4a1599095f81d588194e53c0c3a39d970c0df0c006680e6` |
| Act 2 animation phase 2 | `0x4c7098` | 448 | `5ce4ed73719bc1f1ec250bbf277e9e3e1d939b9df503c8d23a6839d9bf201807` |
| Act 2 animation phase 3 | `0x4c7258` | 448 | `db56110ed12f730f84ea6cf9e6e9cf97fef9d486a11cfb75380de5c99ff7256b` |
| Shared bgTiles | `0x62d274` | 6656 | `4709309b29794c8d13c4f8149603b2dc210cdaa4b69ee46543c469764761249f` |
| Shared bgMap | `0x62ec74` | 2048 | `46dd7aef461e60442f841bae0c2c717fdd0024e885b47b240cde1c30157e1a87` |

The metatile lengths above are the exact prefixes reachable through each grid's
highest index, not a claim that they describe the entire native metatile bank.
Act 2 dimensions and spawn were read independently. The Act 1 and Act 2 tile
and palette hashes differ; the implementation does not reuse the first act's
bytes for the second act.

## Scanline behavior and API boundary

`StageBgUpdate_Zone1Acts12` selects the same scroll routine for both acts.
With native camera-origin compensation at its initial zero, visible rows use:

| Screen rows | Horizontal source offset, modulo 256 |
| --- | --- |
| 0–7 | `stage_time >> 4` |
| 8–23 | `stage_time >> 5` |
| 24–39 | `stage_time >> 6` |
| 40–87 | zero |
| 88–159 | `((row - 86) * camera_x) >> 8` |

Vertical background scroll is zero. `core.c` copies row zero before HBlank DMA
starts on the following record, so sampling is aligned to the visible row.
`zone_one_background_scroll` rejects negative camera values and rows outside
0–159; stage rendering independently bounds camera coordinates to its own
verified dimensions. Its intermediate arithmetic cannot overflow for any
nonnegative `i32` camera value.

`Stage::render_into` retains the previous base-phase, screen-anchored background
output. `Stage::render_at(output, 240, 160, camera_x, camera_y,
completed_updates)` adds shared tile uploads and scanline scroll without
allocating per frame. `Stage::index()` returns the independently loaded stage
index. The caller owns the stage clock and native update scheduling; this API
interprets its tick as completed ordinary background updates since stage
initialization and uses it for the stage-time scroll clock as well.

## Independent parity and malformed-input checks

The original pinned C `UpdateBgAnimationTiles` and
`StageBgUpdate_Zone1Acts12` were compiled privately with register/queue stubs,
then invoked by `ctypes`. A separate Python composer reads ROM pointers,
palettes, packed tile nibbles and metatile maps; it does not invoke Rust.
The private reference helper SHA-256 is
`73d70e8496d7580aeca4af77cbc831f3757ee72227063bccfac644930dd3adf9`.
Its upload offsets were checked at updates
0/1/9/10/11/19/20/29/30/39/40/41/400. Six complete scanline vectors and
27 timed RGBA frame hashes are asserted in `tests/stage.rs`, including both
acts, native animation changes, cloud changes, water parallax, and camera
edges. The existing three Act 1 base-render hashes remain unchanged.

Synthetic tests cover every truncated prefix of a valid layer fixture,
invalid tile and palette references, oversized dimensions, inactive metadata
inconsistency, zero delay, zero/excessive phases, unaligned/incomplete banks,
wrong appended spans, caller budgets and the absolute 1,048,576 decoded-pixel
animation ceiling. A phase uses whole 32-byte tiles and a maximum of 32 phases.
No ROM, extracted asset, native C body or generated image is needed for these
synthetic checks. Exact source-only tests require `SONIC_ADVANCE_ASOE_ROM` and
remain ignored by default.

Private evidence lives under the combined checkout's ignored
`.local/sonic-advance-runtime-research/stage-animation/`: `oracle.json`,
`act2-oracle.json`, `native-spans.json`, source downloads, reference helper,
independent composers and rendered frames. Published files contain offsets,
hashes and tests only.

## Remaining limits

This verifies ordinary Zone 1 terrain uploads and composition under a fixed
zero camera-origin compensation. It does not reproduce later camera-origin
shifts, collect-rings VRAM-source animation, emulator scanout, native camera
motion, other-zone background effects, stage transitions, or timing divergence
between animation updates and stage-time progression. Whole-frame hashes are
parity with the original C scroll/upload routines plus the independent tile
composer, not proof of a full game-engine port or complete retail framebuffer
parity. The caller must reset its presentation clock when loading another act.
