# Native Shadow shoe effects

The former shoe jets reused the Chao head-flame atlas on newly created planes.
This replacement uses the original eight Shadow jet models, preserving their
vertices, oriented triangles, vertex colors and material flags. The head flame
is unchanged. Native assets and conversion outputs remain ignored/local.

## Source and scope

Source: installed SA2 PC `resource/gd_PC/teriosmdl.prs`, SHA-256
`d9c512a137239fbd354d3d9384c834460d64458037ccb2aefd05b51bfb2ef36c`.
SA Tools `GameConfig/PC_SA2/MDL/TERIOSMDL.ini` identifies:

| Side | Toe narrow/wide | Heel narrow/wide |
| --- | --- | --- |
| Left | 92 / 93 | 95 / 94 |
| Right | 96 / 97 | 99 / 98 |

Each native model has 12 vertices and 16 oriented triangles. The effects are
**untextured**, using original opaque-white and transparent-orange vertex
colors, diffuse RGBA 178/178/178/255, double-sided unlit rendering and
SrcAlpha/InvSrcAlpha blending. A missing flame texture atlas is not involved.
RGB conversion matches SAIO 2.2.2; native alpha is preserved exactly.

The Rust bundler fingerprint-checks the original models, validates the existing
Rust SA2 reader's JSON/provenance against native vertex/polygon bytes, and
appends their geometry to copies of existing shoe GLBs. Original node, mesh,
material, accessor and buffer-view entries, image declarations and binary/image
prefix remain exact. The default scene gains jet roots, the buffer length grows
and the unlit extension is declared. Jet geometry receives
the exact same recentering as the existing shoe export. Outputs must be new
files under this checkout's `.local`; the tool does not install aliases.

The browser excludes jets from shoe fitting, contact and shadow selection.
Jets follow the shoe's native foot socket and share its uniform prop scale.
Old shoe aliases without embedded jets display no fabricated substitute.
Existing authoritative movement, skate eligibility and freshness guards remain
the source of effect eligibility. Narrow/wide selection at speed 1.8, fading
with skate blend and fitting Shadow props to Chao are presentation adaptations;
the retail effect draw-time matrices and timing have not been traced.

Boot orientation uses native heel13/23 up and the heel-to-toe vector projected
onto its sole plane. This removes the nominal Shadow-forward reference's
roughly 11-degree inward yaw without changing native Chao joints or scaling.

## Reproduce locally

Activate the existing project Rust toolchain, then run from this checkout:

```powershell
. C:/Users/Admin/Documents/ChatGPT/mmo/scripts/use-rust.ps1
./scripts/build-native-shoe-effects.ps1 `
  -SourceRoot C:/Users/Admin/Documents/ChatGPT/mmo `
  -InspectorExe C:/Users/Admin/Documents/ChatGPT/mmo/.local/worktrees/sa2-assets/asset-tools/crates/sa2/target/debug/sa2-assets.exe `
  -OutputDirectory .local/native-shoe-effects/new-proof `
  -BlenderExe C:/Users/Admin/Documents/ChatGPT/mmo/.local/tools/blender-5.1.2-windows-x64/blender.exe
```

Choose a new output directory on every run. The Blender argument enables an
independent existing-SAIO import and geometry/material parity proof. Without
it, only bounded Rust preparation runs. Inspect each output's provenance and
`build-evidence.json`; no service, canonical profile or save is accessed.

```powershell
node scripts/verify-chao-footwear.mjs --asset-root C:/Users/Admin/Documents/ChatGPT/mmo/.local/imports --shoe-root .local/native-shoe-effects/new-proof --output .local/native-shoe-effects/new-proof/footwear-proof.json
```

For `verify-straight-shoe-basis.mjs`, use an asset directory containing both
prepared shoe GLBs and original `chao-locomotion.glb`, `neutral-chaos.glb`,
`hero-chaos.glb`, `dark-chaos.glb` and `shadow-skate-motion.json`. These may be
copied into an ignored proof directory without installing the aliases.

```powershell
node scripts/verify-straight-shoe-basis.mjs --asset-root .local/native-shoe-effects/new-proof --source-root C:/Users/Admin/Documents/ChatGPT/mmo/.local/extracted/shadow-skate/teriosmdl --output .local/native-shoe-effects/new-proof/basis-proof.json
```

## Evidence and limits

Prepared v3 aliases and a fresh scripted reproduction have identical hashes:

- Left: `3ae2802139ecca7de1ec93e814d411745bd922705a130df7c990f9e0ca96246a`.
- Right: `9a7a30598257fff57b08c382bbcbac5400aa39205ff03381a771c6a8f80b4446`.

Independent SAIO parity and actual glTF PBR jet-material loads pass all eight
models. The PBR proof selects the existing jet nodes in memory so Node does
not need boot-image DOM decoding; it does not alter the prepared assets.
Original shoe binary chunks
remain exactly 16,620 bytes each. Three-form Walk/Run/Skate trajectory proofs
preserve native hierarchies/proportions and alternating lifts. The actual
production mounting function passes 1,086 samples; projected standing toe yaw
is below 0.000001 degrees on both sides. Tests cover missing-asset behavior,
effect selection, following, original alpha and disposal.
Overwrite attempts, truncated GLBs, changed native fingerprints, extreme
inspector offsets and invalid scene indices return errors without panics.

These proofs do not establish an engine port, native effect timing, browser
silhouette/terrain acceptance or a canonical Rust service round trip. Actual
browser material/visual checks and service checks remain integration work.
Prepared aliases have not been installed by this change. Runtime deployment,
server migration and canonical-save changes are outside this preparation.
