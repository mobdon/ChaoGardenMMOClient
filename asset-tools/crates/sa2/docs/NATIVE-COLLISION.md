# Native SA2 collision geometry

This slice bakes original Basic land collision geometry in Rust and exposes bounded, two-sided mathematical ray queries. It does not implement the retail character collision response, object updates, contact/step/slide/gravity rules, native broadphase, race route rules or Karate gameplay.

`collision::bake_stage(&stage, 1.0)` preserves the original axes, units and origin. `0.1` is an explicit uniform world-scale adaptation, recorded as `uniformScale` and `scaleAdapted`; it does not resize source vertices, alter native node scales, recenter, project a heightfield or use visual meshes as replacement collision. Original node local/world matrices come from `pose::evaluate_nodes`. Unsupported transform modes fail through that API.

Each source entry has a descriptor containing its table index/offset, geometry kind, collision classification and all original surface bits. The native landtable's visual-count split determines which attachments are Basic collision. For the legacy `visualCount == -1` representation, the source Visible bit determines the layout, as in the bounded stage reader. Source Solid is bit 0, DynamicCollision bit 27 and Visible bit 31. Visibility and solidity remain separate from attachment layout: invisible Basic geometry remains present, as do non-solid Basic surfaces. Battle GC/SA2 Chunk visual descriptors are recorded without replacing their separate native collision attachments.

Each collision mesh preserves its entry, node and attachment offsets, original surface flags, transformed source vertices and triangle-to-source metadata. Triangle provenance records the original meshset/polygon offsets, polygon type, strip reverse flag, source triple index, material ID and uninterpreted meshset attribute-reference field. Vertices are not welded or reordered; degenerate strip triples remain in output and are counted separately.

Native triangle lists retain their three indices. Native quads are `[0,1,2]` and `[2,1,3]`, sharing original edge 1–2. Strips alternate the first two indices on each triple, XORed with the source reversed flag; degenerate triples still advance parity. NPoly (type 2) remains explicitly unsupported because the inspected modern and legacy conversion sources do not establish one verified native interpretation. No fan or strip guess is substituted.

The default output budget is 64 MiB, including conservative working-set and instance charges. A source attachment referenced by many objects is charged per resulting transformed instance. Malformed public stage structures, missing/duplicate attachments, out-of-range polygon indices, unsupported layouts, invalid scales, nonfinite transformed points and overflowing triangle arithmetic fail before unsafe indexing. `bake_stage_with_options` permits a smaller explicit budget.

`world.raycast(origin, direction, maxDistance)` normalizes the supplied direction, so `maxDistance` and hit distance are actual output-space distances. It queries Solid surfaces and excludes DynamicCollision snapshots by default. `raycast_with_filter` allows explicit required/rejected surface masks; a filter requiring and rejecting the same bit fails. The underlying source geometry remains in the world regardless of query filters.

Ray intersection is a generic two-sided triangle calculation, not recovered retail collision code. The hit normal follows transformed source winding and is never flipped toward the ray. Hits include `frontFace`, barycentric coordinates, mesh/triangle indices, source entry/node and flags. Degenerate triangles and parallel rays do not hit; ties select the first source triangle. Nonfinite or zero direction, invalid maximum distance and nonfinite intermediate calculations fail. A relative `1e-12` parallel/barycentric tolerance is a numerical policy for this query, not a claimed native collision constant.

Primary layout/conversion references inspected November 18, 2026:

- `X-Hax/sa_tools`, commit `cc05c849b1b7017fb0b519932863e206fa34077a`: `Libraries/SAModel/VertexData.cs` `MeshInfo.ToTriangles`, `Poly.cs`, `BasicAttach.cs`.
- `X-Hax/SA3D.Modeling`, commit `2c0044a3e29f4621c4fde74c8936e7bd6416ce0f`: `ObjectData/Enums/SA2SurfaceAttributes.cs`, `Mesh/Basic/BasicMeshSet.cs` and `Mesh/Basic/Polygon/BasicPolygonType.cs`.
- Existing owner-local conversion: tracked `scripts/convert-chao-activities.py` and ignored `.local/activities/{race-normal,race-hero,race-dark,karate-arena}-collision.json`, generated with original Basic collision selected by SAIO. These JSON files use native axes ×0.1, with origin retained; Blender's evaluated triangulation may differ in diagonal/order and must be classified separately from native triangle provenance.

Synthetic tests in `tests/native_collision.rs` verify primitive ordering/reversed winding, degenerate parity, native-versus-visual selection, parent translation/uniform scaling, true-distance/two-sided ray hits, source normals, surface filtering, invalid ranges and allocation bounds. Independent actual-source comparisons are recorded in the native runtime parity tests/documentation. Native inputs, source bytes, converted meshes and comparison outputs remain ignored/local. A complete SA2 + BATTLE gameplay port remains tracked separately.

## Executed native comparison

The ten synthetic tests and strict Clippy checks pass. Direction normalization is checked with ordinary, smallest-subnormal and maximum finite components, including a diagonal ray's true-distance cutoff. Independent owner-local comparison also passes for all **94 native Basic collision meshes, 2,547 reference vertices and 2,898 oriented triangles** in the three race worlds and Karate arena. All actual source meshsets in this corpus are strips. Vertex correspondence is checked in both directions, and cyclic triangle corner rotation is allowed while reversed winding fails.

| World | Native collision meshes | Oriented triangles | Stable vertical queries | Maximum vertex-coordinate error at explicit scale 0.1 |
| --- | ---: | ---: | ---: | ---: |
| Normal race | 31 | 952 | 61 | 0.000007828457640357556 |
| Hero race | 31 | 952 | 61 | 0.000007828457640357556 |
| Dark race | 31 | 952 | 61 | 0.000007828457640357556 |
| Karate arena | 1 | 42 | 8 | 0.00000017398681650959702 |

The 191 stable ray-height comparisons use an independent two-dimensional x/z plane-height oracle, while production queries use the three-dimensional ray algorithm. Maximum stable height error is below 0.000004352 output units. Eleven nearvertical/edge probe heights are explicitly excluded from this numerical comparison because the existing Blender reference uses float32 transforms rounded to seven decimals; the winning reference surface must have upward normal at least 0.3 and barycentric interior margin at least 0.02. Every excluded probe's underlying geometry and winding remain covered by the complete mesh comparison. These are comparator conditioning rules, not native gameplay rules or production collision filters.

Full source/reference SHA-256 identities, exact per-world query errors and reproducible commands are in [NATIVE-RUNTIME-PARITY.md](NATIVE-RUNTIME-PARITY.md). No generated mesh, native binary or reference coordinate array is committed.
