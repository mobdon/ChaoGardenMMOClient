# Offline story stage loading

The Rust Campaign index opens the three single-player story arrays in the supported Steam executable. For a chosen **level action** it resolves the authored character and level, the original single-player start row when present, the native object table, the normal SET placements, and the named static landtable when available. It can build a collision world at native scale and resolve a placement to its object definition. The mounted result distinguishes missing resources from unimplemented gameplay. It does not run a stage to completion.

From the client worktree, with the local installation and SA Tools 786 configuration paths:

```powershell
$tool = 'asset-tools/crates/sa2/target/debug/sa2-assets.exe'
$exe = Join-Path $game 'sonic2app.exe'
& $tool --source-root $game fan-levels $exe .local/story-levels.json $config
& $tool --source-root $game fan-stage $exe .local/story-city.json $config 'Hero Story Sequence' 1
& $tool --source-root $game fan-stage $exe .local/story-route280.json $config 'Dark Story Sequence' 35
```

The ordinal counts each event slot as one action and stops at that entry's first sentinel. For example, Last Story's Final Hazard is action 10. Reading a level by ordinal is a local seek. It never advances a save, acknowledges an outstanding gameplay result, or issues Chao rewards.

## Installed campaign evidence

These are the actual ordered **level IDs** in the story tables (events and credits omitted). The local test includes every corresponding ordinal and character, including different characters for reused boss stages.

| Sequence | Level dispatches | Authored level IDs in order |
|---|---:|---|
| Hero | 24 | 13, 60, 16, 20, 9, 10, 19, 3, 5, 17, 7, 70, 23, 28, 25, 63, 64, 24, 32, 33, 22, 29, 30, 42 |
| Dark | 22 | 11, 61, 18, 21, 14, 26, 27, 12, 20, 8, 62, 4, 19, 70, 6, 67, 44, 33, 43, 29, 40, 42 |
| Last | 3 | 36, 65, 66 |

All 49 dispatches mounted or reported their precise resource gaps. The result contains **94 layer mounts from 84 distinct SET files**, including both normal kart SETs. Across those dispatches, all **24,162 placements** resolve against their selected native definitions. Each placement retains layer identity, source offset, packed ID interpretation, transform and parameters. Definitions include original names, flags, load distances and function addresses. A resolved definition does not implement its function.

**43 of 49** dispatches have a start row for the exact authored character. Big Foot (60), Hot Shot (61), Flying Dog (62), Biolizard (65) and both kart dispatches (70) have no matching general single-player start row. No character substitution or invented start position is used. Final Hazard (66) has an authored pose and a level section, but its configured BossLast2 object-list path lacks a matching mapping in ObjectLists.ini. No normal SET or static landtable is declared/available there.

**44 of 49** dispatches mount verified static terrain from **40 distinct levels**. All 40 built a Rust collision world. Sonic vs Shadow 2 (42) has no static landtable in either story; Final Hazard and both kart courses also have none. Some boss tables contain only visual entries; the existence of a landtable does not imply a solid floor or a port of a boss routine. The original five Cannon's Core sections are not five independent Last Story dispatches: the story selects 36. In-level character handoffs are outside this loader.

## Kart routes

Both kart actions dispatch stage 70, but Hero selects MechTails and Dark selects Rouge. SA Tools 786 PC_SA2/CART.ini names two distinct eight-byte descriptors in the executable: Route 101 at RVA AF82BC and Route 280 at AF82C4. Each gives a preferred pointer and a count of ordered byte-sized part IDs. The normal character-specific SET files resolve against the original 13-record Kart Object List at RVA C0A230. These are offline associations of installed assets; no hard-mode, minigame or base SET is substituted.

| Route | Course part bytes | SHA-256 of part IDs | SET placements |
|---|---:|---|---:|
| Route 101 | 321 | e93d8da2d30b59a53f3e3750323da91d6e3bea470a99975cf5d4438d91b79c1c | 34 |
| Route 280 | 369 | 08154159fc8b6a97bf113308b85e7f93959d818a938d07e55ee025e79a2e1aea | 29 |

