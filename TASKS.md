# Chao Garden MMO — task list

Updated: November 18, 2026. Design: [docs/DESIGN.md](docs/DESIGN.md). Request history: [docs/REQUESTS.md](docs/REQUESTS.md). Full installed-file inventory: [docs/INVENTORY.md](docs/INVENTORY.md).

Status: **DONE** = implemented with recorded evidence; **VERIFY** = implemented but acceptance still needs a live check; **ACTIVE** = being worked on; **TODO** = queued; **IDEA** = a possibility requiring a prototype/decision. Task IDs remain stable across sessions.

Movement rule: original Chao action first; original stage-character movement retargeted only for a missing Chao capability; new motion only for a remaining gap. Preserve the Chao rig/proportions, use uniform asset-parent scale, normalize root motion and check joints/eyes/wings/feet before acceptance. Stage origin does not override this rule.

Asset rule: always search installed native content first. Preserve matching original geometry, images or vertex colors and alpha/material behavior; do not invent or repurpose an effect when its native match exists. Record attachment/timing adaptations separately.

## Continue here

The Vibe feature deliveries through server `282f6ab` remain **UNACCEPTED**. Rust game/shared mechanics are mandatory. Useful Rust build/CI and corrected documentation were separately merged through PRs 8/9/11; the final nine-PR review record is published at server `50c1f5a`. Six incompatible/redundant PRs were closed with source branches retained. The working runtime is unchanged. Root AGENTS.md applies the same requirement to client/platform work.

1. Coordinate through the server repository's docs/coordination/BOARD.md and PROTOCOL.md. Accounts/pairing is active in an isolated worktree; agree the v2 contract and migrate all three clients before merging (NET-03, TCG-06).
2. Connect durable AFK training and its Tiny Garden screen after ownership/lease agreement (TCG-08/16). The tested pure Rust job core is not a running training service.
3. Review the isolated native Android widget delivery and test its real debug APK on a usable device (ANDROID-01). APK/build/JVM and isolated Rust round-trip evidence now exist; launcher, Keystore, paired HTTPS, resize/reboot/Doze and coordinated rollout remain open. Full minigames and AFK controls open Tiny Garden.
4. Finish remaining native motion, multi-client and Tiny game/form acceptance (ANIM-02/07, TCG-05/09/10/15); preserve the recorded Walk → Run → Skate demonstration and all five server games.
5. Keep the playable preview and saves intact while publishing reviewed changes to the independent repositories. Next native-area requests include Kindergarten, Black Market, actual garden exits to the hub and native SA2 stage select with real saved-Chao selection; see SCHOOL-03, MARKET-02 and CATALOG-03.

Test readiness checked November18: existing saved-Chao browser Tiny is `http://127.0.0.1:5173/tiny.html`; its read-only widget preview is `/widget.html`. New original-art previews return200 at `http://127.0.0.1:5177/widget.html?view=phone` and `/tiny.html#garden`. Port5177 has separate browser storage and does not inherit5173 credentials; neither loopback port is phone-reachable. Draft client PRs3/5 remain unmerged. Actual debug APK: `C:/Users/Admin/.codex/worktrees/tiny-widget-native-data/mmo/android/app/build/outputs/apk/debug/app-debug.apk`, SHA-256 `15adadc4aba82d63e4d35a7a058d0ea904c1bd0568a35616f09291ace626a493`. See DESIGN.md for source/test/device distinctions. AFK training remains unavailable in the live service.

