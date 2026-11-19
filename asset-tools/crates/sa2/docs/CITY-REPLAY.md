# City Escape callback replay inputs

The stage host resolves a placement from one of the five installed City Escape SET files and its row from the verified Steam executable's stage 13 object list. Dispatch requires all three fields below, including ASCII spaces in the name. A replay creates fresh callback state and emits observed calls to the host. It does not invoke the game's collision engine, audio mixer, player physics or save service.

| SET ID | Native name | PC function VA | Modeled callback | Placements across five sources |
|---:|---|---|---|---:|
| 0 | `RING   ` | 0x6C0F80 | Ring | 104 |
| 3 | `SPRA   ` | 0x6C4480 | ObjectSpring | 2 |
| 4 | `SPRB   ` | 0x6C4E90 | ObjectSpringB | 27 |
| 59 | `BOARDCOL` | 0x5EA210 | ObjectBoardCol | 13 |
| 81 | `ADXCHG` | 0x5E2D70 | ObjectCeAdxChg | 2 |

The trace input is at most 1 MiB. Placement replays accept 1–2,048 frames. The callback implementations use reconstructed GameCube C. Matching names and PC table addresses identify the Steam placements; retail PC instruction parity is unverified. Per-callback reference traces and documented approximations are in STAGE-GAMEPLAY.md, STAGE-SPRING.md, STAGE-BOARD.md and CITY-MUSIC.md.

**Ring.** Every frame supplies the resolved spin gate, range result and each present player's actual item bits. Set `ccl_hit_slot` to null for no collision, 0 or 1 for a resolved player contact, or 2 for a CCL task that does not resolve to either player. A null item slot means that player is absent. The stage callback takes one tick to initialize. Coordinates are never used to infer CCL contact. Fixed-ring replays reject a Scale.X of -1, any range-out tick and either player wearing the magnetic shield. The calls record collection, collision registration, particles and task removal; rendering and shadow initialization are outside the trace.

```json
{"frames":[
  {"range_out":false,"spin_enabled":true,"player_items":[0,null],"ccl_hit_slot":null},
  {"range_out":false,"spin_enabled":true,"player_items":[0,null],"ccl_hit_slot":0},
  {"range_out":false,"spin_enabled":true,"player_items":[0,null],"ccl_hit_slot":null}
]}
```

**Springs.** Supply the native matrix output point for the selected placement and the resolved CCL hit slot if any. A player observation in Touch mode is for the slot the spring selected. For a launch trace use the actual matrix and player observations. The example initializes the callback only.

```json
{"out_point":[0,0,0],"frames":[{"range_out":false,"hit":null,"player":null}]}
```

**Board collision.** Supply positions in native stage units and player task mode bytes. Both slots can be absent. The first in-range tick installs the executor. The required request field `two_player` is the observed engine global that controls latch rearming. Supply it independently of the SET filename.

```json
{"two_player":false,"frames":[{"range_out":false,"players":[null,null]},{"range_out":false,"players":[{"position":[0,0,0],"mode":76},null]}]}
```

**ADX change.** The bind step requires the engine's Downtown Race (`STAGE_SHORTCITY_2P`) predicate explicitly. A two-player SET filename alone does not establish it. The tick takes the native either-player sphere result using the selected placement's center and Scale.X. Supply the unnamed destructor predicate from fn_8001CC18(0). An out-of-range or Downtown Race bind leaves the task unbound. Tick and destruction require a bound task.

```json
{"frames":[{"op":"bind","range_out":false,"short_city_2p":false},{"op":"tick","range_out":false,"player_in_sphere":true,"game_state":16},{"op":"destroy","game_state":7,"predicate_8001cc18":false}]}
```

**Stage music.** The independent `replay_bgm(request)` entry point uses the required `two_player` engine global from the request to invoke the music portion of the stage prolog, then runs the scheduled BgmStart task for each provided frame. Use an empty array to inspect just the prolog. Nonempty frames require a scheduled BgmStart task.

```json
{"two_player":true,"stage_variant":0,"frames":[{"game_state":16,"player_modes":[0,0]}]}
```

For installed dispatch validation, set `SA2_SET_DIR` to `resource/gd_PC` and run `cargo test --locked --test city_replay -- --include-ignored`. The test verifies the executable hash, reads the native table and parses all five files, then uses synthetic observations for every supported placement. For board triggers the test supplies the opposite engine state from the source layer label and checks repeated contact on the third tick.