For an independent reference, split the two kartcourse sections of CART.ini with SA Tools 786 split.exe binary and the installed executable; hash the decimal lines of each exported course as bytes in order. The local test compares both complete byte sequences to these reference digests. The offline kart geometry query below assembles both tracks. Driving physics, timing and completion remain unverified.

## Rust interface and checks

Campaign::new takes the EXE bytes and the literal contents of ADVERTISE.ini, sa2lvl.ini and ObjectLists.ini. All four revisions are checked. Campaign::levels returns authored level dispatches. Campaign::mount takes a sequence, action ordinal and a caller-supplied resolver for installed resource basenames under resource/gd_PC. Returning None records a missing file; supplying unrelated bytes under a requested SET or landtable name fails. The loader reads normal layers in original placement order and matches LevelGeometry by configured relative path to the specific stage_land symbol, never by picking a nearby stage ID.

MountedStage::resolve(layer, index) returns the original placement and optional definition. MountedStage::collision_world() bakes the explicitly matched terrain on demand; the resulting world supports geometric ray queries. MountedStage::replay_object also runs the verified shared ring and spring callbacks across compatible stages, with caller-supplied observations (see [Shared stage callbacks](SHARED-STAGE-TASKS.md)). The mount does not schedule tasks or implement general player movement, dynamic contacts, mission logic, transitions, boss AI, rendering, or save progression.

Run cargo test --locked --test fan_stage_local -- --ignored from this crate with SA2_GAME_PATH and SA2_PC_CONFIG pointing to the installation and SA Tools PC_SA2 config directory. The installed test checks all authored level dispatches, distinct SETs and resolution counts, exact kart course bytes, start and terrain coverage, all 40 collision-world builds, a City Escape ray, unavailable resources, a wrong SET, and rejection of an event ordinal. Only code, source identities, and observed counts are stored here.

Big Foot’s boss task later places player slot zero at (-90, 0, 0) during its [intro callback](BIG-FOOT.md). The general start-row count above is unchanged.

Both kart mounts also load their character-specific 40-byte story vehicle profile and five nine-byte rank-time rows from the Steam executable. Each row retains the level byte and the four D/C/B/A minute-second pairs; the two unidentified physics floats remain unknown1 and unknown2. Route 101 uses the Tornado car profile (speed cap 10), while Route 280 uses the Bat car profile (speed cap 12). Both have a separate top-speed field of 15. The mounted campaign test compares all named fields, thresholds and raw table hashes against independent SA Tools 786 split.exe binary exports of the two story physics and two rank-time sections in CART.ini. These are tuning and ranking inputs; steering, engine placement rounding, finish detection and rank assignment still require runtime verification.

Run `kart-raycast EXE OUTPUT.json DATA_DLL ROUTE REQUEST.json` with route 101, 280, beginner, standard or expert and a request such as `{"origin":[0,100,-200],"direction":[0,-1,0],"maxDistance":500}`. Coordinates are native units. The query joins the story course descriptor to the DLL's collision pieces, starts at the course origin with zero heading and searches the positioned triangles for the nearest geometric hit. It returns the zero-based course occurrence, piece ID, strip, triangle, original indices, hit point, distance and normal, or null for a miss. Placement uses mathematical sine/cosine. A hit does not establish tire contact, kart suspension, steering or a finish.

`tests/kart_rays_reference.py` takes the CLI, game directory, split course directory, split model directory and a private working directory. It builds independent rays from the SA Tools course lists, exported endpoints/yaw and Basic model triangles. Starting, middle and final occurrences on each of the five courses matched their source triangle and distance within 0.005 native units.

The kart-courses command (EXE OUTPUT.json) reads the five PC_SA2/CART.ini course descriptors. The racing menu courses contain 88 (Beginner), 112 (Standard) and 123 (Expert) piece IDs. They are separate from story dispatches. The Standard and Expert lists include pieces 25 and 26, whose native table entries have displacements but null collision roots. They move subsequent pieces into place and contribute no static triangles. The ray report lists each such occurrence as [zero-based index, piece ID]. Dynamic collision, vehicle controls, lap counting and race completion are unverified.

