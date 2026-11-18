# Chao Garden RPG — working design

## Agreed direction

Play **as a Chao** in a browser RPG with shared gardens. The intended game requires the player's own Steam installation of **Sonic Adventure 2 and BATTLE DLC**. This is a Chao-centered game: its missions can draw on the wider SA2 game, but the player remains a Chao rather than switching to Sonic or another main character.

Start with a normal/Neutral Chao. Choices can develop a mischievous Dark Chao or a helpful Hero Chao and open their corresponding gardens. Chaos Chao evolution is earned through play. The owner's admin Chao starts with perfect stats: S grades in Swim, Fly, Run, Power, and Stamina; maximum numeric grades of 255 in hidden Luck and Intelligence; and level 99 in all seven stats. Hidden grades do not use the original five-stat letter-grade range.

The authorized owner can also choose **Dark Chaos Chao** directly through the admin form action. This sets the owner to Chaos form with −100 alignment, preserves perfect stats and existing rings, and persists the choice. Regular players cannot use that action.

Special Chao become recognizable NPCs. Local conversions are available for Neutral, Hero, and Dark Chaos Chao, plus Tails, Knuckles, and Amy Chao, using the mapped models in the player's PC `Data_DLL.dll`. The hub assigns Knuckles to skill training, Tails to races, and Amy to the shop. Original appearance loading and full native animation/growth behavior are separate milestones.

Sonic-like and Shadow-like Chao are possible earned appearance goals. Chao Island's SA2B galleries identify the Sonic Chao as Neutral/Run/Run and the Shadow Chao as Dark/Run/Run. The PC mapping includes the Neutral Run (`al_nrun`, array index 6) and Dark Run (`al_drun`, index 78) model families; their pointers are present in the inspected local DLL. Reproducing the mature appearances still requires validating the original growth/morph data. These forms are optional scope under discussion and are not implemented in the procedural prototype.

## Core loop

Meet other Chao in the main hub, enter a mission, collect rings and clear its objective, then return home. Visit Amy to spend rings, recover energy, and practice skills with Knuckles. Explore the separate gardens, compete in races, and work toward alignment areas and Chaos evolution.

The requested phone companion is a **Tiny Chao Garden PWA** with the same server-owned Chao: stats, care, short minigames and bounded AFK training. Preserve the original Chao Transporter relationship as a mode/device handoff, not a client-uploaded stats copy. The user supplied Sonic Advance 2 and Sonic Pinball Party GBA archives for local research and requested Rust ports; neither whole game is currently ported. [TINY-GARDEN.md](TINY-GARDEN.md) records source evidence, native systems, Chaos Chao presentation, animal/drive progression, idle training, cross-device leases, saves and offline limits.

Connect original stage animals/drives to care and progression rather than treating them only as scenery. Stage pickups, inventory, needs, stat effects, learned actions and appearance/evolution must use one authoritative system on phone and desktop. Actual animal pickups and full native growth behavior remain implementation work.

The wider SA2 game provides possible mission settings and objectives. Adapting an original stage means designing routes, collision, traversal, enemies, and objectives for a Chao. Importing its scenery alone does not import SA2's gameplay code.

## Current first vertical slice

The current source implements a browser prototype with a local authoritative server:

