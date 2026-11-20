# Iron Gate native rules (Dark story, first stage)

`play_session::Session::iron_gate` builds Iron Gate (level 11, Eggman,
PhysicsArray slot 7) from the mounted Dark story dispatch and runs only rules
read from the game files. It was picked as the smallest walker stage with a
regular goal ring among the 40 story levels with mapped terrain.

**Not playable.** The walker's movement and its collision response are code
in the main program. The Steam executable's code section is encrypted on
disk, so that code is not readable here and nothing in this session moves the
player. A host supplies a `PlayerSample` each frame instead: the player's
collision centre, the mover it stands on, and a goal-ring CCL hit, as the
engine would report them.

## What runs and where it comes from

| System | Source |
| --- | --- |
| Terrain and object geometry | Mounted `objLandTable0011` Basic collision; lift, bridge and elevator models at PC_SA2/STG11.ini RVAs 0x7F0A68, 0x7F58A8, 0x7FED08; death-zone volumes at RVA 0x8040D0. Geometry only: no collision queries run. |
| Lifts (MLIFT, 5) | stg11D.rel section 1 0x3214/0x35DC: rise of `20 + Scale.x` after 30 standing frames at 0.5 units per frame, return after 90 unoccupied frames. |
| Elevators (ELV, 3) | stg11D.rel 0x1A84/0x1F7C: start when stood on with the 120-frame ready counter; doors close at 0.2 per frame from -9.5; travel to the SET Scale vector at 0.3 units per frame; one-way when Rotation.x bit 0 is set. |
| Bridges (BRIDGE, 2) | stg11D.rel 0x2F8/0x688: extend -140 along local X at 8 units per frame while a non-moving elevator is within 450 of the extended point, otherwise retract; 120-frame cooldown. |
| Iron containers | stg11D.rel CONTIRON 0x20714, CCL_INFO at section 5 0x656D8: a 20-unit cube standing on the placement. The shape is added to the geometry; its fall and breaking are not run. |
| Rings (RING) | o_ring_M1 contract: pickup when the collision centre is strictly within 14 units; AddScore(10) and AddNumRing(1). |
| Goal ring | `goal_ring::initialize` and `goal_ring::Contact` with the stg11D.rel program. Only the regular goal (Rotation.x 0) reaches the contact route. A reported CCL hit takes the 0x1013 branch, which writes 2 to the player's stage-module slot word; the session reports that handoff as `Cleared`. The stage task that consumes the word and the results screen are not run. |

`MoverProgram::verify` checks the module digest, object rows, instruction
words and relocated constants every mover rule depends on.

Listed but not run: springs (their callback is `spring::StageSpring`, but the
CCL hit that drives it is engine work), checkpoints, and every other
placement, which the session counts by name in `unimplemented`.

## Verification

`cargo test --release --test iron_gate_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set:

- The installed mover constants match, and the inventory is 5 lifts,
  3 elevators, 2 bridges, 6 springs, 3 checkpoints, 10 rings and 1 live goal
  at (6231.24, -1488, -1229.35).
- Standing on the lift at (800, -270, -638) starts it on frame 31, and it
  stops with its top at -200. Standing in the diagonal elevator starts it; it
  comes to rest at its target one-way, and the bridge at x 2127 then extends.
- A sample 13.9 units from a ring picks it up once (rings 1, score 10). A goal
  hit takes the 0x1013 route and writes slot word 2.
