# Character upgrades and owner equipment

`shared/upgrade-catalog.json` indexes all 28 collectible upgrade flags from the native PC game: Sonic 6, Tails 4, Knuckles 5, Shadow 4, Eggman 5, and Rouge 4. It records each original bit index/mask, concise retail effect, mapped worn model IDs, and field pickup where one is mapped. There are 28 worn model references and 20 mapped field pickup models. A missing standalone wearable model is preserved as an empty list rather than invented geometry.

The native flag enum is the source of the collectible list. Its separate bit15 `SuperSonic` state appears under `specialStates`, not among the 28 collectibles. Chaos Chao evolution is a different system. Amy, Metal Sonic, Tikal, Chaos0, Chao Walker, and Dark Chao Walker have no independent collectible upgrade bits in this enum; their base or alternate-character abilities need a separate behavior audit. This does not mean those characters have no special abilities.

`nativeEffect` describes the retail game. `mmo.nativeEffectImplemented` is false for these cataloged abilities until a separate authoritative implementation is verified. `mmo.proposal` is design work, not completed behavior. The two owner starter selections are `shadow-air-shoes` in `footwear` and `knuckles-hammer-gloves` in `hands`. Their native identities do not automatically grant movement speed, PvP damage, invulnerability, or traversal abilities. Server equipment state and the renderer’s visual attachments are integrated separately.

## Original wearable assets

| Output under ignored `.local/imports/` | Original sources | Geometry |
|---|---|---|
| `shadow-air-shoes-left.glb` | `teriosmdl.prs`65 left heel/toe nodes23/24, upgrade overlay79; `teriostex.prs` | 73 boot vertices plus22 overlay vertices |
| `shadow-air-shoes-right.glb` | `teriosmdl.prs`65 right heel/toe nodes13/14, upgrade overlay78; `teriostex.prs` | 73 boot vertices plus22 overlay vertices |
| `knuckles-hammer-gloves-left.glb` | `knuckmdl.prs`165; `knucktex.prs` | 42 vertices |
| `knuckles-hammer-gloves-right.glb` | `knuckmdl.prs`164; `knucktex.prs` | 42 vertices |

The standalone Air Shoes upgrade models78/79 contain the add-on pieces rather than Shadow’s complete jet boots. The footwear export therefore includes the original full heel/toe mesh from Shadow65, transformed into its native heel basis and assembled with the matching original overlay. This distinction matters when reproducing the requested recognizable jet shoes.

The converter bakes each prop’s native rest transforms, translates its combined bounding-box center to the origin, and embeds the matching original texture pixels. It preserves native mesh proportions. It exports static rigid props without animations or a skin and never loads or edits the playable Chao body or skeleton. Native material flags and source IDs remain in glTF `asset.extras.sa2Source` and material extras; a standard PBR material is an approximation of the original engine lighting.

`.local/imports/upgrade-conversion-report.json` records removed centers, original bounds, glTF dimensions, vertex counts, sources, hashes, and exact native heel rest matrices plus exported local standing-up/forward references. The four GLBs occupy 78,396 bytes after this conversion. Every input archive’s SHA-256 is checked before and after extraction and again by the verifier. Left-side shoe and glove preview renders were inspected. Runtime attachment orientation, motion clearance, cockpit clearance, and Chao fit still require scene validation.

## Attachment references

The audited original Dark Chaos 40-node hierarchy has arm meshes at indices003 and010, terminal hand sockets004 and011, and foot meshes006 and013 with reference sockets005 and012. These mesh/sign references were inspected in the extracted original model; do not substitute address-derived names across different Chao variants. Match the shared hierarchy and native index, then verify character-relative left/right in the renderer. The Chao faces glTF +Z. Equipment requires uniform scaling and an explicit mounting rotation/offset for its target socket; preserve the Chao’s bone hierarchy and proportions.

The complete native audit is ignored `.local/upgrade-geometry-audit.json`. Final attachment transforms belong to the scene renderer because the body, active animation, and mounted pose determine the correct fit. Preserve equipment selection across reconnect, evolution, and mounting; hide or adjust a prop only through a documented visual policy rather than changing ownership state.

## Reproduction and provenance

```powershell
node scripts/catalog-upgrades.js
node scripts/extract-upgrade-assets.js
$env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local/blender-user'
& '.local/tools/blender-5.1.2-windows-x64/blender.exe' --background --factory-startup --python scripts/convert-upgrade-assets.py
node scripts/verify-upgrade-assets.js
```

Add `-- --preview` to the Blender command to render the two left-side source previews. The verifier checks all four output hashes, centered nonempty geometry, embedded textures, rigid-only exports, the complete boot assembly, and unchanged installed source hashes. Regenerate the catalog from the official SA Tools786 mappings; override the tools directory with `SA_TOOLS_DIR` if necessary. All extracted original models, textures, previews, tools, and GLBs remain ignored under `.local/`.

Primary references are X-Hax SA Tools `PC_SA2/MDL/{SONIC,TERIOS,TWALK,EWALK,KNUCK,ROUGE}MDL.ini` and `CommonObjects.ini` Field Upgrades, X-Hax sa2-mod-loader `SA2Enums.h` UpgradeBits/Upgrades, X-Hax Character Model Structures, and the licensed Prima Sonic Adventure2 guide character upgrade descriptions. Source URLs are preserved in the catalog. Knuckles’ “fist bars” are recorded as the user-confirmed Hammer Gloves.
