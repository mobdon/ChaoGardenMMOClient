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
| Springs (SPRA, SPRB) | Existing `spring::StageSpring` (GSNE8P o_spring.c, C-verified); contact shapes from stg11D.rel CCL_INFO at section 5 0xA3574 / 0xA35A4 | Native callback and shapes: SPRA is a cylinder of radius 6 and height 10, SPRB a sphere of radius 14. The launch vector uses the documented Z, X, Y rotation order. The player's own CCL shape is in the main executable, so the walker radius stands in for it. |
| Invisible collision cubes (CCUBE, 3) | stg11D.rel section 1 0x17140 and helper 0x16F94; CCL_INFO at section 5 0x5E5B0 | Native: solid rectangle centred on the placement with half sizes 10 + Scale.x, y, z (form 3, push 0x77) |
| Dash panels (KASOKU, 12) | stg11D.rel section 1 0x1B4C0; CCL_INFO at section 5 0x60CA0 | Read, not run. On contact with a grounded player the panel passes local velocity (Scale.x or 14, 0, 0), its rotation and ceil(Scale.y) (or 60) no-control frames to a player routine in the main executable. How that routine maps local velocity and rotation to the world is not readable here, so the panels are reported only. |
| Rings (RING) | o_ring_M1 contract | Pickup strictly within 14 units of the collision centre; AddScore(10) and AddNumRing(1) |
| Goal ring | `goal_ring::initialize` and `goal_ring::Contact` with the stg11D.rel program | Native initialization route and contact branch. Only the regular goal (Rotation.x 0, route Contact) is live; the Rotation.x 1 goal takes the child-task route. Contact is the regular 25-unit sphere against the walker centre plus radius. |
| Mission clear | Goal contact writes 2 to the contacting player's stage-module slot word (`goal_ring::apply_slot_writes`) | The session treats that word as the clear. The stage task that consumes it and the results screen are not run. |
| Death zones | PC_SA2/STG11.ini table at RVA 0x8040D0 (`death_zones::read`) | Native volumes; inside test is an upward ray parity test. A miss respawns at the last checkpoint with rings reset. |
| Checkpoints (SAVEPOINT, 3) | Placement positions | Adaptation: 30-unit horizontal contact. The Checkpoint_Main callback is not run. |
| Iron and Chao containers | stg11D.rel CONTIRON section 1 0x20714; CCL_INFO at section 5 0x656D8 | Native shape: a 20-unit cube standing on the placement (rectangle a = b = c = 10, centre 10 up). Their fall and being broken are not run. Wooden containers break under player attacks, which live in the main executable, so they are left out instead of blocking. |

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
x 3691). The SET layout and the stage module show the authored route:

1. Walk over the collision cube in front of the pipe room (CCUBE at
   (2719.5, -695, -670), half sizes 60, 30, 40) onto the large pipe.
2. Spring #303 on the pipe lifts the walker onto a -651 floor.
3. An iron container sits on the SPRA spring at (2909, -651, -715). The Laser
   Blaster upgrade, collected in the side room at x 2770, is the likely way to
   clear it.
4. That spring reaches the -601 ledge. The three SPRB springs there
   (Rotation.x 42 degrees, 73 no-control frames) throw the walker north under
   the roof slab, along a ring line at y -538.
5. It lands on the -763 floor at about (2910, -763, -60..-100), where two dash
   panels send it east and down to -803, -853 and -903.

With native code only, the planner reaches the pipe beside spring #303, and
the simulated launch from that spring lands on the -651 floor at about
(2944, -651, -703). Three steps past that depend on player code in the main executable,
which is encrypted on disk in the Steam build: breaking the iron container
(player weapons), the walker's air physics during the 73-frame spring flights
(every simulated flight overshoots the -763 floor by 40 to 90 units), and how
dash panels move the walker. The pipe climbs near (2727, -663, -428) that
static reachability also offers sit above a death pit and are not the
authored route.
