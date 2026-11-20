# Playable Prison Lane (Hero story, Tails' walker)

Prison Lane is the second stage built out to play in the Rust runtime. It
runs on the same session, controller, collision queries and autopilot as
[Iron Gate](PLAYABLE-IRON-GATE.md). `play_session::Session::prison_lane`
builds it from the mounted Hero story dispatch 9 (level 9, MechTails,
PhysicsArray slot 6); `play_session::PRISON_LANE` holds the per-stage native
inputs.

## Play it

From `asset-tools/crates/sa2-play`:

```powershell
cargo run --release -- $game $pcSa2Config --stage prison-lane
cargo run --release -- $game $pcSa2Config --stage prison-lane --headless out.json
```

Controls and the headless report are the same as Iron Gate's.
`SA2_PLAY_TRACK_EVERY=N` samples the report's track every N frames instead
of once a second.

## What differs from Iron Gate

Both stage modules build the MLIFT/ELV/BRIDGE family from the same source.
`iron_gate_movers::MoverLayout` names each module's object rows, callbacks,
checked instruction words and the `lis` sites of every constant, and
`MoverProgram::verify_layout` refuses a module whose digest, rows, words or
constants differ. Offsets for stg09D.rel were found by matching
normalized instruction windows from stg11D.rel and then read directly.

| Rule | stg11D.rel (Iron Gate) | stg09D.rel (Prison Lane) |
| --- | --- | --- |
| Lift rise, speed, timers | 20 + Scale.x, 0.5/f, 30 / 90 frames | same (0x4704, exec 0x4AA0) |
| Elevator travel | 0.3 per frame | 0.6 per frame (sec5+0x3F554) |
| Elevator inner door (task 0x44) | starts -9.5, closes/opens 0.2/f | same |
| Elevator outer door (task 0x48) | starts -9.5, opens to -9.5, closes 0.2/f | starts 0 (shut), opens to -36.3 at 0.2/f, closes 0.32/f |
| Bridge | -140 local X, 8 per frame, elevator within 450 | -140, 1.5 per frame, 450 |

The elevator's idle code opens both doors while the car rests at its
placement and only the outer door at the far end; it tests occupancy only
when the doors it opens were already open. In Prison Lane that means a
newly placed elevator needs 182 frames before it can start, and closing the
outer door (114 frames) gates travel. `Elevator` now models both doors; for
Iron Gate, whose doors share one constant set, the timing is unchanged.

| System | Source | Status |
| --- | --- | --- |
| Terrain | `objLandTable0009`, `collision::bake_stage` | Native geometry, same queries as Iron Gate |
| Object models | PC_SA2/STG09.ini: lift 0xC3C538, bridge 0xC42EA0, elevator 0xC39660, panel 0xC40390 | Native Basic models |
| PANEL (17) | stg09D.rel 0x2A4C passes its model to the static-collision helper 0x9A4C with flags 6 (Y rotation kept, X and Z dropped) | Ported as static solids. They floor the gap past the first lift room. |
| CCUBE (17) | stg09D.rel 0x11CDC: CollisionData form 3 (sec5+0x4C080), sizes set by 0x11B30 to 10 + Scale per axis | Ported as solid boxes with half-extents 10 + Scale, centred on the placement. Adaptation: only Y rotation is applied. The box beside lift 16 tops out at exactly the lift's raised height (838). Not enabled for Iron Gate, whose module was not checked. |
| Death zones | PC_SA2/STG09.ini at 0xC4C6E4 | Native volumes |
| Goal | regular goal at (-4440, 967, -1133.4); the Rotation.x 1 goal is the child-task route | Native contact; slot word 2 is the clear |
| Checkpoints, springs, containers | as Iron Gate | as Iron Gate |

### Not run

Doors and switches are not run, so every door is effectively open. In the
module, CAGED (sliding prison door, exec 0x2000) opens or closes from an
engine query (0x80018E30) on its Rotation.x switch number; what sets those
switches in Prison Lane (enemies, lock-on volumes, the TANK or CAM objects)
was not traced. The cell door on the route near (-3329, 730, 130) is one of
them, so in the game the route there probably depends on clearing the area
first. WALL, CAGE, CAGEB and CAGEC, FENCES and FENCEL, MECHA fuse boxes,
TANK, ROLL, SPHERE and CCYL solids, RING_LINEAR rings (the stage has no
single RING placements, so the ring count stays 0), enemies, lock-on and
the walker's laser are not run either. The headless report lists every
unhandled placement by name.

## Offline verification

`cargo test --release --test prison_lane_play -- --ignored` with
`SA2_GAME_PATH` and `SA2_PC_CONFIG` set.

| Check | Result on the pinned Steam build |
| --- | --- |
| Installed mover rules and inventory | All constants above verified; 15 lifts, 2 elevators, 2 bridges, 3 checkpoints, 1 live goal, 17 PANEL and 17 CCUBE solids; stg11D.rel is rejected for this layout |
| Spawn to goal | Checkpoint (-1235, 237, 40) at frame 3,107; the one-way diagonal elevator at 4,442; checkpoints (-2476, 630, -615) at 5,514 and (-3910, 870, -240) at 8,009; goal contact with sound 0x1013 and slot word 2 at 9,071 (151.2 s). No deaths. |

`examples/walker_probe.rs` (static reachability from the start and each
checkpoint to every checkpoint, mover and the goal) and
`examples/walker_slice.rs` (a top-down text map or per-cell surface list)
are the tools used to find where the route broke.

### Autopilot changes

The autopilot only supplies stick and jump input, but several of its
choices were too loose for this stage. These also apply to Iron Gate, whose
offline tests still pass:

- It no longer skips a ride when it passes over the boarding point in the
  air; it lands on it first.
- A level gap is a jump only where the floor actually breaks, the jump is a
  tap rather than a hold, and it is taken only from the near side once the
  walker faces the target.
- It jumps rises above 6 units, because the session's wall sphere starts
  half a step above the feet and stops a walking body at about that height.
- It looks one waypoint ahead only when that shortcut stays over floor.
- The planner adds a cost to climbs within 20% of the jump limit, so a lift
  that reaches the same place is preferred.
