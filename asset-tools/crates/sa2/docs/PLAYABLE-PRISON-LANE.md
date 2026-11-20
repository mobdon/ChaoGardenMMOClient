# Prison Lane native stage rules (Hero story, Tails' walker)

`play_session::Session::prison_lane` builds Prison Lane (Hero story dispatch
9, level 9, MechTails, PhysicsArray slot 6) from the installed game files
and runs only rules read from them; `play_session::PRISON_LANE` holds the
per-stage native inputs.

It is not playable. The walker's movement, collision response, lock-on and
laser are main-program code. The Steam executable's code is
SteamStub-encrypted on disk and no GameCube main program is available, so
none of it can be read here. The adapted controller, collision queries,
headless autopilot, planner, window and the contact radii that were not
read (checkpoints, springs) have been removed. A host supplies a
`PlayerSample` each frame (the player's collision centre and the mover the
engine says it stands on) and `Session::step` runs the native object rules.

```powershell
# From asset-tools/crates/sa2-play: write the stage's native rule state.
cargo run --release -- $game $pcSa2Config --stage prison-lane out.json
```

Rotations use the GameCube main program's own trigonometry (`ninja_trig`, read from the GSNE8P main.dol of the person's disc; every PC stage-module import maps onto it): njRotateX/Y/Z convert the angle to radians and call MSL sinf/cosf, and njSin/njCos read the start-up table. The sines and cosines are bit-for-bit ports: checked with #426's Unicorn run of main.dol (examples/ninja_trig_check.rs), the 65536-entry njSinCos table hashes identically and MSL sinf/cosf agree on all 3,014 oracle samples (`ninja_trig`, `ninja_matrix`, `ninja_vector`, `msl_math` and the `land_*` ground queries are copied unchanged from #427); the matrix products that combine them are evaluated as plain single-precision steps rather than in the paired-single order of the SDK matrix routines, so a result can differ in its last bit. Collision boxes (CollisionData form 3) follow the GameCube CCL (`ccl_box_tris`): the row centre is moved by the task position and rotation (0x8000EAA0: rotate Z, X, Y, or Y, Z, X with attribute 0x200; none with 0x8000; a world point with 0x20), and the box itself is aligned to the yaw only (Rotation.y plus the row's Y angle; the box tests such as 0x800090E8 turn the relative vector by njRotateY of minus that sum), so X and Z rotations never tilt a box. Round shapes are kept analytic in `Session::shapes` (`RoundShape`), with the same centre rule: a sphere (form 0) of radius a; an upright cylinder (form 1, the 0x8000A054 test) of radius a and half height b; and a capsule (form 2, 0x8000987C) from centre - b * axis to centre + b * axis with radius a, where the axis is (0, 1, 0) through the row angles and then the task rotation (0x8000E960, unless attribute 0x20 or 0x8000). Lift, elevator and
bridge collision objects keep the placement's Y rotation only (stg09D.rel
0x4930, 0x30CC, 0x510; stg11D.rel likewise), and the elevator's object sits
40 below its placement (checked constant in both modules). Goal contact is
reported by the host (`PlayerSample::touching_goal`), and the slot word the
goal ring writes is exposed as is rather than interpreted as a clear.

## Mover rules

Both stage modules build the MLIFT/ELV/BRIDGE family from the same source.
`iron_gate_movers::MoverLayout` names each module's object rows, callbacks,
checked instruction words and the `lis` sites of every constant, and
`MoverProgram::verify_layout` refuses a module whose digest, rows, words or
constants differ.

| Rule | stg11D.rel (Iron Gate) | stg09D.rel (Prison Lane) |
| --- | --- | --- |
| Lift rise, speed, timers | 20 + Scale.x, 0.5/f, 30 / 90 frames | same (0x4704, exec 0x4AA0) |
| Elevator travel | 0.3 per frame | 0.6 per frame (sec5+0x3F554) |
| Elevator inner door (task 0x44) | starts -9.5, closes/opens 0.2/f | same |
| Elevator outer door (task 0x48) | starts -9.5, opens to -9.5, closes 0.2/f | starts 0 (shut), opens to -36.3 at 0.2/f, closes 0.32/f |
| Bridge | -140 local X, 8 per frame, elevator within 450 | -140, 1.5 per frame, 450 |

