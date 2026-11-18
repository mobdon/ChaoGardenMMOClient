# Sonic Pinball Party A3VE asset tools

Read-only Rust tooling for the supplied USA/multilanguage **A3VE revision 0**. This crate owns its manifest, readers, tests and documentation; it changes no shared asset workspace or gameplay code. It is independently buildable pending agreement on the common manifest contract.

The whole 8,388,608-byte source must match SHA-256 `5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b` before any version-specific CLI offset is used. ROMs, native saves, reference downloads and converted art remain local. The committed catalog contains offsets, hashes and evidence labels, never asset contents.

Implemented: bounded BIOS LZ77/RLE, BGR555 palettes, 4bpp/8bpp tiles, text background rendering, OAM fields, 23 visual descriptors with variant indirection, 200 audio headers, the distinct embedded Tiny Garden payload, and opaque Garden save/flash inspection. Sprite assembly, animation scripts, instrument synthesis, native transfer packets and full pinball mechanics remain incomplete. Chao Ball resources have a traced loader and candidate semantic identification; runtime confirmation remains pending.

See [CATALOG.md](docs/CATALOG.md) for coverage/unknowns, [GARDEN.md](docs/GARDEN.md) for save/transfer/minigame evidence, [PINBALL-MECHANICS.md](docs/PINBALL-MECHANICS.md) for the separate mechanics backlog, and [MANIFEST.md](docs/MANIFEST.md) for the proposed interchange contract.

With Rust 1.88+ and Python 3.10+ available, run from the client checkout/worktree root:

```powershell
$sppManifest = 'asset-tools/crates/sonic-pinball-party/Cargo.toml'
$env:SPP_ROM = 'C:/path/to/your/local/A3VE.gba'
cargo test --locked --manifest-path $sppManifest
cargo test --locked --manifest-path $sppManifest -- --include-ignored
cargo fmt --manifest-path $sppManifest -- --check
cargo clippy --locked --manifest-path $sppManifest --all-targets -- -D warnings
python -m unittest discover -s asset-tools/crates/sonic-pinball-party/tests -p reference_parity.py -v
python asset-tools/crates/sonic-pinball-party/tests/reference_parity.py --rom $env:SPP_ROM --catalog asset-tools/crates/sonic-pinball-party/catalog.json
cargo run --locked --manifest-path $sppManifest -- verify $env:SPP_ROM
```

The workstation's existing Rust installation can be activated by dot-sourcing `scripts/use-rust.ps1` from the main checkout that contains `.local/tools`; a fresh worktree does not contain that installation. These tools do not start or restart any preview service.

Inspection emits metadata only:

```powershell
cargo run --locked --manifest-path $sppManifest -- catalog $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest -- audio $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest -- tables $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest -- save $env:SPP_ROM 'C:/path/to/local/native.sav'
```

`catalog` deterministically regenerates the committed catalog. Review regeneration before replacing it. `verify` checks every pinned stored/decoded hash and exact consumption. `save` accepts only a 64 KiB flash image or an exact 0x858-byte Garden record, preserves its opaque bytes and emits checksum/generation metadata. It does not import progression or write saves.

Native output requires an **existing `.local` directory inside the current checkout or outside any checkout**. An outer `.local/worktrees` ancestor does not make tracked files eligible. Output uses fixed names and exclusive creation; existing files are never overwritten.

```powershell
New-Item -ItemType Directory -Force asset-tools/crates/sonic-pinball-party/.local | Out-Null
$sppOutput = 'asset-tools/crates/sonic-pinball-party/.local'
cargo run --locked --manifest-path $sppManifest -- extract $env:SPP_ROM tiny-garden-program $sppOutput
cargo run --locked --manifest-path $sppManifest -- tiles $env:SPP_ROM garden-01614c garden-minigame-obj-palettes 4 $sppOutput
```

`tiles` writes an RGBA PAM tile sheet with the explicitly selected depth and palette. It does not guess sprite poses or timing. The library also renders native 32/64-tile text maps with screen-block ordering, flips and palette banks. Native source verification is opt-in: ordinary tests use synthetic bytes and metadata; the three ignored tests require `SPP_ROM`. No native save was supplied, so save validation currently has native code/table evidence plus synthetic records, not a real-save parity claim.

All decoder/graphics logic was implemented for this tool. Local A3VE bytes and independently implemented Python checks establish the fingerprints. No third-party decompilation code, game code or art is incorporated. Advance 2 comparisons are research evidence only, and no Advance offset or Chao-field schema is inherited.
