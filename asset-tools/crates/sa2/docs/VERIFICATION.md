# SA2 Rust tooling delivery evidence

Recorded November 18, 2026 on the owner's Windows workstation. Native inputs,
reference tools, decoded pixels/audio/geometry and detailed logs remain ignored
under `.local`. Only Rust source, synthetic tests, schemas and documentation are
part of this change. The running preview and original installed games were not
modified.

Final checks passed on the complete crate with Rust/Cargo 1.100.0:
46 synthetic/unit/CLI/robustness tests, all 8 explicitly enabled local parity
tests, `cargo fmt --check` and strict all-target Clippy. The local parity runner
was executed successfully; its dated logs stay ignored. JSON schema syntax was
checked; cross-field validation is enforced by Rust rather than claimed from an
external JSON Schema engine.

| Area | Implemented and checked | Remaining boundary |
| --- | --- | --- |
| Inventory/provenance | Rust streams all 4,351 installed files, 3,253,731,852 bytes; every size and SHA-256 matches the prior verified complete inventory. Relative identities, bounded ranges and output/dependency hashes. | Filename categories are candidates, not decoded-asset counts or entitlement. No final-tree transactional snapshot against concurrent installation updates. |
| Archives | Bounded PRS, GVM, PAK and AFS; 5 PRS race/Karate decompressions match SA3D; 118 PAK payloads match installed loose duplicates byte for byte. | REL/event container extraction, unknown archive variants, PRS compressed-consumption/padding semantics beyond terminator remain unported. |
| Textures/palettes/UI | GVR, GVP, legacy DDS; 400 RGBA images match existing SA Tools/child-palette conversions exactly, including 883,883 nonopaque pixels. Includes all six existing Chao UI archive groups. 117 installed DDS entries decode. | DDS pixel parity against an independent decoder is not established. Base mip only; DX10/unknown masks and unverified small tiled shapes reject. 15 external palette mappings remain unresolved after verifying the 3 known child banks. Sprite use/layout is not a native UI implementation. |
| Models/rigs | All 171 verified PC Chao intermediates parse; original child has 40 nodes/16 attaches. Seven cinematic emeralds match 124 vertices, one mesh, bounds within 0.0001 and native 0.8 diffuse alpha; palm matches 61 vertices/five meshes. | GC visuals, render-strip triangulation/draw cache, Chao weight evaluation and GLB export remain unported. No complete Chao skinning/material/shader parity claim. |
| Motions | All 279 verified native animations parse; Idle/Walk retain 40 parts and native transform keys/BAMS angles. | Morph arrays need matched model cardinality; interpolation, evaluated pose, frame-rate inference and retargeting remain separate. |
| Stages/collision | All 20 Battle + 4 additional SA2 landtables parse. Basic collision vertices, polygon/strip indices, native node transforms and surface bits retained in local JSON. | World triangulation, collision response/navigation and parity with existing world heightfields are not implemented. GC stage visuals remain opaque. |
| Audio | Four ADX streams match 49,661,336 bytes of prior vgmstream PCM with zero differing samples. All 2,727 English AFS entries index; three selected AHX payloads match earlier extractions. | AHX/CSB, encrypted/type2/type4 ADX, native playback/mixing and MP3 encoding remain unported. ADX v4/5 history/header support lacks local parity. |
| PC mappings/Transporter | 488 DLL exports, 7 original Chao roots and all 4 mapped Transporter components checked. Component hashes match existing extraction; original native geometry retained. | Rust PE reads bytes only; native executable-to-model relocation/extraction remains SA Tools work. Machine assembly/collision, entrance/menu interaction, save/handshake ordering and durable device handoff remain open. |

The original DLL SHA-256 is
`6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59`.
Transporter/executable mappings remain tied to the exact local source hashes in
their earlier extraction manifests. A relocated `.sa2mdl` or `.saanim` uses its
own address space; the Rust tools never fabricate an installed-file byte span
for that generated intermediate.

Tests use synthetic corrupt headers, cycles, invalid pointers/indices, alias
amplification, output budgets, deterministic mutations and texture rectangle
truncations. CLI tests exercise hashes, palette dependencies, source-root
identity, unknown arguments, overwrite refusal and a real Windows directory
junction. Output .local checks apply below the **nearest** checkout root, so a
worktree inside another checkout's `.local/worktrees` cannot accidentally write
tracked assets. These tools assume no adversarial concurrent directory changes.

Reproduce source-backed parity after preparing the existing ignored conversions:

```powershell
. path/to/original/client/scripts/use-rust.ps1
cargo test --locked --manifest-path asset-tools/crates/sa2/Cargo.toml
cargo fmt --manifest-path asset-tools/crates/sa2/Cargo.toml -- --check
cargo clippy --locked --manifest-path asset-tools/crates/sa2/Cargo.toml --all-targets -- -D warnings
./asset-tools/crates/sa2/verify-local.ps1 -GamePath 'path/to/SA2' -ReferenceRoot 'path/to/original/client'
```

Missing local reference files fail the opt-in tests instead of being silently
counted as parity. The normal test suite requires no copyrighted assets. This
delivery changes no runtime/server protocol or shared asset workspace. The
manifest remains `proposed-sa2-only` pending integration's common-schema review.
No complete native gameplay, evolution, mission, UI or transfer port is claimed.
