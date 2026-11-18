# Installed SA2 file inventory

Run `node scripts/full-inventory.js` from the project folder. Pass an installation folder as the first argument or set `SA2_GAME_PATH` to override Steam discovery. The command reads the **entire installation root**, including root executables, `redist`, shaders, and every resource subfolder. It writes ignored `.local/full-inventory.json`.

The November 18, 2026 workstation scan read **4,351 regular files, 3,253,731,852 bytes**, with a SHA-256 digest for every file. All 3,253,731,852 bytes were read. No files were skipped or unreadable, and the tree was unchanged at the final enumeration. The game installation was opened for reads only.

| Installation area | Files | Bytes |
| --- | ---: | ---: |
| Root executables/configuration | 6 | 25,430,050 |
| `redist` | 157 | 103,174,432 |
| `resource` and all descendants | 4,188 | 3,125,127,370 |
| **Total** | **4,351** | **3,253,731,852** |

The older `node scripts/inventory.js` is a fast startup detector for `resource/gd_PC`; its API intentionally limits returned samples. It is not the whole-installation report.

## What is recorded

Every regular-file entry includes relative path, exact size, bytes actually read, extension, category, archive candidate group, and SHA-256. The report also totals file count and bytes by extension, filename category, installation area, and archive candidate group. Links and non-regular files are never followed; any skipped or changed file makes `complete` false and causes a failing exit status.

The inspected installation includes **2,451 PRS archives, 231 PAK files, 240 DDS textures, 29 GVR textures, 49 GVP palettes, 76 REL files, 283 CSB sound banks, 157 ADX tracks, two AFS banks, and 33 SFD movies**. These extension counts include all installation folders. Categories and archive groups are discovery hints based on filenames, not a claim that every packed asset has been decoded.

A complete byte inventory establishes which installed files exist and their contents' hashes. Decompressing every archive, identifying every internal model/texture, and converting every game system remain separate tasks. The `conversionManifest` records the assets actually extracted and used, with installed source hashes and generated output hashes.

## Confirmed source to browser mappings

| Browser asset | Installed source and mapping | Texture/animation source |
| --- | --- | --- |
| Child Chao | `Data_DLL.dll`, `AL_RootObject[0]` | `al_body.prs`; child colors from original `al_nc00.GVP` banks 0/1/2 |
| Neutral/Hero/Dark Chaos Chao | Same DLL, array entries 108/114/120 | Original special-body images in `al_body.prs` |
| Tails/Knuckles/Amy Chao | Same DLL, entries 126/132/138 | Original body textures; original white eye surface and dark pupils |
| All seven Chao, Idle/Walk | Same DLL, mapped 40-node motion table | `alm_tati.saanim` / `alm_aruki_ncz.saanim` |
| Neutral/Hero/Dark gardens | Same DLL, `objLandTableNeut/Hero/Dark` | `al_stg_neut_tex.prs`, `al_stg_hero_tex.prs`, `al_stg_dark_tex.prs` |
| Blue head flame | `al_icon.prs` | Original `torch_blue00` through `torch_blue15`, packed losslessly into one atlas |
| Seven cinematic Chaos Emeralds | `event/e0207.prs`, Scene 2 entities 2–8 | Original `event/e0207texture.prs`, matched through internal texlist indices 2–8 |
| Hub grass | `al_stg_neut_tex.prs`, global texture index 120003 | Original seamless `gneut03.png`, 64x64 |
| Neutral garden palm tree | `sonic2app.exe`, `F27C6C` ChaoChunk model | `al_neut_obj_tex.prs`, original `gno_ki_def01/02/03` |

The detailed manifest includes native `.sa2mdl`, texlist and `.glb` paths. All derived output stays under ignored `.local`. The three original gardens additionally have canonical collision heightfields under `.local/terrain`.

## Original seven Chaos Emerald facet textures

Run `node scripts/extract-native-textures.js` to extract all seven genuine color maps and the grass texture. The script copies each archive to `.local/extracted/textures` before running SA Tools ArchiveTool; the installed originals are never passed to an operation that writes adjacent output. SHA-256 is checked before and after extraction. `.local/imports/native-textures.json` records provenance and output hashes.

