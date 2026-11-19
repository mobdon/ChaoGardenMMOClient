# Sonic Advance ASOE asset tools

Standalone Rust tooling for the supplied USA revision-0 source only. The full
8 MiB ROM must have SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
Other regions, modified ROMs and header-only matches are rejected before applying
any catalog offset. The tooling now also supports a first standalone native stage
slice. A complete game port remains unfinished.

The crate provides checked ROM pointers/ranges/header reads, bounded BIOS
LZ77/RLE decoding, BGR555 palettes, 4bpp/8bpp tiles, text maps and explicit RGBA
rendering, terminal-delimited animation commands, UI/map headers and m4a song
metadata. It verifies exact source-span and decoded-output hashes in
`catalog.json`. Semantic unknowns remain in [docs/FINDINGS.md](docs/FINDINGS.md).
Generic rendering support does not establish a resource's original screen,
animation behavior or runtime reachability.

The stage slice implements Rust fixed-point walking/jumping, native terrain
profiles, native Sonic/ring sprite composition and session-local ring pickup in
Neo Green Hill Act 1. Rust produces the full 240x160 framebuffer. JavaScript
loads a local file, forwards bounded inputs and presents the Rust/WASM output.
Camera/terrain settlement, identity-affine walk/run and animation cadence are
limited adaptations; original ASOE execution parity remains open. Exact sources,
offsets, independent output hashes and the remaining full-port gates are in
[docs/NATIVE-RUNTIME.md](docs/NATIVE-RUNTIME.md).

Build the runner with the `wasm32-unknown-unknown` target installed, then start its
separate loopback server. This leaves any existing garden preview running:

```powershell
./asset-tools/crates/sonic-advance/tools/build-runtime.ps1
node asset-tools/crates/sonic-advance/tools/serve-runtime.mjs --port 0
```

Open the printed `http://127.0.0.1:PORT` URL and choose the exact local `.gba`.
Use left/right arrows and Z/Space; Pause/Resume and Reset are session-local.
The server serves only HTML, JS, CSS and the compiled WASM. ROM selection reads
browser memory directly; no source-upload endpoint or persistent storage exists.
No proprietary source or decoded art is included in the PR.

Native/WASM replay checks require that same local ROM and the built native
`runtime-replay` executable (use the platform's executable suffix):

```powershell
node asset-tools/crates/sonic-advance/tools/verify-runtime.mjs $env:SONIC_ADVANCE_ASOE_ROM ./asset-tools/crates/sonic-advance/target/release/runtime-replay.exe
node --test asset-tools/crates/sonic-advance/tools/serve-runtime.test.mjs
```

[docs/RUNTIME-VALIDATION.md](docs/RUNTIME-VALIDATION.md) records the exercised
build, source tests, native/WASM replay parity and the pending interactive browser
acceptance check.

Requires Rust 1.88 or newer. From the isolated worktree/repository root:

```powershell
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml
cargo clippy --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --check

$env:SONIC_ADVANCE_ASOE_ROM = 'C:/path/to/private/.local/gba/ASOE-afc29e3325ba.gba'
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test local_reference -- --ignored
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test garden -- --include-ignored
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --include-ignored
cargo run --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- inspect $env:SONIC_ADVANCE_ASOE_ROM
cargo run --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- extract-garden $env:SONIC_ADVANCE_ASOE_ROM 'C:/path/to/private/.local'
```

Add `--offline` to Cargo commands when dependencies are cached. This checkout's
portable Rust installation can be activated by dot-sourcing `scripts/use-rust.ps1`
from the original project root before entering a worktree.

`inspect` reads the ROM and prints metadata/hashes only. `extract-garden` requires
an existing directory named `.local`, derives a hash-based filename, and publishes
a complete decoded blob without overwriting any existing file. Publication uses
a hard link; unsupported filesystems fail closed. No command reads or rewrites
native saves, downloads a ROM, executes original ARM/Thumb machine code or changes
a live profile. The stage runner executes the new Rust practice implementation.

The proprietary local integration test is explicitly ignored by default; the
synthetic tests need no ROM or downloaded reference. Output parity means exact
decoded bytes/pixels for the tested representation. Native audio playback,
hardware transfer, save legitimacy and game behavior have separate gates.

All ROMs, saves, extracted art/audio, downloaded references and generated binary
outputs stay local. This crate contains original tool code, synthetic fixtures,
hashes and metadata only. It has no dependency on the independent server checkout
or other games' crates. The manifest schema remains a coordination proposal:
[docs/SCHEMA-PROPOSAL.md](docs/SCHEMA-PROPOSAL.md).
