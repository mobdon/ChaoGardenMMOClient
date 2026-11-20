# Original A2NE Tiny shop subjects

Standalone local Rust tool for the seven fruit sprites, twelve egg palette subjects and three toy sprites. It preserves the original 16×16 pixels, palette colors and index-zero transparency. It adds no shopping, care, hatching, toy AI, progression, transfer, accounts or service authority.

It also exports original ASCII glyphs in the native English sidebar's palette, plus composed Luck and Intelligence labels. These added words are explicit presentation adaptations; they were not baked hidden-stat labels in the GBA game. No desktop font or handmade pixel alphabet is used.

## Prerequisite reader

The tool depends on the reviewed `sonic-advance-2-assets` crate at the conventional sibling path **`../sonic-advance-2`**. That reader is included in this source delivery at `asset-tools/crates/sonic-advance-2`, from native asset commit `1a46b2f`. No private absolute path or external worktree dependency is needed. Source publication and merge acceptance remain separate.

`catalog/source.json` is readable source metadata: exact ROM/span fingerprints, numeric native IDs, visually checked subject names, tile/palette selections, output filenames and expected PNG hashes. It embeds no ROM, compressed stream, texture or image bytes. Egg color descriptions do not establish jewel names, material/genetics codes or stock eligibility.

## Run locally

From the client repository root, activate the existing Rust environment if needed, then provide both paths explicitly. The output's parent must already exist; the output directory itself must be new and inside this checkout's ignored `.local` directory.

```powershell
. ./scripts/use-rust.ps1
cargo run --manifest-path asset-tools/crates/tiny-shop-presentation/Cargo.toml -- --source .local/gba/A2NE-f2b7a3329401.gba --output .local/imports/tiny-shop-a2ne
```

Only the supplied 16-MiB A2NE USA revision 0 source is supported. The existing `VerifiedRom` gate checks its cartridge/header, decoded payload and CRC evidence; this tool additionally checks source/span fingerprints, all 22 shop output hashes, three font output hashes and native label parity before writing files. Modified, truncated or other-version sources fail. Existing output directories/files are never overwritten. No installed game, credential or save is written.

The output contains `tiny-native-fruit-{0..6}-a2ne.png`, `tiny-native-egg-{0..11}-a2ne.png`, `tiny-native-toy-{0..2}-a2ne.png`, and `provenance.json` with source/tool/output hashes. Native bytes remain local. A separate exact hash allowlist is required to expose selected pictures in the local phone preview; running this tool does not create routes or publish assets.

Font outputs are `tiny-native-label-luck-a2ne.png` (20×8), `tiny-native-label-intelligence-a2ne.png` (51×8) and `tiny-native-sidebar-ascii-a2ne.png` (128×48). The atlas contains original eight-pixel cells in sixteen columns, ASCII 32–126, with a blank final cell. Labels trim only horizontal background bearings, keep the original eight-pixel baseline, and separate glyph ink bounds by one pixel. Render at integer multiples with nearest-neighbor scaling to retain source pixels.

## Source trace

The original `InitGarden` loader (`sub_02001C60`) loads OBJ palette payload 0x246E4 and expands the stream at 0x268B4 into OBJ VRAM + 0x1580. Its 1D sprite mapping and composition record 0x205E4 produce 16×16 subjects from four original 8×8 tiles. Native constructors select:

| Subjects | Native constructor | Tile index | Palette |
| --- | --- | --- | --- |
| Fruit 0–6 | `0x02008B3C` | `0xBC + 4 × ID` | Banks from 0x20EFC in palette 0x246E4 |
| Egg 0–11 | `0x02008B7C` | `0xD8` for 0, otherwise 0xDC | Row at 0x24AA4 + 32 × ID; native bank 14 |
| Toy 0–2 | `0x02008BC4` | `0xE0 + 4 × ID` | Banks from 0x20F03 in palette 0x246E4 |

Offsets address the decoded 191,340-byte garden payload, not the cartridge's compressed stream. Local reference: SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, `chao_garden/src/game.c`, `chao_garden/asm/code.s` and `chao_garden/data/data3.s`. Original reference/art remains local. The scoped evidence and native/MMO mechanics distinction are in `docs/TINY-CHAO-SHOP-SCOPE.md` at the client root.

The original font stream at payload 0x220EC (2,904 bytes) expands to 337 tiles. `InitGarden` loads it at BG VRAM byte 0x55E0; native text selects `0x2AF + (ASCII − 32)`. The original background palette is at 0x233FC. English baked sidebar labels are in UI stream 0x235FC, with the second 9×20 layout in 0x24618. Their native ink roles bind glyph indices 1/2/3 to palette-zero entries 9/7/5. Index 4 is cream background, made transparent in the exported labels/atlas. The tool proves the complete newly composed Mood, Swim and Stamina rectangles reproduce their original baked sidebar pixels exactly after restoring the native cream background. Other baked labels have individual spacing differences and are not claimed as full-word parity. Luck/Intelligence use the verified native glyph shapes and this explicit one-pixel composition policy.

## Verification

```powershell
cargo test --manifest-path asset-tools/crates/tiny-shop-presentation/Cargo.toml
$env:A2NE_ROM = (Resolve-Path .local/gba/A2NE-f2b7a3329401.gba).Path
$env:ASOE_ROM = (Resolve-Path .local/gba/ASOE-afc29e3325ba.gba).Path
$env:A3VE_ROM = (Resolve-Path .local/gba/A3VE-5690eb0a58ac.gba).Path
cargo test --manifest-path asset-tools/crates/tiny-shop-presentation/Cargo.toml --test native_source -- --ignored
```

Focused tests cover metadata identities, PNG color/alpha roundtrip and invalid lengths. The opt-in native test checks all 22 source PNG fingerprints and 16×16 binary transparency, modified/truncated A2NE input, a wrong cartridge identity and optional actual ASOE/A3VE sources. This is static art parity, not full native-engine or browser/service acceptance.

The same native test checks the three font image fingerprints/dimensions, original ink color channels, binary transparency and native Mood/Swim/Stamina full-label pixel parity. It does not claim hidden-stat gameplay or Rust service integration.