## Immediate work

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| DOC-01 | DONE | Preserve recent feature requests, regular SA2 mechanics, movement list, mechs, Rust, source split and population target. | DESIGN.md and REQUESTS.md contain the scope; this queue records completion gates. |
| DOC-02 | TODO | Keep the work record current at every session handoff. | Update task status/evidence and next steps; do not silently drop unfinished requests. |
| WORK-01 | ACTIVE | Original chat owns integration/release across twelve scoped implementation chats. | Copy-ready briefs in docs/WORKSTREAMS.md; server docs/coordination holds the Vibe/repository board, mirrored contract and all briefs. Accounts/pairing and native movement have reported isolated work; implementation remains subject to integration review. Client native Run helper is already published at279a7b7; avoid a duplicate implementation. Other chat activity stays unverified. |
| OPS-01 | DONE | Separate frontend and backend processes and stop direct private-file serving. | Separate workers, proxy smoke and encoded/private path rejection checked; launcher cleanup checked on Windows. |
| OPS-02 | DONE | Connect and populate the two user-created GitHub repos. | Published 91 client files (31646f2) and 60 server files (a8188a3); fetched Git trees and every blob match audited staged source. Independent prior histories preserved; native assets/ROMs/saves/tools excluded. Repos: mobdon/ChaoGardenMMOClient and mobdon/ChaoGardenMMOServer. |
| OPS-03 | DONE | Isolated Rust/GNU toolchain and reproducible local setup. | Native hello-world cargo test/run passed; scripts/setup-rust.ps1, scripts/use-rust.ps1, docs/DEVELOPMENT.md. |
| OPS-04 | VERIFY | Keep a persistent playable preview running. | Latest backend serves all new asset/audio routes; frontend opens and reconnects after restart; preserve owner profile. |
| NET-01 | DONE | Authority for current movement, terrain, actions and economy. | Node reference regression suite: 26 tests passed, including forged inputs, cooldowns, gates, shop/race/reward checks. |
| NET-02 | DONE | Port implemented custom gameplay authority to Rust. | Rust service is the running preview backend. Release build, 70 host tests, strict fmt/Clippy, copied-native HTTP/WS smoke and durable Tiny reward/profile checks passed before switch. Native retail game code is not a completed Rust port. |
| NET-03 | ACTIVE | Document/version client/server wire schema and shared constants. | Accounts/pairing isolated branch in progress. Integration constraints and proposed v2 envelope in docs/PROTOCOL-COORDINATION.md; client/AFK/widget agreement and migration tests required before merge. Both repos must detect incompatible protocol versions. |
| NET-04 | TODO | Support reconnect recovery and duplicate-session behavior deliberately. | Test disconnect/rejoin during race, combat and purchase; no duplicate rewards or lost progression. |
| REVIEW-01 | DONE | Review the received Vibe batches and record the user's Rust implementation requirement. | First four deliveries reviewed across97paths/25findings at5ec51f9; later59-path PR7 reviewed separately with17findings at282f6ab. Server docs-only commits e09cd62/325db09 publish Rust gates, findings and repair tasks. Feature acceptance remains blocked; no Vibe runtime rollout. Later source needs its own review. |
| REVIEW-02 | DONE | Fix all reviewed Vibe PR merges, retaining only usable work. | Server PRs 8/9/11 repaired and merged after exact-head green CI; 5/6/10/12/19/20 closed without merging. Final open inventory contains independent Rust PRs 14–18, outside this cleanup. Exact heads, checks and limits: server docs/coordination/VIBE-MERGES-2026-11-18.md at 50c1f5a. No backend rollout/migration. |

