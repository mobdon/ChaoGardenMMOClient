# Requested game features and verification

This tracks the requests from the initial Chao RPG discussion and the live preview feedback. “Implemented” describes this local development build, not a production MMO.

| Request | Current state |
| --- | --- |
| Browser game played as a Chao | Implemented with original local Chao geometry and native Idle/Walk animation. |
| Steam Sonic Adventure 2 and BATTLE required | Project requirement; installation and resource detection implemented. Steam account/DLC entitlement authentication is not implemented. |
| Normal, Hero, and evil/Dark Chao | Alignment forms and garden gates implemented. Accurate original dynamic body morphing remains separate work. |
| Perfect admin stats, including hidden stats | Server-provisioned owner has five S grades, all seven levels 99, hidden Luck/Intelligence grades 255. |
| Owner is Dark Chaos Chao | Original Dark Chaos geometry; owner profile set to Chaos form with -100 alignment. |
| Red/gold ADMIN name tag | Implemented; size capped and raised above the head effect. |
| Original Dark Chaos head flame | Original 16-frame sprite animation; upright, additive black-matte removal. User confirmed the flame looks correct. |
| Close third person camera following the Chao | Implemented with right-drag orbit, wheel zoom, and R recenter. |
| Fix backwards/no movement | Camera-relative forward/right basis implemented; browser input verified to change server position. Side panels no longer pause movement. |
| Feet meet terrain; consistent model scale | Original models and original gardens share scale 0.1. Server heights and animated mesh grounding implemented. Hub path stones flattened to the same ground surface. |
| Clean main hub based on the first lobby | Separate hub retained, with original grass texture and original special Chao NPCs. Further native props are being added. |
| Neutral, Hero, and Dark garden destinations | Original geometry/textures imported for all three; shared renderer/server terrain transforms. |
| Exits lead to correct areas | Hub portals have explicit destination actions; garden exits and Home return to the hub. |
| Shop | Amy’s ring shop implemented with fixed server prices, proximity, fruit/drives, and a saved eggshell cap. |
| Missions for rings | Authoritative cooperative robot arena implemented. Original Sonic mission stage layouts and enemy behavior have not been ported. |
| Race and compete | Ordered physical checkpoint race and ring rewards implemented; full original race/karate systems are future work. |
| Alignment determines garden access | Server checks +/-60 alignment. |
| Chaos evolution | Prototype progression milestones implemented; owner has an explicit server-authorized form selection. |
| Special Chao as NPCs | Original Knuckles, Tails, and Amy Chao imported and assigned trainer/race/shop roles. |
| Sonic and Shadow Chao appearances | Identified as evolved running forms; mature morph implementation remains future work. |
| NPC eyes open with correct white surfaces | Special NPC white shells restored; original eye textures and open-eye geometry retained. |
| Solid Chao without transparent body holes | All body/wing materials forced opaque in converter and renderer; facial overlays treated separately. |
| Chaos Emerald aura follows owner | Seven genuine cinematic models now orbit the owner, centered and uniformly sized. Native view-normal texture projection is implemented; live visual verification remains. |
| Seven original gem textures | All seven original e0207 texture images extracted losslessly; original colored facets retained. |
| Actual game tree models | Native Neutral tree geometry and bark/leaves textures converted; seven hub trees now use the native model. Live visual verification remains. |
| Music for hub and gardens | Four original tracks converted locally; Sound toggle switches track by zone. |
| Correct voices for Knuckles/Tails/Amy | Original numbered English introductions extracted from the corresponding source archive and paired by NPC. |
| Full game-file inventory including textures | Complete installed root indexed and SHA-256 hashed: 4,351 files, 3,253,731,852 bytes, zero skipped/errors. See INVENTORY.md. |
| Server authoritative; reject cheating | Movement, jumps, terrain, damage, rewards, stats, gates, purchases, and race validation run on the server. Client sends inputs/actions. This is a loopback development service, not a completed public anti-cheat/account platform. |
| Keep the preview available to play | Local background service and a retained browser tab. |
| Keep client/server separate | Independent frontend/backend workers and an API/WebSocket boundary implemented; private and encoded file paths rejected in live checks. |
| Two separate GitHub repositories | User created mobdon/ChaoGardenMMOClient and mobdon/ChaoGardenMMOServer. Source-only initial synchronization is in progress. |
| Rust server and mechanics | Porting our existing implemented game authority to Rust, preserving the protocol and saved profiles. Original compiled engine code is not treated as source; additional original mechanics need reference and reimplementation. |
| Plan for 100–500 concurrent players | Recorded in DESIGN.md as total service population across instances, with 100/250/500 load and recovery gates. Capacity is not yet measured. |
| Admin and debugging tools | Queued with role checks, audit, motion/attachment/terrain/network diagnostics. User clarified the immediate need is source/process separation. |
| Full project scope in a design document | DESIGN.md now covers decisions, visual/audio acceptance, native assets, regular mechanics, motions, mechs, Rust and scaling. |
| Keep a task list so work continues | Root TASKS.md has stable IDs, statuses, acceptance criteria, dependencies and handoff instructions. |
| Use normal SA2 mechanics as well | Movement, rings/damage, enemies, objectives/checkpoints and traversal added to a reference/reimplementation catalog. Original behavior and RPG adaptations are distinguished. |
| Get all movements from Chao Karate and Chao Races | Entire native Chao motion bank is being extracted: initial inspection found 622 indexed states and 279 unique clips. Activity dependencies and actual source mapping will be recorded. |
| Climbing, running, jumping, flying and falling | Each has an explicit animation/state-machine task; swimming, landing, ledges, transitions and other discovered clips are included in the catalog. Only Idle/Walk are currently integrated. |
| Original Chao Walker and Dark Chao Walker | Native mech model/texture/motion sources identified; extraction and first mounted presentation in progress. These are original two-player mechs; adapting normal missions is new work. |
| Owner Dark Chaos as Dark Chao Walker driver | Authorized owner mount/equipment state, actual seated pilot, cockpit fit, foot grounding, appropriate collision scale and following camera in progress. |
| Gems floating around the mounted walker | Move the genuine seven-emerald orbit with the mounted assembly; preserve flame and raised ADMIN tag. |
| Chao and mech scale properly | Validate native bounds, common world scale, cockpit offsets, foot contact, collision footprint and camera in mounted/unmounted forms. |
| Tails/Eggman mech missions using Chao mechs | Retained as a prototype idea pending short route/combat playtest; check route scale, collision, aiming and walker controls. Full original mech movement/shooting rules are not yet implemented. |
| Keep Sonic/Shadow and Knuckles/Rouge stages in mind | Both stage families are in the mission catalog alongside Tails/Eggman mech stages; each needs a Chao route/objective/traversal proof. |
| Mission running scaled by Chao skill | Current prototype already uses Run level in speed. Expanded acceleration/speed and relevant flight/swim/power traversal curves are queued with server validation and low/mid/max skill tests. |
| Use the original Chao movement base | Full native motion library is the foundation; extraction/conversion, authoritative state behavior and animation blending are tracked separately. |
| Chao motions first; then original stage-character movement for gaps | Agreed precedence: native Chao, then retargeted relevant original-character action, then new motion only if still missing. Log actual gaps. |
| Scale appropriately without breaking Chao models | Preserve native rig/rest pose/proportions; uniform parent scaling, deliberate retargeting, root-motion normalization and frame checks for joints/eyes/wings/feet/cockpit clipping. |
| Kindergarten, toys and all Chao mechanics | Full coverage/use map in MECHANICS.md, with original evidence/unknowns and links to implementation tasks. “Town” was explicitly corrected; no town feature is assumed. |
| Black Market | Existing Amy shop is the first foundation. Expanded stock, items, toys, unlocks and economy are catalog/implementation tasks, not already complete. |
| All-game stage select ready and cataloged | Native stage/mission/resource catalog and stage-select foundation in progress. Catalog availability and playability must be displayed separately. |
| Fix pilot facing and natural mech motion | Live feedback recorded; correct wrapper orientation and native motion timing without changing the Chao rig. |
| Gems five times larger only in mech | Explicit mounted-size requirement; preserve approved on-foot gem size. |
| Mech firing, jetpack and all moves/tech | Core authoritative shooting/boost in progress. Full native move/tech inventory and missing behavior remain tracked work. |
| Map each character's special upgrades and cool extras | Full upgrade catalog in progress, preserving original bit/model/texture/effect references and possible Chao uses. |
| Shadow jet shoes on owner | Confirmed as Shadow Air Shoes. Native left/right props with Chao-scale bone attachments and persistent owner equipment in progress. |
| Knuckles fist bars on owner | User confirmed Hammer Gloves. Native left/right gloves will fit actual Chao hands without stretching the rig. Cosmetic attachment and functional abilities are separate tasks. |
| Take original Chao stat menu and enhance it | Native stat/menu sprite research and extraction in progress. Preserve original identity while improving readable stats/grades/points, hidden details, alignment and gear navigation; data remains server-owned. |
| Maybe shards and WASM | Recorded as evaluation tasks: logical room instances first, distribute workers when measured; small pure Rust/WASM prediction experiment before committing to a client rewrite. |
| Tiny Chao Garden on the go and back to desktop | Agreed phone PWA mode using the same server-owned Chao, inspired by the original Chao Transporter. Full system/device/save design in TINY-GARDEN.md; mobile pairing/handoff is not implemented. |
| Sonic Advance, Sonic Advance 2 and Sonic Pinball Party | All three supplied ZIPs are identified/hashed locally with valid headers; Advance 2 matches the reference USA SHA-1 and its extracted garden payload also matches exactly. Version-specific system/asset audit continues. |
| Port supplied GBA games fully to Rust | Recorded as a staged native-system port with separate main-game and garden parity gates. The Rust native reader and reusable bounded cores are implemented/tested; full supplied-game ports remain queued. Compiled ROMs are not Rust source. |
| Original Chao transfer system | Audit native payload/handshake/save ordering; implement a durable Rust mode/device handoff with one identity, profile revision and exclusive active-device lease. No arbitrary client stats imports into the live economy. |
| AFK phone app with training, stats and minigames | Shared stats and five authoritative minigame adaptations are connected. The bounded Rust training core is tested, but Tiny Garden training controls, durable job service and exclusive device/activity handoff remain queued. Offline practice gives no transferable rewards. |
| Bring Tiny Garden Chaos Chao to life | User reports native type codes with shared-looking sprite. Verify type-to-sprite lookup; preserve full desktop form and add distinct Neutral/Hero/Dark Chaos phone presentation where native art is missing. Report remains a hypothesis until traced. |
| Integrate original animal system with stage gameplay | Catalog native IDs, models/textures, stat/trait/behavior/evolution effects; authoritative stage pickup → shared inventory → care/progression. Stage catalog is present; live animal pickups are not yet implemented. |
| Glowing chest gem and all seven floating gems | Fixed native cyan torso gem plus seven native orbiting originals live checked on foot and mounted. Keep original facets/colors/alpha. Hidden emerald bloom must be blocked by the opaque Chao body; the earlier inclusion-only pass caused false see-through color blobs and was corrected. |
| Use Tiny Chao Garden DX as a reference | Creator page/devlogs inspected. Reference its inventory/stats/Windows–Android transfer UX; no reusable source/license linked on inspected pages. It currently describes Match/Janken, not all five; no assumption Search code is provided. |
| All five Tiny Garden minigames | Memory, Rock–Paper–Scissors, Bounce, Ball and Search have initial Rust adaptation cores and tested server integration. All five controls and a 390×844 mobile viewport were checked without overflow; a complete browser Memory round earned 88 saved rings. Full playthrough/touch acceptance for the remaining games and native fidelity are separate work. These are not full ROM ports. See MINIGAMES.md. |
| Server authority for every minigame | Server clock/private state/collision/score/results, bounded intents, profile/round ownership, stale-ID rejection and atomic once-only saved rewards; no client score/result upload. Host authority checks, all five controls/mobile layout and the browser Memory save/reward path pass; remaining full-round/touch acceptance stays open. |
| Find original SA2 transfer machine | Four genuine GBA-version Transporter components and AL_OBJECT texture archive extracted from installed SA2 with unchanged source hashes; conversion/assembly/desktop interaction still queued. |
| Prioritize Tiny Garden and its Chaos versions | Current delivery priority: five playable games and distinct Neutral/Hero/Dark Chaos phone sprites. Three atlases now rendered from native bodies/Idle/Walk; exact original type-to-sprite behavior remains unconfirmed. |
| Tiny on desktop; mobile strictly Tiny | Agreed: desktop includes Tiny alongside the main game; mobile contains only Tiny care/stats/training/items/minigames. Shared canonical Chao/progression; no mobile normal stages, mechs, hub, races or Karate. |

