# SA2 Battle Rust port: phased plan (November 20, 2026)

This is the working order for the rest of the port. It is ordered toward one
goal first: **a level a person can play on mob's website, with the stage's
native character**. Later phases widen that to every character, stage, boss,
kart and Chao competition. Playing levels as a Chao comes after the whole game
is ported (mob, November 20, 2026).

[FULL-GAME-PORT.md](FULL-GAME-PORT.md) stays the acceptance ledger and
[issue #11](https://github.com/mobdon/ChaoGardenMMOClient/issues/11) stays the
umbrella tracker. Each phase below has its own issue under #11.

## Where we start

From the audit and recount of November 20, 2026 (both repos, all 476 PRs, every
branch, main at e5cbd92; full report in the project's
`reports/stage-port-audit-2026-11-20.md`):

- **Nothing is playable yet.** The engine (frame step, input, stage load,
  camera, renderer) is the blocker.
- **Level data is done for every stage.** Terrain (102 landtables in 79 files),
  all 232 SET files (58,926 of 59,020 placements matched to the native object
  table), start and end positions, paths, death zones, lights, fog and story
  order are read natively and checked against SA Tools. About 85% of the
  game's data file types are read natively.
- **Stage code is the gap.** Ten stages and bosses have their own native code
  (Iron Gate, Prison Lane, City Escape, Wild Canyon, Tails vs Eggman 1, Sonic vs
  Shadow 2, Big Foot, Hot Shot, Finalhazard, kart). The other stages have only
  the shared objects (rows 0–23) and their data. About 5–10% of the 634 native
  story-SET tasks are ported.
- **Port percentage, by native code:**

  | Measure | Ported | Share |
  | --- | ---: | ---: |
  | main.dol functions | 284–364 of 4,713 | 6–8% |
  | main.dol code bytes | of 1,499,216 | 6.6–8.6% |
  | Stage, boss and kart REL bytes, each routine once | 241,588 of 22,936,360 | 1.1% |
  | Same, counting each REL's copy of the shared objects | | 19–27% |
  | Chao REL bytes (server) | 62,160 of 735,788 | about 8% |
  | Data file types read natively | | about 85% |

  Code alone is about **5%**. Weighting code 80% and data 20% gives about
  **21%**. The earlier 20% and 28% figures were weighted feature estimates that
  overstated started-but-small areas.
- **Unmerged native work:** open #455 (Finalhazard core actions), open #456
  (walker frame and land probes), branch `codex/sa2-sonic-ground-movement`
  (path following, slope and smoothed turns; no PR yet), and server branch
  `codex/tiny-chao-shop` (Tiny Chao Garden minigames and garden systems; no PR
  yet). No merged or closed PR lost code on main.

## Rules every phase follows

1. **Native porting only.** Every routine names its main.dol or REL address and
   is checked against the PowerPC under the Unicorn harness (`tests/dol_ppc.py`).
   No invented timers, odds, positions or rules. A rule with no native source is
   left out and fails closed, and the doc says so.
2. **Code reference: the GameCube disc** (GSNE8P, mob's own copy; main.dol SHA-1
   `e4c50f32242a3cd36cdc8ae776661601a52130a2`, matching the roeming/SA2B
   reconstruction's `config/GSNE8P/symbols.txt`).
3. **Data base: the player's Steam install.** Every reader and every runtime
   path loads from the Steam files. Data tables may be read from the player's own
   `sonic2app.exe` / `Data_DLL.dll` (`src/pe.rs`), read-only. The encrypted
   Steam code is never decrypted, disassembled or circumvented.
4. **Never launch the game.**
5. **Nothing from the game goes into git**: no disc files, Steam files,
   converted assets, saves or Gekko estimate tables. Those stay under `.local`.
   Without the Gekko tables the code uses exact sqrt/reciprocal and bit-exact
   tests skip.
6. **One owner per routine.** Before porting a routine, check the ownership
   table below. Shared helpers have one canonical copy (for example angle helpers
   live in `player_turn.rs`; duplicates in `walker_collide` are dropped).
7. **MMO extras stay separate.** Anything that is not native (accounts, other
   players, AFK, phone) lives in its own module whose defaults do not change
   native results.

## Ownership (current threads)

| Area | Owner thread | Open PR |
| --- | --- | --- |
| Shared ground movement fn_8004C734 tree, `player_turn.rs`, Sonic action machine | Build Sonic speed-stage controller | — |
| Walker exec 0x800B3A8C (merged #435), walker_move, `msl_math`, land collision, shared player motion step fn_8003D8D0 and its tree, land probes fn_80067600 / fn_8006779C, Iron Gate session | Make first story level playable | #456 |
| Prison Lane session (Tails walker) | Make Prison Lane playable | — |
| `enemy_lib` (bullets and beams next) | Port native enemy behaviour | — |
| Generic stage objects (rows 0–23), itemBoxManager, Tobitiri, the 78 shared routines found in Boss_Last2D | Port shared stage objects | — |
| Finalhazard's 62 unique routines (Boss_Last2D.rel; setup merged #436) | Verify Finalhazard camera switch path | #455 |
| Steam data loaders, GC↔Steam file map | Load Steam game files natively | — |
| Native Chao Race and Karate (replacing `competition_sim.rs`) | Port Chao Race and Karate natively | — |
| Engine runtime: task system (E1 merged #454), master loop, stage load/unload, input, camera, Ninja/GX renderer, web build ([ENGINE-CORE-PLAN.md](ENGINE-CORE-PLAN.md)) | Port the engine core and renderer | — |
| Knuckles, Rouge, Shadow, Amy, Metal Sonic, Tikal, Chaos (workstream C below) | Port the other playable characters | — |
| GSNE8P address moves (workstream A), remaining stage code (workstream B), remaining bosses, menus, saves, events | **Unowned** | — |

## New workstreams (November 20, 2026)

### A. Move the Steam-addressed stages to GSNE8P

Six stages and bosses record addresses from the Steam copies of their RELs (or
the PC exe) instead of the GC disc. Each needs its routines re-pointed at the
GSNE8P REL and re-checked under Unicorn, as #452 did for Sonic vs Shadow 2
(stg42D).

| Stage | Module | Addresses now |
| --- | --- | --- |
| City Escape | stg13D.rel | Steam REL and PC exe |
| Wild Canyon | stg16D.rel | Steam REL offsets |
| Tails vs Eggman 1 | stg20D.rel | Steam REL |
| Big Foot | boss_bigfootD.rel | Steam REL |
| Hot Shot | boss_hotshotD.rel | Steam REL |
| Finalhazard | Boss_Last2D.rel | Steam REL and PC exe |
| Kart (Route 101 / 280) | CartD.rel | Mixed |

Depends on ENGINE-CORE-PLAN E1 item 7 (the Steam-REL alias table).
**Acceptance:** every routine in these modules names a GSNE8P address and its
Unicorn test runs against the GC REL.

### B. Stage code for the remaining stages

Every stage's data loads; these have none of their own code ported beyond the
shared objects: White Jungle, Pumpkin Hill, Sky Rail, Aquatic Mine, Security
Hall, Metal Harbor, Weapons Bed, Radical Highway, Mission Street, Dry Lagoon,
Sonic vs Shadow 1, Sand Ocean, Crazy Gadget, Hidden Base, Eternal Engine, Death
Chamber, Egg Quarters, Lost Colony, Pyramid Cave, Tails vs Eggman 2, Final Rush,
Green Hill, Meteor Herd, Knuckles vs Rouge, Cannon's Core (stg34D–stg38D),
Final Chase, Cosmic Wall, Mad Space, Flying Dog, King Boom Boo, both Egg Golems
and Biolizard. Green Forest has only the shared ring and spring checks.
Per-stage task lists come from `sa2-assets fan-tasks`. Order follows Phase 5
(mech first). **Acceptance:** each stage's native tasks from its SET files are
ported with Unicorn tests, and the stage runs headless in `sa2-runtime`.

### C. The unported playable characters

Knuckles, Rouge, Tikal and Chaos (treasure hunt), Shadow, Amy and Metal Sonic
(speed variants). Start from a map of each character's loader and exec in
main.dol (`CHARACTER-PORT-MAP.md`), then port the hunter tree, then each
character's deltas, reusing the shared ground movement and walker motion code.
**Owner:** Port the other playable characters.

## Known entry points (main.dol, GSNE8P)

Already identified in this crate. Anything marked "trace" in the phases below is
not yet located; the first task of that phase is to find it in
`symbols.txt` plus the main.dol call graph and add it here.

| Address | What it is | Source |
| --- | --- | --- |
| 0x8000325C | `__start`; sets r2 = 0x803B6520, r13 = 0x803B52C0 | WALKER-PORT-MAP.md |
| 0x80071E0C | `CreateElementalTask` (recorded in the crate as 0x80071BA4, the Steam-REL build address) | ENGINE-MAP.md |
| 0x800709F8 / 0x80071AE0 | `CreateChildTask` / `FreeTask` (Steam-REL 0x80070790 / 0x80071878) | ENGINE-MAP.md |
| 0x80071AF0 | `DestroyTask` (Steam-REL 0x80071888) | ENGINE-MAP.md |
| 0x800706B0–0x80072080 | Task code: lists `btp` 0x801FCEB8, exec pass 0x80071214, display pass 0x80070C54 | ENGINE-MAP.md |
| 0x80065E54 | Instruction in the SET spawner fn_80065C40 (sign-extends rotations into the task) | ENEMY-LIB.md, ENGINE-MAP.md |
| 0x80065120 / 0x80065404 | SET read / SET entry exec gate (flag 0x20) | shared stage objects |
| 0x8000F8B8 | `CCL_Init` (per-task collision setup; the crate's `InitCollision`) | stage objects |
| 0x800232A4 | Dynamic land-object register | stage objects |
| 0x8001A13C | Stage light load | stage lighting |
| 0x8001DD78 | Current stage number getter | tests |
| 0x801CC168 | Game state block G (+0x23 mission, +0x37 stops the frame counter; recorded as 0x801CBD88 in Steam-REL code) | goal ring, spring, ENGINE-MAP.md |
| 0x801CC1E4 | Frame counter | player_analog |
| 0x801E53F8 | Per-player stick table (8 bytes per player) | player_analog |
| 0x801E5624 | Gravity vector | player_analog |
| 0x801E4FF8 | Steam-REL address of the stick table 0x801E53F8 (+0x400), **not a camera block** | ENGINE-MAP.md |
| 0x801E7768 / 0x801E7788 / 0x801E7368 | `playertwp` / `playerpwp` / player tables | player code |
| 0x80196578 | PhysicsArray (0x84-byte rows; ChaoWalker and DarkChaoWalker rows included) | PLAYER-PHYSICS.md |
| 0x8004F838 | Shared player-work setup, called by all 14 character loaders | WALKER-PORT-MAP.md |
| 0x8004E560 | `GetAnalog` (ported, bit-exact) | SONIC-MOVEMENT-NATIVE.md |
| 0x8004C734 | Shared ground movement (0xF9C bytes, 7 character mains) | SONIC-MOVEMENT-NATIVE.md |
| 0x80083E44 / 0x8007E89C | Sonic main / action state machine | SONIC-MOVEMENT-NATIVE.md |
| 0x80086BA4, 0x80086E3C, 0x800871C4, 0x800874C0 | Sonic-family loaders | SONIC-MOVEMENT-NATIVE.md |
| 0x800B5D2C / 0x800B5AC0 | Mech Eggman (row 7) / Mech Tails (row 6) loaders | WALKER-PORT-MAP.md |
| 0x800B56E8 / 0x800B58D4 | Alternate walker loaders, rows 0xE / 0xD, both install 0x800B3A8C | WALKER-PORT-MAP.md |
| 0x800B3A8C / 0x800AF558 | Walker exec / display | WALKER-PORT-MAP.md |
| 0x8010DC88 | MSL `rand` | enemy_lib |
| 0x80119C74, 0x801181A0, 0x802D7758 | Ninja matrix stack, sine-table builder, njSinCosTbl | ninja_matrix, ninja_trig |

## The phases

Each phase lists what it delivers, the entry points, what it depends on and who
owns it. A phase is done only when its acceptance line is met with tests that
compare against the native code.

### Phase 0: Plan hygiene and the Steam base (now, ongoing)

- Keep this plan, issue #11 and the phase issues current; one PR per routine group.
- **Steam base** (owner: Load Steam game files natively): every reader takes the
  Steam install path. Publish the GC↔Steam file map (2,524 of 3,089 Steam files
  are byte-identical to GC) and, for the other 565, which reader handles the
  Steam form. GC REL code stays the code reference; landtables, SETs, models,
  textures and sound come from Steam files at run time.
- Fix stale lines in FULL-GAME-PORT.md (it still says walker movement is
  encrypted main-program code; it is now ported from main.dol).
- **Depends on:** nothing. **Acceptance:** each later phase can name the Steam
  file every runtime input comes from.

### Phase 1: Engine core (headless): task loop, stage load, input

The single biggest gap. Today each routine runs alone from a JSON request.

- **Task system:** `CreateElementalTask` 0x80071E0C (called by loaders, e.g.
  `CreateElementalTask(7, 1, 0x800B3A8C)`), `CreateChildTask` 0x800709F8,
  `FreeTask` 0x80071AE0, `DestroyTask` 0x80071AF0, exec pass 0x80071214,
  display pass 0x80070C54 (ENGINE-MAP.md). The crate's recorded 0x80071BA4,
  0x80070790, 0x80071878, 0x80071888 are the same functions in the Steam-REL
  build (+0x268).
- **Frame step:** native 60 Hz tick (in-stage frame 0x8001E45C), frame counter
  0x801CC1E4, game state 0x801CC168 (0x801CBD88 in the Steam-REL build).
- **Stage load sequence:** stage number 0x8001DD78, SET loader 0x80065E54 and
  exec gate 0x80065404, `InitCollision` 0x8000F8B8, land registration
  0x800232A4, lights 0x8001A13C, start positions, death zones, paths (all
  already readable from Steam files).
- **Input:** fill the per-player stick table 0x801E53F8 and button words
  natively (PAD read and clamp path: trace); browser keyboard and gamepad map
  only into those native words.
- **Character loading:** the 14 loaders that call 0x8004F838 (trace the full
  list from its callers).
- **Deliverable:** a `sa2-runtime` crate that mounts a stage from Steam files,
  spawns its SET tasks and player, and steps ported routines together,
  deterministic and headless, with a replay test (inputs in, per-tick state out).
- **Depends on:** Phase 0. **Owner:** Port the engine core and renderer
  (issue #440). Step-by-step milestones E0–E8, including the camera,
  renderer and web build for Phase 2: [ENGINE-CORE-PLAN.md](ENGINE-CORE-PLAN.md).
- **Acceptance:** Iron Gate mounts and ticks 600 frames with the walker exec
  and its stage objects running inside the task loop; same inputs give the same
  trace every run.

### Phase 2: First playable level on the website

**Target (decided by mob): Iron Gate, played with its native character (Mech
Eggman), loaded exactly as the one-player story does** (loader 0x800B5D2C, row 7,
G+0x1E = 0). Playing as a Chao comes after the rest of the game is ported. The
Chao Walker trace is kept for then: all four walker loaders install the walker
exec 0x800B3A8C, and the Chao rows are reached natively only from the
two-player select (ENGINE-MAP.md 8, 8a, 8b).

- **Player:** walker exec (merged #435, frame in #456; owner: Make first story level playable), the
  Mech Eggman loader 0x800B5D2C, lock-on and laser (trace from the exec's mode
  switch).
- **Stage:** Iron Gate objects (owner: Port shared stage objects); the enemies
  Iron Gate places (owner: Port native enemy behaviour). `sa2-assets fan-tasks`
  gives the exact task list for stg11D.
- **Minimum renderer:** draw the native landtable, player and object models
  with native materials, textures and lighting (decoders exist; the renderer is
  new). WebGL2 in the browser through a Rust/WASM build of `sa2-runtime`
  (the `sonic-advance` crate's wasm build is the pattern to follow).
- **Minimum camera:** the stage's default player camera (camera task
  fn_800C4458 → fn_800C3DA0 → njSetCamera; mode routine still to trace).
  0x801E4FF8 is the Steam-REL stick table, not the camera.
- **Minimum HUD:** rings, time, score, life count from the native HUD routine
  (trace).
- **Sound:** ADX stage music (decoder done) and stage sound effects through
  the native sound call path (trace).
- **Goal and results:** goal ring (ported) into the native results flow (trace).
- **Website:** a page that loads the player's own Steam data locally, runs the
  WASM build and plays Iron Gate. No game data is hosted.
- **Depends on:** Phase 1, #456.
- **Acceptance:** on mob's website, a person plays Iron Gate Mission 1 from the
  start to the goal ring as Mech Eggman, with input-driven native ticks and
  a stored replay that reproduces the run.

### Phase 3: Engine breadth: renderer, camera, HUD, pause, sound, saves

- **Renderer:** skinned models, alpha and material modes, fog (stage fog done),
  native effects and particles, water, shadows.
- **Camera:** all stage camera modes and camera-change objects; event camera
  (reader done).
- **HUD and pause:** full HUD, mission messages (message banks done), pause menu.
- **Sound:** mixer, sound banks, voice (AHX decode done), positional effects.
- **Saves:** write side of story progress, stage results, emblems and Chao
  data, matching the native save layout (readers done). The MMO server stays
  authoritative for anything it owns.
- **Depends on:** Phase 2. **Owner:** renderer and camera: Port the engine core
  and renderer (ENGINE-CORE-PLAN.md E6–E7); HUD, pause, sound and saves: unowned.
- **Acceptance:** Iron Gate and Prison Lane play with full HUD, pause, all
  their cameras and sound, and results write a native-layout save.

### Phase 4: Character movesets

Ordered by how many stages each unlocks.

| Moveset | Characters | Entry points | Depends on | Owner |
| --- | --- | --- | --- | --- |
| Mech walkers | Tails, Eggman, Chao Walker, Dark Chao Walker | 0x800B3A8C exec, 0x800AF558 display, loaders 0x800B5D2C / 0x800B5AC0 (alternate rows 0x800B56E8 / 0x800B58D4); lock-on, laser, hover (trace) | #435 (merged), #456 | Make first story level playable; Make Prison Lane playable |
| Speed | Sonic, Shadow, Amy, Metal Sonic | 0x8004C734 tree; main 0x80083E44; actions 0x8007E89C; loaders 0x80086BA4… ; homing, spin dash, light dash, grind, somersault (trace) | Phase 1 | Build Sonic speed-stage controller |
| Treasure hunt | Knuckles, Rouge, Tikal, Chaos | Loaders (trace via 0x8004F838 callers); glide, climb, dig, emerald radar and hints (trace) | Phase 1; Wild Canyon emerald logic (done) | Port the other playable characters |
| Super forms | Super Sonic, Super Shadow | Boss_Last2D.rel routines (PPC-verified) | Phase 1 | Verify Finalhazard camera switch path |
| Kart | all | CartD.rel | Phase 1 | See Phase 7 |

### Phase 5: Stages (31)

Every stage is already mounted from data. Each needs its character, its stage
module's own code and the shared objects it places. Order: walker stages first,
then speed, then hunters, then the rest.

| Family | Stages (module) | Needs |
| --- | --- | --- |
| Mech | Iron Gate (stg11D), Prison Lane (stg09D), Weapons Bed (stg12D), Mission Street (stg17D), Hidden Base (stg23D), Sand Ocean (stg21D), Eternal Engine (stg24D), Lost Colony (stg27D), Cosmic Wall (stg43D) | Mech moveset |
| Speed | City Escape (stg13D, board done), Radical Highway (stg14D), Metal Harbor (stg10D), Green Forest (stg03D), White Jungle (stg04D), Sky Rail (stg06D), Pyramid Cave (stg28D), Crazy Gadget (stg22D), Final Rush (stg30D), Final Chase (stg40D), Green Hill (stg31D) | Speed moveset; rails |
| Treasure hunt | Wild Canyon (stg16D), Pumpkin Hill (stg05D), Aquatic Mine (stg07D), Dry Lagoon (stg18D), Egg Quarters (stg26D), Security Hall (stg08D), Death Chamber (stg25D), Meteor Herd (stg32D), Mad Space (stg44D) | Hunter moveset |
| Last story | Cannon's Core (stg34D–stg38D) | All five movesets |
| 2P | stg39D–stg59D | Phase 8 |

Per-stage tasks come from `sa2-assets fan-tasks` (634 distinct native tasks
across the story SETs; about 5–10% done).

### Phase 6: Bosses

| Boss | Module | Status | Needs |
| --- | --- | --- | --- |
| Big Foot | boss_bigfootD.rel | Replay | Walker, live loop |
| Hot Shot | boss_hotshotD.rel | Replay | Walker |
| Flying Dog | boss_fdogD.rel | Missing | Walker |
| Tails vs Eggman 1 / 2 | stg20D.rel / stg29D.rel | Replay / missing | Walker |
| Sonic vs Shadow 1 | stg19D.rel | Missing | Speed |
| Sonic vs Shadow 2 | stg42D.rel | Replay (#422, #432) | Speed |
| Egg Golem (Sonic) / (Eggman) | Boss_GolemD.rel / Boss_GolemED.rel | Missing | Speed / walker |
| Knuckles vs Rouge | stg33D.rel | Missing | Hunter |
| King Boom Boo | boss_bigbogyD.rel | Missing | Hunter |
| Biolizard | boss_last1D.rel | Missing | Speed |
| Finalhazard | Boss_Last2D.rel | Replay; setup #436 | Super forms |

### Phase 7: Kart

- CartD.rel logic is mostly PPC-verified; course collision, layout, textures
  and billboards are native (#423).
- **Needs:** Phase 1 loop driving CartD from native input, Phase 2 renderer,
  then Route 101 / Route 280 and the kart race. Engine import 0x800D241C
  (kart buffer) to trace.
- **Owner:** unowned (kart threads finished).

### Phase 8: Chao Race, Karate and the garden as a playable space

- **Chao Race and Karate** (owner: Port Chao Race and Karate natively): native
  routines from the GC Chao modules replace `server/src/competition_sim.rs`;
  arenas from `ChaoStgRace*` and `ChaoStgKarate` (data-verified). MMO extras
  stay in a separate module.
- **Garden:** the player's Chao controller in the garden is still presentation
  code; it moves to the native Chao movement once Phase 1 exists.
- **Depends on:** server Chao life/body/AI state (merged), Phase 1 for the
  client side.

### Phase 9: Menus, story flow, events, 2P

- Title (titleD.rel), main menu, stage select, story sequence (campaign tables
  read), mission select, emblems from native stage clears only.
- Event player: event scenes, cameras and effects (readers done), Sofdec movies
  (demux done), subtitles and voice timing.
- 2P battle stages and character select.
- **Depends on:** Phases 3–6. **Owner:** unowned.

## Order of work in one line

Steam base → engine core → **Iron Gate (native Mech Eggman) on the website** →
engine breadth → mech stages and bosses → speed moveset and stages → hunters →
kart → Cannon's Core and Finalhazard → menus, events, 2P. Chao Race and Karate
run in parallel on the server.

## Decided

- **First playable: Iron Gate with its native character, Mech Eggman**
  (mob, November 20, 2026). Use only the native characters for now.
- **Playing as a Chao is deferred** until after the rest of the game is ported
  (mob, November 20, 2026). The earlier choice of the Chao Walker for Iron Gate
  is withdrawn; ENGINE-MAP.md 8a/8b keep the trace of how the game reaches the
  Chao Walker rows.

## Open questions for mob

1. How the website gets the player's Steam data: a local folder picker in the
   browser is assumed here; nothing is hosted.