## Visuals, controls and native content

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| VIS-14 | DONE | Stop head bob completely. | Native visible head016 stays fixed relative to the actor through idle, walk, run, skate and stops; mounted pilot pose retained. 26 tests/build and 11,880 actual-native frames pass across three forms; all 21 head/eye meshes steady, other local channels/scales/parents unchanged. docs/HEAD-STABILITY.md. |
| VIS-01 | VERIFY | Close third person follow camera and correct controls in every area. | WASD/arrows follow camera basis; right-drag/wheel/R work; nonmodal panels retain movement; touch pad works. |
| VIS-02 | VERIFY | Correct Chao/garden scale, grounding and collision. | Feet meet hub and all three native terrains at rest/motion; no cliff/void tunneling; path stones do not bury feet. |
| VIS-03 | VERIFY | Opaque bodies and artifact-free Chao. | Inspect owner and all NPCs from multiple angles; only intended face cutouts/translucent effects use alpha. |
| VIS-04 | VERIFY | Open aligned eyes on all Chao; restore special NPC whites. | Native white shells/pupils are visible and centered; no unintended half-closed/lazy eyes. |
| VIS-05 | DONE | Original Dark Chaos blue flame, upright and transparent. | Original 16-frame atlas with correct orientation/blending; user explicitly confirmed it looks correct. Preserve during future edits. |
| VIS-06 | VERIFY | Seven genuine cinematic emerald models/textures in owner aura. | Correct seven colors/facets/translucency with native view-normal projection; equal 0.09-unit height; no flat/all-red replacement; follows Chao or mounted mech. |
| VIS-07 | VERIFY | Clean first-style main hub with original grass and trees. | Native grass and seven native Neutral garden trees load; sensible prop/world scale and clear navigation. |
| VIS-08 | VERIFY | Separate original Neutral/Hero/Dark gardens and correct exits. | Bounded clicks on all three verified native doorway recesses use the existing Rust hub action; artificial return arches hide after successful tagging. Neutral actual walk/click→Rust hub passed; Dark approach/traversal and Hero terrain-domain expansion remain open. Home retained, alignment gates unchanged. See docs/NATIVE-GARDEN-EXITS.md. |
| VIS-09 | VERIFY | Red/gold elevated ADMIN tag and Sonic page styling. | Tag clears head/flame, remains readable, and works mounted; HUD does not obstruct the play view. |
| VIS-11 | DONE | Fix animated footwear, forward toe alignment and inset chest gem. | Actual 006/013 sockets; native alternating Walk/Run preserved. Native heel-to-toe basis removes about 11 degrees inward yaw. Shoes uniformly 15.1% smaller than prior fit; gem inset another quarter-depth into skin. Final 15/15 tests/build and three-form 1,086-sample proof pass; actual UI Walk/Run/Skate recorded. See docs/FOOTWEAR-FIX.md. Broader terrain/contact remains VIS-02/ANIM-07. |
| VIS-12 | VERIFY | Use original Shadow toe/heel exhaust instead of adapted Chao flame sprites. | Actual teriosmdl 92–99 geometry/vertex colors/unlit SrcAlpha blend; original boot bytes retained, jets excluded from fit/contact. Rust bundler, independent SAIO/PBR parity and renderer guards pass. Installed through existing asset routes and actual browser checked; exact native draw matrices/timing remain unresolved. See docs/NATIVE-SHOE-EFFECTS.md. |
| VIS-13 | VERIFY | Exact mirrored shoes, matching title portrait and stable glove contact. | Current source reuses original right model78 and96–99 jets with exact reflected left rest fit, common0.90 scale ratio and native006/013 parents. Three-form vertex/mount proofs pass. Gloves now mount visible palm003/010, with004/011 only distal bind markers; real original-prop2,166-frame proof max drift3.43e-8world. Title source mirror error1.676e-8. Actual front/title silhouettes and equipped gear checked in isolated browser; all-terrain/transitions remain open. docs/FOOTWEAR-FIX.md and MIRRORED-GEAR-FIT.md. |
| AUD-01 | VERIFY | Original hub and three garden music tracks. | Sound toggle starts audio after gesture, changes with zone, mutes and restores correctly; no stale route404s. User-requested mute verified in actual UI (Value 0) and preserved through presentation updates. |
| AUD-02 | VERIFY | Match Knuckles/Tails/Amy native voice introductions. | Listen to each identity and test the correct NPC interaction with replay cooldown. |
| AUD-03 | DONE | Improve Sonic-themed music controls and native theme/base-track choices. | Blue/gold music toggle and custom volume slider; independent NPC voices. Existing native tracks preserved. Actual isolated Rust/browser playback, mute, volume while muted, voice independence, reload persistence and keyboard isolation passed; desktop 1280/1848 layouts checked. See docs/FOOTWEAR-FIX.md. |
| WATCH-01 | DONE | Explicit3D Watch and Take control control. | Actual isolated browser start→server Watch, Take control→manual snapshot, canvas focus and ordinary movement recovery pass. Existing server protocol with pending/error/timeout/reconnect handling; browser owns no AI. Held inputs clear before requests. Broader resident multi-client acceptance remains WATCH tasks. |
| ASSET-01 | DONE | Inventory every installed game file, including texture/archive sources. | 4,351 files, 3,253,731,852 bytes, every file SHA-256 hashed; no skipped/errors. This does not mean every archive is decoded. |
| ASSET-02 | DONE | Original gardens, special Chao, flame, grass, cinematic emeralds and Neutral tree conversions. | Provenance and native source hashes in ignored manifests; conversion scripts and INVENTORY.md document actual outputs. |
| ASSET-03 | TODO | Catalog remaining packed content by stage/system. | Internal model/texture/motion/audio manifests with source mapping and explicit unknowns, not only filename categories. |
| ASSET-04 | TODO | Add more native hub props/Entrance content after traversal approval. | Preserve the hub layout the user liked; verify texture mapping, collision and scale for each addition. |
| ASSET-RUST-01 | TODO | Port asset tooling/catalog for every supplied game to Rust. | Separate SA2+BATTLE, Advance, Advance2 and Pinball Party tracks; bounded readers/converters, all resource categories, exact source hashes, parity tests and explicit unknowns. Full mechanics/game ports remain separate; raw assets stay local. |

