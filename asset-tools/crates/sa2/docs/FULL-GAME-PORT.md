# Sonic Adventure 2 + BATTLE full Rust port

The user requested the whole game in Rust and asked that the work remain
accessible on GitHub. The persistent tracker is
[client issue 11](https://github.com/mobdon/ChaoGardenMMOClient/issues/11).
This document is a scope/evidence ledger, not a completion claim.

The source/data/authority boundary remains unchanged: game code, shared
mechanics and readers/converters are Rust; browser/platform code presents
state and submits bounded intents through agreed Rust/WASM/service boundaries.
Authoritative identity, saves, stats, progression, inventory, rewards and trusted
time belong in the independent Rust server repository. No compiled retail
executable is treated as Rust source.

## Published foundation

[Client PR 7](https://github.com/mobdon/ChaoGardenMMOClient/pull/7), commit
`c6cd348`, contains the bounded reader/conversion milestone and reproducible
local parity evidence. It is a reviewable source branch, not a merged full game.
Its models/rigs, native keys, Basic collision descriptors, textures/palettes and
audio establish inputs for runtime work; parsing or converting those inputs
does not establish native behavior.

The isolated branch `codex/sa2-native-runtime`, based on `c6cd348`, adds
executable native hierarchy/motion evaluation and Basic collision world/query
work, with independent local comparisons recorded in
[NATIVE-RUNTIME-PARITY.md](NATIVE-RUNTIME-PARITY.md).
It does not replace the browser presentation or working
canonical service. Shared schema adoption and runtime integration remain
reviewed follow-ups, tracked through the server coordination board.

The `codex/sa2-stage-gameplay` milestone follows the original campaign tables
through the first Hero, Dark and Last event cameras and can mount both normal
City Escape layers. [Run it locally and see exactly what was compared](CAMPAIGN-ENTRY.md).
The next [City Escape board milestone](CITY-BOARD.md) resolves original board models and stage motions, executes the attached-board update, and queries the native stage collision below Sonic’s spawn. It still requires externally observed rider state. Passing these checks does not satisfy the main-game or integration gates below.

## Full-game acceptance gates

| Area | Required completion evidence |
| --- | --- |
| Sources and resources | Exact supported retail builds, source hashes, packed-resource reachability and explicit known/unknown fields across every required subsystem. |
| Native scene/animation/rendering | Original node transforms, skins/weights, supported motion evaluation, mesh/material/alpha behavior, textures/palettes, camera, lighting and matching native effects. Compare poses, surfaces, frame timing and actual rendered results with independent references. |
| World and collision | Native landtables, surfaces/flags, transforms, geometry, queries and response, triggers, SET objects and stage transitions. Static ray intersections alone do not complete character collision. |
| Main game | Speed, treasure-hunt, mech, kart and boss families; traversal/moves, enemies, upgrades, objectives, scoring, mission completion/checkpoints and progression, with playable stage-family parity gates. |
| Chao life | Care/needs, animals/drives, evolution and lifecycle, learning, toys/trees, Kindergarten and Black Market; source rules distinguished from explicit MMO adaptations. |
| Competitions | Genuine race routes/rules, Karate states/AI, eligibility/results and durable once-only rewards for authoritative adaptations. Resident wandering is not a competition port. |
| UI, audio, events and saves | Native menu/font/UI logic, scene/cutscene/event playback, required audio codecs/mixing/timing, configuration and source-backed save compatibility. |
| Transporter | Matching machine geometry/assembly, menus, native transfer/save ordering and an explicitly adapted secure device handoff, preserving one canonical Chao. |
| Integration and regression | Agreed Rust/WASM/platform contracts, actual canonical service round trips for integrated mechanics, restart/replay tests, deterministic reference traces and full-game playable regression evidence. |

Each completed gate must name its commit/PR, exact source/reference identity,
passing tests and independent observations. Unknown behavior must fail explicitly
or remain unavailable; it must not be replaced silently with invented native
rules, handmade effects, unrelated source effects or client-trusted progression.

## Preservation and publication

Publish Rust source, schemas, metadata/hashes, synthetic tests and documentation
to reviewable GitHub branches/PRs. Keep installed originals, converted geometry,
images/audio, ROMs, saves, credentials, external reference binaries and detailed
native reports ignored/local. Always search matching installed content before
considering a fallback. Preserve rigs/proportions, original pixels/vertex colors,
alpha/material behavior and record attachment/timing changes separately.

Client/server histories and other workstream ownership remain separate. Source
publication does not authorize live preview restart, runtime rollout or save
migration. The full-game tracker stays open until the complete acceptance gates
are met; no percentage or finished-game claim is inferred from asset counts.

## Story, terrain and mission tables

[Story run](STORY-RUN.md) reports where the three installed story openings stop. [Campaign mounts](FAN-STAGE.md) covers 49 authored level dispatches, and [static terrain](STAGE-LANDS.md) covers primary and alternate tables across 79 installed modules. The mount reports unsupported systems per level. Chao rig adaptation for story play comes after native gameplay; no Chao stats are awarded here.

From the checkout, use the PC_SA2 directory from the supported SA Tools mappings and fresh output paths under .local:

    sa2-assets fan-levels GAME/sonic2app.exe .local/levels.json PC_SA2
    sa2-assets fan-stage GAME/sonic2app.exe .local/green-forest.json PC_SA2 "Hero Story Sequence" 13
    sa2-assets stage-raycast GD_PC/stg03D.rel .local/probe.json objLandTable0003 REQUEST.json
    sa2-assets mission-ranks GAME/sonic2app.exe .local/ranks.json

A ray request contains origin, direction and maxDistance; a null hit is a valid result. The verified Green Forest mount has 517 entries and 144 Basic attachments; a downward 5,000-unit ray from the original Sonic start returned no static hit. That is not evidence the in-game player falls.

The Steam EXE provides 164 ordered rank rows: 30 each for missions 1, 3, 4 and 5; 34 for mission 2; five per kart route. Rust exposes native score units or time in seconds and retains the source address. The independent SA Tools 786 split of the five Mission rank sections from PC_SA2/Characters.ini and the two Route rank sections from CART.ini matched all level/mission associations and 656 D/C/B/A values. Reproduce with tests/mission_rank_reference.py OUTPUT_JSON SPLIT/figure/Ranks SA2Enums.h using the SA2 Mod Loader level enum. The reader does not calculate the final mission score, decide ties or modify the player's rank or save.

For an inventory of the original object behaviors that still need implementation, run:

    sa2-assets fan-tasks GAME/sonic2app.exe .local/tasks.json PC_SA2

Across the normal story SETs, 24,162 placement visits reduce to 24,003 distinct placed rows after repeated visits to the same layout and object table are counted once. All map to 634 distinct native task addresses. The report groups names and level IDs by address and counts rows with partial callback adapters (1,390 including shared ring, spring, whistle pipe and Chao container tasks). A resolved address identifies a task; it does not execute it. This inventory excludes objects created dynamically and systems outside those normal SETs. For example, 877 placements reference the emerald task: they do not by themselves implement treasure hunting.

For Sonic vs Shadow 2, duel-platform EXE OUTPUT.json reads the separate local-space platform collision model. Its motion, placement and live contact handler remain open.

[Shared stage callbacks](SHARED-STAGE-TASKS.md) documents mounted replay and magnetic shield selection.

[Damage and dropped ring behavior](DAMAGE-RINGS.md) covers ring loss, scatter, flight, static contact response and expiry.

[Goal-ring contact](GOAL-RING.md) covers native initialization decisions and activation calls across the 28 mapped story levels. The mission controller and automatic story advancement remain unimplemented.

[Playable Iron Gate](PLAYABLE-IRON-GATE.md) runs the first Dark stage live: walker movement, collision, lifts, elevators, bridges, springs, rings, checkpoints, death zones and the goal ring. Two of its three segments clear offline; the controller is an adaptation.

[Playable Prison Lane](PLAYABLE-PRISON-LANE.md) runs Tails' walker stage on the same session with its own stg09D.rel mover rules, PANEL floors, CCUBE solids and ring lines, building only what is read from the game files. The offline autopilot clears it from the spawn with no deaths.

[Big Foot opening](BIG-FOOT.md) covers boss startup and the arena boundary in the second Hero encounter; its combat loop and stage clear remain open.