The elevator's idle code opens both doors while the car rests at its
placement and only the outer door at the far end, and it tests occupancy
only when the doors it opens were already open. A newly placed Prison Lane
elevator therefore needs 182 frames before it can start, and closing the
outer door (114 frames) gates travel.

## Stage objects

| System | Source | Built |
| --- | --- | --- |
| Terrain | `objLandTable0009`, `collision::bake_stage` | Geometry only |
| Object models | PC_SA2/STG09.ini: lift 0xC3C538, bridge 0xC42EA0, elevator 0xC39660, panel 0xC40390 | Native Basic models |
| PANEL (17) | stg09D.rel 0x2A4C passes its model to the static-collision helper 0x9A4C with flags 6 (Y rotation kept, X and Z dropped) | All 17 |
| CCUBE (17) | stg09D.rel 0x11CDC: CollisionData sec5+0x4C080 (form 3, attr 0x2000), sizes 10 + Scale per axis (helper 0x11B30) | All 17, by the CCL box rule |
| FENCES (39), FENCEL (4) | 0x2A94 / 0x2AF8 register CollisionData rows sec5+0x3C54C (1) and +0x3C57C (3) via helper 0x2B5C, read at load by `play_objects::collision_rows` | Every form 3 row by the CCL box rule: 39 FENCES and 4 FENCEL boxes; FENCEL's two form 2 rails become capsules |
| CONTIRON (18) | 0x1978C registers CollisionData sec5+0x52A18 through 0x8000F7D8: form 3, centre (0, 10, 0), half-extents 10 (a 20-unit cube; Iron Gate's row is identical) | All 18 |
| CONTCHAO (3), CONTWOOD (17) | 0x18E1C and 0x1A3A4 register the same cube rows (sec5+0x51D20, +0x53A08); breaking (player attacks) is not run | All 20 |
| MECHA (12) | 0x527C registers sec5+0x41DB4: form 3, centre (0, 0, 2), half-extents (10, 10, 5); its exec only runs that collision | All 12 |
| Enemy groups | GSNE8P 0x80018C88/0x80018C10 (count up), 0x80018B88/0x80016598 (count down), 0x80018E30 (query: count is zero); stg09D.rel E EMI (callback 0x4E4C4 calls 0x4C848, not counted when its kind, Scale.x truncated, is 4), E KUMI (0x4C408 runs 0x4A3A8, always counted), E AI 0x513D4, E NAMIE 0x54080; E GOLD (0x494AC) takes an enemy block through 0x80018D28 without counting | Group = low byte of Rotation.y. Counts are built from the SET; `Session::destroy_enemy` reports a kill (the walker's attacks are not ported). Enemy behaviour itself is not run |
| CAGED sliding cell doors (11) | 0x1EB0 set-up, 0x2000 exec | Ported: closed until the group named by Rotation.x's low byte is clear, then slide open by Scale.x per frame to 50 (sound 0x1020 while sliding, 0x1021 at the stop), settle with velocity 0.5, pull 0.4 and keep 0.96, and snap shut if the group fills again. Its collision box (sec5+0x38F68: form 3, centre (0, 30, 0), half-extents (5, 31, 30)) has its centre Z set to +slide or -slide by Rotation.z bit 0 each frame (0x22EC); `Session::door_solid` places it by the CCL box rule |
| CAGE (14), CAGEB (2), CAGEC (3) cell gates | CAGE 0xF64/0x10B4 (sec5+0x36258, 1 row, stop 48), CAGEB 0x1480 (sec5+0x36E98, 4 rows, stop 39), CAGEC 0x1994 (sec5+0x380B8, 4 rows, stop 39) | Ported with the CAGED state machine: CAGE raises its box (centre Y + slide); CAGEB and CAGEC lower their bar row (row 1 centre Y - slide) and drop all their collision once fully open (state 2) |
| SPHERE (4), CCYL (1), ROLL (4), FENCEL rails | SPHERE 0x11B94 (sec5+0x4C020), CCYL 0x11C38 (+0x4C050), sizes 10 + Scale; ROLL 0x53FC (+0x43A84, its exec only spins the model); FENCEL rows 1-2 | Round shapes: 4 spheres, 1 cylinder, 4 gear capsules and 8 rail capsules |
| WALL barred gates (2) | 0x66B0 set-up, 0x6800 exec; sec5+0x48158 (5 rows) | Ported (ElevatorWall): start raised (slide 50); while no registered elevator that is not moving lies within 450 (3-D distance, GSNE8P 0x8011B65C), drop by Scale.x per frame to 0, then settle (kick 0.5, pull 0.4, keep 0.96) and stay down; row 1 centre Y + slide |
| TANK (1) | 0x5D7C registers sec5+0x47848: an upright cylinder (form 1), centre (0, 27.5, 0), radius 15, half height 29.5 | The cylinder. Breaking it (debris, its two child tasks) is driven by player attacks and is not run |
| RING_LINEAR (17 lines) | 0x1B448, count helper 0x1B1E8, placement helper 0x1B21C | All 17 (64 rings): Scale.z clamped to 1..=8, ring i at (0, Scale.y * njSin(i * (0x4000 / n)), -i * (10 + Scale.x)) under rotate Z, Y, X (GSNE8P njRotateZ 0x801158A8, njRotateY 0x80115828, njRotateX 0x80115C60; the arch call is njSin 0x80112398) |
| Rings | o_ring_M1 | Pickup strictly within 14 of the collision centre |
| Goal | regular goal at (-4440, 967, -1133.4); the Rotation.x 1 goal is the child-task route | Native contact branch on a host-reported touch; writes slot word 2 |
| Death zones | PC_SA2/STG09.ini at 0xC4C6E4 | Volumes only |
| Checkpoints, springs | placements; `spring::StageSpring` | Checkpoint positions; spring launch vectors (njRotate); contact is not run |
| SIREN (7), FLUO (34), SEARCHLIGHT (12), SE, SIGNA | `prison_visuals` (SIREN 0x5A30/0x5B20, FLUO 0x418C/0x4260, SEARCHLIGHT 0x5708/0x57A0, SE 0x560C, SIGNA 0x58D8) | Per-frame state ported: siren spin (random start, +1120), fluorescent flicker (0.25 steps, rand timers), searchlight tilt (njSin sweep 0-45 degrees), emitter sound (9 or 0xD); sirens and lights keep their radius-5 spheres. SIGNA only draws. Random draws use MSL rand (GSNE8P 0x8010DC88, seed 1) in this session's call order, which differs from the game's global order |
| CAM security cameras (24) | `prison_cam` (0x2450 set-up, 0x25DC exec; lens child 0x28C8 / 0x2948) | Ported: a radius-8.5 body sphere (sec5+0x3A7C4); the lens (radius 4, sec5+0x3A7F4) sits at the model's child node (0, 0.8, 12.6) through njRotateY(Rotation.y). While a player's collision centre passes the range test (GSNE8P 0x800398D8, `player_range`: the length is Gekko's `frsqrte` then `fres` estimate) against 300, the head turns toward the player's position by trunc(10430.38043493439 * atan2f(dx, dz)), at most 128 BAMS a frame. A host-reported lens hit sets NextAction, and on the next frame the camera is destroyed (`Event::CamDestroyed`: the lens point 10 lower, velocity (0, 0.2, 0), sound 0x14). The lens takes an enemy block with group 0xFF, so cameras never count toward a door. The ground probe and the debris and explosion spawns are not run, and neither is the object range check (0x800655F4; the session does not stream SET objects) |
| EFFOBJ0 effect emitters (10) | `effect_emitter` (0x1CF60 set-up, 0x1CFB4 exec) | Ported: unless the host sets `Session::effects_suppressed` (the byte at GSNE8P 0x801CC168 + 0x37), each frame emits with probability ((Rotation.x >> 8) & 0xFF) / 255 by MSL rand. The velocity (Scale.y * (u - 0.5), Scale.x + Scale.y * (u - 0.5), Scale.y * (u - 0.5)) is turned by njRotateY(Rotation.y) then njRotateZ(Rotation.z) through njCalcVector (`ninja_matrix`, copied unchanged from #426, which matches main.dol on 1,500 samples), and the size is min(0.05 + Scale.z, 20) times 0.9 to 1.1. Rotation.x's low byte picks one of 18 spawners (jump table sec5+0x54B1C), reported in `Event::Effect`; the spawned effects are not run |
| CHAOPIPE (3) | `chao_pipe` (0x18A68 set-up, 0x18B18 exec) | Ported: the range test radius is 25 for the walkers (player 0's character 6 or 7, 0x80038108) and 13 otherwise. A player in range whose task Action (`PlayerSample::action`) is 0x2A starts a 300-frame timer and the squash spring (speed 0.08; each frame squash += speed, speed = 0.95 * speed - 0.15 * squash); a player in range without it gets the action requested (byte +0xD of its 0x801E7728 entry, `PipeEvent::RequestAction`). At 270 a chao of kind (u32)Scale.x % 3 is released at (0, 3, 2) through njTranslate, njRotateZ, X, Y and njCalcPoint (matches main.dol on 1,500 samples) with sound 0x100A, and the pipe is used up. The chao (0x800260FC) is not run, and the form-6 collision row (sec5+0x51574) is not built |
| SOLIDBOX (6) | `solid_box` (0x3AD5C set-up, 0x3AE5C exec); stack registry 0x8F04/0x9154, collapse 0x92C4 | Ported: a 20-unit cube (sec5+0x8E484) placed by the CCL box rule (`Session::solid_box_solid`). `Session::collapse_stack` is 0x92C4, which a breaking container or ORI runs at its position: each box higher than that point plus 11 and within 8 horizontally gets 20 more to fall. A falling box steps by its speed (or the rest of the fall), speed = 0.99 * speed - 0.08, and when it stops within 5 of its ground it raises dust (`Event::SolidBox`). The ground height is fn_80067AF0 (`land_probe::ground_height`; its ray test fn_800D05D0 matches main.dol on 1,500 node sets), which the host supplies as `Session::ground_probe` from its land collision. Containers register in the same stack, but their own fall and their breaking (walker attacks) are not ported |
| ORI cage block (1) | `ori` (0x31058 set-up, 0x311D4 exec) | Ported: the same 20-unit cube and fall as SOLIDBOX, in the same stack (`Session::ori_solid`). A hit (host-reported: `Session::ori_attacks` for the attack query 0x800060EC(task, 0x13), `Session::ori_rockets` when a rocket links to it) waits trunc(max(distance - 18, 0)) frames for a rocket (0x8011B65C, matched against main.dol on 4,000 samples) or 1, plays sound 0x1011 (variant = break count mod 5) and 0x14, collapses the stack above on its second frame, and from its fourth throws 16 + 4 + 4 debris pieces (unless (Rotation.x & 0xF) % 3 is 2) and 14 sparks drawn from MSL rand, then goes. The debris and spark tasks themselves are not run |
| BIG THE CAT | 0x33E3C | The GameCube module's callback does nothing |

Everything else is listed by name in `Session::unimplemented`: item boxes,
enemies, KNUDAI, ROCKETMISSSILE and MINIMAL. The colour blend those objects
share (GSNE8P 0x800334B0) is ported as `color_blend::blend_argb`. What holds
the rest back, as read from the code:

* ROCKETMISSSILE (0x35AC8, a rocket the player rides, registered as a
  dynamic land object) stays in its waiting state until an ORI with its id
  (Rotation.z & 0x1F) is in the table at sec5+0x88204 (ORI adds itself
  through 0x35A1C), or until its enemy group is clear (0x80018E30) with a
  player within 100, depending on Rotation.x & 3. Its exec (about 1,400
  instructions) is not ported yet.
* MINIMAL's callback is in the main program (GSNE8P 0x80025C7C): it spawns
  the small animals, which walk with ground physics.
* KNUDAI (0x2979C) is the Mystic Melody shrine: a dynamic land model
  (sec5+0x62D98, yaw only), a range of 38, the player actions 0x2D to 0x30,
  the hint messages (0x800E0118) and the melody bit at 0x803ADC14 (bit
  Rotation.x & 7).

## Offline verification

`cargo test --release --test prison_lane_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set:

| Check | Result on the pinned Steam build |
| --- | --- |
| Installed mover rules and inventory | All constants above verified; 15 lifts, 2 elevators, 2 bridges, 3 checkpoints, 1 live goal, 17 PANEL, 9 CCUBE, 21 fence boxes, 64 rings; stg11D.rel is rejected for this layout |
| Standing on the one-way diagonal elevator | Starts on frame 183, arrives at its Scale target and stops for good; the bridge that extended toward it on frame 1 stays out (its 120-frame cooldown was not met) |
| Ring and goal contact | Rings within 14 of the centre are taken for 10 points each; a host-reported goal touch plays sound 0x1013 and writes slot word 2 |
