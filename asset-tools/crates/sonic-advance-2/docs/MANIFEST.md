# Proposed common asset manifest contract

This is a **local proposal**, not an agreed shared schema or a change to other game tracks. Only `sonic-advance-2` consumes it. Integrators can adopt/translate it after coordinating the SA2, Advance and Pinball Party workstreams.

The metadata-only runtime manifest uses `schemaVersion: 1`, `tool`, `referenceCommit`, `behavior`, full source hashes, payload hashes/consumed length, `resources` and `unresolved`.

Each resource carries:

| Field | Meaning |
| --- | --- |
| `id` | Stable ASCII identifier; only letters, digits, dot, underscore and hyphen. Never a source filename or display title. |
| `source` | Explicit `rom` or decoded `garden` address space. |
| `offset`, `length` | Nonnegative byte range relative to that source, checked before reading. |
| `category` | Source-supported system grouping, including `unclassified`. |
| `format` | Decoder selection, independent of category. `opaque` never guesses content. |
| `evidence` | Pinned reference path/function/declaration or recorded source proof. |
| `status` | `verified`, `decoded`, or `opaque`; no runtime reachability claim implied. |
| `notes` | Native interpretation, remaining uncertainty and relevant limits. |
| `sha256` | Hash of the exact source range. |
| `decoded` | Optional format, byte count, unit count and hash of deterministic decoded output. No source contents. |

The manifest identifies native data; it does not carry translated MMO rules, entitlement, ownership or progression authority. A valid native checksum is not a server authorization primitive. Output dimensions/color/transparency rules belong to each converter's documented contract.

Semantic resources may overlap source-boundary ranges. Coverage/reachability metrics need an interval union and traced consumers, not a sum of resource lengths. Unknown blocks are exported losslessly only on explicit local extraction, never embedded into public metadata.
