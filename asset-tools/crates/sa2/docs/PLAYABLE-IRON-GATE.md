# Playable Iron Gate (Dark story, first stage)

Iron Gate is the first stage built out to play in the Rust runtime. It was
picked because it is the smallest walker stage with a regular goal ring: of
the 40 story levels with mapped terrain, it and Prison Lane have the fewest
collision triangles (2,756 here). `play_session::Session::iron_gate` runs it at 60 Hz from the
mounted Dark story dispatch 1 (Eggman, PhysicsArray slot 7).

## Play it

From `asset-tools/crates/sa2-play`, with the Steam game folder and SA Tools
786's `GameConfig/PC_SA2` folder:

```powershell
cargo run --release -- $game $pcSa2Config
```

WASD or the arrow keys move relative to the camera, Space jumps, Q/E turn
the camera and C recentres it. The title bar shows time, rings, score and
the latest event. Esc quits. Nothing is written to disk.

`--headless OUT.json [MAX_FRAMES [CHECKPOINT]]` runs the offline autopilot
instead and writes a report (plans, events, a once-a-second track, mover and
goal state). It never opens a window.

## What is native and what is adapted

| System | Source | Status |
| --- | --- | --- |
| Terrain | Mounted `objLandTable0011` Basic collision (`collision::bake_stage`), solid and non-dynamic surfaces | Native geometry. The queries are new: vertical ground probes and sphere push-out over an XZ grid (`play_world`). They do not run retail CCL code. |
| Start | Single-player start row for Eggman, level 11 | Native |
| Walker parameters | PhysicsArray slot 7: run_accel, rush_speed, air_accel, slow_down, jmp_y_spd, jmp_addit, jump2_timer, weight, lim_v_spd, rad, height, center_height | Values native; the controller that uses them is an adaptation (below) |
| Lifts (MLIFT, 5) | stg11D.rel section 1 0x3214/0x35DC | Ported: rise of `20 + Scale.x` after 30 frames standing, 0.5 units per frame, return after 90 unoccupied frames. `iron_gate_movers::MoverProgram::verify` checks the module digest, object rows, instruction words and relocated constants. |
| Elevators (ELV, 3) | stg11D.rel 0x1A84/0x1F7C | Ported: start when stood on with the 120-frame ready counter, doors close at 0.2 per frame from -9.5, travel to the SET Scale vector at 0.3 units per frame, one-way when Rotation.x bit 0 is set |
| Bridges (BRIDGE, 2) | stg11D.rel 0x2F8/0x688 | Ported: extend by -140 along local X at 8 units per frame while a non-moving elevator is within 450 of the extended point, retract otherwise, 120-frame cooldown |
| Object collision | PC_SA2/STG11.ini models at RVAs 0x7F0A68 (lift), 0x7F58A8 (bridge), 0x7FED08 (elevator) | Native Basic models, placed by SET position and Y angle |
| Springs (SPRA, SPRB) | Existing `spring::StageSpring` (GSNE8P o_spring.c, C-verified) | Native callback. The launch vector uses the documented Z, X, Y rotation order. Contact distance is an adaptation (10 plus the walker radius) because shape ID 4's size is not read. |
| Rings (RING) | o_ring_M1 contract | Pickup strictly within 14 units of the collision centre; AddScore(10) and AddNumRing(1) |
| Goal ring | `goal_ring::initialize` and `goal_ring::Contact` with the stg11D.rel program | Native initialization route and contact branch. Only the regular goal (Rotation.x 0, route Contact) is live; the Rotation.x 1 goal takes the child-task route. Contact is the regular 25-unit sphere against the walker centre plus radius. |
| Mission clear | Goal contact writes 2 to the contacting player's stage-module slot word (`goal_ring::apply_slot_writes`) | The session treats that word as the clear. The stage task that consumes it and the results screen are not run. |
| Death zones | PC_SA2/STG11.ini table at RVA 0x8040D0 (`death_zones::read`) | Native volumes; inside test is an upward ray parity test. A miss respawns at the last checkpoint with rings reset. |
| Checkpoints (SAVEPOINT, 3) | Placement positions | Adaptation: 30-unit horizontal contact. The Checkpoint_Main callback is not run. |
| Iron and Chao containers | Placement positions | Adaptation: solid 20-unit cubes on the placement. The SET stacks them at exactly that pitch. Wooden containers are breakable in the game; breaking is not implemented, so they are left out instead of blocking. |

The controller is the main adaptation. It accelerates toward `rush_speed`
times stick magnitude by `run_accel`, slows by `slow_down`, jumps with
`jmp_y_spd`, adds `jmp_addit` for up to `jump2_timer` frames while the button
is held and rising, and applies `weight` as gravity. In the air the stick adds
`air_accel` and brakes velocity that opposes it by up to `air_break`. That gives a held-jump
peak of 56.52 units. Collision steps up 12 units, snaps to ground within 12
units while walking and stops at ceilings. No build-matched reference for
the walker controller exists here: the Steam executable's x86 text is
encrypted on disk and no retail trace has been captured, so none of these
choices are claimed as retail behavior. Walker hover, lock-on, lasers,
enemies, item boxes, doors, switches, dash panels, camera objects, sounds,
models and textures are not run (the headless report lists every unhandled
placement by name).

## Offline verification

`cargo test --release --test iron_gate_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set. The autopilot (`play_autopilot`)
plans over the live geometry plus ride and spring links and only produces
stick and jump input; the session moves the walker.

| Check | Result on the pinned Steam build |
| --- | --- |
| Installed mover rules and inventory | All constants verified; 5 lifts, 3 elevators, 2 bridges, 6 springs, 3 checkpoints, 10 rings, 1 live goal at (6231.24, -1488, -1229.35) |
| Spawn through the first third | Rides lift 9 at frame 733, the diagonal elevator at 1,774, the bridge extends at 3,336, reaches the SAVEPOINT at (2241, -663, -1027.5) at 3,747, no deaths |
| Last checkpoint to the goal | From the SAVEPOINT at (3691, -903, -685): elevator 4, bridge, goal contact with sound 0x1013, slot word 2, clear after 3,630 frames (60.5 s), no deaths |

**Not verified: the middle third** (from the SAVEPOINT at x 2241 to the one at
x 3691). The SET layout shows the authored route:

1. Drop from the -663 corridor onto the large pipe in the room at x 2780-2950
   (its top is at -694.5).
2. Spring #303 on the pipe lifts the walker toward the -601 ledge.
3. The three SPRB springs on that ledge (Rotation.x 42 degrees, Scale.y 3.9,
   73 no-control frames) throw it north under the roof slab. Their ring line
   runs at y -538.
4. It lands on a -763 floor at about (2910, -763, -60..-100), which carries two
   dash panels (KASOKU).
5. From there the route drops east to -803, -853 and -903.

The planner now reaches the pipe and the springs. Every spring flight it can
simulate misses the -763 floor, though, overshooting north by 40 to 90 units
with or without air braking. The dash-panel floor is also a dead end without
a dash panel port. Both need the player's air and boost physics, for which no
reference exists here. The pipe climbs near (2727, -663, -428) that static
reachability also offers sit above a death pit and are not the authored route.
