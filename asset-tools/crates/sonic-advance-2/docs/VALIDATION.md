# Reproducible validation

Verified November 18, 2026 with the existing project-local Rust/Cargo 1.100 GNU Windows toolchain. No source ROM, installation, live save, running preview or server file was changed. Only this crate is included in the PR.

## Results

- **37 tests passed**, including 33 synthetic/metadata tests and all four opt-in actual-source tests. The tests cover all short headers, malformed/truncated/oversized LZ77, invalid back references, save marker/CRC/generation/ties, palette/tile/map limits and flips, motion record preservation, pointer domains/null slots, dummy audio headers, transfer lengths/order and local output restrictions.
- Strict `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` passed.
- Independent Python stdlib decoding verified the exact ROM and embedded payload, then generated **261 source views: 94 decoded and 167 opaque**. These are overlapping views, not an exhaustive coverage percentage. The count comprises 156 reference boundaries, 103 semantic views and two built-in identity/payload views.
- Rust matched every source SHA-256 and every decoded byte count, unit count, format and SHA-256 in that oracle. Palette words, tile nibbles, map words, motion fields and pointer values also re-encode to the actual source bytes.
- Three main-game map Git blob SHA-1s matched the pinned source tree. Rust descriptor checks and composed RGBA hashes matched an independent direct packed-nibble renderer for the title background, Zone 1 Act 1 background and enter-name UI.
- All 256 native CRC lookup words match reflected polynomial `0x8408`; source-derived name/default tables retain their exact fingerprints and unknown bytes.
- CLI `catalog`, `extract`, `render-map`, `save` and `transfer` smoke checks passed. The extracted garden equals the pinned payload SHA-256; the CLI RGBA output equals the independently rendered title output. Save/transfer CLI inputs used explicitly synthetic headers containing the actual ROM-derived opaque default block.
- Vendored reader file hashes remain identical to the published server snapshots documented in [REUSE.md](REUSE.md). All public files are source, metadata, tests or docs; native outputs stay ignored under `.local`.

`catalog/parity.json` SHA-256: `750e250affb1438db3332a3acd28dc98b921d212a8513cc3d06194e0c2c5e9d4`.

## Reproduce from a clean checkout

```powershell
$crate = 'asset-tools/crates/sonic-advance-2/Cargo.toml'
cargo test --locked --manifest-path $crate
cargo clippy --locked --manifest-path $crate --all-targets -- -D warnings
cargo fmt --manifest-path $crate --check
# Set this to the owner's exact local raw A2NE file; do not commit it.
$env:A2NE_ROM = (Resolve-Path '.local/gba/A2NE-f2b7a3329401.gba').Path
python asset-tools/crates/sonic-advance-2/tests/independent_parity.py
cargo test --locked --manifest-path $crate -- --include-ignored
```

The Python helper reads original bytes, implements LZ77 and typed conversions independently of Rust, and writes fingerprints/counts only. Its three map renders run directly from packed source pixels without reading extracted images or Rust output. It rejects unsupported formats rather than silently skipping them. The committed oracle lets normal reviewers inspect evidence without possessing a ROM; executing the opt-in tests requires the owner's local input.

## Practical limits

No supplied native `.sav` or real JoyBus transcript was available. Synthetic headers and word lists validate preservation, CRC/selection and framing, not real-device transfer acceptance. M4A parsing validates table/header structure and pointer starts; target lengths, waveform reconstruction and hardware mixing are unknown. Sprite composition records and animation pointers do not implement either motion VM. Unnamed minigame groups retain exact routine identities, without unproved Memory/Bounce ownership or reward semantics. See [CATALOG.md](CATALOG.md) for the reference/ROM discrepancy left opaque and the rest of the native-system gaps.

No MMO adaptation, production importer, new protocol, pairing, training, progression grant or live save mutation is included. Full main-game mechanics and exhaustive native-system parity remain separately gated.
