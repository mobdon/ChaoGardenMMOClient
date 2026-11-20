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
| Springs (SPRA, SPRB) | `spring::StageSpring` tasks and native launch vectors; contact is not run |
| Rings | o_ring_M1: pickup strictly within 14 of the collision centre, AddScore(10), AddNumRing(1) |
| Goal ring | `goal_ring` initialization and Contact branch; 25-unit sphere against the collision centre plus radius; contact writes slot word 2, treated as the clear |
| Death zones | PC_SA2/STG11.ini RVA 0x8040D0 (volumes only; entering one is not run) |
| Checkpoints | Placement positions only |

`MoverProgram::verify` checks the module digest, object rows, instruction
words and relocated constants. Containers, doors, switches, enemies, item
boxes and everything else are listed by name in `Session::unimplemented`.

## Offline verification

`cargo test --release --test iron_gate_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set: the installed mover rules and
inventory, a lift raised by standing on it (starts on frame 31 and ends at
its native top), and goal contact (sound 0x1013, slot word 2, clear).