## Movement, races and karate

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| ANIM-01 | DONE | Extract the full original Chao motion bank, race and karate dependencies. | 622 indexed states/279 common clips plus19 activity clips; all298 decoded/exported in8GLB libraries with count/name/channel/source-hash checks. See ACTIVITY-MOTIONS.md. Gameplay integration remains separate. |
| ANIM-02 | ACTIVE | Ground locomotion and transitions. | Genuine Run119 loaded at native base rate0.21; bind-relative adaptation preserves native Chaos rigs. Live Walk0.798 → Run1.236 → Skate1.994 units/s recorded in182 real frames, with pace selection and authoritative speed. Source and three-form transform checks pass. Dedicated sprint/turn/stop motion integration remains open. |
| ANIM-03 | TODO | Jump, falling and landing. | Distinct takeoff/air/fall/landing states driven by server vertical state; correct contact on slopes. |
| ANIM-04 | TODO | Climbing and ledges. | Server validates climbable surface, grip, ascent/top-out/drop; native matching clips blend without teleporting. |
| ANIM-05 | TODO | Flying and swimming. | Server state/range/stamina rules plus correct native motions, water/land/air transitions and camera behavior. |
| ANIM-06 | TODO | Activity and life states from the rest of the bank. | Catalog eating/carrying/sitting/sleep/emotions plus actual remaining clips; priority order follows playable requirements. |
| ANIM-07 | VERIFY | Shadow Air Shoes: Chao locomotion below threshold, smooth source-adapted skating above it. | Latest source: raw1.55/1.30 hysteresis; whole-body0.42s eased entry/0.28s exit;0.32s speed filter; continuous172–174 with metadata state8–10 rates0.12/0.15/0.20. Head35%before18°cap, torso24°; arms/feet/native positions/scales unchanged. Eight focused tests and648-frame three-form overlay proof pass. Earlier live clip uses old cadence; new browser head/stroke/gear acceptance pending. See SHADOW-SKATE.md. |
| RACE-01 | DONE | Prototype ordered checkpoint race and rewards. | Server regression checks reject claimed checkpoints/finishes; physical completion awards once. |
| RACE-02 | TODO | Original race movement and obstacle behaviors. | Reference actual motion/state usage; one course demonstrates climb/swim/fly/jump/fall where appropriate. |
| RACE-03 | TODO | Multiplayer race competition. | Server start/countdown/finish/rank, participant isolation, reconnect policy and stat effects are documented/tested. |
| KARATE-01 | TODO | Native karate animation playback library. | Validate actual ready/attack/dodge/hit/recovery/victory clips and variants on the player skeleton. |
| KARATE-02 | TODO | Authoritative Chao Karate match. | Timed attacks, distance, avoidance/recovery, stamina/health, victory/reward and spectator state; no client-claimed hits. |