- A controllable Chao, a separate shared main hub, Neutral/Hero/Dark garden areas, a mission arena, and a checkpoint race. New sessions, Home, race completion, defeat, and reconnect return to the hub. The Neutral Garden is its own destination.
- Server-simulated movement and jumping, proximity-based ring collection, robot combat with range and cooldown limits, and one mission-clear reward per participant per encounter.
- Fruit costing three rings at the Neutral Garden grove, energy recovery, and skill training. The server requires proximity: two world units for the grove and hub trainer, three for the Neutral Garden pond. Current training and progression values are prototype tuning.
- A server-priced Chao shop using earned rings: Garden Fruit costs 5 rings and restores 25 energy and fullness plus 10 happiness; Run Drive and Power Drive each cost 12 rings and add 2 levels to their respective skills; Eggshell Cap costs 25 rings and becomes a persistent owned/equipped cosmetic. The cap can be bought once. Purchases require sufficient rings and being within two world units of Amy in the main hub. Client-supplied positions, prices, or rewards have no effect.
- Alignment choices bounded between −100 and +100. Hero Garden opens at +60; Dark Garden opens at −60. This is our prototype rule rather than a claim about original SA2 progression.
- A race whose ordered checkpoints must be reached through movement. Completing it awards rings and improves Run.
- A prototype earned Chaos evolution gate: three mission clears, one race completion, Run/Swim/Power level 10, and happiness 80. The owner has the separate authorized direct Dark Chaos action; regular players follow the earned gate.
- Persistent local profiles, server-owned stats and rewards, and the first local profile receiving the owner's admin role.
- Steam/game-file discovery and a local inventory that identifies resource candidates and checks relevant DLL exports without executing the game DLL.

The renderer can load locally converted original Chao and Neutral/Hero/Dark garden assets through an exact-file local asset endpoint, with procedural fallbacks. The current main hub remains a separate procedural gathering area; original Entrance/Lobby reuse is still an asset milestone. The mission arena and race are prototype layouts, and original SA2 stage missions are planned work.

Garden terrain heightfields are generated from the same canonical converted geometry and loaded read-only by the server. Server simulation owns ground height, jumping, bounds, missing-cell barriers, and uphill step checks; the client cannot supply its own height or teleport. Missing terrain files use the explicit flat development fallback reported by `/api/status`; malformed present files fail startup. The mission and race currently use flat ground.

The installation detector reports local file evidence; it does not authenticate a Steam account or verify BATTLE entitlement. The intended ownership requirement and the current technical checks are separate.

## Asset proof and next work

Follow [ASSET-PIPELINE.md](./ASSET-PIPELINE.md) for the local extraction and conversion process. The current conversions include all three gardens, the base Chao, Chaos forms, and special Chao models. Original Dark Chaos emotion-flame frames have also been converted to a local image atlas. These files stay in ignored local working storage.

Continue checking native textures, shared world/character scale, original animation fidelity, and traversal in each garden. The next larger content proof is one original normal SA2 stage adapted into a playable Chao mission. Accurate original Chao growth/morphing needs its own validation; imported static models alone do not establish it.

Original resources, extracted assets, converted files, and game saves remain local and outside the tracked source. The distributed prototype contains source code and procedural placeholder art.

## Project record and delivery rules

Updated November 18, 2026. This document records agreed behavior and design decisions. [REQUESTS.md](REQUESTS.md) preserves the request ledger; [TASKS.md](../TASKS.md) is the ordered work queue. Each work session should read those files, continue the next unblocked task, record evidence, and update status before stopping. Visual work remains awaiting verification until it has been checked in the running browser.

The project is a browser game with an authoritative Rust service. The installed game provides reference behavior and locally converted content. Our existing custom gameplay code is being ported to Rust. Compiled SA2 binaries and extracted models are not original source code, and importing them does not port the original engine or unlock all its mechanics automatically.

## Original movement and activity library

The user explicitly requested the movements from Chao Karate and Chao Races, including running, jumping, flying, climbing, and falling. Preserve the entire original motion bank so actions outside those examples are not lost. The initial native table inspection found 622 indexed Chao states resolving to 279 unique motion clips on a 40-node skeleton. These are discovery counts; extraction, conversion, playback, and gameplay integration have separate completion states in [ACTIVITY-MOTIONS.md](ACTIVITY-MOTIONS.md).

Movement coverage must include the following states where a matching source is identified:

