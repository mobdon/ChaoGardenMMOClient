# Sonic Pinball Party A3VE Rust port and asset tools

Rust tooling and source-port work for the supplied USA/multilanguage **A3VE revision 0**. All changes remain inside this independently buildable crate. The full game is **not yet ported or playable**; [PORT-STATUS.md](docs/PORT-STATUS.md) records implemented routines, explicit dependency boundaries and remaining acceptance gates.

The whole 8,388,608-byte source must match SHA-256 `5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b` before any version-specific CLI offset is used. ROMs, native saves, reference downloads and converted art remain local. The committed catalog contains offsets, hashes and evidence labels, never asset contents.

Asset tools implement bounded BIOS LZ77/RLE, BGR555 palettes, 4bpp/8bpp tiles, text background rendering, OAM fields, 23 visual descriptors with variant indirection, a 200-entry audio-header catalog, the distinct embedded Tiny Garden payload, and opaque Garden save/flash inspection. The separate Rust game modules translate input, ball/flipper helpers, collision paths, frame control, OAM composition, palette processing and audio routing/player lifecycle. Their verified scopes are narrower than a complete game. Chao Ball resources have a traced loader and candidate semantic identification; runtime confirmation remains pending.

Tiny Garden is the current priority. Its Rust board setup now runs for all four native rounds and enters the first frame, which stops at unported `0x02001420`. Care, shop, audio, saves/transfers and complete minigame sessions still prevent an end-to-end Garden pass. See [GARDEN-SYSTEMS.md](docs/GARDEN-SYSTEMS.md) for exact original subsystem roots.

See [CATALOG.md](docs/CATALOG.md) for coverage/unknowns, [GARDEN.md](docs/GARDEN.md) for save/transfer/minigame evidence, [PINBALL-MECHANICS.md](docs/PINBALL-MECHANICS.md) for the separate mechanics backlog, and [MANIFEST.md](docs/MANIFEST.md) for the proposed interchange contract.

With Rust 1.88+ and Python 3.10+ available, run from the client checkout/worktree root:

```powershell
$sppManifest = 'asset-tools/crates/sonic-pinball-party/Cargo.toml'
$env:SPP_ROM = 'C:/path/to/your/local/A3VE.gba'
$env:A3VE_ROM = $env:SPP_ROM
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

`tiles` writes an RGBA PAM tile sheet with the explicitly selected depth and palette. It does not guess sprite poses or timing. The library also renders native 32/64-tile text maps with screen-block ordering, flips and palette banks. Native source verification is opt-in: ordinary tests use synthetic bytes and metadata; ignored tests require the local ROM variables above. No native save was supplied, so save validation currently has native code/table evidence plus synthetic records, not a real-save parity claim.

The original asset CLI remains the default binary. Source-port diagnostics are explicit:

```powershell
cargo run --locked --manifest-path $sppManifest --bin spp-game -- status $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest --bin spp-game -- verify $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest --bin spp-game -- run $env:SPP_ROM
cargo run --locked --manifest-path $sppManifest --bin spp-game -- garden-board $env:SPP_ROM 0 1
cargo run --locked --manifest-path $sppManifest --bin spp-game -- board $env:SPP_ROM 3 --headless
```

`garden-board` takes a native round index 0..3 and a u32 PRNG seed. It runs complete puzzle setup on explicitly synthetic RAM, attempts the first frame, prints its exact missing dependency and returns failure while that dependency is unported. It does not run home Garden, render a full screen or read a native save.

`run` executes the Rust startup wrapper and stops at its first unported dependency, currently ROM `0x6EF4`. `board` renders original background pixels; on Windows, omit `--headless` for the finite diagnostic window and manual Up/Down viewport control. These commands do not run an emulator, alter a native save, or contact the MMO service.

All readers and Rust source translations were written for this work. Native ROM instructions, BIOS bytes, third-party implementation source, art and audio are not embedded. Local A3VE bytes, independent Python checks and bounded reference execution establish the evidence. Advance comparisons are research only; no Advance offset or Chao-field schema is inherited.
