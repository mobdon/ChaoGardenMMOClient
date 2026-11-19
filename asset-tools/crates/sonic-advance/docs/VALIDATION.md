# Validation receipt

Verified November 18, 2026 with Rust 1.100.0, Cargo 1.100.0 and Node 26.13.0 on
Windows GNU. The crate declares Rust 1.88 as its minimum; that older compiler
was not available locally and was not independently exercised.

From the isolated worktree, with `SONIC_ADVANCE_ASOE_ROM` set to the private,
exact-fingerprint raw source:

```powershell
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --include-ignored
cargo clippy --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --check
```

- All **26 tests passed**, including all four source-dependent tests. The
  default suite has 22 synthetic/metadata tests and four explicitly ignored
  local-source tests; passing the default suite alone is not source parity.
- Every one of 667 catalog ranges matched its SHA-256. The complete ASOE source,
  compressed garden consumption/length and decoded program fingerprints matched.
- All 256 CRC-table words matched generated polynomial entries. Corrupting
  every table word individually was rejected without changing the input.
- The 906-slot sparse sprite indexes, 89 map headers, 65 UI records and 308 song
  references passed bounded parsing. The first animation script reconstructed
  exactly. Regional counts, null slots, shared palettes and four-byte dummy
  headers were tested on the actual source.
- Rust conversion matched 202 independent output hashes: 65 UI 4bpp-index
  candidate spans and 137 BGR555 palette spans. The JavaScript oracle was
  regenerated after adding a bounded file read and produced identical metadata.
- Strict all-target Clippy and rustfmt passed.

The CLI was also exercised against the same local source. `inspect` returned a
parseable 667-record metadata report. `extract-garden` wrote 138,752 bytes into
an ignored `.local` directory and matched SHA-256
`989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0`.
A repeat extraction refused to overwrite it and left its hash unchanged.
A directory not named `.local` and a ROM changed outside its header were both
rejected. Local smoke reports and output remain outside tracked source.

The supplied archive hash was independently rechecked:
`694735c56c1cac7791e1891f1552f5054a6cc5ddb441db0f20328eb6c39791d9`.
The isolated changes contain only this crate's original source, synthetic
fixtures, documentation and hash metadata. ROMs, extracted assets, saves,
downloaded references and generated binaries are excluded. No live preview or
profile was changed. Other games' crates and shared workspace files are untouched.

No native gameplay, sound playback, visual placement, hardware JoyBus exchange,
native save roundtrip or complete garden asset graph was validated. Those gates
remain open in [FINDINGS.md](FINDINGS.md). The common schema is proposed, pending
integration review, as described in [SCHEMA-PROPOSAL.md](SCHEMA-PROPOSAL.md).
