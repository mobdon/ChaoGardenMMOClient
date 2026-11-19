# Reused bounded readers

`src/reused/gba.rs` and `src/reused/native_save.rs` are unchanged copies from the user's independently published `mobdon/ChaoGardenMMOServer` repository, commit `a8188a3030eb176ec4c3b7156f93410da5bb6f09`:

| Original path | Git blob | File SHA-256 |
| --- | --- | --- |
| `crates/tiny-garden-core/src/gba.rs` | `7a3da9127012090e8725562883a38ddeebeade33` | `0b635aaae6a9d96b85de53c756af3da18d0c4506f90616bf6240d56f2b97b30d` |
| `crates/tiny-garden-core/src/native_save.rs` | `5a52d91dcb00fd547b3957979d4c191d9221af48` | `608a1dbd2b3a127c34a1b6608f3bc330e27e9d31badcf501369f677880acc47e` |

Reuse includes their corruption/truncation tests, output caps, overlapping-LZ77 handling, exact consumed length, raw header/save preservation, native X-25 checksum and unsigned generation/tie semantics. The asset crate adds a stricter full-ROM size/SHA-256 gate before version-specific catalog use. It does not pull in `TrainingLedger` or server/gameplay authority.

A vendored snapshot allows a clean client checkout to build without an ignored sibling server checkout or a moving Git dependency. A future coordinated common reader crate should replace these snapshots without silently diverging implementations. This PR changes no server file and no shared `asset-tools` workspace/schema.

The SAT-R/sa2 material is used as pinned format/boundary evidence. New modules are independent implementations of format facts. No SAT-R implementation, ROM include, art, audio, native table contents or third-party source license has been imported. If future work incorporates external implementation code, its license must first be resolved and attribution preserved.
