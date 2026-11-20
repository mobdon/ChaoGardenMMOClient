# From GX to the web (E7)

How `sa2-render` draws what SA2B's GameCube engine draws with GX. The game's
logic stays the native port in `sa2-runtime`; this file covers only drawing.

## Backend

- Rust `wgpu` 30: WebGPU in the browser, with wgpu's WebGL2 fallback
  enabled (`webgl` feature) for browsers without WebGPU; DX12, Vulkan or
  Metal natively, which the tests use to draw offscreen.
- One WGSL pipeline today. GX's fixed-function TEV stages will become WGSL
  generated per material (see "Sources" below).

## Data

- **Land** comes from the Steam `stg11D.rel` landtable. Its visual entries
  are in the GameCube model format (the sa2 crate keeps them opaque), read
  by `sa2-render/src/gc.rs` through the module's relocations: vertex
  attribute arrays (position, normal, color 0, texcoord 0), meshes with
  their parameter lists (index attribute flags, texture id), and GX display
  lists (quads, triangles, strips, fans; 8- or 16-bit indices per attribute
  in GX order). Entries in Ninja chunk format go through `land.rs`'s strip
  decoder instead.
- **Object transforms** follow Ninja: translate, rotate Z·Y·X (Y·X·Z with
  the ZXY flag 0x20), scale; eval flags 1, 2 and 4 skip each part.

## Choices

| GX | Web | Note |
| --- | --- | --- |
| Projection (`GXSetProjection`, depth -1..0 mapped by hardware) | wgpu clip space, depth 0..1 | `gpu::look_at` builds the matrix |
| Display-list primitives | indexed triangle lists | strips and fans unrolled with their winding |
| Back-face culling (`GXSetCullMode`) | off for now | per-mesh cull comes with the material parameters |
| Lighting channels | one fixed lambert term | to be replaced by the material's lighting parameter (type 2) |
| Textures (TEX0 via texture id) | not yet bound | next step: the stage's texture list from the Steam PAK/PRS files |
| TEV, blend, alpha compare | not yet | from the material parameters, translated with the noclip GX code (below) |

## Sources

- The GC model reader and the draw path are our own.
- mob approved adapting noclip.website's MIT-licensed generic GX code
  (its `src/gx/` folder only: TEV state to shaders, vertex formats, texture
  decoding, blend/Z/cull mapping) into Rust. It's pinned at
  magcius/noclip.website `2cebac2a5d1d7c44535397a14f51e764ca823767`, cloned
  under `.local/` and never committed. Every derived file will carry
  "Copyright (c) 2018 Jasper St. Pierre" and the MIT notice, and
  `sa2-runtime/THIRD-PARTY.md` will list it. No game-specific noclip code
  is used. Dolphin is a reference for hardware behaviour only; none of its
  code is copied.
