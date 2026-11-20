# Static stage terrain: independent conversion and campaign coverage

All 102 mapped static tables in 79 fingerprinted Steam-installed files match
SA Tools 786 conversion. The files are 64 uncompressed RELs (81 tables) and
15 PRS-compressed Chao modules (21 tables). At one native unit per output unit
the comparison covers **27,936 entries, 8,816 Basic collision meshes, 258,977
transformed vertices and 297,210 triangle index triples**. Tables with zero
Basic meshes can still contain visual entries. The two Egg Golem base tables,
for example, each have 14 visual entries and no Basic meshes.

The comparison checks entry geometry type and flags, bounding spheres, node
counts, positions, rotations, scales, and exact baked mesh indices and positions.
SA Tools may set node skip-position or skip-rotation flags when the corresponding
vector is zero. Only those specific additions are allowed by the comparator.
Stage 00, a six-entry test level, uses -1 for its visual count and interleaves
Basic and visual entries. SA Tools converts it to three visuals followed by
three collisions in source order [1,3,4,0,2,5]; the reader retains the source
order. Final Rush's supplementary UV header declares 516 visuals for its 514
entries; its 514 original flags are exclusively VISIBLE and the separately
converted view reports 514 visuals. These are semantic geometry comparisons,
not a claim that original addresses or serialized bytes are identical.

The verified mappings cover the shipped numbered files 00 and 03–59, except
42, seven dedicated boss files, and named Chao modules. The GC configuration
for 58 wrongly refers to stg56D.rel. Four symbols are reused across files,
so lookups use filename plus symbol. The corrected-address fixture records
26 offsets that differ from the source GC mappings for these exact installed
hashes. Meteor Herd's additional 17-entry UV table is named by PC_SA2/
LeftoverDLL.ini and identified in this Steam REL at 0x90D14, although omitted
from the GC config. Alternate/UV tables are separate reads; parity does not
say which alternatives a running game draws or activates.

## Coverage of the actual story tables

The Steam executable (SHA-256
4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16)
and PC_SA2/ADVERTISE.ini identify the three 12-byte StoryEntry arrays.
Counting only Level records up to End (excluding events and credits) gives:

| Sequence | RVA | Level IDs in native order |
| --- | --- | --- |
| Hero | 0x133A148 | 13, 60, 16, 20, 9, 10, 19, 3, 5, 17, 7, 70, 23, 28, 25, 63, 64, 24, 32, 33, 22, 29, 30, 42 |
| Dark | 0x133A370 | 11, 61, 18, 21, 14, 26, 27, 12, 20, 8, 62, 4, 19, 70, 6, 67, 44, 33, 43, 29, 40, 42 |
| Last | 0x133A580 | 36, 65, 66 |

Of 43 distinct direct level IDs, 40 have mapped static landtables: 33 numbered
IDs, plus dedicated bosses 60 Big Foot, 61 Hot Shot, 62 Flying Dog, 63 King
Boom Boo, 64 Egg Golem (Sonic), 65 Biolizard and 67 Egg Golem (Eggman).
Cannon's Core is dispatched as 36; its four other character sections 34, 35,
37, 38 also have separately verified terrain. Stage 00, Green Hill, multiplayer
and Chao modules increase inventory coverage but are not additional direct
campaign rows. Menu, title, story events and credits are not gameplay terrain.

| Direct ID without a generic landtable | Source evidence and limit |
| --- | --- |
| 42, Sonic vs Shadow 2 | PC_SA2/STG42.ini identifies a GC platform at RVA 0x1251C70 and its Basic collision object at 0x1251FA0. Rust duel_platform::read reads the separate collision object from the verified EXE. The standalone SA Tools SA1MDL corroborates the object's transform, 24 vertices and normals, bounds, material, and five strips including native index order and winding. The installed stg42D.rel DODAIGEN object places copies of an identical template every 300 units along +X ahead of the players; [DUEL-PLATFORMS.md](DUEL-PLATFORMS.md) replays it against the native PPC code. DODAI's activation and fall are replayed the same way. |
| 66, Finalhazard | GC_SA2B/Boss_Last2D.ini identifies the boss and character animations; PC_SA2/BOSS_LAST2.ini identifies the boss at RVA 0x13048DC and weakpoint at 0x12D36BC. Both yielded standalone Chunk models with SA Tools. No static terrain table or gameplay collision behavior was established. |
| 70, Route 101 / Route 280 | PC_SA2/CART.ini maps two separate courses at RVAs 0xAF82BC and 0xAF82C4. SA Tools exported 321 and 369 ordered segment IDs. The Win32 Data_DLL CartCourseModels export at RVA 0x58B190 (85 entries, PC_SA2/CART_DLL.ini) split into 81 Basic collision models and 81 GC visuals using an RVA-mapped DLL image with preferred base 0x10000000. The GC CartD.rel mapping at 0x10B61C did not convert on the Steam revision. Segment assembly and driving physics are separate; the multiplayer kart menu is not a story row. |

## Reproduce locally

In PowerShell 7, from the Rust crate, select a **new ignored output directory**
and set paths for the installed game and SA Tools 786. No original models or
conversion output belong in Git.

```powershell
$env:DOTNET_ROLL_FORWARD = 'Major' # when needed for the installed SA Tools runtime
$env:SA2_GAME_PATH = $game
$env:SA2_SET_DIR = Join-Path $game 'resource/gd_PC'
./tests/stage_land_reference.ps1 -ToolsRoot $tools -GameData $env:SA2_SET_DIR -GameExe (Join-Path $game 'sonic2app.exe') -PrivateOutput $privateOutput
$env:SA2_LAND_ORACLES = Join-Path $privateOutput 'rows.json'
$env:SA2_DUEL_PLATFORM_ORACLE = Join-Path $privateOutput 'platform.sa1mdl'
cargo test --locked --offline --test stage_land_parity --test stage_land_duel -- --ignored --nocapture
```

The driver checks the split.exe hash, decompresses with SA Tools' PRS.dll,
retains the .rel extension for relocation, and splits each table separately.
Each offset correction is tied to its source hash. The Rust test independently
decodes both sources, then uses the same Rust static geometry baker for each.
This verifies conversion, not the baker against the retail engine, visual
rendering, dynamic collision, stage progression or complete playable levels.

For Finalhazard, `finalhazard-geometry EXE OUTPUT.json` reads the 28 native
Chunk attachments by RVA; stream offsets within each attachment are raw file
offsets. Against the two local SA Tools splits, all 28 bounding spheres and
all 48 vertex chunk kinds matched. Comparing polygon words in stream order
gave 134 exact matches across 145 chunks. The other 11 (ten boss, one weakpoint) differ only in the high byte
of the ambient NRGB word of type 19 (NJD_CM_DA): 255 in the executable,
zero in the split. Ninja's njcnk_material.h identifies N as NOOP. Ambient
RGB and diffuse ARGB, including alpha, match. The Rust export keeps the
original byte. Reproduce with tests/finalhazard_polygon_reference.py and the
geometry JSON, rig JSON, and local split models directory as arguments. These model bounds are not hitboxes.
