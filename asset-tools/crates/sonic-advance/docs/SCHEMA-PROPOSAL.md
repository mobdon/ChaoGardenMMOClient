# Shared asset manifest coordination proposal

Status: proposed, not an agreed cross-game contract. No workspace/core or other
game files are changed. The crate is standalone (`[workspace]`) until the asset
workstreams and integration owner approve shared ownership and dependencies.

Proposed conventions for the SA2, Advance, Advance 2 and Pinball workstreams:

- `schemaVersion` is an integer; game/revision/full-source SHA-256 gate every
  version-specific locator. Header identity alone never authorizes an offset.
- Stable IDs are namespaced (`sonic-advance.asoe.r0.*`). Offsets and lengths are
  unsigned byte counts. JSON uses integers; documentation may display hex.
- `addressSpace` distinguishes `rom` from `garden-decoded`. The current crate
  has one implicit decoded parent: `sonic-advance.asoe.r0.garden.program`.
  An explicit `parentId` is proposed for the future common schema, especially
  before supporting multiple decoded containers. Decoded offsets are never
  physical ROM ranges. Hashes fingerprint the bounded range, not padding.
- Store source offset/length/hash and optional decoded length/hash separately.
  Compressed input consumption is exact; extraction does not guess an end from
  the next pointer. Hashes are lowercase hexadecimal SHA-256.
- Representation status (`decoded`, `opaque`, `unknown`) and semantic confidence
  are independent. A valid stream/pointer does not prove an asset's purpose,
  reachability, execution, unused status, or complete gameplay parity.
- Unknown offsets/lengths/hashes are null with an explanation, never fabricated
  zero-length records. Preserve unknown record bytes through borrowed views or
  lossless local extraction; do not normalize away unrecognized fields.
- Manifests contain metadata/hashes only. Inputs, decoded art/audio/save data,
  downloaded references and private machine paths never enter public manifests.

The local manifest is intentionally narrow. Integration should choose the common
types and move generic codecs only after output parity across the exact versions
has been demonstrated. `tiny-garden-core` lives in an independent server history;
this client crate does not depend on a relative path into that ignored checkout.
Generic checked-read/LZ77 behavior is reused conceptually. Its A2NE-specific save
layout and retail offsets are not imported into ASOE.

Consumer request: review these fields before adding this crate to a common
workspace. No runtime/MMO protocol changes are required by this read-only tool.

Coordination, November 18, 2026: exchanged proposals with the SA2 asset
workstream. The proposed common envelope includes game/version, source size and
SHA-256, stable asset IDs/categories, source/output ranges and hashes, format,
parity and explicit unknowns. ASOE requested hierarchical address spaces and
containers, logical rather than absolute private source paths, and independent
representation/fidelity statuses. Both workstreams retain standalone proposals;
field casing, adapters and shared workspace ownership await integration review.