| Original image in `event/e0207texture.prs` | Local browser output |
| --- | --- |
| `chaoseme_red.png` | `.local/imports/emerald-red.png` |
| `chaoseme_purple.png` | `.local/imports/emerald-purple.png` |
| `chaoseme_lightblue.png` | `.local/imports/emerald-cyan.png` |
| `chaoseme_green.png` | `.local/imports/emerald-green.png` |
| `chaoseme_blue.png` | `.local/imports/emerald-blue.png` |
| `chaoseme_yellow.png` | `.local/imports/emerald-yellow.png` |
| `chaoseme_white.png` | `.local/imports/emerald-white.png` |

All seven are original 64x64 facet images without recoloring. The event red/purple/lightblue PNG hashes exactly match the separately decoded Security Hall images `miu64_sh013/014/015`, establishing that this source geometry's UVs use the same facet layout. Each emerald clone should retain its image with a white material multiplier. Removing the texture loses the original facet detail.

The grass output is `.local/imports/nativehubgrass.png`, copied directly from `.local/extracted/textures/al_stg_neut_tex/gneut03.png`. It is an original game grass image, separate from the hub's procedural terrain geometry.

## Actual cinematic geometry and real garden tree

Run `node scripts/extract-cinematic-assets.js` to split a local working copy of the original `e0207.prs` event with its motion sidecar. This uses official SA Tools `splitEvent`, producing native models plus `InternalTexlist.satex`. The individual blue/green/cyan/purple/red/white/yellow emeralds are **Scene 2 entities 2/3/4/5/6/7/8**, with matching texture IDs. Scene 3 entity 3 additionally contains the combined seven-emerald model. The source event contains Sonic/Shadow Super models and the genuine seven-color texture set.

The same helper extracts the real Neutral garden palm tree from `sonic2app.exe` using `PC_SA2/CHAOSTGNEUT.ini`, address `F27C6C`, preserving its `ChaoChunk` geometry and original object textures. Every installed input's hash is verified unchanged after extraction; `.local/cinematic-extraction.json` records all source hashes.

Convert with portable Blender:

```powershell
$env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local/blender-user'
& '.local/tools/blender-5.1.2-windows-x64/blender.exe' --background --factory-startup --python 'scripts/convert-cinematic-assets.py' -- --preview
```

The output is seven **separate** `.local/imports/emerald-{blue,green,cyan,purple,red,white,yellow}.glb` files and `.local/imports/tree-neutral.glb`. Each cinematic emerald has 124 native vertices. The original materials specify **alpha 0.8, double-sided rendering, environment mapping, source-alpha/inverse-source-alpha blending and specular exponent 11**. Their GLBs preserve native alpha as `BLEND`, original diffuse factors and texture maps, double-sided flags, and the original material settings in `extras.sa2Source`. The specular exponent is represented by standard PBR roughness approximately 0.392. This is separate from the opaque Chao body rule.

The original cinematic UV coordinates are all zero: these images are sphere-mapped from normals, rather than sampled with ordinary model UVs. Inspection of SAIO 2.2.2's original `SAIO UV Tiling` shader template confirms the projection: transform the surface normal from world to camera coordinates, then calculate **`uv = cameraNormal.xy * 0.5 + 0.5`**. The converter's previews use this exact source projection, which is also recorded in material extras. The browser must implement the same view-dependent texture coordinates; ordinary glTF albedo sampling alone would appear flat. A generic cube-map reflection uses a different coordinate rule.

Native event placement is preserved in these exports, so each model needs bounds-based recentering before placement in the orbit. The tree has five native meshes and 61 source vertices, with three original images and masked leaf material. `.local/imports/cinematic-conversion-report.json` records geometry bounds and every original material; `--audit` instead writes the complete 85-model event audit.

The earlier `.local/imports/chaos-emerald.glb` was a Security Hall pickup-model experiment. It is retained as an identified intermediate and is superseded by the actual cinematic model exports.

## Fidelity and ownership limits

The player must own Steam Sonic Adventure 2 and BATTLE DLC for the intended project. Finding installed karate resources does not verify account entitlement; the detector reports file evidence separately. This prototype does not authenticate Steam ownership.

The browser runs our game code and converted local assets. Original executable code and native missions do not automatically become browser gameplay. Dynamic Chao facial expressions, evolution morphs, original shaders and mission behaviors require separate implementation and validation. Chao eyes currently use an explicit awake presentation: half-closed eyelid overlays are omitted. Special NPCs preserve white eyes and original pupils; the child retains the requested pupil cutout adaptation. Every Chao body material is opaque; source body alpha is not interpreted as missing geometry.

Do not commit or publish `.local`, installed game files, extracted textures/models/audio, or derived GLBs. The source repository contains the pipeline and game implementation; game-derived content stays on the owner's computer.