| Family | Required behaviors and transitions |
| --- | --- |
| Ground | Idle, walk, run, sprint, start, stop, turn, crawl, and recover balance. |
| Air | Jump takeoff, rising, airborne movement, falling, landing, flying, and returning to ground. |
| Vertical | Climbing, ledge interaction, reaching the top, letting go, and falling from a climb. |
| Water | Enter water, swim, change direction, recover to land, and relevant race transitions. |
| Race | Start/readiness, course locomotion and obstacles, fatigue/recovery, finish, celebration, and defeat. Retain the shared movement references as well as activity-specific states. |
| Karate | Ready stance, movement, punches, kicks, dodges, blocks if present, hit reactions, knockdown, getting up, exhaustion, victory, and defeat. Preserve actual native variants rather than inventing a single generic attack. |
| Life and interaction | Eating, carrying, sitting, sleeping, emotions, NPC interaction, and other motions found in the full bank. These follow the locomotion/combat integration milestones. |

**Movement precedence:** (1) use the original Chao movement for every stage action the Chao system supports; (2) only for a missing action, adapt the relevant original stage character's movement/mechanic to the Chao; (3) create new motion only for a remaining gap. Log the missing native capability and the chosen fallback; do not replace existing Chao locomotion merely because a stage originally belonged to Sonic, Shadow, Knuckles, Rouge, Tails or Eggman. The mech may use its own native vehicle motions while its Chao pilot uses appropriate Chao poses/movements.

**Protect the Chao model:** keep native bone hierarchy, rest pose and proportions intact. Scale imported assets through a common parent transform; do not stretch individual bones/mesh axes to make another character's animation fit. Any fallback needs deliberate skeleton retargeting and root-motion normalization. Check representative animation frames for collapsed joints, detached eyes/wings/feet, broken skin weights, foot sliding and cockpit clipping before integration. Match movement distances/collision to Chao scale and tune camera/attachment offsets separately. Preserve a known-good native Chao export for comparison and rollback.

Native footwear must follow animated foot 006/013 rather than static leg pivot 005/012. Capture prop offsets before animations start; preserve the original uniform shoe fitting and rest orientation. The chest emerald sits halfway into the actual torso surface while seven original gems continue orbiting. Shadow shoe jets currently adapt the original Chao flame atlas; they follow the feet only during eligible snapshot-driven skating. See [FOOTWEAR-FIX.md](FOOTWEAR-FIX.md) for evidence and remaining contact limits.

The exact native names, indices, duration, skeleton binding, and missing/unsupported clips belong in the motion manifest. No animation is marked usable solely because its filename suggests a behavior. Check foot contact, root motion, orientation, looping, transitions, and skeleton compatibility in the browser. The server decides whether jumping, climbing, swimming, flying, attacking, or falling is permitted; animation only presents that state. Original motion offsets must not let a client move through walls or change authoritative height.

## Regular SA2 mechanics and mission adaptation

The game may also use normal SA2 mechanics, beyond the gardens. Build a reference and implementation catalog instead of assuming every original system is already available. Candidate systems include ring collection and damage response, ground and aerial movement, enemies, attacks, switches and gates, mission objectives, checkpoints, scoring, and stage traversal. Preserve original model/motion/sound references when useful, then implement the chosen rules in our server and browser.

Each mechanic needs: original evidence; intended Chao behavior; required assets; server-owned state; client presentation; measurable acceptance criteria; and a fidelity status. Original rules and intentionally changed RPG rules must be labeled separately. For example, our current alignment thresholds and Chaos evolution requirements are prototype rules.

Keep all requested stage families in view: Sonic and Shadow stages, Knuckles and Rouge stages, and Tails and Eggman mech stages. Their Chao adaptations may need different route objectives, camera behavior, traversal, enemy interactions and collision. Start with one segment per family rather than claiming scenery imports make whole stages playable.

The original Chao movement bank is the animation foundation for mission traversal. Mission running should scale with the Chao's server-owned Run skill; design related flight, swimming, climbing/jumping or power checks around relevant skills where appropriate. Tune baseline, middle and maximum levels against real routes. Clamp movement by mode, apply acceleration and safe speed limits, and make animation rate match actual velocity. Stats should create meaningful progress without breaking collision or making early routes impossible. Mounted mech movement needs its own tuning rather than blindly inheriting on-foot scale/speed.

