# Proposed game asset manifest v1

This is an isolated proposal for coordination with the SA2, Advance and Advance 2 asset tracks. No shared workspace, registry, server route or cross-game core was changed. Each game can adopt the schema only after agreeing its source identities and address spaces; game-specific readers stay independent.

`catalog.schema.json` describes the metadata-only `catalog.json`. A source carries game code, revision, exact length and full SHA-1/SHA-256. Each asset carries a stable ID, source address space, offset/hex display, exact stored length/hash, encoding, decoded length/hash, same-space pointer-reference offsets, category, status and evidence text. Compressed stored length excludes alignment padding. Raw records repeat the source hash as decoded hash.

`rom` offsets address the original source file. `garden` offsets address the version-verified decoded program, based at 0x02000000. Resolving Garden bytes requires first verifying and decompressing the pinned container; offsets must never be added directly to the ROM container offset. Library extraction verifies container identity, stored hash, exact consumed length and decoded hash before returning bytes.

Statuses distinguish `structural-candidate`, `mapped-span`, `inferred-allocation`, `verified-container`, `verified-obj-tiles`, `verified-bg-tiles` and `verified-render`. Categories are descriptive, not proof of use. Source bytes, filenames containing user paths, decoded pixels/audio, saves and unknown opaque field contents never enter this manifest. Multiple records may share spans or overlap, and pointer literals may be incidental data. Thumb pointers require explicit code evidence rather than silently clearing a low bit.

Future schema evolution should add explicit container relationships and typed relocations for other games rather than interpreting their layouts using these A3VE assumptions. This proposal deliberately publishes no global game asset IDs, MMO field contracts or common asset crate changes.