`kart-vehicles EXE OUTPUT.json` reads all 15 kart physics records mapped in CART.ini. The two story vehicles, six standard karts, placeholder and six alternate/bonus karts retain their table order, source RVA, 40-byte source digest and ten float values. The story mounts use the same decoder. Unknown1 and Unknown2 retain their source labels; these parameters alone do not determine how steering, drift, braking or collision response is integrated each tick. The ChaoKart record is native bonus vehicle data.

To reproduce: split all kartphysics sections of PC_SA2/CART.ini with SA Tools 786, then run `python tests/kart_vehicles_reference.py OUTPUT.json SPLIT_PHYSICS_DIR CART.ini`. The checker compares the name, slot and configured address and reconstructs the original bytes from all named values.

`kart-objects SET OUTPUT.json EXE` resolves the 13-entry native Kart Object List for any of the eight pinned setCart files. The 204 placements across those files include the base, three Mini sets and alternate story sets; they do not all run together. Each object ID indexes its executable callback definition. Position, raw angles and the three parameter floats retain their source values.

The same report includes the two standalone Basic collision meshes mapped by PC_SA2/CART.ini: JUMPDAI (6 vertices) and CHECKGATE (48 vertices). These are in model-local coordinates. GATE and GATE2 have distinct callback addresses; the names do not establish which callback uses CHECKGATE, how it transforms the mesh, or when a checkpoint counts. Ramp launch behavior is likewise unverified. To repeat the comparison, split the two basicmodel sections with SA Tools 786, set SA2_GAME_PATH and SA2_KART_OBJECT_SPLITS (the directory containing both sa1mdl files), then run `cargo test --locked --test kart_objects_local -- --include-ignored`.

`kart-ramp-init CartD.rel OUTPUT.json TASK_POSE.json` reproduces the post-allocation block of the installed BATTLE JUMP PANEL callback. Input supplies the current task position and full signed yaw, for example {"position":[100,0,-400],"yaw":16384}. Module 80's slot 1 name and executable relocations identify section 1 + 0xBC98. After the work allocation succeeds, instructions at 0xBD04â€“0xBDEC copy a 56-byte node, clear its two low flags, set translation and yaw, zero pitch and roll, issue two external calls and initialize task-work +0x28. The result represents the attachment pointer as a section and offset; it does not expose an emulator address.

Run `python tests/kart_ramp_ppc.py CLI CartD.rel PRIVATE_DIR` with Unicorn installed. The test executes the original instructions for three poses and compares every initialized node word, both call argument lists and the final work field. The calls themselves are not executed. This establishes ramp setup given the task pose, not SET-to-task conversion, engine collision registration, a kart launch, or checkpoint credit.

`kart-ramp-tick CartD.rel OUTPUT.json OBSERVATIONS.json` replays the active JUMP PANEL contact loop (section 1 + 0xBDF4 through 0xBF2C). The timer decreases by one per update and clamps to zero. When contact bit zero is set, the task checks each observed contact, in order, against six non-null registry entries (helper 0x1714). Every match sends the local vector [0, work+0x24, work+0x20] through the yaw transform and invokes the kart response. The timer gates only the later 0x5B90 call; a nonzero return resets it to 30. Multiple contacts can therefore receive a response in the same tick.

The report records transform inputs and subsequent conditional calls, not final kart velocity. Contact generation, the matrix result, the internals of the kart response at 0x5524 and external call effects are outside this replay. The request supplies post-response kart fields and the indexed course byte when required. See `tests/kart_ramp_contact_ppc.py` for 15 runnable inputs and the comparison with the actual contact loop, registry and secondary helper instructions; run with CLI, installed CartD.rel and an ignored temporary directory.

The secondary helper at 0x5B90 returns zero for a cleared work+0x44 field or a signed work+0x48 slot of 8 or above. For mapped slots 0–7 and a nonzero work+0x44 it returns one, resetting the panel timer. Negative slots precede the verified table and are rejected. Helper 0x979C reads a signed course byte: 50–71 inclusive passes 0 to the imported routine at 0x8006C97C; other bytes pass -127. The subsequent call to 0x8006B710 selects its first argument from the native two-column table (ramp column 0), re-reading the slot after the first call. The replay accepts an optional changed slot for this reload, otherwise retaining the observed one. Call effects and the source of the live course index remain external.