| System | Current implementation | Next fidelity work |
| --- | --- | --- |
| Movement and ground collision | Bounded server movement, jumping, gravity, grid ground checks. | Original motion transitions; water, climb and fly state machines; collision appropriate to each traversal mode. |
| Rings and damage | Server ring pickups, robot damage and mission rewards. | Reference normal-stage damage/ring-loss behavior and choose the Chao adaptation. |
| Enemies and missions | Five-robot cooperative prototype arena. | One adapted original stage segment with working collisions, enemies, checkpoints and an end condition. |
| Chao stats and growth | Seven stats, prototype training, five main grades, hidden Luck/Intelligence, alignment and Chaos gate. | Original growth/morph references, drives/animals/food, reincarnation and evolution catalog; implement selected systems in stages. |
| Race | Physical ordered checkpoints and a reward. | Original movement/obstacle states, stamina and stat effects, course rules and multiplayer competition. |
| Karate | Basic prototype arena attack is separate from Chao Karate. | Native karate motions, validated attack/dodge/recovery timing and an authoritative match loop. |
| Shop and cosmetics | Server prices, proximity, four items and persistent cap. | Larger catalog, inventory ownership, preview and equip flows after persistence design. |
| Mechs | Requested and being prototyped. | Original Dark Chao Walker presentation first; piloting, aiming, attacks and missions require separate gameplay work. |

No full-engine conversion is promised by the asset inventory. Prefer one complete playable mechanic at a time with reference evidence and multiplayer checks.

## Chao Walker and Dark Chao Walker

The user identified the original Chao-piloted mechs and explicitly asked to put the owner's Dark Chaos Chao in the **Dark Chao Walker**, with the Chaos Emeralds floating around the mounted character. SEGA describes Chao Walkers in SA2's two-player battles; adapting those mechs to normal mission stages is our new design work, not proof that the original contains Chao story missions.

First prototype: use the actual Dark Chao Walker model and matching textures, replace or hide its stock pilot as needed, seat the owner's native Dark Chaos model at the cockpit, and move the emerald orbit to the mounted assembly. Keep the upright original blue head flame and raised red/gold owner nameplate visible where they fit. Mounting and dismounting must preserve the user's character, stats, rings and alignment. The user proposed adapting Tails and Eggman's mech stages to these Chao mechs; validate one short segment before expanding to whole stages.

Scale is a first-class requirement. Determine native model bounds and cockpit offsets; fit the pilot relative to the actual cockpit, ground the mech at its foot contact point, and make the third person camera follow the assembled vehicle. Avoid globally enlarging the garden to hide a mismatched import. Distinguish the mech's collision footprint from the seated Chao's visual size. Check garden exits, NPC interaction distance and camera clipping for mounted and unmounted states.

Equipment state belongs to the server. The first owner preview may use admin-only mounting while regular-player acquisition is undecided. Visual mounting can initially share the existing movement controller; it does not establish original mech driving, lock-on, shooting, health, or stage compatibility. Add those as explicit follow-up tasks with server validation.