## Chao RPG, normal missions and mechs

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| RPG-01 | DONE | Hub shop and persistent cap ownership. | Fixed server prices/proximity; fruit/drives/cap transaction and duplicate purchase checks pass. |
| RPG-02 | DONE | Prototype alignment, garden gates and Chaos evolution. | Server gates and milestones tested. Accurate original growth is separate. |
| RPG-03 | DONE | Owner's perfect Dark Chaos form and hidden stats. | Five main S grades, seven level99 stats, hidden Luck/Intelligence255, owner-only authority and saved form. |
| RPG-04 | TODO | Original Chao growth/evolution mechanic catalog and implementation. | Trace food/drives/animals/stat grades/alignment/morph/reincarnation requirements; label original vs adapted rules. |
| RPG-05 | TODO | Mature Sonic-like and Shadow-like Chao appearances. | Validate growth/morph data before claiming the static run families reproduce mature forms. |
| MISSION-01 | DONE | Cooperative robot arena for rings. | Server combat range/cooldown, participant reward once, defeat/return and save checks pass. |
| MISSION-02 | TODO | Choose and adapt one normal SA2 stage segment. | Chao-scale route with native scenery, collision, enemy/objective/checkpoint, clear condition and hub return. |
| MISSION-03 | TODO | Catalog/use selected normal SA2 mechanics. | Ring/damage behavior, enemy actions, movement/traversal, switches/gates, mission objectives and scoring each have reference and testable adaptation. |
| MISSION-04 | TODO | Keep Sonic/Shadow and Knuckles/Rouge stage adaptations in scope. | Reference routes/objectives/assets and prove one short segment per family with Chao-compatible traversal. |
| MISSION-05 | TODO | Skill-scaled mission movement using the native Chao motion foundation. | Server Run speed and relevant Fly/Swim/Power traversal curves tested at low/mid/max skills; safe acceleration/collision limits; animations match actual velocity. |
| MECH-01 | ACTIVE | Extract actual Chao Walker/Dark Chao Walker assets. | Native model + textures/motion sources identified and hashed; separate converted output, no source installation changes. |
| MECH-02 | ACTIVE | Owner's Dark Chaos Chao as Dark Chao Walker pilot. | Actual cockpit pilot/Idle/Walk and saved owner mount implemented. Live feedback: correct facing, natural movement and wider camera framing before acceptance. |
| MECH-03 | ACTIVE | Emerald aura follows mounted mech at five times on-foot gem size. | Keep on-foot gems unchanged; mounted originals5x, orbit the vehicle assembly; flame and ADMIN tag remain properly placed. |
| MECH-04 | ACTIVE | Mech firing and bounded jetpack, plus native move/tech catalog. | Authoritative ranged damage/cooldown, boost energy/duration/landing; client effects and controls. Validate footprint, camera and native movement separately. Full native tech suite remains additional work. |
| MECH-05 | IDEA | Adapt Tails and Eggman mech stages for Chao Walker/Dark Chao Walker missions. | Prove a short route/combat objective first, including walker scale, camera, collision and aiming; decide if these become a core activity after playtesting. |

## Production and 100–500-player milestones

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| SCALE-01 | TODO | Room/instance ownership, capacity and transitions. | Hub/garden/mission/race separation, spawn validation, party/reconnect behavior; initially one service with multiple rooms. |
| SCALE-03 | IDEA | Expand logical rooms into shard/worker deployment when needed. | Measured reason to split; explicit handoff, ownership, reconnect and party consistency; no duplicate character authority. |
| WASM-01 | IDEA | Shared Rust/WASM movement and prediction experiment. | Pure module builds natively and for browser; replay parity, frame/load measurements and reconciliation justify adoption. Keep privileged logic on native server. |
| SCALE-02 | TODO | Interest filtering, bounded queues and snapshot efficiency. | Relevant entities only; slow clients do not stall rooms; measure bytes and tick timing before optimizing. |
| DATA-01 | TODO | Production persistent account/Chao/inventory store. | Versioned migrations, atomic spending/rewards, backup/restore and JSON profile migration tested. |
| AUTH-01 | TODO | Production accounts, owner roles and Steam/DLC ownership approach. | Replace first-local-owner bootstrap; no secrets in client; installation presence is not reported as entitlement proof. |
| ADMIN-01 | TODO | Server-authorized admin and debug tools. | Role checks/audit plus motion/attachment/terrain/network diagnostics; do after current playability work. |
| LOAD-100 | TODO | 100 concurrent clients. | Repeatable representative load across rooms; record hardware, tick p95, snapshot age, bandwidth, CPU/RAM and persistence results. |
| LOAD-250 | TODO | 250 concurrent clients. | Repeat movement/combat/race/shop/transition workload, slow peers and reconnect churn with no reward duplication. |
| LOAD-500 | TODO | 500 concurrent clients and soak/recovery. | Measured load/soak, restart recovery, durability and room limits; publish observed limits before capacity claims. |
| OPS-05 | TODO | Public playtest deployment. | Auth, durable store, backups/recovery, HTTPS/WSS, version compatibility and local/owned asset delivery design exercised. |
| PERF-01 | TODO | Browser loading and bundle optimization. | Profile Babylon imports, asset sizes and memory; preserve visuals and controls; track current large-bundle warning. |

