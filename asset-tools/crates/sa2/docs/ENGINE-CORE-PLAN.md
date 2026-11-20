# Engine core and renderer: working plan (November 20, 2026)

This is the step-by-step plan for the engine half of the port: the master loop
and task system, stage load and unload, input, the camera and the Ninja
renderer path. It expands Phase 1 of [PORT-PLAN.md](PORT-PLAN.md) (issue
[#440](https://github.com/mobdon/ChaoGardenMMOClient/issues/440)) and the
renderer, camera and web-build parts of Phases 2 and 3 (#441, #442).

**First target (mob's choice): Iron Gate played as the Chao Walker, in a
browser.** The walker exec 0x800B3A8C is ported in PR #435. This plan makes the
engine able to run that stage's tasks around it.

## Rules (from PORT-PLAN.md, restated for this work)

- Every engine routine is ported from main.dol (GSNE8P, SHA-1
  `e4c50f32242a3cd36cdc8ae776661601a52130a2`) and names its address. Logic is
  checked against the PowerPC under Unicorn with `tests/dol_ppc.py`.
- Nothing is invented. An engine routine that is not ported yet fails closed:
  the task that needs it is listed as unported and is not run, rather than run
  with a stand-in.
- Data comes from the player's own Steam install through the existing loaders.
  Data tables may be read from `sonic2app.exe` / `Data_DLL.dll` (`src/pe.rs`);
  the encrypted Steam code is never decrypted or disassembled.
- The game is never launched. No game data and no Gekko tables go into git.
- The renderer is the one place where native code cannot run as is: GX writes a
  GameCube GPU command stream. Every GX-to-web translation choice is written
  down in `RENDERER-GX-WEB.md` (milestone E7) with the GX call it replaces.

## What already exists

| Piece | Where | State |
| --- | --- | --- |
| Ninja matrix stack, trig, vectors, MSL math | `ninja_matrix`, `ninja_trig`, `ninja_vector`, `msl_math` | Bit-exact |
| PADClamp | `input.rs` | Ported |
| GetAnalog 0x8004E560 | `player_analog.rs` | Bit-exact |
| Walker land collision, walker frame and ground | `walker_collide`, `walker_frame`, `walker_ground`, `land_*` | Bit-exact (#427) |
| Walker exec and movement steps | `walker_move.rs` | PR #435 |
| Iron Gate stage rules (lifts, elevators, bridges, containers, rings, goal) | `play_session.rs` | Native, but driven by a host-supplied `PlayerSample` |
| Enemy library and its "host" interface | `enemy_lib/` | Native; engine calls are recorded as `host::Event`s |
| Stage mounting from data (landtables, SETs, starts, death zones, paths, lights, fog) | `stage*.rs`, `set.rs`, `stage_lighting.rs`, `stage_fog.rs` | Data-verified |
| Chunk model surfaces, GC meshes (kart), textures, poses | `visual.rs`, `model.rs`, `kart_course.rs`, `texture.rs`, `pose.rs` | Data-verified |
| A Rust-to-WASM build with a browser page | `asset-tools/crates/sonic-advance/wasm` and `preview/` | Pattern to follow |

### Engine calls the ported routines already make

The ported stage objects and bosses record the engine calls they make instead
of running them. The task module is the cluster at 0x80070790–0x80072820 in
main.dol. Addresses below are the ones recorded in this crate today, with how
often they appear; their names and exact roles are traced in E0, not assumed.

| Address | Seen in | Count |
| --- | --- | ---: |
| 0x80071BA4 | Task allocation (goal ring, duel fog/debris/setup, enemy_lib) | 26 |
| 0x80071878 | Free/destroy calls in ring group, goal ring | 21 |
| 0x80071888 | Child free in duel director, fog, debris | 16 |
| 0x80070790 | Child task creation (goal ring, duel title) | 11 |
| 0x80071AF0 | `DestroyTask` (shared stage objects) | 3 |
| 0x80071AE0, 0x80071E0C, 0x80071EB0, 0x80071FB0, 0x80072008, 0x800725B8, 0x80072820, 0x800709F8 | Duel stage setup and others | 1–2 each |

The task header offsets already used across the crate: exec at +0x10, display
at +0x14, further callbacks at +0x18, +0x1C, +0x28, +0x2C (installed by the
walker loader), `twp` at +0x34, a second work pointer at +0x38, collision at
+0x40. E0 confirms the full header against main.dol.

## Milestones

Each milestone is one PR (or a few small ones), linked to #440 unless noted.
Each one ends with tests that compare against main.dol.

### E0. Trace the engine (read-only, on mob's PC)

Work from main.dol and the reconstruction's `config/GSNE8P/symbols.txt`, both
kept local. Output: `ENGINE-MAP.md` with addresses, sizes, callers and callees.

1. The task module 0x80070790–0x80072820: every function in it, the task header
   layout, the task lists and their priorities (the `list` argument of
   0x80071BA4 takes values such as 2), allocation sizes, child links and
   destructors.
2. The per-frame passes: which routine walks the lists and calls +0x10 (exec)
   and +0x14 (display), in which order, and how pause and time-stop
   (0x801CBD88 +0x37) gate them.
3. The master loop: from `main` down to the per-frame function, the game-mode
   switch, the frame counter 0x801CC1E4 and vsync wait.
4. Stage load and unload for level 11: the order of calls between the stage
   number 0x8001DD78, the stage module (stg11D.rel) prolog/epilog, landtable
   registration, the SET loader 0x80065E54, InitCollision 0x8000F8B8, land
   registration 0x800232A4, lights 0x8001A13C, the player loader and the
   camera setup.
5. Input: the PAD read path from PADRead through PADClamp (ported) into the
   per-player stick table 0x801E53F8 and the button words (held, pressed,
   released).
6. The camera: the routine that fills the camera block 0x801E4FF8 for the
   stage's default player camera.
7. The draw path: what the walker display 0x800AF558, the landtable display and
   a stage object's display call, down to the Ninja/GJ draw routines and their
   GX calls.
8. The Chao Walker: which of the 14 callers of 0x8004F838 loads it, and whether
   it installs the walker exec 0x800B3A8C (the Phase 2 premise).

### E1. Task system, headless (`sa2-runtime` crate)

- New crate `asset-tools/crates/sa2-runtime`, depending on `sa2`. It owns the
  engine only; ported game routines stay in `sa2` and are registered into it.
- `Task` with the native header (callbacks as Rust function references to
  ported routines, keyed by their main.dol or REL address), the native lists
  and priorities, allocation, child tasks, the destroy path and destructors.
- The exec pass and display pass in native order.
- **Verification:** run main.dol's own task routines under Unicorn on a synthetic
  task list whose callbacks are small stubs that log their call order. Compare
  call order, list links and freed tasks with the Rust loop, tick by tick.
- The recorded engine calls in `goal_ring`, `ring_group`, `duel_*` and
  `enemy_lib::host` become real calls into the runtime. Their existing tests
  keep passing.

### E2. Frame step and game state

- The 60 Hz tick as the master loop runs it: frame counter 0x801CC1E4, the game
  state blocks 0x801CC168 / 0x801CBD88, pause and time-stop gating.
- Headless and deterministic: no wall clock inside the tick. The browser drives
  ticks from requestAnimationFrame with a fixed 1/60 s accumulator (a web
  choice, written down in E7's doc).
- **Verification:** the per-frame function under Unicorn with stubbed callees,
  compared with the Rust step.

### E3. Input

- Port the PAD path traced in E0 into the native stick table and button words.
- The browser maps keyboard and gamepad only into `PadStatus` (the raw pad
  struct). Everything after that is native. The key map is a web choice and is
  documented, never a gameplay rule.
- **Verification:** the native fill routine under Unicorn on recorded pad
  states, bit for bit.

### E4. Stage load and unload: Iron Gate

- Load level 11 in the traced order from Steam files: landtable, SET tasks
  through the SET loader and exec gate, objects, lights, fog, death zones,
  paths, start position, then the player loader (Mech Eggman 0x800B56E8 first,
  then the Chao Walker once E0 step 8 confirms it).
- SET rows whose task is not ported are listed by name and not spawned.
- Unload frees every task and returns the runtime to its empty state.
- `play_session::Session` stops taking a host `PlayerSample`: the walker task
  (from #435) produces the player state inside the loop.
- **Coordination:** the walker and land collision belong to "Make first story
  level playable"; generic objects to "Port shared stage objects"; enemies to
  "Port native enemy behaviour". This milestone only spawns and ticks their
  tasks.

### E5. Headless acceptance (closes #440)

- Iron Gate mounts and ticks 600 frames with the walker exec and its stage
  objects inside the task loop.
- A replay test: pad inputs in, per-tick task state out; the same inputs give
  the same trace every run.
- Runs in CI without game data by skipping cleanly, and fully on a PC with
  `SA2_GAME_PATH` set.

### E6. Default player camera (Phase 2, #441)

- Port the camera routine traced in E0 step 6 and its camera block.
- **Verification:** under Unicorn against recorded player states.

### E7. Ninja renderer path (Phase 2 minimum, #441; breadth in #442)

The native draw path ends in GX calls. The port keeps everything above GX
native and replaces GX itself with a web backend.

- **Native part (ported and verified):** the display callbacks, matrix stack
  (done), model and motion evaluation, material and texture selection, light
  and fog setup, and the GX state each draw sets.
- **Draw list:** the ported routines produce a draw list: transformed vertices
  or vertex buffers plus the GX state (TEV stages, blend, Z mode, cull, texture
  and sampler state, lights, fog) for each draw.
- **Verification of the draw list:** GX on the GameCube writes its commands to
  the write-gather pipe at 0xCC008000. The Unicorn harness can hook that
  address and record the exact GX command stream a native display routine
  emits. The Rust draw list is compared against that stream.
- **Web backend: WebGL2 first.** It runs in every current browser, including
  phones. The draw list is backend-neutral, so a WebGPU backend can be added
  later without touching the native part.
- **`RENDERER-GX-WEB.md`** records each translation: TEV stages to generated
  shaders, GX blend and logic ops to WebGL blend state, Z compare and update,
  cull mode, texture formats (Steam DDS and GC textures) to WebGL formats,
  wrap and filter modes, channel lighting to shader lighting, GX fog types to
  shader fog, and the GX projection and depth range to WebGL clip space. Each
  line names the GX call and the reason for the choice.
- **Steam data note:** the code reference is the GC GX path. Where a Steam
  file differs from the GC one (565 of 3,089 files), the Steam reader must
  hand the same draw path the same native data; any asset where that is not
  true is listed, not papered over.
- Pixel output cannot be compared with the real game (it is never launched).
  The checkable boundary is the GX command stream.

### E8. Web build: Iron Gate in a browser (Phase 2, #441)

- `sa2-runtime-wasm`, following `sonic-advance/wasm`: the runtime, the WebGL2
  backend and input in one WASM module.
- A page that reads the player's own Steam folder locally (folder picker; the
  open question in PORT-PLAN.md) and plays Iron Gate. Nothing from the game is
  hosted or uploaded.
- With the walker, objects, camera and minimum renderer, this meets Phase 2's
  acceptance once HUD, sound and results land (#441).

## Order and dependencies

```
E0 trace ─┬─ E1 tasks ── E2 frame ──┬─ E4 stage load ── E5 headless (#440 done)
          │                         │
          ├─ E3 input ──────────────┘
          ├─ E6 camera ─────────────────────────────┐
          └─ E7 renderer (native part, GX stream) ──┴─ E8 web build (#441)
```

E0 needs mob's PC (main.dol and symbols.txt are local). E1 and E3 can start as
soon as their part of E0 is written. E7's native part can run in parallel with
E1–E5.

## Ownership

This thread ("Port the engine core and renderer") owns: the task module,
master loop and frame step, stage load and unload, the PAD path, the camera,
the Ninja/GX draw path, the GX-to-web backend and the web build. It does not
port the walker, land collision, `player_turn`, fn_8004C734, generic stage
objects or enemies; it calls them. Shared helpers stay in their owner's file.

## Open questions

1. How the website gets the player's Steam data (folder picker assumed, as in
   PORT-PLAN.md).
2. Whether the Chao Walker loader installs the walker exec (E0 step 8). If it
   does not, Phase 2 needs its own exec port before Iron Gate is playable as a
   Chao.
