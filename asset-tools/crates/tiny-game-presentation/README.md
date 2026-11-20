# Native Tiny minigame presentation

Rust exports the original Memory, Rock-Paper-Scissors, Chao Ball and Chao Bounce
graphics from the owner's exact supplied sources. This crate reuses
`sonic-advance-2-assets` for cartridge identity, bounded GBA LZ, BGR555 palettes,
4bpp/8bpp tiles and text maps. It supplies presentation assets to the browser;
rules, inputs, trusted time, identity, saves and payouts belong to the separate
Rust authority.

`catalog/native-games.json` is a metadata-only allowlist: filename/key, native
dimensions, PNG/RGBA SHA-256 and precise tile/palette/descriptor provenance.
The PNGs, cartridges and decoded programs stay in ignored `.local/`. No original
pixels, palettes, ROMs, program bytes or saves are embedded in this crate.

## Exact inputs and export

| Source | Identity |
| --- | --- |
| A2NE revision0 cartridge | `f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260` |
| A2NE decoded Tiny program, 191340 bytes | `84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca` |
| ASOE decoded Tiny program, 138752 bytes | `989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0` |
| A3VE revision0 cartridge, 8MiB | `5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b` |
| A3VE decoded Tiny program, 188716 bytes | `33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb` |

The A3VE program comes from cartridge LZ offset `0xD0970`, consuming exactly
152552 bytes. `--decode-a3ve-rom ROM --write-a3ve NEW_LOCAL_FILE` exports only
that verified decoded program. All output paths must be new under this
checkout's ignored `.local`; existing output/saves cannot be overwritten.

From the client repository root:

```powershell
. ./scripts/use-rust.ps1
cargo run --manifest-path asset-tools/crates/tiny-game-presentation/Cargo.toml -- --a2ne-rom PRIVATE_A2NE_ROM --asoe-payload PRIVATE_ASOE_PROGRAM --a3ve-rom PRIVATE_A3VE_ROM --output .local/NEW_DIRECTORY
```

The full source is gated before version-specific offsets. Internal A2NE/ASOE
LZ streams use signature byte `0x01`, accepted by the original native loader.
The compatibility adapter normalizes only this byte to `0x10` in a temporary
buffer so the existing strict LZ reader can decode them. Their data, palette
indices and pixels remain unchanged. Other signatures fail closed.

## Original presentation map

| Minigame | Native source and exported subjects |
| --- | --- |
| Memory / A2NE | `0200A944` video loader; `0200A45C` cards. Back, seven public pair faces, three board zones and the 240×160 original English board/sidebar. Pair filenames are public art IDs; native hidden placement IDs are absent. |
| RPS / ASOE | `0200B55C` video loader; tables `11F78`/`11FB4`. Three targets, three thrown cards/back and English background. Five original actor frames use the native five-piece OAM helper `8190/8284/82FC/8324`, retained normal appearance0 palette at `1C700`, and original 24×32 proportions. |
| Ball / A3VE | `0200D294` video loader, `0200E888` cells, `0200DE24` launcher, `0200E134` shot, `0200F194/F2DC` ring actors. Background, six cell types, eighteen launcher poses in both facings, two shot colors plus three original trail textures, falling/caught rings. The original BG3 is 8bpp; BG2/OBJ are 4bpp. Raw palettes use CpuFastSet, with OBJ loaded at bank1 and retained normal actor palette at `24050`. |
| Bounce / A2NE | `0200D294` video loader and `02001088` OAM producer. Original 240×320 scrolling world, fixed transparent HUD and85 actually submitted sprite tuples. BG3 control `9A83` selects 8bpp and 32×64 map storage; the mapped source region has24×40 entries. HUD uses original BG2/BG1/BG0 priority. |

`catalog/bounce-poses.json` contains descriptor/tile/style/transform/packed
palette-layer tuples observed at the original `02001398` sprite submission
across32 original scheduler traces /48503 frames. A prior active-task inventory
also saw unsent control records with stale inherited descriptors; those are
excluded. The source proof for this capture belongs to the server/core's
`verify_native_bounce.py` differential, retained locally under
`.local/reviews/native-bounce-submitted-a/`.

Each Ball/Bounce image preserves its original signed `anchorOffset`. Bounce
keys are `bounce-obj-{descriptorHex}-{tile}-{style}-{transform}-{packed}`.
The packed field is native byte18 (OBJ palette/priority), not a sprite count.
Its `composition.parts` records OAM flags, native canvas size and OBJ mode.
Non-affine subjects use the original piece order and per-piece H/V flips.
Affine subjects export the original untransformed source texture and retain
`requiresNativeAffine:true`, original matrix index and native canvas dimensions.
The renderer must apply the actual native affine/blend/window state to reproduce
those scene effects. These textures alone establish no affine/timing parity.

Colors expand original five-bit channels by bit replication. Palette index0
is transparent where original OBJ/BG composition requires it. Palette bit15
has no invented alpha meaning. No recolor, redraw, smoothing or proportions
adjustment is performed.

## Verification

Ordinary checks:

```powershell
cargo test --manifest-path asset-tools/crates/tiny-game-presentation/Cargo.toml
cargo clippy --manifest-path asset-tools/crates/tiny-game-presentation/Cargo.toml --all-targets -- -D warnings
```

Set `TINY_NATIVE_A2NE_ROM`, `TINY_NATIVE_ASOE_PAYLOAD` and
`TINY_NATIVE_A3VE_ROM` to run the actual-source Rust catalog test. It reproduces
every PNG/RGBA hash, checks exact ordering and rejects a one-byte source change.
Focused tests also cover the signature adapter, PNG roundtrip and an OBJ tile
overflow that could otherwise carry into a tilemap flip/palette bit.

Independent original-CPU proof (Unicorn2.1.4 and Pillow, installed locally):

```powershell
python asset-tools/crates/tiny-game-presentation/tools/verify_native_art.py --a2ne PRIVATE_A2NE_PROGRAM --asoe PRIVATE_ASOE_PROGRAM --a3ve PRIVATE_A3VE_PROGRAM --catalog .local/NEW_DIRECTORY/catalog.json --unicorn-dir PRIVATE_PYTHON_MODULE_DIRECTORY
```

The proof executes the four original Thumb video loaders, compares complete
BG tile/map, OBJ tile and palette layouts, runs original RPS five-part and
Ball/Bounce descriptor OAM producers, and independently decodes every exported
pixel. Explicit host shims supply only BIOS copies/LZ. Affine subjects compare
their original source texture and native OAM metadata; hardware affine
sampling, color response, scene fades and audio remain outside this proof.
`proof/native-art.json` records the accepted metadata-only result.

This is an original asset reader/exporter, not a full game-engine port or a
canonical service round-trip test. The fifth Tiny minigame, Chao Search, has
no accepted source in this exporter yet; it is not replaced with invented art.