Reference sources: [SEGA Sonic Channel, Chao Walker discussion](https://sonic.sega.jp/SonicChannel/special/sidestory/comic/20161202_001245/) and [X-Hax character model structures](https://github.com/X-Hax/SA2BModdingGuide/wiki/Character-Model-Structures). Local source hashes and conversion details belong in the asset manifest.

Recent mounted feedback is part of acceptance: pilot and mech must face the same movement direction; motion should look natural at the actual travel speed; the mounted camera should fit the hull and nameplate; gems stay at their approved size on foot and become **five times larger while mounted**. The user also requested firing, jetpack movement and the mech's broader move/tech set. Implement validated core firing/boost first and preserve the remaining native technique catalog as explicit work.

## Character upgrades as Chao extras

Map every story character's special upgrades in [UPGRADES.md](UPGRADES.md), with a machine-readable item catalog, original upgrade bits, worn/field model references, texture sources, original effects and potential Chao uses. Preserve the user's interest in the especially striking upgrades as optional Chao extras rather than losing them in a stage-only asset list.

First explicit owner loadout: **Shadow's Air Shoes** and **Knuckles' Hammer Gloves**, the latter confirmed by the user after “fist bars.” Use their actual left/right wearable models and matching textures. Fit individual shoes/gloves to the Chao's native feet/hands through uniform attachment transforms and verified bone sockets. Keep body proportions and skeleton unchanged, check motion frames and cockpit clearance, and preserve equipment across mounting, evolution and reconnect.

The first loadout is a server-owned cosmetic preview with owner equip/unequip actions. Catalog native abilities separately; do not infer a speed/damage/flight bonus merely from loading a prop. Functional upgrade unlocks, acquisition, balance, incompatible combinations and normal-player use are later server gameplay decisions. Continue to use Chao-native movements wherever available before adapting a character-specific ability animation.

## Complete Chao-system and stage catalog

The user asked to map all ideas and uses so nothing is lost, explicitly including Kindergarten, toys and the Black Market. “Town” was corrected to “mechanics”; do not add a separate town requirement. [MECHANICS.md](MECHANICS.md) holds the system-by-system coverage map and intended MMO use. It must retain unknowns and future uses as well as implemented features.

Catalog the Kindergarten facilities and classroom skills, health/naming interactions, toys/instruments and learned play actions, Black Market stock and inventory/equipment, food/trees, animals/drives, stats/growth, appearance/genetics, breeding/eggs, reincarnation/life cycle, needs/emotions/personality/social behavior, races and karate. Identify original evidence and assets before presenting assumptions as faithful original behavior. The existing four-item Amy shop is a working foundation, not completion of the original Black Market economy.

The all-game stage-select foundation needs a machine-readable catalog and [STAGES.md](STAGES.md): native IDs/names, character/family, resource/model/texture/motion/audio candidates, mission variants and readiness. Keep Sonic/Shadow, Knuckles/Rouge and Tails/Eggman families visible along with relevant Chao/race/karate/other activity entries. Cataloged stages can show information and future uses, but must not pretend imported scenery is already a playable mission. Only implemented routes may enter play; new mission stages remain unavailable until objective, collision, traversal, authority and return paths work.

## Visual and audio acceptance

The user requested the actual original Chao stat-menu appearance as a base, enhanced rather than replaced by a generic card. [STAT-UI.md](STAT-UI.md) will record original panel/icon/sprite sources and layout evidence. Reuse matched local textures where appropriate and rebuild functional browser controls around server-owned stats. Improve clarity of levels, grades, progress/points and hidden-stat details; integrate alignment, gear and growth access without distorting the original visual identity. Asset extraction is not a port of the original native UI code. Verify native artwork, mouse/keyboard navigation and small-screen legibility before completion.

Retain the separate first hub layout the user liked, refine its scale and use native grass, trees and props. Neutral, Hero and Dark gardens remain separate areas reached through correctly labeled exits. The main page uses Sonic blue/gold styling; the owner's ADMIN tag uses red/gold and sits above the head effect.

Chao bodies and wings must remain solid without unintended transparency or artifacts. All Chao need aligned open eyes; special NPCs retain their actual white eye surfaces and pupils. Feet should meet terrain while idle and moving. Decorative hub paths may not bury or float the character. A close following third person camera and camera-relative controls must work in every area, with recenter and zoom.

The owner uses the actual animated Dark Chaos blue flame: upright, black matte removed with the intended blending. The user has confirmed this flame is correct; preserve it while changing other attachments. Do not substitute an angel sphere or a newly invented effect.

Use all seven actual cinematic Chaos Emerald models and their corresponding original colored facet textures. Keep native translucency separate from the opaque Chao rule, use the original camera-normal texture projection, normalize the gems to a consistent aura size, and orbit the active Chao/mech assembly. A flat recolor, missing texture, all-red set, or Security Hall pickup replacement does not satisfy the request.

Music comes from the original hub and corresponding garden tracks, with user-controlled playback. Match Knuckles, Tails and Amy introductions to their own NPCs. The Sound toggle must work after a browser gesture, survive area changes and mute both channels. Extracting sound files is separate from checking their identity and audible browser playback.

## Source repositories and process separation

User-created repositories:

- [ChaoGardenMMOClient](https://github.com/mobdon/ChaoGardenMMOClient): browser renderer/UI, public protocol data, local import tooling, design and task list.
- [ChaoGardenMMOServer](https://github.com/mobdon/ChaoGardenMMOServer): Rust authority, persistence, network adapter, tests and temporary Node reference implementation.

The repositories are independent. Public interaction constants may be copied/generated from a versioned schema, but the client must not import a backend source directory. Record protocol compatibility and check shared values. Server source separation does not itself provide authority: the server must still validate every action and keep credentials and saved data outside client artifacts.

Run frontend and backend as separate processes. The browser receives static client assets and exchanges inputs/snapshots through an API/WebSocket boundary. Local development denies direct access to the server directory, saves, tools, scripts and tests. Only explicitly allowed converted local assets are served by the local asset bridge. Existing profile data must survive runtime changes.

Both repos contain our source and documentation. Installed/extracted models, textures, audio, movies, saves, binary tools and converted game assets remain outside Git under ignored local storage. The user asked for separation; a full new admin platform is future work rather than a prerequisite to the current preview.

## Rust migration and protocol

Keep browser presentation in JavaScript/Babylon while migrating implemented authority to Rust. The initial service uses Axum/Tokio with JSON over HTTP/WebSocket. Maintain the existing session, input, action, snapshot, notice and event contract during the migration, including hashed session tokens and the current profiles. Read terrain exports generated from the same renderer transforms.

Initial cadence remains 20 simulation ticks per second and 10 snapshots per second. Preserve bounded inputs, movement checks, cooldowns, proximity, shop prices, ring accounting, ordered race checkpoints, mission reward idempotency and owner permissions. Use the Node version as a regression reference until the Rust tests, real two-client WebSocket checks and profile compatibility checks pass. Only then change the default preview backend. A future shared Rust/WASM movement module is optional if prediction needs it; a full renderer rewrite is not required now.

### WASM and shards: evaluate in stages

The user suggested shards and WASM as possibilities, not a settled requirement. Treat shards as logical world/room instances first: the authoritative Rust server can initially own several in one process, with worker distribution added after measurements. Preserve parties, character ownership and destination handoffs as players change instances. There is no need to create a separate database per room.

For WebAssembly, first isolate pure movement/state code from sockets, filesystem access, profile authority and the Tokio/Axum runtime. Compile a small shared Rust module for browser prediction through wasm-bindgen and compare identical input sequences against native Rust. JavaScript/Babylon can retain rendering and browser I/O. The server remains authoritative; a downloaded WASM module is editable by the client and does not prevent cheating or conceal logic.

Accept the WASM experiment only if build size, startup, frame time or prediction consistency improves enough to justify its integration. Test browser support, native/WASM numeric differences and reconciliation. Keep privileged progression/economy/account logic on the native service. Rust's browser target does not provide ordinary OS filesystem or thread-spawn behavior, so compiling the entire server unchanged is not the plan. References: [wasm-bindgen guide](https://rustwasm.github.io/docs/wasm-bindgen/) and [Rust browser target support](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html).

## Population target: 100–500 concurrent players

The target is **100–500 connected players across the service**, with measured room limits. It is not yet a verified capacity claim and does not require every player to occupy one rendered garden. A single crowded 500-player room is a separate performance question.

Start with one deployable Rust service owning multiple room simulations. Give each hub/garden/mission/race instance one authority and assign players to it. Use configurable room caps, party transitions and explicit destination/spawn validation. Keep the room design independent of process count so additional workers can be added when profiling justifies it.

1. Connection/session boundary validates identity, protocol version, message size, rate and allowed action shape.
2. Room simulation processes bounded input queues on a fixed tick; no persistence or asset conversion blocks that tick.
3. Interest filtering sends only relevant nearby entities, prioritizing the player and important interactions. Add delta snapshots and sequence/acknowledgement data after baseline correctness.
4. Persistence writes account/Chao ownership, progression, inventory and transactions through a durable store. PostgreSQL is the intended production candidate; the current local JSON save remains development storage until a tested migration exists.
5. A room directory/session coordinator tracks room ownership and reconnect placement. Start in-process; introduce cross-process coordination only when deploying multiple workers.
6. Metrics record tick duration, queue depth, connected players, outbound bytes, slow/disconnected peers, save latency and errors. Slow clients must not stall other rooms.

Bound queues and disconnect or back off overloaded peers. Keep spending/reward operations atomic and idempotent so reconnects and retries cannot duplicate currency or items. Do not broadcast every player to every other player across unrelated rooms. Send static model/audio assets separately from gameplay messages.

Production identity must replace the first-local-profile owner bootstrap. Roles and owner/admin actions are server assigned. The local installation detector is not Steam entitlement verification. Steam account/DLC checks, the local import bridge, public HTTPS/WSS hosting, and distribution setup need a dedicated design before public access.

## Capacity and release gates

| Gate | Required evidence |
| --- | --- |
| Local playable slice | Two independent clients see each other; camera/controls, terrain, exits, shop, race and mission work; reconnect preserves progression. |
| Rust parity | Core regression suite, invalid-input cases, real HTTP/WS two-peer test, save/reload and native asset smoke pass. Keep Node reference until this is proven. |
| 100 connected players | Reproducible scripted clients across rooms; measured tick/snapshot timing, bandwidth, CPU/RAM and save behavior; no forged rewards or cross-room leaks. |
| 250 connected players | Repeat representative movement, races, fights, purchases and transitions; include reconnect churn and slow clients. |
| 500 connected players | Representative load plus a soak run and room/worker restart recovery; publish hardware, room counts, payload sizes and observed limits. |
| Public playtest | Auth/roles, entitlement approach, durable saves/backups, recovery, HTTPS/WSS and an asset distribution workflow are in place and exercised. |

Provisional performance budgets: simulation p95 below 50 ms at a 20 Hz tick, and p95 snapshot age below 250 ms under controlled local load. Treat these as proposed acceptance thresholds to measure and refine, not current results. Test normal and burst input, duplicate/replayed actions, disconnect mid-purchase/reward, save failure, malformed traffic and client/server version mismatch.

## Admin and debugging backlog

Preserve the authorized owner's perfect Dark Chaos Chao and red/gold tag. Future tools include a server-authorized role/equipment panel, room population/tick display, collision/terrain and attachment overlays, motion browser, asset/provenance inspector, network statistics and structured action/error logs. Privileged changes need server permissions and an audit record. Never send secret configuration or session credentials to a public debug panel.

Debug tools should help resolve the user's actual reported problems: movement direction, foot contact, scale, eye materials, flame orientation, emerald texture projection, portal destinations and mech pilot fit. Develop these after the immediate playable fixes rather than blocking the preview on a broad admin system.

## Ordered delivery

The working queue is [TASKS.md](../TASKS.md). First preserve and verify the current preview, complete the Rust transition and source split, and finish the movement/mech asset proof. Then implement movement states and one authoritative race/karate/mission feature at a time. Add production persistence and account ownership before public hosting. Validate 100, then 250, then 500 concurrent players before describing capacity as delivered.
