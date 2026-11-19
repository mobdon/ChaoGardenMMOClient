# Sonic Advance ASOE asset tools

The native Rust port continuation now includes a shared desktop and browser stage runtime:
[port status and local play commands](docs/PORT-STATUS.md). Full-game parity
remains unfinished. Start now opens the [English main menu](docs/MAIN-MENU.md);
its five destinations are signals only.

Standalone Rust tooling for the supplied USA revision-0 source only. The full
8 MiB ROM must have SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
Other regions, modified ROMs and header-only matches are rejected before applying
any catalog offset. This is a verified tooling slice, not a complete game port.

The crate provides checked ROM pointers/ranges/header reads, bounded BIOS
LZ77/RLE decoding, BGR555 palettes, 4bpp/8bpp tiles, text maps and explicit RGBA
rendering, terminal-delimited animation commands, UI/map headers and m4a song
metadata. It verifies exact source-span and decoded-output hashes in
`catalog.json`. Semantic unknowns remain in [docs/FINDINGS.md](docs/FINDINGS.md).
Generic rendering support does not establish a resource's original screen,
animation behavior or runtime reachability.

Requires Rust 1.88 or newer. From the isolated worktree/repository root:

```powershell
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml
cargo clippy --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --check

$env:SONIC_ADVANCE_ASOE_ROM = 'C:/path/to/private/.local/gba/ASOE-afc29e3325ba.gba'
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test local_reference -- --ignored
cargo test --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test garden -- --include-ignored
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
native saves, downloads a ROM, executes game code or changes a live profile.

The proprietary local integration test is explicitly ignored by default; the
synthetic tests need no ROM or downloaded reference. Output parity means exact
decoded bytes/pixels for the tested representation. Native audio playback,
hardware transfer, save legitimacy and game behavior have separate gates.

All ROMs, saves, extracted art/audio, downloaded references and generated binary
outputs stay local. This crate contains original tool code, synthetic fixtures,
hashes and metadata only. It has no dependency on the independent server checkout
or other games' crates. The manifest schema remains a coordination proposal:
[docs/SCHEMA-PROPOSAL.md](docs/SCHEMA-PROPOSAL.md).
