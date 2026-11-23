# Local Sonic Adventure 2 content pipeline

Sonic Adventure 2 on Steam (app 213610) and BATTLE DLC (app 217900) are requirements for the intended game. The local browser runtime now loads converted original Chao models and three original gardens. The social hub retains its separate procedural layout with a genuine original grass texture. See [INVENTORY.md](INVENTORY.md) for the whole-installation inventory and detailed source provenance.

## What discovery does now

Run `npm run inventory` (or `node scripts/inventory.js`). An optional installation folder can be passed as the first argument. `SA2_GAME_PATH` overrides automatic discovery. Discovery reads Windows Steam's registry path, common installation roots, `libraryfolders.vdf`, and the SA2 app manifest. It inventories `resource/gd_PC` without changing the installation or saves. `.local/game-inventory.json` stores the result outside tracked source.

The local installation inspected during development contains `ChaoMain.prs`, `ChaoMotionsD.rel`, `al_body.prs`, `al_common_tex.prs`, `al_eye.prs`, Neutral/Hero/Dark garden archives, race archives, and `ChaoStgKarate.prs` with karate texture archives. Categories are filename-based candidates, not proof that every file in a category has been decoded. Inventory counts cover all regular resource files; the API returns the first 500 prioritized file entries to keep status responses manageable.

The detector reports karate file evidence separately from BATTLE ownership. Shared installation data can contain resources independently of entitlement. Confirm BATTLE in the player's Steam DLC settings. No Steam authentication or publisher entitlement verification is implemented in this prototype.

For the Steam PC version, a stronger extraction path is `resource/gd_PC/DLL/Win32/Data_DLL.dll`. The detector parses its PE export table as bytes without loading or executing the DLL. The installed file contains `AL_RootObject`, `al_motion_table`, and the Neutral/Hero/Dark garden and Karate landtable exports. Maintainer `GameConfig/PC_SA2/Chao_DLL.ini` maps the `AL_RootObject` array to these candidates:

| Array index | Model |
| --- | --- |
| 0 | Neutral Child Chao (`al_ncz`) |
| 108 | Neutral Chaos Chao (`al_nchaos`) |
| 114 | Hero Chaos Chao (`al_hchaos`) |
| 120 | Dark Chaos Chao (`al_dchaos`) |
| 126 | Tails Chao (`al_tails`) |
| 132 | Knuckles Chao (`al_knuckles`) |
| 138 | Amy Chao (`al_amy`) |

The detector also checks that these array pointers map into the DLL's file-backed sections. This is an extraction feasibility check, not a rendered-model preview. The config describes a 40-node Chao motion table. These original special models are candidates for garden NPCs as requested.

Current SA Tools CLI source documents `split binary <file> <inifile> [output path]` for a mapped DLL and `split dllexport <file> <type> <name> [-id array ID] [output path] [-p numparts]` for a single export. Use the PC config for the PC DLL; the separate GameCube `ChaoMain.prs` mapping has big-endian and compressed-REL assumptions and should not be applied blindly to the PC files. Validate exact switches against the downloaded utility version before running extraction.

## Verified extraction on this workstation

X-Hax SA Tools release **786** (official x64 release archive) has been prepared locally under `.local/tools/sa-tools-786`. The archive was extracted with the standalone `7zr.exe` from 7-zip.org because Windows' bundled tar reported that its LZMA codec was unsupported. Neither utility is part of tracked source.

The installed PC DLL was successfully extracted with the mapped `binary` mode. Release 786's `dllexport` command instead failed on a duplicate export-address dictionary key (`4096`), so use the mapped command for this file. This is a tool limitation, not a failure of the game installation.

Run `node scripts/extract-native.js` to reproduce the successful path. `SA_TOOLS_DIR` can point to another prepared release folder. The helper uses a subprocess-only `DOTNET_ROLL_FORWARD=Major` policy because this host has .NET Desktop 9/10 rather than Desktop 8. It verifies the original DLL's SHA-256 before and after extraction and writes `.local/extraction-report.json` and `.local/extraction.log`.

