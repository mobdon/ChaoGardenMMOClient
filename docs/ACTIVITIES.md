# Original Chao Races and Karate

The local activity extraction now includes original race configurations, rival templates, Karate opponents and skill parameters, course object sets, camera resources, three race worlds, and the Karate arena. The existing 298-clip motion extraction is documented separately in `ACTIVITY-MOTIONS.md`. These resources establish original content references. A native race does not become playable solely because its scenery or menu entry has been imported.

## Verified local outputs

| Output | Verified content |
|---|---|
| `.local/activities/races.json` | 62 original race configurations, 115 race-rival templates, native toy-unlock IDs, source addresses, world IDs and unchanged-scale bounds |
| `.local/activities/karate.json` | 28 appearance rows, 28 independent eight-short skill rows, 27 joined named opponent descriptors; four proposed retail tier groupings with five opponents each |
| `.local/activities/race-sets.json` | All 14 installed Chao race SET files decoded with original object types, rotations, positions and parameters |
| `.local/activities/race-cameras.json` | All ten installed race camera files retained byte-for-byte in decoded records, with candidate point blocks and explicit interpretation gaps |
| `.local/imports/activities/*.glb` | Original Normal/Dark/Hero race scenery and Karate arena, with embedded original textures |
| `.local/activities/*-collision.json` | Original collision triangles and surface flags, transformed into canonical native ×0.1 coordinates |
| `.local/extracted/activities/` | Separate native intermediates, 115 mapped stage prop/light sections, extraction logs and input hash manifest |

All 39 original inputs were read and SHA-256 checked unchanged. The extracted original files, decoded tables, textures, GLBs and reports stay ignored under `.local/`. The reproducible scripts and this document contain source code and provenance only. Existing garden and Chao exports are not overwritten by this pipeline.

## Race configurations and rivals

The native configuration sequence at executable virtual addresses `0x01354828` through `0x01354d00` contains 62 twenty-byte records: 12 Beginner entries, 30 Jewel entries, 12 Challenge entries, four Hero entries and four Dark entries. Beginner contains four courses with three levels each; Jewel contains six courses with five levels each. Challenge/Hero/Dark entries retain their native sequence and numeric rival-set selection, rather than invented opponent descriptions.

Source course IDs0–9 correspond to installed suffixes `BegSwm`, `BegFly`, `BegRun`, `BegPow`, `JwlSwm`, `JwlFly`, `JwlRun`, `JwlPow`, `JwlIL`, and `JwlLng`. Retail display aliases in the catalog are Crab Pool, Stump Valley, Mushroom Forest, Block Canyon, Aquamarine, Topaz, Peridot, Garnet, Onyx, and Diamond. The source suffix and table index remain authoritative identifiers independently of those display names.

The raw five integers are preserved for every record. Interpreted fields distinguish rival-set index, landtable index, course index, paired course index, and toy-unlock ID. The first four field roles are supported by their repeated values and the native tables, but the complete consuming runtime path still needs tracing. The existing CWE header’s four-entry Challenge declaration covers only a prefix; actual contiguous installed data contains twelve Challenge records before the mapped Hero table. Do not truncate this catalog to that declaration.

The 115 rival records at `0x011ce2a0` use the primary mod developer’s 84-byte `RaceBotData` schema. Appearance bytes, eight morphology parameters, stat mode, shared stat, and per-stat short pairs are preserved. Selection, random-range interpretation and resulting race behavior are not inferred from those fields alone.

Native toy-unlock integers are source data. The `rewardRings:25` field is an explicitly proposed MMO reward, not an original race payout. Toy acquisition, medals, eligibility, race completion flags and unlock conditions still need their full native behavior audit. Every native course currently has `available:false`, no claimed spawn, and no checkpoints. The prototype checkpoint race is a separate implemented activity.

## Karate source tables

The original appearance table at `0x01296c10` contains 28 `AL_SHAPE_ELEMENT` records of52 bytes. Their inline names and skill slots are zero in the installed binary; using them as opponent stats would produce incorrect empty combatants. The independent native skill table is `0x01296a50`, with224 signed shorts:28 rows of8 values.

An adjacent sequence at `0x01296940` contains27 ten-byte descriptors. Its appearance index, localized name index and skill-row index have been joined and bounds-checked against those tables. Extra descriptor fields are retained without invented meanings. The localized `MsgChaoKarateName_e.bin` uses big-endian string offsets, a `0xffffffff` sentinel and the game’s Chao character encoding. It yields27 labels including duplicate names; IDs preserve native descriptor indices so duplicates do not collide.

The four five-opponent groups are Beginner (Chaosky through Happy), Standard (Lucky through Stinky), Expert (Magic through Wacky), and Super (Crunch through Flash). The roster follows the ordered native descriptor/name sequence. The exact tournament-selection arithmetic has not yet been independently traced, and `tierSelectionCallsiteValidated:false` records that gap. Two earlier descriptors and five later descriptors remain preserved outside the proposed20-opponent tier grouping.

