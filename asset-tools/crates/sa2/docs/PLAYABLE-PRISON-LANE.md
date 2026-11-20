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

Rotations use the GameCube main program's own trigonometry (`ninja_trig`, read from the GSNE8P main.dol of the person's disc; every PC stage-module import maps onto it): njRotateX/Y/Z convert the angle to radians and call MSL sinf/cosf, and njSin/njCos read the start-up table. The sines and cosines are bit-for-bit ports; the matrix products that combine them are evaluated as plain single-precision steps rather than in the paired-single order of the SDK matrix routines, so a result can differ in its last bit. Lift, elevator and
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
| CCUBE (17) | stg09D.rel 0x11CDC: CollisionData form 3 (sec5+0x4C080), sizes 10 + Scale per axis (helper 0x11B30) | The 9 whose shape cannot depend on the unread rotation rule (no X/Z rotation, yaw 0 or a half turn) |
| FENCES (39), FENCEL (4) | 0x2A94 / 0x2AF8 register CollisionData rows sec5+0x3C54C (1) and +0x3C57C (3) via helper 0x2B5C, read at load by `play_objects::collision_rows` | Form 3 boxes by the same rule: 20 FENCES, 1 FENCEL |
| CONTIRON (18) | 0x1978C registers CollisionData sec5+0x52A18 through 0x8000F7D8: form 3, centre (0, 10, 0), half-extents 10 (a 20-unit cube; Iron Gate's row is identical) | The 11 placed at a whole quarter turn, since a cube square in XZ is unchanged by those |
| CONTCHAO (3), CONTWOOD (17) | 0x18E1C and 0x1A3A4 register the same cube rows (sec5+0x51D20, +0x53A08); breaking (player attacks) is not run | All 3 CONTCHAO; the 14 CONTWOOD at quarter turns |
| MECHA (12) | 0x527C registers sec5+0x41DB4: form 3, centre (0, 0, 2), half-extents (10, 10, 5); its exec only runs that collision | The 2 at yaw 0 (the centre is off the Y axis) |
| Enemy groups | GSNE8P 0x80018C88/0x80018C10 (count up), 0x80018B88/0x80016598 (count down), 0x80018E30 (query: count is zero); stg09D.rel E KUMI 0x4C848 (not counted when its kind, Scale.x truncated, is 4), E EMI 0x4E900, E AI 0x513D4, E NAMIE 0x54080, E GOLD 0x4A3D4 | Group = low byte of Rotation.y. Counts are built from the SET; `Session::destroy_enemy` reports a kill (the walker's attacks are not ported). Enemy behaviour itself is not run |
| CAGED sliding cell doors (11) | 0x1EB0 set-up, 0x2000 exec | Ported: closed until the group named by Rotation.x's low byte is clear, then slide open by Scale.x per frame to 50 (sound 0x1020 while sliding, 0x1021 at the stop), settle with velocity 0.5, pull 0.4 and keep 0.96, and snap shut if the group fills again. The collision box (form 3, centre (0, 30, +/-slide) by Rotation.z bit 0) is not built until the box rotation rule is read |
| RING_LINEAR (17 lines) | 0x1B448, count helper 0x1B1E8, placement helper 0x1B21C | All 17 (64 rings): Scale.z clamped to 1..=8, ring i at (0, Scale.y * njSin(i * (0x4000 / n)), -i * (10 + Scale.x)) under rotate Z, Y, X (GSNE8P njRotateZ 0x801158A8, njRotateY 0x80115828, njRotateX 0x80115C60; the arch call is njSin 0x80112398) |
| Rings | o_ring_M1 | Pickup strictly within 14 of the collision centre |
| Goal | regular goal at (-4440, 967, -1133.4); the Rotation.x 1 goal is the child-task route | Native contact branch on a host-reported touch; writes slot word 2 |
| Death zones | PC_SA2/STG09.ini at 0xC4C6E4 | Volumes only |
| Checkpoints, springs | placements; `spring::StageSpring` | Checkpoint positions; spring launch vectors (njRotate); contact is not run |

Everything else is listed by name in `Session::unimplemented`, including
the rotated CCUBE and fence placements, the angled fence rows (form 2),
CAGE/CAGEB/CAGEC (gated on the same group query as CAGED, not yet ported), WALL (player-distance logic), ROLL (an angled form 2 row and a
spinning model), TANK, SPHERE, CCYL, item boxes and enemies.

## Offline verification

`cargo test --release --test prison_lane_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set:

| Check | Result on the pinned Steam build |
| --- | --- |
| Installed mover rules and inventory | All constants above verified; 15 lifts, 2 elevators, 2 bridges, 3 checkpoints, 1 live goal, 17 PANEL, 9 CCUBE, 21 fence boxes, 64 rings; stg11D.rel is rejected for this layout |
| Standing on the one-way diagonal elevator | Starts on frame 183, arrives at its Scale target and stops for good; the bridge that extended toward it on frame 1 stays out (its 120-frame cooldown was not met) |
| Ring and goal contact | Rings within 14 of the centre are taken for 10 points each; a host-reported goal touch plays sound 0x1013 and writes slot word 2 |
