# Native runtime validation receipt

Verified November 18, 2026 with the project-local Rust/Cargo 1.100.0 toolchain
on Windows GNU. The declared minimum Rust 1.88 was not independently exercised.
This receipt covers the native-runtime continuation on the source catalog's
exact ASOE revision-0 SHA-256. All native inputs/outputs remain local.

From the isolated worktree, with `SONIC_ADVANCE_ASOE_ROM` set:

```powershell
cargo test --offline --locked --all-features --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --include-ignored
cargo clippy --offline --locked --all-features --all-targets --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --check
```

All 53 tests passed, including the source-dependent cases. Strict all-target,
all-feature Clippy and formatting passed. All 667 original catalog span hashes
and 202 original graphics conversion hashes still pass.

New source checks include all 18 stage/collision graphs, 21 independent stage
RGB hashes, 25,280 collision samples, 35 independent sprite RGBA hashes,
3,600 animation execution/render ticks, all 18 ring layouts and all 3,837
ordered ring coordinates. The ring coordinates independently match every
corresponding pinned CSV; source and decoded hashes are in `native-rings.json`.

The actual 600-frame Rust integration replay walks, blocks at geometry, jumps
three times, lands four times and collects one local ring. Its receipt, trace
and RGB image agree. Repeated output publication rejects replacement, and a
deliberate receipt-path conflict preserves the pre-existing file while rolling
back the two new siblings. Invalid sources, output directories and frame
budgets produce no replay files. The runtime and CLI tests were completed by
the explicitly requested Astra agent, then included in the complete check.

The optional native-window build passed and its actual Windows window was
observed at the original spawn, settled to (104,446). Desktop capture timed
out and a later lookup found the window closed. Consequently interactive
keyboard behavior and screenshot-based desktop acceptance remain unverified.
The generated native RGB frame was inspected separately and shows the original
Sonic sprite and stage art. No capture/image artifacts are published.

These checks establish the documented readers, supported state paths and
representation/replay results. They do not establish a retail-frame gameplay
comparison, complete collision/physics, a completed act, audio playback,
save/transfer parity, a full Tiny Garden port or a canonical MMO service round
trip. This standalone local testbed adds no MMO mutation or network API. No
live preview, profile, wallet, credential or saved data was changed.
