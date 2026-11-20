# Original Tiny Chao Garden background and sidebar

`android/tools/export-tiny-garden.ps1` composes the supplied Sonic Advance 2
USA revision 0 garden background using the independently delivered Rust
`sonic-advance-2-assets` crate (client PR #1). It does not duplicate its ROM,
LZ77, palette, tile or map readers. All native inputs and outputs stay local.

```powershell
. C:/path/to/original/client/scripts/use-rust.ps1
./android/tools/export-tiny-garden.ps1 `
  -A2neCrate C:/path/to/a2ne-worktree/asset-tools/crates/sonic-advance-2 `
  -Rom C:/path/to/original/client/.local/gba/A2NE-f2b7a3329401.gba `
  -Output C:/path/to/client/.local/imports/tiny-garden-a2ne.png
```

The script generates an ignored Cargo harness, runs rustfmt checks, focused
Rust tests and strict Clippy, then writes a new PNG and `.png.provenance.json` receipt. It
requires the local Rust toolchain and cached dependencies (`png` 0.17.16).
It refuses an existing image/receipt. It neither reads nor writes Chao saves.
The committed renderer lockfile fixes the tested dependency versions; the
generated harness uses `--locked --offline` to retain deterministic PNG bytes.

To include this original background in the owner's local APK, copy only the
verified output into the ignored asset directory before building:

```powershell
New-Item -ItemType Directory -Force android/app/src/localArt/assets
Copy-Item .local/imports/tiny-garden-a2ne.png android/app/src/localArt/assets/
```

Android checks the pinned hash and 176×160 dimensions before displaying it.
Source-only builds without the owner's export retain the regular widget
background. Never commit or publicly distribute an APK containing native art.
The isolated browser instructions are in [the preview README](../preview/README.md).

Source gates are the asset crate's exact 16 MiB ROM SHA-256
`f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260`
and decoded garden SHA-256
`84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca`.
The crate also verifies native header/CRC evidence before offsets are used.

The native layout is traced in SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, `chao_garden/src/game.c`:
`sub_02001c60` uploads four LZ77 streams to BG VRAM at `0x1C00`, `0x41E0`,
`0x55E0` and `0x8000`, then composes its repeated UI tiles. `InitGarden`
copies the 22×20 map at decoded garden offset `0x21BC4`, adding tile base
`0x200`. The register table sets BG3CNT to `0x1F83`: 8bpp text background,
character base 0, screen block 31. The 256-color BGR555 palette starts at
`0x233FC`. Offsets are in the decoded garden, never added to the compressed
cartridge offset. The exporter preserves map flips and pixel values and
expands five-bit channels by bit replication without filtering.

The default resulting image is the **176×160 original garden background crop**. The
adjacent native status sidebar, OBJ/Chao/item sprites, OAM, animated state,
native behavior and save data are outside this garden export. Layering the canonical
MMO Chao over this background is a presentation adaptation. It must not replace
the Chao identity with the generic native sprite or imply a complete native
Tiny Garden port. The widget still displays Rust-authoritative needs/stats and
submits bounded care intents; pixels cannot grant progression.

Output dimensions, compressed/decoded source spans and hashes, map/palette
hashes, RGBA hash and PNG hash are recorded in the local receipt. No native
pixel arrays, ROMs, saves or generated images are included in Git.

Verified November 18, 2026 on the owner's exact A2NE source:

| Output | SHA-256 |
| --- | --- |
| `tiny-garden-a2ne.png`, 176×160 RGBA8 | `75eb4d2839418fc730773377a1135776a0c361cbb1b6967af8134d8d4b5469ae` |
| Decoded RGBA pixels, 112,640 bytes | `954fef181ef241422c58a2e4cc6b83c26897a0cabe945a1ca5f82026f3b8c8f9` |
| Original 880-byte map | `05bb04f9013c42a4cca06d9159f5fcd36d6e29ab6abf2ef06bff7c9a16603a36` |
| Original 512-byte palette | `3d8663b493740046758985db3c5897c3683d518bee45c8e932c66cb974f76e2f` |

Both focused Rust tests, strict all-target Clippy and rustfmt passed. An
independent local Python oracle reused the A2NE workstream's separately
expressed LZ77 oracle, reconstructed BG VRAM, applied the native map base and
directly indexed every palette pixel. All 28,160 PNG pixels (112,640 RGBA
bytes) and every upload source/decoded receipt matched. The PNG was opened
for visual inspection. Git exclusion checks passed for image and receipt.
This verifies this static background composition, not native app packaging,
runtime mechanics, service integration or animation.

## Original status sidebar

`-Kind Sidebar0` and `-Kind Sidebar1` export each original static BG2 sidebar
as a separate **72×160 RGBA PNG**. `Garden` remains the default and retains its
existing PNG bytes. For example:

```powershell
./android/tools/export-tiny-garden.ps1 `
  -A2neCrate C:/path/to/a2ne-worktree/asset-tools/crates/sonic-advance-2 `
  -Rom C:/path/to/original/client/.local/gba/A2NE-f2b7a3329401.gba `
  -Kind Sidebar1 `
  -Output C:/path/to/client/.local/imports/tiny-garden-sidebar-1-a2ne.png
```

The same exact-source gate, independently delivered Rust LZ77/tile/map/palette
readers and four native VRAM uploads apply. `InitGarden` decompresses the map
stream at decoded garden `0x24618` into 720 bytes. The source copies either
the first 360 bytes or the 360 bytes at decoded stream offset `0x168` as a
9×20 tile map and adds `0xE0` to each native map word. BG2CNT is `0x1E02`:
4bpp, character base 0, screen block 30. The exporter retains native flips,
palette banks, colors and palette-index-zero transparency. Neither original
map is edited to insert data or relabel it.

Native BG2 HOFS is `-171` and VOFS is zero. On a 240×160 screen, place the full
72-pixel source at `(171, 0)` and clip the screen at x240. The final three
source columns are outside the hardware screen. Both images also contain
three transparent columns at their left and right edges: alpha bounds are
`[3, 0, 69, 160)` and exactly 960 pixels are transparent. Preserve this padding
when composing the source garden/sidebar. Scaling a combined frame is a
presentation adaptation; no filtering or stretching occurs in the exporter.

Visual inspection confirmed Japanese labels in variant 0 and English labels
in variant 1. The English sidebar contains Mood, Belly, Swim, Fly, Run, Power,
Stamina, the original level marks and bottom multiplication mark. **No Chao
name, stat numbers, mood/belly values or ring total is baked into either
output.** The native mode flag's complete behavior and language selection
remain unported. The sidebar has five native skill labels; the MMO's seven
stats remain separately presented from canonical Rust state.

[TINY-MENU-ASSETS.json](TINY-MENU-ASSETS.json) records the checked source,
map/VRAM/palette facts and exact output hashes for local consumers. Both PNGs
and receipts stay ignored under `.local/imports`.

| Output | PNG SHA-256 | RGBA SHA-256 |
| --- | --- | --- |
| `tiny-garden-sidebar-0-a2ne.png` | `bb658deaf565e34bfb8ef700aab1f2a02f7b892cc04a1124a7fd89dd1ae5484f` | `79e6995f572e779e77a396ad0d00a81badf8b603f5613b69bdf8e8d997c8e964` |
| `tiny-garden-sidebar-1-a2ne.png` | `cdad97d68c5af4423e6eb2a0e5d0c1edface2821b920755758053332f531baad` | `37584d9e88f041cefabe4caa74d35869a64dd1b6a355f54cdadecdc424629494` |

Verified November 18, 2026: three focused Rust tests, rustfmt and strict
all-target Clippy passed. An independent local verification oracle reused
the asset workstream's separately expressed LZ77 oracle, reconstructed VRAM
with 32-bit word OR operations, and directly indexed every original map/tile/
palette pixel. Both 11,520-pixel outputs (92,160 RGBA bytes total), all source
upload receipts and map/palette hashes matched. Each map has 20 horizontal
flip entries; the Rust fixture also exercises palette and vertical-flip bits.
The fresh garden regression export retained PNG SHA-256 `75eb4d28…b5469ae`.
Local evidence is `.local/verification/a2ne-sidebar/evidence.json`; each image
also has a complete `.png.provenance.json` receipt. Both sidebar images were
opened for visual inspection.

This exports the original static sidebar presentation. Dynamic glyphs, menu
selection/cursors, additional BG/OBJ layers, native UI state transitions,
items, minigame scoring, AFK behavior and native save/mechanics are outside
this tool. Display live values only from the agreed Rust service; original
pixels cannot establish a completed native Tiny Garden system port.