The verified mapped output contains **171 native models (`.sa2mdl`), 279 animations (`.saanim`), 20 Battle-format landtables, and 4 additional landtables**, plus metadata. The seven requested model candidates are all present as nonempty files, including the standard child, three Chaos variants, Tails, Knuckles, and Amy. Neutral/Hero/Dark garden landtables are also present. These are native extraction intermediates, not browser-renderable `.glb` files.

The helper additionally makes local working copies of `al_body.prs`, `al_eye.prs`, `al_stg_neut_tex.prs`, `al_neut_obj_tex.prs`, `al_stg_hero_tex.prs`, `al_stg_dark_tex.prs`, and `al_icon.prs` and runs the release's `ArchiveTool -png` conversion. The first six archives produce230 original PNG textures; `al_icon` adds the original emotion imagery. `al_body` alone contains143 PNG textures including the special Chao variants. ArchiveTool must never be pointed directly at an installed archive because it writes converted output alongside its input.

## Verified browser geometry conversion

Official portable Blender **5.1.2** and SAIO **2.2.2** are prepared under ignored `.local/tools`. SAIO 2.2.2's manifest requires Blender 5.1 or newer. The portable Blender archive came from `mirror.blender.org`; the SAIO extension archive came from its official GitHub release. No machine-wide Blender installation or saved Blender preferences are required.

Run from the project folder in PowerShell:

```powershell
$env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local\blender-user'
& '.local\tools\blender-5.1.2-windows-x64\blender.exe' --background --factory-startup --python 'scripts\convert-assets.py'
```

Append `-- --preview` to render `.local/imports/chao-preview.png`, or `-- --preview-all` for all seven Chao previews. The script installs the extension's bundled Windows/Python wheels into a local folder, registers SAIO only in the running process, imports the native geometry and animations, embeds extracted texture maps in standard glTF materials, and exports only the active scene. Its output is `.local/imports/{chao,neutral-chaos,hero-chaos,dark-chaos,tails,knuckles,amy,garden,hero-garden,dark-garden}.glb` plus `conversion-report.json`.

Each Chao imports the original 16-part geometry, omits the two half-closed eyelid overlays for the requested awake appearance, and retains both original `Idle` (`alm_tati`) and `Walk` (`alm_aruki_ncz`) animation clips, with 120 animated channels each. Rigid mesh parts follow animated nodes; these GLBs need not contain a weighted glTF skin to animate correctly. Face direction is Blender -Y, exported as glTF +Z. Native child height is approximately 4.72 glTF units. Evaluated foot minima sampled across each native clip are approximately 0 for the child/Tails/Amy, -0.0181 for Neutral Chaos, 0.00023 for Hero Chaos, -0.09997 for Dark Chaos, and -0.00658 for Knuckles. These evaluated values supersede the earlier unevaluated rest-bound estimate of -0.063 for the child. The Chao variants have different heights; normalize from their actual loaded bounds.

All three original gardens have been converted with their respective stage textures and vertex colors. Their materials use the standard `KHR_materials_unlit` baseline because these legacy textures/vertex colors carry baked lighting. Large original sky enclosures are retained in the GLBs and identified by per-mesh bounds in `conversion-report.json`; the scene can omit them from navigation/collision/scale calculations.

The child texture palette has now been reconstructed from the original `al_nc00.GVP` rather than tinted. Maintainer `ChaoPalettes.cs` maps `al_child02/03/04` to its banks 0/1/2; `TexturePalette.cs` and `RGB565PixelCodec.cs` document the big-endian RGB565 palette decode. ArchiveTool's CI4 fallback PNGs store the original 4-bit index as `index * 16`; the converter replaces those pixels with the original palette colors and embeds the resulting PNGs in the GLB. A rendered preview was visually checked: cyan body with yellow tips and feet. A separate Knuckles preview correctly renders red with yellow/green shoes from its original non-palettized texture.

