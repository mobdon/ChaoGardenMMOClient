# City Escape original physical boards

The Rust extractor reads seven Chunk-format models directly from the pinned Steam sonic2app.exe (SHA-256 4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16). It copies their original contiguous data, rebases hierarchy/attachment/array pointers into a standalone SA2MDL container, and resolves texture names through the original PC texlist. It does not use the other GC-format render models, generate mesh data, or substitute graphics.

| Model | PC root RVA | Native rest geometry | Textured static GLB |
|---|---:|---:|---|
| Streetboard (one player) | C89BBC | 60 triangles (40 + 12 + 8) | 3 original textures; OPAQUE, OPAQUE, BLEND |
| ShadowBoard | C8B974 | 124 triangles | 1 texture, OPAQUE |
| AmyBoard | C8C674 | 124 triangles | 1 texture, OPAQUE |
| MetalBoard | C8D36C | 124 triangles | 1 texture, OPAQUE |
| SonicBoard (two-player default) | C8E06C | 124 triangles | 1 texture, OPAQUE |
| CastShadow | C8A80C | 52 triangles | Requires the shadow pass |
| Modifier | C8ACB8 | Original type-56 volume chunk | Requires the modifier pass |

The five primary PC hierarchies agree with the model pointers in the installed stg13D.rel resource selector: a root and one child, both animatable, with identical native transform bits and flags; all texture-list names and order also agree. This checks the skeleton used for pose sampling. The verified triangle payloads come from the PC executable. The static GLB bakes the original rest transforms, supplies UVs and vertex colors, embeds the original base-mip pixels, and preserves native material flags as metadata. It is not a native-lighting emulator or a skinned animation file. The native archive is resource/gd_PC/objtex_stg13.prs (SHA-256 0f74e2d685043944ddc4a14d2552b26312d451d3b39508ab0e1198e8966321b3). Texture indices are local texlist positions, not stage archive indices.

To reproduce, run tests/board_model_reference.ps1 with -GamePath, -ToolsRoot pointing to SA Tools 786, and -PrivateOutput pointing to a new ignored directory. Set DOTNET_ROLL_FORWARD=Major only if the tools require a compatible installed framework. Set SA2_GAME_PATH to the installation root and SA2_BOARD_REFERENCE to that private output; run cargo test --locked --test board_model_local -- --ignored from this crate. The independent reference uses split.exe binary with PC_SA2/STG13.ini and ArchiveTool.exe -png on a private copy of the archive.

The comparison checks every node and native vertex/polygon payload against the independently serialized models. SA Tools clears the unused high byte of the ambient RGB field and strips one zero alignment word in the modifier volume. The test verifies those exact differences; the Rust output retains both original values. Seven original texture PNGs compare exactly after decoding to RGBA. The GLB check verifies triangle counts and alpha modes for all five textured boards. No original data or generated media belongs in Git.
