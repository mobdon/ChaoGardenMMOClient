# Static native Chunk surfaces

Rust reconstructs draw surfaces from SA2 Chunk vertex and polygon streams. The verified original models are the seven cinematic emeralds from event E0207 scene 2, entities 2 through 8, and the neutral garden palm. The output retains native axes, units, proportions, hierarchy transforms and draw order.

`visual::evaluate_static_model` exposes world positions, source-local normals, normalized inverse-transpose world normals and original RGBA bytes. Each vertex retains the owning node, vertex chunk and original cache index. Strip corners retain the source byte offset of the index word and the original signed UV words. Triangle indices address corners within that strip. The signed strip length selects the initial winding; each source corner advances parity, including a repeated vertex. Repeated-index degenerate triangles are omitted from the triangle list but every source corner remains in the strip. UV resolution is 1/256 or 1/1024 according to the strip type. An environment-mapped strip with no authored UVs reports absent UVs.

The reader accepts static position, position/diffuse ARGB and position/normal vertex streams (34, 35 and 41). It handles plain, UV and HD-UV strips (64–66), single-texture commands (8) and material updates (17–23). It keeps ambient and diffuse colors, the specular color/exponent word, blend flags and both native texture command words. Material fields persist across subsequent draws; an absent field has no authored override. Unsupported polygon commands, cache dependencies across attachments, overlapping vertex ranges, weights, unknown vertex flags, per-triangle user words, malformed lengths, unresolved indices and singular normal transforms return errors. GC model attachments have a different source format. Nonuniform and reflected transforms are tested separately. Weighted Chao skinning and draw-list caching require their own source-backed implementations.

## Independent reference

The pinned SAIO 2.2.2 / SA3D.Modeling 1.2.2 reference commit is `f23c73df68c0c69a96f28746c41b6fc32643340a`. Its `Mesh/Chunk/Structs/ChunkStrip.cs`, `Mesh/Chunk/PolyChunks/StripChunk.cs`, `TextureChunk.cs` and `Mesh/Converters/ChunkConverter.cs` describe the strip, UV, state and texture formats. The comparison uses GLBs from the separate Blender 5.1.2 SAIO import/export recorded by `scripts/convert-cinematic-assets.py`.

`tests/visual_local.rs` pins each source model and comparison GLB SHA-256. It independently reads glTF accessors and composes the reference node transforms from quaternion/translation. For each native draw it compares the multiset of wound triangles, permitting cyclic corner rotations. Every triangle compares all three world positions, UVs, original vertex color/alpha where authored, and source normals where authored. Blender's vertex-color export converts sRGB to linear RGB; alpha remains linear. Numeric tolerances are 0.00005 in world/UV/color components and 0.0005 in normalized normals. Materials compare diffuse RGBA, texture index, double-sided state, blend factors, alpha usage and environment mapping. All seven emeralds passed at 86 triangles each; the tree passed at 68, for 670 total.

Set `SA2_VISUAL_LOCAL` to the original checkout's ignored `.local` directory and run from this crate:

```powershell
cargo test --locked --test visual_local -- --ignored --nocapture
cargo test --locked --test visual_glb_local -- --ignored --nocapture
```

## GLB export

`visual_glb::export` embeds the exact supplied PNG bytes, with source texture index and SHA-256 on each image. Only the three measured strip-material profiles are exported. All referenced indices must be mapped. The caller provides PNGs from the source texture list, with expected dependency hashes checked at the CLI boundary. The palm uses `al_neut_obj_tex` indices 4–6. Event emeralds use `e0207texture` indices 2–8. Geometry is emitted as wound triangles in native world space. UVs, transformed normals and linearized vertex RGBA use standard glTF attributes. Diffuse opacity, double-sided rendering, native clamp flags and bilinear texture filters are retained. Native material words and original node/attachment/polygon offsets accompany each draw.

Standard viewers can display the textured palm and translucent emerald geometry. The emerald texture uses camera-space normals in the original effect. Its GLB material records the exact texture and projection in `extras.sa2Chunk`; a viewer must apply that projection to display the original reflection. The standard material leaves that binding unused. glTF lighting, alpha sorting and filtering are viewer behavior; pixel parity with the game has not been established. The export bakes rest transforms and has no animation, collision, game systems or effect scheduling.

The GLB readback test checks each embedded image against its original PNG bytes, the stored hashes, the binary bounds, the primitive world-position stream and the presence of profile-specific material/vertex attributes. It also rejects missing textures and invalid PNGs. The comparison runs in memory. Use the GLB command to write a file with its provenance sidecar under the ignored .local directory.

Each binding has a native `index`, a forward-slash `path` relative to its bindings file, and the expected 64-digit `sha256`. The file is an object with a `textures` array. Exact index coverage, unique canonical files inside the bindings directory, byte limits and input hashes are checked before export. The dependency list includes both the bindings file and each PNG. Blender 5.1.2 independently imported all eight Rust-generated GLBs and counted the same 670 triangles.
