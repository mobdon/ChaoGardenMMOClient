# ASOE stage graph and viewport evidence

Verified November 18, 2026 against the supplied 8 MiB USA revision-0 source:
SHA-256 `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
`Stage::load` requires this complete fingerprint before applying any native
offset. `Stage::from_records` is a separately exposed generic bounded graph
reader for explicit caller-provided records and synthetic tests; its evidence
reports the hash of the actual input, without claiming ASOE identity.

The implementation consumes native tiles, palettes, maps, packed collision
profiles and angles. It never substitutes an invented landscape or texture.
Outputs and native reference files remain ignored/local.

## Exact source graph

The existing 89-entry tilemap table is at ROM `0x4A6C40`. Its first 54 entries
form 18 groups of foreground 0, foreground 1 and background. The following
table at `0x4A6DA4` contains 18 collision-header pointers, 72 bytes, SHA-256
`95bae0d3f9027a10a12e0f4dd99ae2c8024c64777a13f804c772f78a511edd6d`.
The next word after these 18 pointers is not a canonical ROM pointer.

All 18 graphs were parsed on the actual source. Each collision header's two
map pointers, metatile pointer, dimensions and pixel dimensions are checked
against the corresponding foreground headers. ASOE uses **u8** metatile
indices, 12 by 12 tile chunks and 96-pixel chunk dimensions. Neither Advance 2
index widths nor European offsets are reused.

| Stage slot | Collision header | Native pixel dimensions |
| ---: | ---: | ---: |
| 0 | `0x4BF1F8` | 13056 × 1344 |
| 1 | `0x4D64B0` | 11520 × 1440 |
| 2 | `0x4ED2F4` | 10368 × 1536 |
| 3 | `0x506F68` | 9024 × 1920 |
| 4 | `0x520310` | 7104 × 2592 |
| 5 | `0x53D71C` | 7680 × 2784 |
| 6 | `0x55B150` | 10464 × 2880 |
| 7 | `0x577DE8` | 8832 × 2496 |
| 8 | `0x5950A8` | 8928 × 2688 |
| 9 | `0x5B1C00` | 9600 × 1728 |
| 10 | `0x5CFFF0` | 4224 × 7968 |
| 11 | `0x5EA450` | 10560 × 2496 |
| 12 | `0x5F1F38` | 3840 × 960 |
| 13 | `0x5F5C7C` | 3072 × 288 |
| 14 | `0x603D20` | 1920 × 1152 |
| 15 | `0x611444` | 1920 × 1152 |
| 16 | `0x62039C` | 1344 × 1920 |
| 17 | `0x62D254` | 1920 × 1344 |

Slot 0 requires 177 metatile definitions and a collision/tile prefix reaching
tile 981. These are bounds needed by the graph, not assertions that every byte
in those prefixes is reachable at runtime or that they are whole array extents.
`Stage::evidence()` reproduces exact offset, consumed length and SHA-256 for all
its bounded source spans. Representative slot-0 spans:

| Span | ROM offset | Bytes | SHA-256 |
| --- | ---: | ---: | --- |
| Collision header | `0x4BF1F8` | 32 | `e880e0d1791c8b57e319297b8df40a8f1db6686d519f3d3f7ae93a3191002f80` |
| Foreground-0 header | `0x4BF1B0` | 36 | `d06374e55ad0e0b685d790e0bf6470aed5bd0552972bbb3a1ab1a118cca6fd04` |
| Foreground-1 header | `0x4BF1D4` | 36 | `2fc47b755d955bbddbb01ca20558df0146e413c904a239e1f624c93ba0d424eb` |
| Foreground-0 byte map | `0x4BE1D8` | 1904 | `ff06829ea0b281e7b059739ca627c64d3f3c711c8b235bfafb07a3eaba4d9d88` |
| Foreground-1 byte map | `0x4BE948` | 1904 | `a7785dae502068ba8c2edcf3de6d96156b05e7cbd040a08b1498f3c2bbff6945` |
| Metatile prefix | `0x4B1AB8` | 50976 | `ec205d8e21302c4d7f50ff0dd6e77f7dc30f375126bca2ecc3145c22f35968ee` |
| Packed height prefix | `0x4AF830` | 7856 | `2c13a531cca06371f829e5334ca82d73446b36e01c469b7ae448c1ebb89f0551` |
| Angle prefix | `0x4B16E0` | 982 | `3c9b6c2097f83af5298a0d30f702eb979aa2b6dabf75ed7d31ff8c6f6793da33` |
| Flag prefix | `0x4BF0B8` | 246 | `f9d23e5db890b8bd71cae71c1888132b505435db1005566aeda2562cf0f5a2c2` |
| Initial foreground tiles | `0x4A7670` | 31424 | `13586261d24a52ae77533910c3b09fbdf6a8fd13a87c54f5c10c444f5b3acc68` |
| Shared foreground palette | `0x4A7470` | 512 | `749c529f84d653d9987737d23184ebd3b2705cdd2630bea1b951f6b96aa92927` |

## Representation and execution scope

The high nibble of a collision byte is a signed vertical height indexed by
tile-local x; its low nibble is a signed horizontal width indexed by tile-local
y. Signed -8 is the native full-tile sentinel. Tile X/Y flips transform sample
coordinates, signed heights and byte angles; packed two-bit flags are preserved.
`collision_at` returns both axis values and angles. `solid_at` exposes vertical
profile pixel occupancy. Raw flags stay available for future path/one-way policy.

`ground_below` is a bounded **upright testbed adaptation** over those actual
profiles. It scans at most 512 pixels, finds the top solid boundary, and accepts
only the near-upright angle band. It is not the original multi-sensor collision
resolver for every wall, ceiling, loop, path switch and one-way surface. Outside
world coordinates are empty instead of being clamped to the last world pixel;
the separate player testbed must enforce world bounds.

`render_view` composites actual palette-bank and flip-aware 4bpp tiles from
the background and both foreground layers. Foreground palette index zero is
transparent. The shared palette is honored when the background palette pointer
is null. Source maps are linear; VRAM screen-block packing is not imposed on
their source representation. Native camera configuration gives foreground 0
priority 1, foreground 1 priority 2 and background priority 3. `overlay_front`
can composite the front layer again after priority-2 actors, preserving native
foreground occlusion without turning transparent pixels opaque.

At a native 240×160 viewport, Zone 1's cloud-row shifts and water-row horizontal
offsets follow the pinned native background function. The supplied camera x is
treated as its relative x input; camera-origin changes remain the runner's
responsibility. Other stages currently display their actual static background.
The first foreground's animated prefix is uploaded from the source tiles and subsequent contiguous frames. In Neo Green Hill Act 1 this is 448 bytes at offsets 0x4A7670, 0x4AF130, 0x4AF2F0 and 0x4AF4B0, advancing every ten updates and wrapping after forty. Six additional Act 2 screen comparisons cover the first four phases and wrap. Palette cycles and the remaining zone-specific display effects are incomplete.

## Tests and independent parity

The three synthetic tests verify graph agreement, palette/channel order,
transparency and front-layer occlusion, signed profiles/flips/angles/flags,
bounded probes, truncated graphs, invalid pointers/tiles/dimensions, source
identity rejection and output overflow/budgets. The fourth test is explicitly
ignored without `SONIC_ADVANCE_ASOE_ROM`.

The opt-in test parsed all 18 actual stage graphs, rechecked every evidence
span, and compared **21 RGB24 viewport hashes** to an independent local
JavaScript oracle which reads the raw pointers/bytes directly. This oracle is
only a hash-producing validation tool; no browser mechanics or authority were
implemented in JavaScript. Eighteen views use camera (0,0), tick 0. Three extra
slot-0 views exercise actual terrain and scrolling:

| Camera x,y / tick | RGB24 SHA-256, native 240×160 |
| --- | --- |
| 0,360 / 0 | `1ef1c1917a3fb8b0ce7097a5b3f90f20a48e99b213f17970931dc850edba5df5` |
| 104,444 / 0 | `fe5933997e0db422d0435af4ed580855e7fec39ec3186216802b30c171e059ce` |
| 1024,512 / 320 | `be0ce1f7141a7724b4b2e909f42beae5cd7c041ae745ba4333ade1832fa2cec4` |

A second independent raw-byte oracle sampled **25,280** slot-0 collision
points over both layers: y=0..1343 step17, x=0..2047 step13. For each sample,
the little-endian tile word, signed vertical/horizontal profile bytes, flags,
both angles and vertical pixel occupancy serialize to eight bytes. Combined
SHA-256 is `4aeeb78e04ef05a509c5a78052871ca3ac88b0387ef4772720b20765c1f87d1d`.
Rust matches this digest exactly. These are format/representation checks,
not an original-hardware framebuffer or full collision-execution comparison.

Run from a checkout with the crate and an already configured Rust toolchain:

```powershell
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test stage
$env:SONIC_ADVANCE_ASOE_ROM = 'C:/private/.local/gba/ASOE.gba'
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test stage -- --include-ignored
```

## Primary reference provenance

Structural/algorithm research uses SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. The reference describes Europe;
the ASOE roots, graph bounds and all hashes above were derived independently
from the supplied USA source. No European offset shift was applied. Original
Rust implementation and synthetic fixtures are published; downloaded reference
implementation, source artwork and ROM contents are not included.

| Pinned reference path | UTF-8 file SHA-256 |
| --- | --- |
| `include/core.h` | `6b940bf2d0dff3499fe0ae787773a7598a2cb16eee1d791c229d3b7eb1cac0ef` |
| `include/global.h` | `37d5d16cf5be24e40fbc34015aa22909c8a295fcbcb1dd7e7ec09b243f3053b9` |
| `include/tilemap.h` | `b6d78ba62949d0653590fb3f3e364c59558a76beb1578130972b477ef5540e3c` |
| `data/sa1/maps/zone_1/act_1/tilemaps/fg/header.c` | `47b99a464373272183bc34b18362bde4a99a9262cef0a2ccbf7c0cc0fa7aa521` |
| `src/game/shared/stage/terrain_collision.c` | `777229b37e656108cbb8dafafff9ea361012e682cba4e63b987b2de474e5c0d2` |
| `src/game/shared/stage/camera.c` | `feea71aeb35722c9977241d06e96e48482b1b217dd74d18fa8e52a630157c713` |
| `src/game/sa1/stage/backgrounds/zone_1.inc.c` | `0e0b92cbe9ba95cffe458c9cce202a845f55905408fca0dbaba1ad1f2e5f6452` |

Native entity scheduling, player mechanics, enemies/bosses, hazards, ring
economy, goals, checkpoints, save/transfer/audio and Tiny Garden remain separate
systems. A bounded stage graph and software viewport do not complete them.
