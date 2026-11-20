# Sonic vs Shadow 2 platforms (story level 42)

Level 42 has no landtable. Its floor is built at run time by two objects in the
installed GC module `resource/gd_PC/stg42D.rel` (SHA-256
`51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d`, module 41):

| Object list row | Code (section 1) | Role |
| --- | --- | --- |
| 24 `DODAI` | +0x1D68 | One platform: copies the Basic object at section 5 +0x13C8 and registers it as dynamic collision. |
| 25 `DODAIGEN` | +0x2278 | Generator placed once by `SET0042_S.BIN` (record 0, at the origin). |
| 26 `FIREBALL` | +0x24BC | Meteor; eight are placed by `SET0042_S.BIN`. See [Meteors](#meteors). |

The REL object list at file offset 0x2682C (SA Tools `GC_SA2B/stg42D.ini`) has
the same 27 names as the PC list at RVA 0x12546E8. The PC executable's code
section is not readable as plain instructions, so the PowerPC module is the
behavior source. The REL template is geometrically identical to the PC
collision object at RVA 0x1251FA0 (24 vertices, normals, bounds, material,
strip order and winding), which SA Tools already corroborates.

## The platform

A 300 × 15 × 100 box, X −150..150, Y −10..5, Z −50..50 in its own space, 24
vertices in one strip meshset. DODAI copies the object, sets the copy's
position and angles from its work area, enlarges the copied model's bounding
radius by 15 (to 173.29) and registers it with flags `0x28000001` (solid,
dynamic collision, size class 0x20000000 for radius 100..200).

## The generator

Per update, with work fields `+0` mode, `+4` flags, `+6` frame counter and
`+0x14` cursor X:

1. Mode 0 sets the cursor to 0.0 and enters mode 1.
2. Mode 1, if game byte +0x42 is set: spawn one platform at X −300 with timer
   0x8000 and enter mode 2. The authored end rows (Sonic −308, Shadow −292)
   stand on it.
3. Otherwise, for each non-null player-table slot (0x801E7368) in order, while
   `cursor − player.x < 2100`: spawn DODAI at the cursor, set its timer to
   `trunc(max(0.4 × (player.x − cursor), −3600))` (low 16 bits), and add 300
   to the cursor. After the deadline flag, each new platform instead gets
   timer 900 and its activation byte set.
4. Unless game byte +0x37 is set, increment the frame counter; above 36000
   it stays at 36000 and sets flag 0x1000.

DODAI starts to fall on the first update whose signed timer exceeds 900. When
the first slot of the player data table (0x801E7328, field +0xA0) references
an inactive platform, that platform is activated with timer 720; an activated
timer counts up on updates where game byte +0x37 is clear and +0x42 is not 1,
so it falls 181 updates after activation. Meteor objects are not ported here.

## The platform update

`duel_platform_motion` ports DODAI's update (code +0x1D68) as a state machine
whose engine queries are inputs and whose engine calls are recorded:

- Mode 0 (first update): allocate 0x38 bytes (null: stop) and 0x28 bytes
  (null: free the first and stop), copy the template object and model, and
  register with flags `0x28000001`. A failed constructor leaves mode 0, so the
  next update retries. The update tail runs either way.
- Mode 1: a visibility query with radius `16 + 173.29` sets or clears flag
  0x100. With a signed timer above 900 the platform enters mode 2, calls the
  local debris helper (+0x1E8DC) with 150, 50, 7, 0.5 and 1, requests a
  60-frame rumble of strength 2 if player 0's flag bit 1 is set and that
  player is on this platform, sets its Y velocity to −0.15 and plays sound
  (4, 0, 0, 0x50).
- Mode 2: the same visibility query, then Y += velocity and each angle +=
  its spin; the collision object copies Y and the angles. Once Y is below
  −100 the engine task check runs (true ends the update with flag 0x100
  cleared), the velocity drops by 0.1 per update until it reaches −2.1, and
  if the X spin is zero three `rand()` values set the spins: X and Y are
  `24 − (trunc(64·r/32768) & ~15)` and `32 − (…)`, Z is
  `trunc(16·r/32768 − 32)`; words +0..+0x18 are then copied to +0x1C. Below
  −100 every update also calls the fall effect (5, work, 1, 0x28, 0x78).
- Mode 3 frees the task and returns.
- Tail: a player standing on an inactive platform activates it with timer
  720, and an active timer counts as described above.

If task +0x30 is nonzero, the engine task check runs first and a true result
skips the whole update. The debris helper body, rendering and collision
response are not executed.

With both player slots at X 0 (the authored start rows), the generator's
second update spawns seven platforms, X 0 to 1800. The generator writes only
X, the timer and the activation byte; Y, Z and angles come from the engine
allocator, which is not executed, so this port does not assemble a world mesh.

## Meteors

`duel_fireball` ports the meteor's constructor (+0x24BC), update (+0x2670) and
destructor (+0x2590); the display callback (+0x25BC) is not executed.

- Constructor: unless the engine task check is true, it installs the three
  callbacks, initialises collision from section 5 +0x1E48 (count 1, list 4)
  and requests a light (kind 1, at the work position, radius 55, squared
  3025), keeping the returned handle at task +0x4C. The destructor frees a
  nonzero handle.
- Update: after the task check, a player hit or a timer above 60 explodes
  the meteor. Otherwise it adds itself to collision; a zero timer runs a
  visibility query with radius `30 + SET scale X` and becomes 1 when seen,
  and a nonzero timer counts up. Angles X and Y advance by
  `trunc(182.04445 × 0.36)` and `trunc(182.04445 × 0.45)` BAMS. One
  `rand()` below 0.2 (of 32768) spawns smoke with velocity
  `(2(u−0.5), u−0.2, 2(u−0.5))`, scale `20 + 30u` and colour `0xFF3020FF`.
- Explosion: sets work +1, then for 20 pieces draws three `rand()` values:
  height `0.5 × radius × (1+u)` and up speed `1 + 2u`, then two turns of
  `trunc(182.04445 × 720u)` BAMS passed to Ninja rotations. The transformed
  point and vector come from the engine; the vector's Y gains 0.5 before the
  shared debris task (+0x8308) receives it with floor −100000, mode 0x2000
  and, for the first five pieces, the section 5 +0x1DD8 model and +0x1C7C
  texlist (otherwise +0x1C14 and +0x1A28). The task then pops two matrices
  and destroys itself.

The engine results (task check, hit, visibility, allocations, rand, matrix
transforms and the collision radius at work +0x2C) are request inputs.

## Debris pieces

`duel_debris` ports the module's shared debris manager (task "EfCrash3DExec"):
spawn (+0x8308 wrapper, +0x8348), update (+0x888C), delete (+0x8820), the
wind query (+0x7558) and the two trail-effect spawners (+0x1DFE0, +0x1E194).
A spawn creates the manager task on first use, allocates a 0x50-byte piece,
places it at the engine-transformed model centre, draws a 160 + 55u timer and
a random spin unless flags 0x80/0x100/0x200 zero an axis, and pushes it on
the list head. Each update moves every piece by its velocity (optionally
through a signed axis permutation, flag 0x4000, or pulled by a wind result,
flag 0x20000), applies 0.989 damping and 0.089 gravity in the air (none with
0x40000), and on reaching its floor either keeps sliding (0x40800), bounces
at -0.6, or rests and fades by 0.05. Flag 0x20 makes one splash on landing,
0x10 leaves trails while airborne, and 0x40 kills it on landing. Below a
timer of 10 the piece fades by 0.1; a piece with no life left is freed, and
an empty list frees the task. The display callback (+0x91A8) is not executed.

## Platform dust

`duel_dust` ports the helper DODAI calls as it starts to fall (+0x1E8DC) and
its particle spawner (+0x1DDFC). With half width 150, half depth 50 and
density 0.5 it visits 15 points along each long edge and 5 along each short
edge; each engine-transformed point gets a particle whose outward velocity is
normalised to the given speed, with two random angles, a random size field
and a random addition to field +0x18.

## Per-frame routine and SET wrap

The stage's registered per-frame routine (+0x3078) asks the engine to handle a
fall-out (0x8003B694 with 0) when the game pointer at 0x803AD5D0 is set with a
zero +0xC word, both player slots are filled without player-data bit 0x4000,
and player 1 is below Y -100. It then always runs the SET wrap (+0x2B8C):
once player 1 has passed X 300, every SET entry whose object-list flags lack
bit 2 and whose status lacks bits 9 is moved by 2400 along X until it lies
within (X - 1200, X + 1200] of player 1, then marked for respawn (status
0x8000, bytes 0). The platform objects carry list flag 2 and never wrap; the
rings and meteors do, so they keep reappearing ahead of the runners.

## Setup and teardown

`duel_stage_setup` ports the registered setup (+0x2CF0) and teardown
(+0x2F88) with their module helpers. Setup asks the engine two mode queries,
and when a rival occupies player slot 2 it hands that slot over (player byte
+2 = 3), creates the rival title task (+0x18AC) with a child gauge task
(+0x20470, gauge 5.0, 0.8, 0.4) and loads the Sonic or Shadow title texture.
It then loads both texture sets, the SET pair `set0042_s.bin`/`set0042_u.bin`
with the stage object list, creates the background task (`BgExec`), builds
the light tables from `stg42_light.bin` (the `stage_lighting` derive, then an
optional `stg42_light_gc.bin` override), loads `stg42_fog.bin` or falls back
to cleared fog with a 100000 far plane and creates `FogtaskManExec`, loads
the `mh0042` and `mhsys` message banks for the current language letter
(`jefsg`, then `j`, then none) and rebases their pointers, and finally sets
the ambient colour, stage name, two engine callback slots, the 14-word
callback table at 0x801E73D8, the `se_vs_shadow.mlt` sound bank, music
`boss_05.ADX` and the draw range (-3, -20000). Teardown frees the message
banks, clears the callback slots it owns, removes the fog manager and fog
state, releases the texture sets and SET data, and sets the draw range
(-3, -3000). The tasks created are recorded with their callbacks; their own
update and display code is not part of this port.

## Fog tasks

\duel_fog\ ports the fog manager \FogtaskManExec\ (update +0xAE04, delete
+0xAE94) and the worker \FogTaskExec\ it spawns (update +0xAD48, delete
+0xADE4) with the apply (+0xAA50) and blend (+0xAB5C) helpers. The manager
respawns a missing worker; the worker applies the current fog block or blends
it toward a target block by the factor at s6+0x64C (clamped to 0..1). A far
plane of 100000 or more disables fog (type 0, near -10); otherwise flag 2
selects the type from the module table by flags bits 16..19 with the block's
near, and no flag 2 means type 5 with near -10 (-50 when blending). Colours
go to the engine as RGBA. Deleting the manager frees the worker and, if it
owns the engine fog, resets it.

## Duel director

\duel_director\ ports the \ControlShadow\ task update (+0x3CC) with its
helpers: the lane clamp (+0x0, keeps the rival within |Z| < 50), the camera
target (+0x8C, smoothed toward 80 ahead of player 1 when they are more than
80 apart), and the three facing tests (+0x168, +0x280, +0x334) built on the
engine atan2 and angle difference (< 0x5000 BAMS) and distance (< 19). Each
update first handles a fall below Y -50 or falling 150 behind (mode 9, the
lose sequence and voice line), then dispatches 13 modes: 0 wait for game
mode 0x10 and open the dialogue; 1 intro camera and the rival's first line;
2/12 and 5 race pacing, which scales the rival's speed by 1.005-1.007 when
behind and by 0.999/0.985/0.972/0.96 (by gauge) when ahead and moves to the
turn (3), knock-back (6) or win (10, reached 90 + 250 - 20 x gauge ahead)
modes; 3, 4, 6 and 7 the jump and landing checks; 8 and 10 the win camera;
9 the lose staging at 60, 300 and 330 updates. The tail runs the lane clamp
and camera target, resets on game mode 7 or a cleared game word, advances
the timer and keeps the gauge task alive. Stores of engine doubles use the
architectural \stfs\ conversion (no rounding). The dialogue window
(+0x22070), message effect (+0x18620) and gauge (+0x20470) are recorded as
local calls.

## Inputs, not assumptions

Nothing in these replays is filled in by the port. Engine results are request
inputs: allocator results (`failedAllocations`, per call in update order),
task checks, hits, visibility, `rand()` values and matrix results. The work an
engine allocates is supplied whole (`work`); the generator's own spawns report
only the three fields it writes, or `null` for a null allocation. Like the
native loop, a generator update with a player far enough ahead that the f32
cursor stops advancing does not return.

## Rust interface

- `duel_platform::read_rel(rel)` reads the template from the fingerprinted REL.
- `duel_platform::generator(rel)` reads every generator and DODAI constant
  through the relocation or instruction that consumes it.
- `Generator::step(state, frame)` replays one DODAIGEN update.
- `duel_fireball::Fireball::{init, update, delete}` replay the meteor callbacks.
- `duel_platform_motion::Platform::step(work, frame, rand)` replays one DODAI
  update from the supplied work.

```powershell
sa2-assets duel-generator <game>\resource\gd_PC\stg42D.rel .local\out\gen.json REQUEST.json
sa2-assets duel-platform-update <game>\resource\gd_PC\stg42D.rel .local\out\platform.json PLATFORM.json
sa2-assets duel-meteor <game>\resource\gd_PC\stg42D.rel .local\out\meteor.json METEOR.json
```

`REQUEST.json` is `{"frames":[{"players":[x|null,x|null],"resultMode":false,"paused":false,"failedAllocations":[]}, ...]}`.
`PLATFORM.json` is `{"work":{"mode":0,"activated":0,"flags":0,"timer":0,"angles":[0,0,0],"position":[x,y,z],"motion":[14 words]},"taskField30":false,"rand":[...],"frames":[{"taskCheck":false,"visible":true,"standing":false,"playerFlag":false,"paused":false,"gameByte42":0,"failedAllocations":[]}, ...]}`.
`METEOR.json` is `{"work":{"exploded":0,"timer":0,"angles":[0,0],"position":[x,y,z],"scaleX":0},"collisionRadius":32,"lightHandle":1,"rand":[...],"frames":[{"call":"init"},{"visible":true,"hit":false,"smokeAllocated":true,"transforms":[20 × {"point":[…],"vector":[…]}]}, ...]}`.

## Verification

```powershell
$env:SA2_GAME_PATH = '<Steam>\Sonic Adventure 2'
cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml --test duel_terrain -- --include-ignored
python asset-tools/crates/sa2/tests/duel_generator_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_platform_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_meteor_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_debris_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_dust_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_stage_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_stage_setup_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_fog_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
python asset-tools/crates/sa2/tests/duel_director_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
```

The Python commands need Unicorn 2.1 on `PYTHONPATH` and must run from the
repository root (it writes temporary requests under the ignored `.local`).
It relocates DODAIGEN's own code and runs it under Unicorn with stub
allocator and register-save imports. Over a 36,800-update race (pause,
missing player slot, a 12,000-unit jump, null allocations, the deadline), the
result-platform path with and without a null allocation, and a
timer-wraparound case, every update's mode, cursor bits, flags and counter and
all 3,260 allocator results (X bits, timer and activation byte, or null) match
`duel-generator`. The stubs show the generator never writes a spawned
platform's Y or Z. The engine allocator itself is not executed.

`duel_platform_ppc.py` runs DODAI's constructor, update and collision
transform helper (+0x1AD0..+0x2278) with sections 4 and 5 relocated and every
engine import stubbed. Across three platforms (both constructor allocation
failures, stand, activate, pause, game byte 0x42, rumble, fall and removal;
nonzero starting Y, Z, angles and flags with task +0x30 set; spawned after
the deadline), all 2,458 updates match
`duel-platform-update` in every work field, the collision object's Y and
angles, all 14 motion words, and the ordered engine calls with their
arguments. Mode 3 is not reached by these inputs.

`duel_meteor_ppc.py` runs the meteor constructor, update and destructor the
same way, with the Ninja matrix calls and the debris task as recording stubs
that return scenario values. Four meteors (a visibility timeout with smoke
on alternate updates, a touch after a failed construction, deletion with its
light, and a zero light handle) produce 86 callbacks whose work fields and
ordered engine calls, including all 40 debris pieces, match `duel-meteor`.

`duel_debris_ppc.py` drives the debris manager's spawn, update and delete
through the original code: twenty meteor pieces until they fade, and a mixed
set covering a failed task and piece allocation, splash, trails with failed
effect allocations, wind, axis permutation, a given spin, no-gravity,
sliding, kill-on-landing, resting and a piece spawned while paused. All 533
callbacks match `duel-debris` in every piece field and ordered engine call.
`duel_dust_ppc.py` runs the dust helper with DODAI's arguments (40 points)
and with odd arguments and failed allocations; all 398 calls match
`duel-dust`. `duel_stage_ppc.py` runs the per-frame routine on 400 random
frames; every fall-out call and all SET entry bytes, statuses and X bits
match `duel-stage-frame`.

`duel_stage_setup_ppc.py` runs setup and teardown with every engine import
stubbed (string functions as C strings, loads and queries from the
scenario) and a write hook on every location the module can change. Five
scenarios (solo; rival Sonic with a light override and a language fallback;
rival Shadow with failed allocations and existing state; full and empty
teardown) match `duel-stage-setup` in every ordered engine call and every
written location. \duel_fog_ppc.py\ runs 120 random fog callbacks; every engine call
and all fog state match \duel-fog\. \duel_director_ppc.py\ runs 600 random director updates covering
every mode; all 2,136 engine and local calls, the work area and every
written engine global match \duel-director\.
Nothing launches the game.
