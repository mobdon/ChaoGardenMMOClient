# Sonic Advance ASOE: first native stage slice

Evidence date: November 18, 2026. This work begins a standalone Rust gameplay
slice for Neo Green Hill Zone Act 1. It uses the supplied game's stage pixels,
collision data, sprite commands and ring positions. It does not establish a
complete Sonic Advance port or frame-by-frame ASOE execution parity.

The exact local source is the 8,388,608-byte USA revision-zero ROM:

- Header: `SONICADVANCE`, `ASOE`, maker `78`, revision `0`.
- SHA-1: `d842afa7dd1e84de08addb94a51506f1bcafd551`.
- SHA-256: `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.

The complete fingerprint gates native locators. Other regions, modified images
and a merely matching header are rejected. ROMs, rendered frames, downloaded
reference texts and native saves remain local. Public source contains original
Rust implementations, synthetic fixtures and hash metadata.

## Implemented boundary

`stage.rs` reads the native foreground/background representations and composes
their original 4bpp pixels at a 240x160 viewport. `collision.rs` reads the native
two-layer metatile graph and directional boundary profiles. `movement.rs`
implements the selected flat-ground/free-air rules using integer positions and
velocities with eight fractional bits. `sprite.rs` reads bounded native scripts,
dimension records, OAM subframes, palettes and tile references. `entities.rs`
decodes the stage-zero ring map and owns an ephemeral practice pickup ledger.
`runtime.rs` combines these representations into the solo stage loop.

The slice accepts left, right and jump intents. It has bounded frame batches,
camera tracking, wall/ceiling/floor sampling and explicit practice reset. The
Rust simulation and decoder state remain the authority within this standalone
practice process. Browser glue should only load the user-selected local ROM,
submit bounded button masks and display the Rust-produced framebuffer/state.
Build and actual browser-control evidence belong in [RUNTIME-VALIDATION.md](RUNTIME-VALIDATION.md);
a native unit-test result alone does not prove a working WASM/browser session.

This process has no canonical MMO profile, authenticated activity lease, saved
ring wallet or progression endpoint. Practice pickups do not grant transferable
currency. Reset/reload clears the practice ledger. No code imports native saves
into the MMO or changes the working garden service, credentials or live profile.

## Exact stage and collision locations

All offsets in this table are physical ASOE ROM offsets. A prefix contains the
bytes needed by the reachable graph; it is not a claim about unused trailing
records or a complete original array extent.

| Representation | Offset | Bytes / interpretation |
| --- | --- | --- |
| Main tilemap index | `0x4A6C40` | 89 pointer slots. The following pointer begins collision data, not map 89. |
| Collision stage index | `0x4A6DA4` | 18 pointer slots / 72 bytes. |
| Stage-zero foreground header | `0x4BF1B0` | 36-byte MapHeader; grid 136x14 metatiles. |
| Stage-zero back header | `0x4BF1D4` | 36-byte MapHeader; same metatile/tile/palette banks. |
| Collision header | `0x4BF1F8` | 32 bytes; two grid dimensions and two pixel dimensions are 16-bit. |
| Height-map prefix | `0x4AF830` | 7,856 bytes: eight packed boundary bytes for each of 982 referenced tiles. |
| Angle prefix | `0x4B16E0` | 982 one-byte tile angles. |
| Metatile prefix | `0x4B1AB8` | 50,976 bytes: 177 metatiles, each 12x12 little-endian tile words. |
| Foreground grid | `0x4BE1D8` | 1,904 byte-valued metatile indices, row major. |
| Back grid | `0x4BE948` | 1,904 byte-valued metatile indices, row major. |
| Collision flag prefix | `0x4BF0B8` | 246 bytes sufficient for 982 two-bit flags, packed eight tiles per u16. |
| Stage palette | `0x4A7470` | 256 little-endian BGR555 colors / 512 bytes. |
| Base foreground tiles | `0x4A7670` | Intrinsic base size 31,424 bytes; extra animated tile frames are separate. |
| Background header | `0x62F474` | 28-byte Tilemap; 32x32 entries, with a null/shared palette pointer. |
| Background base tiles / layout | `0x62D274` / `0x62EC74` | 6,656 4bpp tile bytes / 2,048 layout bytes. |
| Spawn pair | `0x4A6DEC` | Signed little-endian 16-bit coordinates `(104,444)`. |

The collision header contains six pointers: height map, angle bytes, metatile
words, front grid, back grid and packed flags. Grid dimensions at bytes 24/26
are `(136,14)`. Pixel dimensions at bytes 28/30 are `(13056,1344)`. The ASOE
grid indices here are **u8**, not the u16 layout candidates from the earlier
broad catalog. Tile words retain the 10-bit tile index, X/Y flip bits and
four-bit palette bank.

Each height-map byte stores a vertical boundary in the high nibble and a
horizontal boundary in the low nibble. Zero is empty, nibble eight means full,
and other values encode a signed partial boundary. Directional sampling applies
the native coordinate flips, boundary transforms, two-bit flags and angle
transforms. Layer 0/1 selects the grid; option `0x80` suppresses tiles whose first
flag bit is set. Other layer bits are rejected. Outside-world samples return
`None`; retail edge clamping is deliberately not claimed.

Selected source span hashes:

| Span | SHA-256 |
| --- | --- |
| Collision index, 72 bytes | `95bae0d3f9027a10a12e0f4dd99ae2c8024c64777a13f804c772f78a511edd6d` |
| Stage-zero collision header | `e880e0d1791c8b57e319297b8df40a8f1db6686d519f3d3f7ae93a3191002f80` |
| Height-map prefix | `2c13a531cca06371f829e5334ca82d73446b36e01c469b7ae448c1ebb89f0551` |
| Angle prefix | `3c9b6c2097f83af5298a0d30f702eb979aa2b6dabf75ed7d31ff8c6f6793da33` |
| Metatile prefix | `ec205d8e21302c4d7f50ff0dd6e77f7dc30f375126bca2ecc3145c22f35968ee` |
| Foreground grid | `ff06829ea0b281e7b059739ca627c64d3f3c711c8b235bfafb07a3eaba4d9d88` |
| Back grid | `a7785dae502068ba8c2edcf3de6d96156b05e7cbd040a08b1498f3c2bbff6945` |
| Flag prefix | `f9d23e5db890b8bd71cae71c1888132b505435db1005566aeda2562cf0f5a2c2` |

## Independent collision and pixel evidence

Synthetic collision tests cover every encoded nibble under all four flip
combinations. Their expected occupancy reflects the raw pixel coordinates,
independently of the reader's signed-boundary transform. Separate cases cover
directional angle behavior, flag suppression, missing/truncated graph data,
invalid pointers/dimensions/layers and probe budgets.

An independent local Python reader follows the raw source pointers and checks
these stage-zero floor queries. Results identify the first occupied pixel, not
the character center or a complete native player sensor result.

| Query: x, starting y, layer | First solid y | Angle byte |
| --- | --- | --- |
| `104,444,1` | 460 | 0 |
| `240,400,1` | 460 | 0 |
| `512,400,1` | 416 | 8 |
| `800,400,1` | 425 | 242 |
| `1200,400,1` | 436 | 0 |

The same `(104,444)` query on layer zero finds no floor within 500 pixels. The
reference initializes the player on the back collision layer, so the runtime
must not silently use layer zero. The practice body uses half-width six and
half-height fourteen. Its ground settlement/sweep is an explicit adaptation;
the complete retail sensor placement and penetration correction are unported.

An independent Python renderer reads packed tile bytes directly, applies palette
banks/flips and bit-replicated BGR555 expansion, then composes screen-anchored
background, back layer and front layer. Index zero is transparent for overlays.
No Rust decoder is imported into this oracle. These hashes cover 153,600 RGBA
bytes per 240x160 frame, without any OBJ/player/ring overlay:

| Camera origin | RGBA SHA-256 |
| --- | --- |
| `(0,364)` | `4961cea784a74d95e3364ddc17175bbd31989dd3e1ca69dfdc922db42a7e2643` |
| `(400,336)` | `b093a6582ba219fcf199b3e1954b52f3d2760e6348a1318e307f5cc2a6196333` |
| `(680,345)` | `5fc5902cbc8d65887728b2d6cebb5dc87b063d27ccccb7768ec21d4f8c9b5904` |

These are representation/composition parity checks for the stated policy.
Screen anchoring, static base tile-animation phase and camera following are
current presentation choices; original parallax, palette effects and animation
scheduling need separate evidence. The local PNGs are research artifacts and
are not distributed with the crate.

## Locomotion, sprites and rings

The selected reference rules use 256 subpixels per pixel. Base ground
acceleration/friction is eight, ground braking is 96, air acceleration is 16,
gravity is 42, jump impulse is 1,248 and released-jump vertical limit is -768,
all in the relevant fixed-point units. Input-accelerated ground speed caps at
1,152, or 4.5 pixels per tick. The run-animation threshold is source-derived;
this limited state selection is not the original complete character state
machine. The reference's SA1 path does not use the SA2-only fall-speed cap.

Selected native script roots for the presentation slice are:

| Animation / variant | Script offset | Bytes | Source-script SHA-256 |
| --- | --- | --- | --- |
| `0 / 0` | `0xD86A4` | 768 | `8445dbf890ab13fa1e6cd9508cce84d562436009890e8ffc0a36f75a8888e02b` |
| `4 / 0` | `0xD8E5C` | 192 | `aa403db60adfca1663f13ea286bb67f92b691d9ad2b964cc920d2f4a6ee34c4b` |
| `5 / 0` | `0xD8F20` | 112 | `341e0600d73f66ee4f3225081de97e0284ab928b8be44357cbc9cf85d971ba1e` |
| `8 / 0` | `0xD904C` | 124 | `608bc84205b08eb54f905bc49db1a85441e6985e5fd2387ba605d1b6789ab8f1` |
| Ring `707 / 0` | `0xF502C` | 64 | `32e8505bf4d90c96ea328d1b46873ab9bd4c83c42fdb7b5cd4ef74d6cb573085` |

Source OAM/tile/palette data supplies the images. Where clips need affine
objects, this slice uses an explicitly chosen identity matrix; it does not
recover the original scale/rotation matrices from an index alone. Fixed native
script durations are retained rather than claiming original velocity-dependent
cadence, transition timing, terrain rotation or state-machine execution parity.

The ring pointer table is at `0xB7F4C`. Its stage-zero RLE stream is at
`0x6C8D8C`: 969 bytes decode to 1,886 bytes and 291 placements. Compressed
SHA-256 is `50a06a7609e24ca43a1b8ccc2cdc2e1d330c074572717e8494eaa89cc51a3987`;
decoded SHA-256 is
`fdf280edc42746ec4e505ce5618828f5a6ff3fbb82d6734eca19e9e60445132f`.
The ordered packed little-endian i32 `(x,y)` coordinate digest is
`3e02b6cfb65a35132218f1ba779eb5974aec4b09b94230894bcd222686e87f5d`.
The independently fetched pinned placement CSV agrees with those coordinates.

Pickup bounds follow the reference ring rectangle `[x-8,x+8]` by `[y-16,y]`.
The practice ledger records each source identity once and resets locally. Native
region scan scheduling, damage/ring loss, lives, shields, magnetic attraction,
sound and multiplayer remain outside this pickup slice.

## Reference provenance

Primary research is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. It describes the SA1 reference and
European layout; ASOE offsets above were verified against the separately
fingerprinted USA bytes. No global regional offset shift is assumed. Third-party
implementation text and artwork are not copied into the public Rust source.
The reference informs format/rule facts; incorporating its implementation would
require a separate license/attribution review.

Selected exact reference-text SHA-256 values:

| Pinned path | SHA-256 |
| --- | --- |
| `include/core.h` | `6b940bf2d0dff3499fe0ae787773a7598a2cb16eee1d791c229d3b7eb1cac0ef` |
| `data/sa1/maps/zone_1/act_1/tilemaps/fg/header.c` | `47b99a464373272183bc34b18362bde4a99a9262cef0a2ccbf7c0cc0fa7aa521` |
| `src/game/shared/stage/terrain_collision.c` | `777229b37e656108cbb8dafafff9ea361012e682cba4e63b987b2de474e5c0d2` |
| `src/game/shared/stage/player.c` | `300f37f946203ab4bb20fbbd5c62b20b0a5e9f55618ffc58bbd8f2b13d49bdb8` |
| `include/game/shared/parameters/characters.h` | `bb8489c7f0c5681e05970dce43e96495de997d8584fa71ff735a9ecf33423aed` |
| `src/game/shared/stage/rings_manager.c` | `fe99525a59b16084231b05784002aba9b78137149de5fce478e4cb099014e73f` |
| `src/data/sa1/entity_tables.c` | `d8921767fe3c14fc12ed1779262ccc8ff15a11e27b5f3b06dbae2a8b821e5297` |

The existing catalog findings remain in [FINDINGS.md](FINDINGS.md). Broader
manifest/workspace adoption remains the proposal in
[SCHEMA-PROPOSAL.md](SCHEMA-PROPOSAL.md). This slice changes no server protocol,
other game crate, shared workspace, canonical profile or deployed service.

## Complete-game acceptance still open

- Native ASOE input/replay comparisons must establish exact frame ordering,
  arithmetic, collision sensors, slope/loop/wall/ceiling traversal and transitions.
  Same-output Rust replay only proves this implementation's determinism.
- Rolling, spindash, braking/skid transitions, attacks, springs, moving platforms,
  enemies/bosses, water, powerups, death/lives/checkpoints and goal/results need
  native behavior and failure-path evidence. A practice reset is not native death.
- Other acts/zones, special stages, other characters, menus, language/font paths,
  multiplayer and full-stage completion remain unported runtime paths.
- Correct OBJ affine transforms, full animation state/cadence, tile/palette
  animation, parallax, draw priorities/windows and native audio need independent
  representation and execution checks.
- Native save fields/checksums/generations and hardware transfer success/failure
  ordering remain unresolved. Tiny Garden's complete resources, care/shop rules
  and minigames remain separate native-system work.
- Browser/WASM builds need actual input, rendering, recovery and performance
  evidence for their exact artifact. Mobile MMO integration additionally needs
  the agreed authenticated Rust service round trip; this solo slice grants none.

A successful native stage frame, ring decoder, deterministic practice replay or
WASM build advances its own gate. Each remaining gameplay subsystem still needs
implementation, version-specific evidence and an exercised playable path.