The converted neutral facial default omits the alternative open/toothy mouth polygons instead of drawing both at once. The mod-loader's ChaoMouth enum begins with ToothySmile=1, so the selected default0 has no mouth overlay. For the user's requested open eyes, meshes attached to nodes019/022 of the shared 40-node Chao skeleton are omitted while the eyeballs at018/021 and all animation bones are preserved. Child, Dark Chaos, Tails, and Knuckles were rendered and visually checked with open eyes; all seven conversions remove exactly two lid overlays. A full dynamic facial animation/expression controller and custom material effects remain future fidelity work.

The initial conversion connected PNG alpha to every material, causing even solid body materials to export as `BLEND`. Every Chao body and wing material now exports opaque, preventing original texture alpha from creating missing body sections. No Chao material masks or uses `BLEND` in the export. The normal-eye sprite is taken from the original `al_eye/al_eye01.png`. The child, Tails, Knuckles and Amy keep its opaque white eye surface and dark pupil pixels as the game has them; Chaos variants retain their own original eye images. Source eye geometry remains intact. Garden water/effects retain source alpha where applicable. GLBs are written to temporary filenames and replaced atomically with bounded Windows file-lock retries so the running local server never reads a half-written asset. Append `-- --special-npcs-only --preview-all` to convert and render only Tails, Knuckles and Amy, or `-- --chao-only --preview-all` for all seven Chao while keeping gardens and their manifest entries.

## Evolution bodies and face

The 18 native child/adult body families, morph targets, eyelids and face tables are converted by `scripts/convert-chao-evolution.py`. The colour table is read by `scripts/extract-chao-look.py`. See [CHAO-EVOLUTION.md](CHAO-EVOLUTION.md) for sources, verification and what remains unresolved. The original `chao.glb` path above is unchanged.

## Original head icons and Chaos Emerald

`al_icon.prs` contains the original emotion sphere, punctuation/heart/ring sprites, and the sixteen `torch_blue00`–`torch_blue15` blue flame frames. The converter emits `.local/imports/chao-flame.png` (also `emotion-flame-atlas.png`) as a **512x32 RGBA atlas, 16 columns by one row, 32x32 per frame**, preserving the original frame pixels and alpha. `emotion-flame.png` is the first original flame frame; `emotion-sphere.png` is the original sphere. This allows the renderer to replace procedural head effects with original game imagery. Atlas orientation is handled by the browser's UV convention, not by redrawing the source flame.

The first `.local/imports/chaos-emerald.glb` was a Security Hall pickup-model experiment from `sonic2app.exe` address `A8CA7C`. Its local texture0 maps through `EMERALD_RED` to `miu64_sh013.png`, rather than global stage archive texture0. That intermediate is retained for provenance and is superseded by the **actual cinematic models**, as requested.

Run `node scripts/extract-cinematic-assets.js` and then portable Blender with `--python scripts/convert-cinematic-assets.py -- --preview`. Official SA Tools `splitEvent` splits a local copy of `event/e0207.prs` with its motion sidecar. Scene 2 entities2–8 are the original individual emeralds, mapped to blue/green/cyan/purple/red/white/yellow by `InternalTexlist.satex` texture IDs2–8. These export as seven distinct `.local/imports/emerald-{blue,green,cyan,purple,red,white,yellow}.glb` assets, using each model's original material and `chaoseme_*` image from `e0207texture.prs`. All have124 source vertices. Native material alpha0.8 is retained as `BLEND`, along with double-sided rendering and original environment/specular flags in `extras.sa2Source`; Chao body opacity is unrelated. Original sphere mapping requires a corresponding browser reflection shader. See [INVENTORY.md](INVENTORY.md) for exact mappings and limitations.

