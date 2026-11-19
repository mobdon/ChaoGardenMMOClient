# Native model, motion and landtable readers

The Rust conversion is an inspectable native JSON representation. Source coordinates, rotation integers, hierarchy, scale, mesh bounds, texture references, material bytes and source offsets are retained. It does not rescale Chao, remove eyelids, override body alpha, triangulate render strips or retarget animations. The existing Blender pipeline remains the verified GLB export path.

| Input | Decoded representation | Explicit limit |
| --- | --- | --- |
| SA2MDL v0–3 | Original node tree; Chunk attach bounds; vertex positions/normals plus every attribute word; polygon/material/texture chunks with raw words and native diffuse RGBA | Supported vertex types 32–39 and 41–46; unsupported types reject. Strip triangulation, draw cache and Chao weights remain raw. |
| SA2BMDL v0–3 | Node tree, optional quaternion scalar, metadata labels and attach references | GC attach contents remain opaque. |
| SAANIM v1–2 | Explicit part count, short/long BAMS rotations, native frame numbers, typed transform/light/camera channels and raw key words | Morph targets retain bounded pointers; matching model cardinality is required to decode morph arrays. No interpolation evaluator or frame-rate inference. |
| SA2LVL / SA2BLVL v0–3 | Land descriptors, bounds, surface bits, visual/collision distinction, original node trees and texture-list references | GC visuals remain opaque. |
| Basic collision attaches | Source vertices/normals, triangle/quad/polygon/strip indices, strip reversal, material words, vertex colors and signed UVs | Local coordinates retain their node transforms. World triangulation, collision response and navigation are separate. |

Every reader checks referenced spans and count limits before reading. A shared 64 MiB decoded allocation budget covers node, geometry and keyframe arrays, including aliased source spans; label storage has an additional 8 MiB cap. Cycles/shared hierarchy nodes, truncated streams, non-finite floats, invalid indices and unsupported structural variants fail explicitly. Original files are immutable byte inputs; no reader opens a game installation for writing. Empty pointers with zero counts remain empty.

`tests/readers_robustness.rs` exercises deterministic synthetic random input, every truncation of valid synthetic headers, hostile integer/pointer fields, and small rectangular texture payloads. It contains no native data and uses small explicit PRS/audio output budgets.

## Verified local parity on November 18, 2026

The opt-in native tests passed against the ignored SA Tools 786 intermediates:

- All 171 models, 279 motions, 20 Battle landtables and four additional SA2 landtables in the verified `pc-chao` extraction parsed successfully.
- The original child retains 40 nodes and 16 native attaches. Native Idle and Walk retain 40 motion parts. This deliberately precedes the Blender presentation's eyelid and mouth adaptations.
- All seven cinematic emeralds match the existing verified Blender conversion report: 124 vertices and one mesh each, transformed bounds within 0.0001 unit, and original diffuse alpha 204/255. The bounds test applies SAIO's `(x, -z, y)` mapping only in the comparison adapter.
- The Neutral garden palm matches the report's five meshes and 61 vertices. Its transformed bound parity is not asserted.
- All four Transporter model files match the SHA-256 recorded by the existing extraction manifest and retain geometry: button 25 vertices, shutter 12, casing 50, base 377. Assembly, handoff interaction and collision behavior remain open.

Reproduce with the prepared extraction and conversion report in the original checkout; no native fixtures are in Git:

```powershell
$env:SA2_REFERENCE_ROOT = 'path/to/original/client/checkout'
cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml --test native_local -- --ignored
```

The test compares Rust geometry with verified local Blender metadata. It does not claim byte-identical GLBs, Chao evaluated skinning parity, all stage render geometry parity, matching heightfields or a complete gameplay port. The broad corpus verifies structural readers; successful parsing is separate from semantic completeness.

## Primary format references

This is an independent Rust implementation of layouts documented by the X-Hax SA Tools maintainer source. Unknown payloads remain explicitly opaque. Consult the original source and its license when extending the supported variants:

- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/ModelFile.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/NJS_OBJECT.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/VertexChunk.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/PolyChunk.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/Animation.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/LandTable.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/COL.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/BasicAttach.cs`
- `https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAModel/NJS_MESHSET.cs`

Local report paths, source/output hashes and generated native JSON stay under ignored `.local`; the committed tests contain synthetic malformed data and assertions only.
