# Shared asset manifest coordination

Proposed November 18, 2026. Contract: `proposed-sa2-only`, `schemaVersion: 1`.
The SA2, Advance ASOE, Advance 2 A2NE and Pinball A3VE chats were contacted before
shared edits. ASOE confirmed the principles below and requested hierarchical
address spaces, logical paths and independent representation/fidelity states.
No common asset workspace/core owner has been selected. Integration owns final
cross-game types; this standalone crate does not define an agreed shared API.

| Field | Meaning |
| --- | --- |
| `game`, `revision` | Game/platform scope; exact build is gated per source hash, never inferred from extension/header alone. |
| `sources[]` | Logical relative path, stable `sa2.pc.source.*` ID, full byte count and lowercase SHA-256. No absolute private machine paths. |
| `assets[].id` | Stable namespaced locator within the logical source context. Pair with exact source hash when loading/cache-keying. |
| `span` | Source ID, byte offset/length/hash, address space and optional parent ID. |
| `addressSpace: file` | Span in the exact source file named by `sourceId`. |
| `addressSpace: decoded-container` | Span in a decoded parent output. Parent is a decoded `prs-decoded` asset for this implementation. Its output length/hash identifies the containing bytes. |
| `representation` | `decoded`, `opaque`, `unknown`; independent of whether native purpose is understood. |
| `fidelity` | `verified`, `adapted`, `unknown`; successful decoding alone never upgrades semantic fidelity. CLI conversions conservatively use `unknown`. |
| `output` | Optional relative path, format, byte count and SHA-256. Paths resolve beside the manifest. No embedded output bytes. |
| `dependencies` | Other exact sources used by conversion. External palettes include source ID, bounded color span/hash, relative bank and 16/256 entry count. Full palette source hash also preserves header identity. |
| `unknowns` | Nonempty explanations for unknown fidelity; zero/null values never fabricate discovery. |
| `policies` | Native units/hierarchy/proportions and straight RGBA preserved; original versus presentation adaptation separated; gameplay/entitlement limits. |

The machine schema checks field shapes; the Rust validator additionally checks
unique IDs, source references, range arithmetic, whole-source hash consistency,
decoded-parent ownership/type and typed palette dependencies. Validation does not
reopen local files and must never be reported as fresh content verification.

Current conversions preserve source units, raw native BAMS angles, node scales,
original alpha and unknown material/vertex words. Eyelid omission, body opacity,
child pupil cutouts, centering, root-motion changes and shader adaptations from
the existing Blender/browser flow are **not** applied by these Rust readers.

Future shared types must support multiple decoded levels (GBA `rom` versus
`garden-decoded`, SA2 PRS/archives/native intermediates) without flattening those
offsets into a fictitious physical source range. This crate currently validates
one PRS-decoded parent level; further nesting needs an explicit versioned change.
SA Tools intermediate files are source identities in their own right; the
existing extraction report links them to installation sources. Rust does not
invent a contiguous DLL/executable range for a relocated native intermediate.

No client/server wire contracts or gameplay authority change is needed. A future
adapter should preserve hierarchical provenance and local-only boundaries;
moving codecs into a shared core remains integration work after parity review.
