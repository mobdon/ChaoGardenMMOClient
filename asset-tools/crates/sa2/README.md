# SA2 + BATTLE local Rust asset tools

Standalone, unpublished Rust crate for owner-local Steam PC resources and the
verified SA Tools 786 intermediates. No common workspace/core or runtime files
are changed. This ports asset readers and selected conversions; it does not port
the retail game engine, Chao simulation, missions or device transfer protocol.

The CLI emits native structured JSON, raw archive entries, lossless RGBA PNGs and
native-rate PCM WAVs. The existing SA Tools/Blender pipeline remains the verified
model extraction/GLB export path. Do not replace its live outputs with these tools.

```powershell
# Use the original checkout's already prepared Rust session, or a normal Rust toolchain.
. C:/path/to/original/client/scripts/use-rust.ps1
cargo test --locked --manifest-path asset-tools/crates/sa2/Cargo.toml
cargo run --locked --manifest-path asset-tools/crates/sa2/Cargo.toml -- --help
```

For repeated commands, build once and invoke the resulting `sa2-assets` binary.
Commands take explicit inputs and new output paths; every output must be under
the nearest checkout's ignored `.local` directory. Existing outputs are refused.
Archive names are metadata only: extracted files use numeric names, preventing
archive paths from being interpreted as filesystem paths. Installed inputs are
opened read-only; no saves or installed archives are modified.

```powershell
$game = 'path/to/Steam/steamapps/common/Sonic Adventure 2'
$tool = 'asset-tools/crates/sa2/target/debug/sa2-assets.exe'
& $tool catalog $game .local/sa2-rust/catalog.json
& $tool --source-root $game extract "$game/resource/gd_PC/al_icon.prs" .local/sa2-rust/icons
& $tool texture .local/sa2-rust/icons/00000.gvr .local/sa2-rust/icon.png
& $tool --source-root $game adx "$game/resource/gd_PC/ADX/chao_hall.adx" .local/sa2-rust/hall.wav
& $tool inspect path/to/local/al_ncz.sa2mdl .local/sa2-rust/child.json
& $tool inspect path/to/local/alm_tati.saanim .local/sa2-rust/idle.json
& $tool inspect path/to/local/objLandTableNeut.sa2blvl .local/sa2-rust/garden.json
& $tool transporter path/to/local/PC_SA2/ChaoCommon.ini .local/sa2-rust/transporter.json
& $tool validate .local/sa2-rust/hall.wav.manifest.json .local/sa2-rust/validation.json
```

`prs INPUT OUTPUT` explicitly decompresses PRS to a bounded local binary.
`extract` accepts PRS-wrapped GVM, direct GVM, PAK or AFS and writes a manifest
alongside exact entry bytes; texture/audio conversion is explicit. For external
CI4/CI8 palettes use `texture INPUT OUTPUT.png PALETTE.gvp BANK`. The bank is
relative to the first palette color; its source span/hash and entry count are
recorded. Supplying an unused palette is an error.

Use `--source-root INSTALL` before a conversion command to retain the same
installation-relative source identity as `catalog`. Without it, a single-file
conversion uses its basename as its logical source identity. Consumers must key
provenance by logical identity **and exact source SHA-256**, rather than trusting
an ID or filename across installations. Native intermediate offsets belong to
that intermediate; a PRS-decoded offset is not a physical compressed-file offset.

Each conversion gets a `.manifest.json` sidecar with full input hashes, bounded
span identities, output hashes and independent representation/fidelity states.
The schema is a proposal scoped to SA2, not an agreed shared API. `validate`
checks structure and relationships; it explicitly reports `contentVerified:false`
because it does not reopen and rehash source/output files. See
[schema proposal](docs/SCHEMA-PROPOSAL.md) and [schema](manifest.schema.json).

Inputs are capped at 512 MiB per direct conversion, PRS output at 128 MiB and
audio PCM at 128 MiB. Native decoded geometry/key storage and total labels have
separate aggregate limits. The installation catalog streams files in 64 KiB
chunks. Reparse/link outputs are refused; these offline tools do not provide
race-proof directory-handle isolation against another process replacing paths.

Run all source-backed parity checks with prepared ignored references:

```powershell
./asset-tools/crates/sa2/verify-local.ps1 -GamePath $game -ReferenceRoot 'path/to/original/client'
```

These tests are opt-in and fail if required references are absent. Normal tests
contain synthetic headers, malformed streams and CLI fixtures only. No original
asset bytes, art, audio, ROMs, saves, downloaded tools, generated JSON or converted
outputs belong in Git. The detailed logs stay under `.local/sa2-parity`.

Coverage, parity and limitations are recorded in [native formats](docs/native-formats.md),
[textures/archives](docs/TEXTURE-ARCHIVE-REFERENCES.md), [audio/PE](docs/audio-pe.md)
and [delivery evidence](docs/VERIFICATION.md). Finding BATTLE resources does not
verify entitlement. Models/textures decoded successfully do not establish native
gameplay parity.
