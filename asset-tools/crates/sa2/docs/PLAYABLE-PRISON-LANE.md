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
| RING_LINEAR (17 lines) | 0x1B448, count helper 0x1B1E8, placement helper 0x1B21C | The 15 flat lines turned only about Y (0x801154E0, the goal ring spin call): 56 rings, Scale.z clamped to 1..=8, spaced 10 + Scale.x along local -Z |
| Rings | o_ring_M1 | Pickup strictly within 14 of the collision centre |
| Goal | regular goal at (-4440, 967, -1133.4); the Rotation.x 1 goal is the child-task route | Native contact; slot word 2 is the clear |
| Death zones | PC_SA2/STG09.ini at 0xC4C6E4 | Volumes only |
| Checkpoints, springs | placements; `spring::StageSpring` launch vectors | Data only; contact is not run |

Everything else is listed by name in `Session::unimplemented`, including
the rotated CCUBE and fence placements, the angled fence rows (form 2), the
arched and tilted ring lines, CAGED and CAGE/CAGEB/CAGEC (gated on the
engine switch query 0x80018E30), WALL (player-distance logic), MECHA, ROLL,
TANK, SPHERE, CCYL, containers, item boxes and enemies.

## Offline verification

`cargo test --release --test prison_lane_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set:

| Check | Result on the pinned Steam build |
| --- | --- |
| Installed mover rules and inventory | All constants above verified; 15 lifts, 2 elevators, 2 bridges, 3 checkpoints, 1 live goal, 17 PANEL, 9 CCUBE, 21 fence boxes, 56 rings; stg11D.rel is rejected for this layout |
| Standing on the one-way diagonal elevator | Starts on frame 183, arrives at its Scale target and stops for good; the bridge that extended toward it on frame 1 stays out (its 120-frame cooldown was not met) |
| Ring and goal contact | Rings within 14 of the centre are taken for 10 points each; goal contact plays sound 0x1013, writes slot word 2 and clears |