Original assets, extracted outputs, audio, tools, terrain, and saves remain under ignored `.local/`. See `ASSET-PIPELINE.md` for provenance and conversion commands.

Continue with [the task list](../TASKS.md). A completed file inventory does not mean every packed asset is converted; a converted motion does not mean its gameplay state is implemented. Keep those statuses separate when updating this ledger.

## Body, shoes and away-time corrections

| Request | Recorded direction / evidence |
| --- | --- |
| Chaos bodies still look transparent | Reproduce and correct the underlying body/effect ordering; never make the approved flame or original translucent emeralds opaque as a workaround. |
| Shadow skating for Air Shoes | Keep normal Chao walk/run below threshold, blend into genuine source-adapted Shadow skating at higher authoritative speed, scale cadence with speed, preserve Chao proportions/rig. |
| Stop the pigeon-toed stance | Correct the rigid shoe basis using each original heel-to-toe direction, without counterrotating native foot animation. Three-form 1,086-sample proof passes; actual browser stance/movement checked. |
| Make shoes more proportionate | Uniformly shrink the previous fit by 15.1%; preserve foot sockets, original shape, forward alignment and foot contact policy. |
| Put the chest gem farther into the body | Move the gem center another quarter-depth inside actual torso skin, halving its nominal exposed depth. Preserve original cyan facets/glow and all seven orbiting gems. |
| Always use native effects/assets first | Persisted in AGENTS.md/TASKS.md/DESIGN.md. Original Shadow jet meshes 92–99 replace adapted Chao-flame shoe planes; source geometry/material parity is proved, exact emitter draw matrices/timing remain open. |
| Let me mute the music | Existing Sound toggle switched off and verified in the actual live UI; it mutes music and NPC voices. Setting preserved. |
| Fix useful Vibe PR merges only | Repaired and merged server 8/9/11 after green CI; closed 5/6/10/12/19/20 without merging. Final record 50c1f5a; no runtime rollout or save migration. |
| Check on Chao while it runs AI | A custom Rust resident watch/care routine controls the same canonical Chao's idle/wander/rest behavior. Persistent residency and restart-safe restoration are implemented/tested in the live local release; resident UI acceptance is starting. Retail Chao AI fidelity and offline competition are separate work. |
| Others see your Chao in gardens while you are away | Persistent shared-garden residency is implemented with durable opt-in, one canonical Chao and safe restart restoration. Simulation continues while the server runs; no activity or rewards are fabricated for server downtime. Live multi-player presentation acceptance is starting. |
| Compete in AI races/Karate while away | Queued as COMPETE-01. Requires explicit opt-in, authoritative AI entries/matches/placements and durable once-only results/rewards; the resident watch routine does not implement competition. |
| Tiny Garden is where AFK training happens | Agreed location for AFK skill training. The standalone Rust training core is tested, but skill-selection controls, server countdown and durable once-only job integration are not connected yet (TCG-08/16). Mobile remains strictly Tiny Garden. |