## Whole-game and Chao-system catalog

The user explicitly corrected “town” to **all mechanics**. The hub stays the gathering area; a separate town is not an agreed requirement. Preserve ideas/uses in docs/MECHANICS.md even before implementation.

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| CATALOG-01 | DONE | Whole Chao-system coverage map and intended uses. | MECHANICS.md stable records cover known systems, evidence/unknowns, intended uses, assets/motions, authority and acceptance work. Exhaustive native behavior audit remains open. |
| CATALOG-02 | DONE | All-game stage catalog and stage-select foundation. | 106 mapped stage/activity entries with1,129 installed resource references; searchable family-filtered browser archive, existing playable routes separate; Shadow search and Dark Garden selection checked live. |
| CATALOG-03 | TODO | Actual native SA2 stage-select presentation with saved-Chao selection. | Inventory/reuse matching original menu assets/navigation; select a real authorized canonical Chao with its form/stats/gear. NPC art alone is not selection. Keep unavailable stages honest and validate destinations, spawn and hub return. Existing archive remains the foundation. |
| SCHOOL-01 | TODO | Chao Kindergarten facilities and interaction design. | Catalog original facilities/classroom/health/naming/shop links and design Chao-playable interactions; validate sources rather than inventing original behavior. |
| SCHOOL-02 | TODO | Classroom skills and learned actions. | Native learning/skill/motion references, lesson progression and server-owned unlocks; playable action presentations. |
| SCHOOL-03 | ACTIVE | Original Kindergarten area and its real hub connection. | All nine native landtables inspected; 365 room textures decoded through existing Rust tools; actual door/NPC/motion sources found. Source/bounds/material/collision audit in docs/NATIVE-KINDERGARTEN.md. Exterior → Commons → Black Market is the next slice; Rust GC visuals, transitions/spawn/collision and real authority/routes remain integration gates. Geometry does not implement lessons. |
| TOYS-01 | TODO | Toys, instruments and garden play behaviors. | Inventory actual items, unlock/acquisition, animation/prop attachment and intended social/solo uses. |
| MARKET-01 | TODO | Expand current Amy shop into the planned Black Market system. | Actual item groups/catalog provenance, server stock/prices/unlocks, owned inventory, equip/use and persistence. |
| MARKET-02 | ACTIVE | Original Black Market space and correct garden/hub exits. | Exact native room, 45 original textures, Merchant/HalfFish and animated entrance mapped; docs/NATIVE-KINDERGARTEN.md. Preserve original additive/alpha materials; native menu-room collision behavior remains untraced. A generic shop panel/portal is not this area delivery; economy remains MARKET-01. |
| LIFE-01 | TODO | Food, fruit trees, animals/drives, breeding, genetics and life cycle. | Reference/index each sub-system, label original versus adapted behavior, link implementation tasks and preserve unknowns for later investigation. |
| LIFE-02 | TODO | Needs, emotions, personality and social behaviors. | Reference state/motion usage; server rules, observable interactions and multiplayer-safe progression. |
| GEAR-01 | ACTIVE | Map every character's special upgrades and potential Chao extras. | Native upgrade bits, wearable/field models, textures and actual effects distinguished from MMO proposals; keep striking upgrade ideas cataloged. |
| GEAR-02 | ACTIVE | Equip owner's native Shadow Air Shoes and Knuckles Hammer Gloves. | Four correctly textured/scaled left/right props on actual animated hand/foot sockets, no rig distortion; owner-only persistent equip slots; browser on-foot/cockpit checks. |
| GEAR-03 | TODO | Choose functional upgrade abilities/acquisition for normal Chao. | Original mechanic evidence, Chao-native action first, server-owned unlock/effects and balance; cosmetic presence alone grants no ability. |
| UI-01 | ACTIVE | Port and enhance original Chao stat-menu presentation. | Identify/extract actual native panel/icons/sprites; rebuild functional browser menu preserving source identity, improve legibility/grades/points/hidden details, alignment and gear access; live comparison and responsive check. |