The seven principal skill fields are Swim, Fly, Run, Power, Stamina, Luck and Intelligence; the eighth short is retained separately. Native hit, evasion, stamina, zeal, timing, timeout, draw, ring-out, reward and unlock equations are not reproduced simply by reading these values. The backend may implement an explicitly labeled MMO combat adaptation using the original roster/stat references. `rewardRings:50` is a proposed adaptation. Original appearance parameters also require the appropriate Chao morphology/material implementation before visually accurate opponent forms can be claimed.

## Geometry, collision and scale

The PC `Data_DLL.dll` exports `objLandTableRace`, `objLandTableRaceDark`, `objLandTableRaceHero`, and `objLandTableChaoKarate`. Official SA Tools maps extract those exports directly. Dedicated outputs are:

| Asset | Visual meshes / original vertices | Original collision triangles |
|---|---:|---:|
| `race-normal.glb` |120 /11,471 |952 |
| `race-dark.glb` |128 /11,301 |952 |
| `race-hero.glb` |128 /11,627 |952 |
| `karate-arena.glb` |27 /9,171 |42 |

The four textured GLBs occupy9,818,060 bytes. GLBs retain native source units and origin; runtime scale is0.1 with translation `[0,0,0]`. Collision JSON already applies this same scale. Native race collision bounds are X−22.1740875 through70.5 and Z−74.3283005 through39.0000954. They exceed the garden’s earlier ±22 bounds and need their own world bounds. Do not shrink a course to fit an unrelated garden limit. Large visual skyboxes are not collision bounds.

The Karate collision ring spans X±2.8381584 and Z−1.0661585 through4.6101584, with its upper surface at Y0.75. Original surface flags and triangles are preserved. This is input for collision/terrain work, not a completed server collision engine or validated race route. Stage decorations and controllers are extracted separately; not every prop is already instantiated from SET entries in the browser.

## Course sets and original cameras

SET decoding follows the primary SA Tools `SETItem` schema: big-endian count,32-byte header, and32-byte records. The race’s native twelve-entry object list is read from `0x013533f8`/`0x01353338`; entries include named decorations, Omochao controllers, collision shapes and wall planes. These placements are not automatically ordered race gates. Camera control points are not checkpoint gates either. Original route/path definitions, obstacle trigger conditions and start/finish ordering remain required for authentic playable courses.

All ten `ChaoRaceCam*.bin` resources have15,360 bytes. Repeating layout suggests80 slots of192 bytes per resource;79 nonzero candidate segments occur across the files. Raw192-byte records, all48 big-endian float interpretations, leading flag bytes, candidate point blocks at offsets8/40/52/104/136/148, and the final float at188 are preserved. Reassembling the raw records exactly matches each installed file. Candidate fields are explicitly labeled because their runtime meanings are not yet verified.

The native camera is useful reference material for course framing. Its mode enum, segment selection, target switching, interpolation, collision and precise fixed/follow/rail behavior still need tracing. The browser can adapt verified camera position/target references with the same0.1 world transform while retaining its interactive third person controller. These binary resources cannot be dropped into the browser as executable camera behavior, and current extraction does not claim an exact native camera port.

## Reproduction and remaining acceptance

```powershell
node scripts/extract-chao-activities.js
$env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local/blender-user'
& '.local/tools/blender-5.1.2-windows-x64/blender.exe' --background --factory-startup --python scripts/convert-chao-activities.py
node scripts/verify-chao-activities.js
```

The verifier checks all input hashes, table counts and opponent joins, all62 unavailable-course declarations, exact camera byte coverage, embedded GLB textures/output hashes, collision triangle indices, and unchanged coordinate origin/scale. Live visual inspection, native path/gate extraction, obstacle mechanics, per-course rival/eligibility selection, complete match/race rules, camera interpretation and authoritative multiplayer playtests remain acceptance work. Regenerating extraction resets availability; the converter reapplies geometry metadata. Keep playable route authoring and its verification as a separate deliberate step.

Primary mappings: X-Hax SA Tools786 `PC_SA2/Chao_DLL.ini`, `CHAOSTGRACE.ini`, `CHAOSTGKARATE.ini`; X-Hax sa2-mod-loader `SA2Variables.h` and `SA2Structs.h`; X-Hax SA Tools `Libraries/SAEditorCommon/DataTypes/SETItem.cs`; Exant64/CWE `al_race.h` and `SA2Structs.h`; Ngolinvaux/SA2Randomizer `SA2Rando.h` and `SA2RStructs.h`. Headers identify layouts/addresses; actual values come from the user’s unchanged installed copy. Mod-added rewards or stat modifications are not copied into the retail data interpretation.
