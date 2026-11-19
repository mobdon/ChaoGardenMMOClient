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
Passing these checks does not satisfy the main-game or integration gates below.

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