## Tiny Garden, phone companion and native progression

Detailed decisions/evidence: [docs/TINY-GARDEN.md](docs/TINY-GARDEN.md). A full GBA game port is separate from a Tiny Garden subsystem port. Keep statuses tied to runnable behavior.

| ID | Status | Work | Acceptance / evidence |
| --- | --- | --- | --- |
| TCG-01 | DONE | Identify and protect all three supplied GBA archives. | Valid headers: ASOE Advance, 8 MiB; A2NE Advance 2, 16 MiB; A3VE Pinball Party, 8 MiB. Hashes in ignored .local/gba/inventory.json; Advance 2 and its decoded native garden match reference SHA-1 targets. Bounded inventory script; attachments/ROMs/saves ignored and private-path test passes. |
| TCG-02 | DONE | Rust native GBA/Tiny Garden reader and reusable core. | Header, bounded LZ77, native checksum/sector selection and unknown-byte preservation; 21 tests plus real local-source integration pass, strict Clippy. Standalone core is separate from production integration. |
| TCG-03 | TODO | Complete per-game native Tiny Garden system/asset catalog. | Advance2/Pinball differences, care/shop/food/egg/toy/motion/UI/audio/minigame/transfer tables with source offsets, hashes and unresolved fields; no assumptions that versions are identical. |
| TCG-04 | TODO | Reimplement native care/progression in Rust. | Verified native-versus-adapted rules, item transactions, stats/needs/behavior and persistence; no client-supplied outcomes. |
| TCG-05 | VERIFY | Phone stats/garden presentation. | Current5173 has canonical stats/five games/Watch. Draft PR5f4f2f05 adds Garden/Stats/Training/Games/Items and touch/reconnect fixes;86 source/browser/PWA cases pass. New5177 original-art preview includes that delivery but has separate credentials. Real isolated Rust Memory82rings/one receipt survives restart. Physical-phone and four other complete earned/failure rounds remain. |
| TCG-06 | ACTIVE | Authenticated device pairing and exclusive activity handoff. | Draft isolated Rust PR15at56af22d implements pairing/revocable credentials/revisions/leases; Tiny/widget adapters have isolated real-service evidence. Coordinated desktop migration, combined care semantics and paired-phone acceptance remain unmerged/unshipped. Current live legacy WS policy is not sufficient. |
| TCG-07 | DONE | Pure Rust bounded server-timed AFK training core. | One job per Chao, server start/end, eight-hour bound, rules version and once-only settle/cancel; restart-state tests pass. Exact tuning labeled MMO adaptation. Durable production wiring remains TCG-08. |
| TCG-08 | TODO | Integrate/persist phone training jobs in authoritative service. | Atomic cost/result/job/revision updates; survival acrossrestart, no clientclock influence, repeat claims safe; conflicting desktop activity blocked by lease. DependsTCG-06/07. |
| TCG-09 | ACTIVE | All five authoritative Tiny games, then native fidelity. | Memory, RPS, Bounce, Ball and Search initial Rust adaptations integrated; 22 core tests plus durable result/receipt isolation host tests. Complete browser Memory7/7 earned88 saved rings. Exact original physics/art/constants and Search original payload remain open; do not claim full ROM ports. |
| TCG-10 | VERIFY | Distinct Neutral/Hero/Dark Chaos phone forms. | Three eight-direction animated atlases rendered from actual native Chaos bodies and Idle/Walk motions, original40-bone rigs retained; Dark flame live checked. GBA type-to-shared-sprite hypothesis remains unverified, full three-form live QA remains. |
| TCG-11 | VERIFY | Install/offline/cache/resume PWA behavior. | Draft PR5 has tested production Tiny-only public shell/approved-art cache and view-only last-synced stats; no API/credential caching or offline mutation queue. Four production PWA cases pass in isolation. Physical-phone install/network/resume acceptance and rollout remain open. |
| TCG-12 | TODO | Native save compatibility research isolated from MMO progression. | Checksum/sector/field roundtrips against own saves; preserve unknownbytes; native imports local/practice only, never accepted as trusted live stats/inventory. |
| TCG-13 | TODO | Plan and execute full supplied-game Rust ports in stages. | Independent Advance/Advance 2/Pinball Party catalogs and playable subsystem/stage/pinball/menu/audio/save parity gates; don't claim completion from the Tiny Garden decoder. Preserve full user scope. |
| TCG-14 | TODO | Assemble the original Chao Transporter as the desktop Tiny Garden entrance. | All four actual native GBA machine parts/textures extracted with hashes; convert/assemble and validate scale/collision/interaction. Desktop entry must use durable save/handoff. |
| TCG-15 | VERIFY | Check on the same Chao while a server-owned resident garden routine runs. | Resident opt-in/position persists across last disconnect and safe restart; Watch, pet/rest acknowledgment and take-control live checked. Host tests cover stale sessions and reward isolation. Broader shared-garden/device QA remains. No passive currency/stat rewards or fabricated downtime activity; this is not a retail Chao AI port. |
| TCG-16 | TODO | AFK training screen and durable job lifecycle in Tiny Garden. | Draft Tiny UI contains a disabled Training screen/proposed contract. Live Rust advertises no durable job capability: no invented options/timers/rewards. Integrate actual server jobs, costs/caps/settlement and lease agreement through TCG-06/08; standalone ledger tests do not complete this task. |
| ANDROID-01 | ACTIVE | Native Android Tiny Garden home-screen widget and actual device acceptance. | Draft PR3cf3b236 has a real debug APK, build/lint/44 JVM checks and Java-adapter pairing/care/restart against isolated Rust. Hash/path above verified locally. Compact/wide/large RemoteViews and original-art browser preview exist. No usable device/emulator proves installation, Keystore, launcher placement, resize/reboot/Doze or paired HTTPS; phone reachability/auth rollout and AFK service remain open. |
| COMPETE-01 | TODO | Chao competes in AI races/Karate while owner is away. | Explicit opt-in, stat-scaled server AI, eligibility/scheduling/budgets, authoritative matches/placements, durable once-only results/rewards, spectator/replay. Keep AFK training in Tiny Garden; never reuse passive watch movement to trigger player race rewards. |
| ANIMAL-01 | TODO | Native animal/drive ID and effect catalog. | Every ID/model/texture/stageobject plus verifiedstatdeltas/traits/learnedactions/evolutionlinks; unknown fields explicit. |
| ANIMAL-02 | TODO | Authoritative stage animal/drive pickups and shared inventory. | Server spawn/reach/claim/ownership; one pickup awarded once; save/reconnect/duplicateclaim tests; actual adapted stage path. |
| ANIMAL-03 | TODO | Shared desktop/phone animal care and growth effects. | Atomic iteminteraction/stat/behavior/appearance effects; originalreuse versus MMOconsumption explicitlychosen; no unlimited idle farming. |
| VIS-10 | DONE | Fixed genuine cyan chest emerald and subtle glow on all gems. | Separate native torso attachment plus all seven orbiting originals retained; native facets/colors/alpha preserved, mounted/front and on-foot live checked. Hidden gems now blocked by opaque bodies in bloom pass. |
| UI-02 | DONE | Native-asset title screen, local Chao slots and save/return flow. | Legacy owner credential retained; new Chao preserves prior slots; continue uses resume-only validation; Save & title and Tiny entry wait for durable server acknowledgement. Slot tests and live owner134-ring resume/return passed. |
| BRAND-01 | DONE | Native Discord icon and title portrait. | Native Dark Chaos/flame/seven emeralds/gear rendered locally, original source hashes in brand-render-report.json; reproducible render-brand-assets.py, actual PNGs visually checked. |
| WEB-01 | DONE | Website address and deployment layout plan. | docs/WEB-PRESENCE.md records proposed main/play/tiny/api addresses and dated registry check; no purchase or public deployment. |

## Handoff checklist

- Update tasks with actual file/test/preview evidence. Leave failed or unverified items open.
- Keep the design and request ledger consistent with new user steering.
- Preserve profiles and original assets; leave a working preview available when possible.
- Keep source repos independent and local content excluded from commits.
- Continue the next unblocked item; ask the user only when a missing decision actually prevents useful work.
