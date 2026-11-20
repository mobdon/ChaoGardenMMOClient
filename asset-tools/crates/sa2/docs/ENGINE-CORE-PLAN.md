# Engine core and renderer: working plan (November 20, 2026)

This is the step-by-step plan for the engine half of the port: the master loop
and task system, stage load and unload, input, the camera and the Ninja
renderer path. It expands Phase 1 of [PORT-PLAN.md](PORT-PLAN.md) (issue
[#440](https://github.com/mobdon/ChaoGardenMMOClient/issues/440)) and the
renderer, camera and web-build parts of Phases 2 and 3 (#441, #442).

**First target (mob's choice): Iron Gate played with its native character,
Mech Eggman, in a browser.** The stage loads exactly as the one-player story does:
loader 0x800B5D2C (PhysicsArray row 7), G+0x1E = 0. The walker exec 0x800B3A8C
is ported in PR #435. This plan makes the engine able to run that stage's tasks
around it. Playing as a Chao (the Chao Walker rows) is deferred until after the
full port (mob, November 20, 2026); ENGINE-MAP.md 8a/8b keep the trace for then.

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
of running them. **E0 found that these addresses come from the RELs in the
Steam `gd_PC` folder, which are linked against a different main.dol build
than GSNE8P** (offsets in [ENGINE-MAP.md](ENGINE-MAP.md) section 0: code +0x0
up to 0x8001ADD3, +0x84, +0x248, then +0x268 from 0x80027A80 to 0x800DDCAB,
+0x2DC, +0x348; data 0x801CBD88 → 0x801CC168 is +0x3E0, 0x801E4FF8 → 0x801E53F8
is +0x400). The task code in GSNE8P is 0x800706B0–0x80072080. The right column
is the GSNE8P function each recorded address is; the runtime keys callbacks by
GSNE8P address and keeps this table as the Steam-REL alias map.

| Recorded (Steam-REL build) | Seen in | Count | GSNE8P function |
| --- | --- | ---: | --- |
| 0x80071BA4 | Task allocation (goal ring, duel fog/debris/setup, enemy_lib) | 26 | `CreateElementalTask` 0x80071E0C |
| 0x80071878 | Free/destroy calls in ring group, goal ring | 21 | `FreeTask` 0x80071AE0 |
| 0x80071888 | Child free in duel director, fog, debris | 16 | `DestroyTask` 0x80071AF0 |
| 0x80070790 | Child task creation (goal ring, duel title) | 11 | `CreateChildTask` 0x800709F8 |
| 0x80070724 | — | — | `FreeTaskC` 0x8007098C |
| 0x80071EB0, 0x80071FB0, 0x80072008, 0x800725B8 | Duel stage setup | 1–2 each | motion-table cluster 0x80072118, 0x80072218, 0x80072270, 0x80072820 |
| 0x80071AF0, 0x80071AE0, 0x80071E0C, 0x800709F8 (already GSNE8P) | Shared stage objects | 1–3 | same names; check each call site's build before porting |
| 0x801CBD88 / 0x801E4FF8 (data) | Many | — | G = 0x801CC168 / stick table 0x801E53F8 (**not** a camera block) |

Task header (GSNE8P, 0x50 bytes; full table in ENGINE-MAP.md 1.1): links
+0x00/+0x04/+0x08/+0x0C, exec +0x10, displays +0x14/+0x1C/+0x20 (depth-sorted)
/+0x24/+0x28, destructor +0x18, pre-pass display +0x2C, SET entry +0x30,
`twp` +0x34 (0x30 bytes), +0x38 (0x40), +0x3C (2×0x1C), +0x40 (0x10; player
tasks keep their player work here, it is **not** collision), name +0x44.

## Milestones

Each milestone is one PR (or a few small ones), linked to #440 unless noted.
Each one ends with tests that compare against main.dol.

### E0. Trace the engine (read-only, on mob's PC) — **done**

Result: [ENGINE-MAP.md](ENGINE-MAP.md). Its corrections are folded in below
and its "Unresolved" list feeds E1–E7. The item list is kept as written; its
Steam-REL addresses map as in the table above.

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
- **Start here (from ENGINE-MAP.md):**
  1. [x] Crate skeleton `sa2-runtime` with an arena of `Task` slots and the 8
     `btp` list heads (0x801FCEB8), `clearBtp` 0x80071F2C.
  2. [x] `GetTask` 0x80071D04 / `CreateElementalTask` 0x80071E0C (im bits
     0x2→+0x34 0x30 B, 0x4→+0x3C 2×0x1C, 0x1→+0x38 0x40 B, 0x8→+0x40 0x10 B,
     tail insert, level 7 unlinked, `no_op` default exec).
  3. [x] `CreateChildTask` 0x800709F8 (child ring, parent link, twp
     +0x08..+0x1C copy), `FreeTask` 0x80071AE0, `FreeTaskC` 0x8007098C,
     `DestroyTask` 0x80071AF0 in its exact order (destructor first, children,
     unlink, list or parent head fix-up, work frees, SET bit clear).
  4. [x] Exec pass `fn_80071214` (levels 0–5; head re-read after each task,
     children through `LoopTaskC`), and `fn_80071A40(0)` alone for the
     G+0x49 path.
  5. [x] Display pass `fn_80070C54` order (+0x14, +0x20 sorted, +0x1C, +0x24,
     +0x28) with the levels 2–5 viewport gate, and the +0x2C pre-pass.
  6. [x] Unicorn oracle `tests/task_system_ppc.py` with stub callbacks that log
     order; Rust loop compared tick by tick.
  7. [ ] Steam-REL alias table (above) so existing recorded calls map to the
     runtime.
- **Verification:** run main.dol's own task routines under Unicorn on a synthetic
  task list whose callbacks are small stubs that log their call order. Compare
  call order, list links and freed tasks with the Rust loop, tick by tick.
- The recorded engine calls in `goal_ring`, `ring_group`, `duel_*` and
  `enemy_lib::host` become real calls into the runtime. Their existing tests
  keep passing.

### E2. Frame step and game state

- The 60 Hz tick as the master loop runs it: in-stage frame 0x8001E45C (pad
  copy, exec, fn_80006B00, fn_800D0904, frame counter fn_80062B38 per tick;
  then display), stage modes 0x803AD81A (7 start, 0x10 play, 0x11 pause =
  display only), frame counter G+0x7C (0x801CC1E4) stopped by G+0x37, the
  stage clock, and the vsync tick count 0x803AD8AC. G is 0x801CC168
  (0x801CBD88 in the Steam-REL build).
- **Done** (`sa2-runtime/src/frame.rs`): `fn_8001E45C`, `fn_8001E89C`,
  `fn_8001E12C`, `fn_80062B38`, `fn_80070C20`, `fn_80070BA8`. The tick count's
  float formula turned out to be dead code: its result is overwritten with 1 at
  0x8001E518, so ticks are 1, plus 1 every fifth frame when G+0xF == 2.
  Engine callees not ported yet are logged and not run. Not ported here: the
  GX pre-pass `fn_80070F5C` (so the +0x2C walk it runs is not run yet), the
  stage-mode switch `fn_8001F368` and the master loop around it.
- Headless and deterministic: no wall clock inside the tick. The browser drives
  ticks from requestAnimationFrame with a fixed 1/60 s accumulator (a web
  choice, written down in E7's doc).
- **Verification:** the per-frame function under Unicorn with stubbed callees,
  compared with the Rust step.

### E3. Input

- Port the PAD path traced in E0: fn_80119A04 (records at 0x80357E60, held
  +0x8 / pressed +0x10 / released +0x14, button table 0x801D51A4), the per-tick
  copy fn_800226E0 into 0x801E5438 / 0x801E55D8, and the stick table
  0x801E53F8 from fn_80022048.
- The browser maps keyboard and gamepad only into `PadStatus` (the raw pad
  struct). Everything after that is native. The key map is a web choice and is
  documented, never a gameplay rule.
- **Verification:** the native fill routine under Unicorn on recorded pad
  states, bit for bit.

### E4. Stage load and unload: Iron Gate

- Load level 11 in the traced order from Steam files: landtable, SET tasks
  through the SET loader and exec gate, objects, lights, fog, death zones,
  paths, start position, in the order of stg11D's init fn_11_A434
  (ENGINE-MAP.md 4.2), with players loaded first by fn_8001DEA8 as the session
  init does, exactly as the one-player story: character 7, loader
  **0x800B5D2C** (Mech Eggman, row 7), G+0x1E = 0 (no player 2, one viewport).
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

- Port the camera task (fn_800C4458; exec fn_800C3DA0, display fn_800C3FCC,
  fn_800C4278 → `njSetCamera`). Which mode routine Iron Gate uses by default is
  still to be traced. 0x801E4FF8 was not a camera block (it is the Steam-REL
  address of the stick table).
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
2. ~~Whether the Chao Walker loader installs the walker exec~~ — answered by
   E0: yes, all four walker loaders install 0x800B3A8C.
3. ~~How the game sets G+0x1E and G+0x55+i~~ — traced (ENGINE-MAP.md 8a/8b):
   only the two-player select and the title demos do. Not needed for the first
   target, which uses the native character.
