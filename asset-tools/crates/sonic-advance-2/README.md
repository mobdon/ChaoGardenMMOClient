# Sonic Advance 2 A2NE: asset tools and Tiny Garden

Rust catalog, converters and standalone Tiny Garden practice runner for **one verified USA revision 0 cartridge** and its embedded Tiny Chao Garden. All code, tests, metadata and documentation stay inside this workstream's crate. The crate has its own workspace/lockfile so it does not change the shared asset workspace before the other tracks agree a contract.

The Garden runner executes Rust translations of the original behaviors. Its test oracles can separately execute locally supplied cartridge instructions to check parity. Opening an asset catalog alone does not play the Garden.

## Exact source gate

| Input | Size | SHA-1 | SHA-256 |
| --- | ---: | --- | --- |
| `SONICADVANC2`, `A2NE`, revision 0 | 16,777,216 | `7bcd6a07af7c894746fa28073fe0c0e34408022d` | `f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260` |
| Tiny Garden decoded payload | 191,340 | `2c7da8e1c9af85986eecf4effae39197cdac9abf` | `84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca` |

The payload starts at cartridge offset `0xAED70`; type-0x10 LZ77 consumes exactly 155,273 bytes. Full-ROM size/SHA-256, header, source SHA-1, decoded SHA-1/SHA-256, consumed length and every native CRC table entry are checked before using any catalog offset. A different region/revision is rejected even if its header looks valid.

`rom` offsets are relative to the cartridge file (native base `0x08000000`). `garden` offsets refer to the **decoded payload**, loaded at `0x02000000`; they cannot be added to `0xAED70` to locate compressed data. Evidence is pinned to SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, using the dedicated `chao_garden/mb_chao_garden.sha1` rather than the subproject README's main-ROM hash.

## Play the cartridge Tiny Garden locally

Build the standalone Rust runner and open the desktop viewer with your own verified A2NE ROM. Start with blank flash or supply an existing flash image as a read-only input. The viewer sends buttons and displays the pixels returned by Rust; gameplay and save state stay in that single local process.

```powershell
$manifest = 'asset-tools/crates/sonic-advance-2/Cargo.toml'
cargo build --locked --release --manifest-path $manifest --bin a2ne-garden
$exe = 'asset-tools/crates/sonic-advance-2/target/release/a2ne-garden.exe'
python asset-tools/crates/sonic-advance-2/tools/play_garden.py $exe $env:A2NE_ROM
```

Arrows move, Z is A, X is B, and Enter is Start where supported. Use the Garden buttons for hatching, shop, feeding, weeds and game entry. Click the Garden to place a held item. **Save copy** writes a new file under an explicit `.local` directory; it refuses to overwrite an existing file. Append the saved path after the ROM to reopen the same Chao. The executable also works without Python/Tk: enter `memory` or `bounce`, use `pad MASK [FRAMES]` and request `status`, `frame .local/view.pam`, or `save .local/new.sav`.

The tested cartridge path covers Memory and Bounce, native ring credit, hatching, purchasing and feeding fruit, weeds, and flash recovery. The command controls enter selected Garden callbacks directly. Free roaming Chao behavior, all toy interactions, full menus, native sound and physical transfer are unfinished. Search, Janken and Ball do not belong to the A2NE cartridge. See [Garden execution evidence](docs/GARDEN-PORT.md) for exact coverage.

## Build and use

Validated with the project's Rust 1.100 toolchain (edition 2024). Requires a local raw ROM supplied by its owner. No ROM download or extraction runs during normal builds/tests; a minimum supported Rust version is not claimed.

```powershell
$crate = 'asset-tools/crates/sonic-advance-2/Cargo.toml'
cargo test --locked --manifest-path $crate
cargo clippy --locked --manifest-path $crate --all-targets -- -D warnings
cargo fmt --manifest-path $crate --check
$env:A2NE_ROM = (Resolve-Path '.local/gba/A2NE-f2b7a3329401.gba').Path
python asset-tools/crates/sonic-advance-2/tests/independent_parity.py
cargo test --locked --manifest-path $crate -- --include-ignored
cargo run --locked --manifest-path $crate -- catalog $env:A2NE_ROM
cargo run --locked --manifest-path $crate -- extract $env:A2NE_ROM rom.tiny-garden.lz77 .local/a2ne-extracted
cargo run --locked --manifest-path $crate -- save .local/native-flash.sav
cargo run --locked --manifest-path $crate -- transfer 00040022 .local/native-transfer.bin
cargo run --locked --manifest-path $crate -- render-map $env:A2NE_ROM rom.bg.title.map rom.bg.title.tiles rom.bg.title.palette 32 64 .local/title.pam
```

On this workstation, dot-source the **original checkout's** `scripts/use-rust.ps1` to use its installed local toolchain. Worktrees do not contain a second copy of `.local/tools`.

`catalog ROM [REPORT.json]` emits only metadata, offsets and hashes. `extract` writes the exact source range plus an optional deterministic decoded stream; it requires a `.local` output path, rejects parent traversal/symlink ancestors and refuses to overwrite files. ROM filenames and archive paths never determine output names. `render-map ROM MAP_ID TILES_ID PALETTE_ID WIDTH_TILES HEIGHT_TILES .local/OUTPUT.pam` composes a caller-selected **row-major 4bpp text map** to RGBA PAM with transparent index zero. It does not infer tile bases, native VRAM assembly, OAM, affine layout or screen-block ordering. Those choices need the specific source evidence.

`save` reads six garden sectors in the first 64 KiB bank, reports other trailing bytes, and never repairs/writes a save. `transfer` inspects an explicitly supplied opaque payload and its native length command, not a live hardware transcript or networking session. See [save and transfer evidence](docs/SAVE-TRANSFER.md).

## Coverage and provenance

See [catalog evidence](docs/CATALOG.md), [manifest proposal](docs/MANIFEST.md), [reader reuse](docs/REUSE.md) and [validation](docs/VALIDATION.md). Every metadata range receives a source-byte SHA-256 at runtime. Range boundaries alone are `opaque`; semantic views can overlap these ranges and must not be summed as exhaustive coverage. Unknown fields stay available as original bytes, with no guessed Chao form/stat labels.

Native palette channels, tile pixels, map entries, motion records, save layout and JoyBus framing are compatibility evidence. New phone Chaos art, server-owned minigame rewards, AFK training, pairing/device leases and MMO balance are separate adaptations. This tool does not grant any of them.

ROMs, saves, transcripts, reference downloads and decoded assets belong in ignored `.local` directories. No native byte fixture, extracted image/audio, Chao save or third-party implementation is committed. Normal tests use synthetic buffers; opt-in tests compare source-backed hashes against the user's actual ROM. Public parity metadata contains fingerprints and counts only.
