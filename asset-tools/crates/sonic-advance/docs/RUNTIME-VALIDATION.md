# Native stage validation receipt

**Historical receipt for commit 8903db9.** See [HANDOFF.md](HANDOFF.md) for the stopped PR head, current browser verification, and two failing retail terrain tests.

Evidence date: November 18, 2026. Scope: the standalone ASOE stage-zero Rust
practice implementation, stacked on the catalog/tooling foundation in PR #4.
This receipt is not acceptance of a complete Sonic Advance engine port.

## Source and output checks

The exact local ROM is 8,388,608 bytes with SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
No ROM, save, image, sound, extracted blob or third-party reference source is
distributed. Source span hashes and independent Python/CSV/JavaScript pixel or
coordinate oracles are recorded in [NATIVE-RUNTIME.md](NATIVE-RUNTIME.md), the
integration tests and the existing catalog.

With portable Rust 1.100 and its `wasm32-unknown-unknown` target:

The declared Rust 1.88 minimum was not independently exercised in this checkout.

```powershell
$env:SONIC_ADVANCE_ASOE_ROM = 'C:/path/to/private/ASOE.gba'
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --include-ignored
cargo clippy --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --all-targets -- -D warnings
cargo clippy --offline --locked --manifest-path asset-tools/crates/sonic-advance/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo fmt --manifest-path asset-tools/crates/sonic-advance/Cargo.toml -- --check
cargo fmt --manifest-path asset-tools/crates/sonic-advance/wasm/Cargo.toml -- --check
./asset-tools/crates/sonic-advance/tools/build-runtime.ps1
node asset-tools/crates/sonic-advance/tools/verify-runtime.mjs $env:SONIC_ADVANCE_ASOE_ROM ./asset-tools/crates/sonic-advance/target/release/runtime-replay.exe
node --test asset-tools/crates/sonic-advance/tools/serve-runtime.test.mjs
node --check asset-tools/crates/sonic-advance/preview/runtime.js
```

- All **61 Rust tests passed**, including the explicitly ignored proprietary-source
  tests. Synthetic malformed-input tests require no ROM.
- Both parent/native and WASM-target Clippy checks passed with warnings denied.
  Both manifests passed formatting checks and release builds.
- The 530-tick replay contains eight checkpoints including the initial state.
  Every serialized state and SHA-256 of 153,600 RGBA bytes matches between the
  native Rust executable and the WASM module instantiated by Node.
- The replay exercises rest, walking, a held jump, landing, ring pickup,
  reversal, running and the left stage boundary. It ends grounded with three
  practice rings. A second WASM instance replayed in batches of up to four ticks
  reaches the same final state and pixels.
- Rejected input bits/batch sizes preserve both state and pixels. Invalid ROM
  allocation/load, wrong fingerprints, absent-runtime calls and reset are checked
  through the compiled WASM ABI. A failed replacement preserves the valid session.
- The loopback server test passes: exactly four literal routes, GET/HEAD only,
  bounded files, no directory traversal/source routes/request-body uploads, and
  rejection of foreign Host/Origin and cross-site subresource requests. A
  cross-site top-level navigation may open the root document.

The hash-only captured replay output is [RUNTIME-PARITY.json](RUNTIME-PARITY.json).
Its module hash identifies this local release build, not a claim that all toolchain
versions reproduce the identical binary. Native/WASM agreement proves the two
builds of this implementation agree; it does not establish retail ASOE replay
equivalence.

## Browser check and remaining acceptance

Chrome 157 loaded the loopback page and instantiated the WASM module. The local
ROM selector enabled, Resume/Reset correctly remained disabled before load, and
no console warning/error was captured. The page's empty state and controls were
visually inspected.

Interactive ROM selection could not complete in the connected browser: its
extension reported local-file access disabled. The native picker fallback could
not capture the Chrome window (`window capture timed out: timed out waiting on
channel`). No browser/OS permission was changed. Consequently actual browser
movement, jump/pickup, pause/resume/reset, tab-blur behavior and performance remain
**pending**, despite the compiled native/WASM replay passing. The browser adapter
is reviewable source with a startup smoke check, not fully exercised gameplay UI.

No canonical service round trip was attempted: this practice process has no MMO
integration endpoint. Service acceptance must use the separately agreed Rust
contract and server coordination board; this work does not satisfy that gate.

## Coordination and publication boundary

Only `asset-tools/crates/sonic-advance/` is changed. Other games' crates, the
browser garden entry points and the independent ignored server checkout remain
outside this patch. The existing manifest is still a schema proposal; no shared
protocol/schema adoption or runtime rollout is implied by source publication.
Any future integration should take this evidence and [SCHEMA-PROPOSAL.md](SCHEMA-PROPOSAL.md)
to the server board for agreement before altering common entry points.

The next full-port gates include exact native sensor/slope/loop physics, character
states, enemies/interactables, death/checkpoints/results, other stages/characters,
audio/menus, legitimate native save/transfer behavior and Tiny Garden rules and
minigames. Their detailed unknowns remain in [NATIVE-RUNTIME.md](NATIVE-RUNTIME.md)
and [FINDINGS.md](FINDINGS.md).
