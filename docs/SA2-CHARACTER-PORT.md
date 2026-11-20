# SA2 character controller port — November 19, 2026

The opt-in local roster preview uses the Steam PC executable and each installed character's own model and motion assets. Source references: PhysicsArray and the per-character AnimationInfo lists in SA2Variables.h; PhysicsData layout in SA2Structs.h; the PC_SA2/MDL motion mappings used by SA Tools. The Rust sa2-roster reader maps PE virtual addresses to raw file offsets and extracts the tables into a private TSV. The character models, motion libraries, executable and extracted tables stay outside Git.

The separate Rust service reads CHAO_ROSTER_TABLE once at startup. It accepts a bounded dev-actor selection from the authenticated owner. Sixteen distinct character physics rows serve seventeen selections (Black Metal uses Metal). Browser adapters send directions and action intents. Auto sends full analog magnitude without adding a Chao acceleration ramp. The server applies ground/air acceleration, neutral deceleration, reversal braking, speed limits, jump impulse and weight in garden coordinates. At the garden scale Shadow reaches 18 units/s after one second of flat full input; Metal reaches it in a quarter second. These are controller test results, not retail gameplay measurements. Vector steering, garden terrain, collision, melee damage, and the walker cannon are MMO adaptations.

The local exporter reads character motion slots and playback rates, and resolves shared-motion aliases. Each selection has six ground slots plus jump, fall, and a primary attack presentation; slots can share an original motion. Super forms use their flight animations. Walker gait slots use walker-compatible actions. A playing stride carries its phase into the next moving gait. The Chao shoe retarget separately locks the underlying Run119 pivots to Shadow's skate phase; original rig parenting and the accepted torso-height correction remain intact.

Verification: verify-roster-service.mjs uses an actual Rust HTTP/WebSocket server and disposable users, exercises all 17 and rejects an unauthorized visitor. verify-roster-rigs.mjs exercises all 17 rendered rigs in Babylon NullEngine (including return from attack). verify-metal-hover.mjs and verify-steady-head.mjs guard eye clearance, neck parenting, gait, phase and torso height. None compares engine behavior frame-for-frame with a running retail game.

Playable character-tech pass (isolated preview, November 19):

| Family | Authoritative behavior and controls |
| --- | --- |
| Sonic, Shadow, Metal and Black Metal | Airborne F selects a living robot in the forward cone, moves toward it, and rebounds on confirmed contact. Q near the mission rail attaches in the facing direction; Space releases. |
| Knuckles, Rouge, Tikal, Chaos Zero | Hold Q while airborne to glide, or facing a suitable wall to climb. G starts a timed dig on diggable ground. The outpost's marked soil contains one shared buried ring per mission. |
| Tails, Eggman and both Chao Walkers, in their walkers | Hold Q to acquire up to five distinct visible-facing mission targets over time. Release to launch a staggered salvo. Rust advances missiles toward live targets; hit or expiry is decided in the service. F retains the cannon. |
| Super Sonic and Super Shadow | Q ascends; releasing Q maintains altitude; Space descends. While flying the server spends one ring per second; no rings ends powered flight. |

The Rust PC reader's `--sky-rails` mode follows STG06's path list (RVA D7E4E8), checks each original RailPath callback (004980C0), and exports 18 paths, 822 points. CHAO_SKY_RAILS supplies the private output. The mission transplants path 14 at 0.06 scale. This is one training rail, not the Sky Rail stage. The server broadcasts its polyline; the client never supplies a rail coordinate or target ID.

CHAO_NATIVE_LAND points to the private extracted pc-chao/Chao folder. Rust reads the static BASIC collision section of all three SA2BLVL v3 garden files, including transforms, triangle strips and surface flags. Loaded: neutral 671, hero 486, dark 520 nondegenerate solid triangles. Roster movement tests walls and overhead obstructions, refines the existing garden floor height against nearby native triangles and respects diggable (0x20) and unclimbable (0x80) surfaces. Mission buildings use the courtyard's own geometry. Ordinary Chao still use the accepted ground behavior.

Validation: server/tests/character_tech.rs exercises actual authoritative moves, health events, climbing, missile impact, one-time dig pickup and flight ring cost. Set CHAO_ROSTER_TABLE and CHAO_SKY_RAILS and run the ignored local-data test. verify-roster-service.mjs checks a real owner/visitor socket pair and an authoritative flight/ring deduction. verify-roster-rigs.mjs loads each original rig and exercises its available special clips. These checks are not retail input replays.

Remaining retail-parity work:

- Capture reference input sequences and trajectories: start, release, reverse, turn, jump press/hold/release, slope changes and wall contact. Compare Rust positions, velocity, action IDs, and animation frame per retail frame.
- Speed characters: compare homing and grind timing to retail; port spin/roll variants, rail switching/tricks, light dash and multiplayer abilities.
- Hunters: compare glide, climb and dig traces; port original treasure placement/radar, wall dig and attack chains. On-foot Tails/Eggman and the other multiplayer characters require distinct action availability checks.
- Walkers: compare reticle, missile and weapon timing; port original stage-specific targeting and hover. Super forms: verify flight, ring and hit behavior against Finalhazard; port boost and boss interactions.
- Reproduce full character collision volumes, rotating gravity/loops, moving platforms, stage objects and character-specific landing contacts. Verify each family against recorded native behavior before marking it ported.
- Keep save-changing rewards under Rust authority. Do not promote a local preview into the existing owner service without separate rollout review.
