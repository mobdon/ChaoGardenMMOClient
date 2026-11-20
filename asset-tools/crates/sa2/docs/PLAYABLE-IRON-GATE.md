# Iron Gate native stage rules (Dark story, first stage)

`play_session::Session::iron_gate` builds Iron Gate (Dark story dispatch 1,
level 11, MechEggman, PhysicsArray slot 7) from the installed game files
and runs only rules read from them. It is not playable: the walker's own
movement, collision response, lock-on and laser are main-program code,
which is SteamStub-encrypted on disk in the Steam executable, and no
GameCube main program is available. The adapted controller, collision
queries, headless autopilot, window, container cubes and the contact radii
that were not read have been removed.

A host supplies a `PlayerSample` each frame (the player's collision centre
and the mover the engine says it stands on), and `Session::step` runs the
native object rules against it. See
[PLAYABLE-PRISON-LANE.md](PLAYABLE-PRISON-LANE.md) for the shared design.

Rotations use the GameCube main program's own trigonometry (`ninja_trig`, read from the GSNE8P main.dol of the person's disc; every PC stage-module import maps onto it): njRotateX/Y/Z convert the angle to radians and call MSL sinf/cosf, and njSin/njCos read the start-up table. The sines and cosines are bit-for-bit ports; the matrix products that combine them are evaluated as plain single-precision steps rather than in the paired-single order of the SDK matrix routines, so a result can differ in its last bit. Lift, elevator and
bridge collision objects keep the placement's Y rotation only (stg09D.rel
0x4930, 0x30CC, 0x510; stg11D.rel likewise), and the elevator's object sits
40 below its placement (checked constant in both modules). Goal contact is
reported by the host (`PlayerSample::touching_goal`), and the slot word the
goal ring writes is exposed as is rather than interpreted as a clear.

## Native rules

| System | Source |
| --- | --- |
| Terrain | Mounted `objLandTable0011` Basic collision, solid and non-dynamic surfaces (geometry only) |
| Start | Single-player start row for Eggman, level 11 |
| Walker parameters | PhysicsArray slot 7 (values only) |
| Lifts (MLIFT, 5) | stg11D.rel 0x3214/0x35DC: rise `20 + Scale.x` after 30 standing frames at 0.5 per frame, return after 90 unoccupied frames |
| Elevators (ELV, 3) | stg11D.rel 0x1A84/0x1F7C: both doors -9.5 and 0.2 per frame, 120-frame ready counter, travel to the SET Scale vector at 0.3 per frame, one-way when Rotation.x bit 0 is set |
| Bridges (BRIDGE, 2) | stg11D.rel 0x2F8/0x688: extend -140 along local X at 8 per frame while a resting elevator is within 450 of the extended point, 120-frame cooldown |
| Object collision models | PC_SA2/STG11.ini RVAs 0x7F0A68 (lift), 0x7F58A8 (bridge), 0x7FED08 (elevator) |
| Iron containers (CONTIRON, 7) | stg11D.rel 0x20714 registers CollisionData sec5+0x656D8 through 0x8000F7D8: form 3, centre (0, 10, 0), half-extents 10, a 20-unit cube; all 7 placements are built |
| Chao and wooden containers (CONTCHAO 3, CONTWOOD 27) | 0x1FDA4 and 0x2132C register the same cube rows (sec5+0x649E0, +0x666C8); all built as solids, breaking (player attacks) not run |
| Fuse boxes (MECHA, 13) | 0x53E4 registers sec5+0x51198: form 3, centre (0, 20, 2), half-extents (10, 20, 5); its exec only runs that collision. The 4 placed at yaw 0 are built; the centre is off the Y axis, so the others wait on the rotation rule |
| Springs (SPRA, SPRB) | `spring::StageSpring` tasks and launch vectors (njRotate); contact is not run |
| Rings | o_ring_M1: pickup strictly within 14 of the collision centre, AddScore(10), AddNumRing(1) |
| Goal ring | `goal_ring` initialization and Contact branch, run on a host-reported touch; contact writes slot word 2 |
| Death zones | PC_SA2/STG11.ini RVA 0x8040D0 (volumes only; entering one is not run) |
| Checkpoints | Placement positions only |

`MoverProgram::verify` checks the module digest, object rows, instruction
words and relocated constants. Doors, switches, enemies, item boxes and
everything else are listed by name in `Session::unimplemented`.

## Offline verification

`cargo test --release --test iron_gate_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set: the installed mover rules and
inventory, a lift raised by standing on it (starts on frame 31 and ends at
its native top), and a host-reported goal touch (sound 0x1013, slot word 2).
