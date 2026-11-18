# Original Tiny Chao Garden background

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

The script generates an ignored Cargo harness, runs focused Rust tests and
strict Clippy, then writes a new PNG and `.png.provenance.json` receipt. It
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

The resulting image is the **176×160 original garden background crop**. The
adjacent native status sidebar, OBJ/Chao/item sprites, OAM, animated state,
native behavior and save data are outside this export. Layering the canonical
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