The cinematic helper also extracts the real Neutral garden palm tree from exe address `F27C6C`, using the official `CHAOSTGNEUT.ini` ChaoChunk mapping. It exports `.local/imports/tree-neutral.glb` with five original meshes,61 native vertices, original `gno_ki_def01/02/03` textures and masked foliage. No procedural tree substitute is needed.

`node scripts/extract-native-textures.js` separately exports all seven original `chaoseme_*` color PNGs plus the original Neutral garden grass image. These texture outputs preserve source facet pixels without recoloring.

## Authoritative native terrain

`node scripts/build-terrain.js` loads the three local garden GLBs with Babylon's NullEngine and the exact shared transforms/exclusions in `src/world-layout.js`. It writes ignored `.local/terrain/{garden,hero,dark}.json` in canonical world coordinates. Each field is89x89 samples at0.5 spacing across x/z[-22,22], row-major. Null cells have no walkable surface. The downward ray starts at y100 with length250 and selects the closest hit whose upward normal exceeds0.3, matching the renderer's query. The files include GLB SHA-256, transform, excluded meshes, and source geometry counts.

The validated fields contain5,031 walkable Neutral cells,4,968 Hero cells, and6,314 Dark cells. All preferred spawn points(0,6) lie on heightapproximately0. Empty glTF parent meshes must be skipped without disabling them, because disabling an empty parent also disables its real primitive children. The intended hub currently retains the separate procedural social layout; these heightfields describe the three original garden zones.

## Conversion milestone

1. Extract a single Chao and the Neutral garden from the player's own installation with X-Hax SA Tools. Work in `.local/extracted`, never edit `resource/gd_PC` directly.
2. Use Sonic Adventure IO (SAIO) in Blender to import the extracted model, textures, and supported animations. Preserve units, bone names, texture mappings, and animation names in a conversion manifest.
3. Inspect one idle and one movement animation in Blender. Chao body variations and procedural morphing need separate validation; a successful static import does not prove evolution compatibility.
4. Export glTF 2.0 binary (`.glb`) with embedded textures to `.local/imports/chao.glb` and `.local/imports/garden.glb`. The runtime loads these local converted assets.
5. Serve the import folder through the local asset endpoint and load with the browser's glTF loader. Build collision/navigation data separately from visual geometry. Native SA2 gameplay, collision routines, and missions do not execute inside a browser just because a model imports.
6. Expand only after the Chao, garden, animation, scale, and collision checks succeed. Mission scenery, enemies, audio conversion, other gardens, race tracks, and karate content each need their own extraction validation.

The distributed project contains our source and procedural proxy art. Original files, extracted material, converted files, inventories, and game saves stay local and belong in ignored `.local/` storage. Do not add them to source-control commits or deployment archives.

## Maintainer references

- SA Tools: https://github.com/X-Hax/sa_tools/wiki
- Exporting assets: https://github.com/X-Hax/sa_tools/wiki/Exporting-Assets
- SAIO model/level/texture/animation importing: https://x-hax.github.io/SonicAdventureBlenderIO/guides/asset_importing/
- SA2 Mod Loader (native Windows PC modding, separate from this browser runtime): https://github.com/X-Hax/sa2-mod-loader
- PC Chao export mapping: https://github.com/X-Hax/sa_tools/blob/master/GameConfig/PC_SA2/Chao_DLL.ini
- Split command source: https://github.com/X-Hax/sa_tools/blob/master/CommandLine/split/Program.cs
- Chao palette/bank mapping: https://github.com/X-Hax/sa_tools/blob/master/Libraries/SAEditorCommon/ChaoPalettes.cs
- Palette format: https://github.com/X-Hax/sa_tools/blob/master/Libraries/TextureLib/TexturePalette.cs
- Palette RGB565 codec: https://github.com/X-Hax/sa_tools/blob/master/Libraries/TextureLib/PixelCodecs/RGB565PixelCodec.cs
- Facial expression enum: https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Enums.h
