# Stand-in Sonic controller and City Escape session

`speed_controller` moves Sonic or Shadow on speed stages, and
`speed_session::SpeedSession::city_escape` runs City Escape (Hero story,
level 13) on it at 60 Hz. **The controller is not a port of the native
player code.** It is a labelled stand-in, built so that speed stages can run
end to end while a verifiable reference is missing.

## Why there is no native port yet

The player controller is engine code, not stage code:

| Source | What it holds |
| --- | --- |
| Steam `sonic2app.exe` | The PC player routines (for example PGetAcceleration at 0x45B610, PGetAccelerationAir at 0x45D770, PGetFriction at 0x45F4D0, per the mod-loader declarations). The `.text` section is SteamStub-protected (a `.bind` section is present; the bytes at those addresses have about 7.96 bits of entropy). This project does not decrypt or unpack it. |
| `resource/gd_PC/DLL/Win32/Data_DLL.dll` | Data. Its `.text` section is 0x8AA bytes and holds no player code. |
| `resource/gd_PC/*.rel` | GameCube stage, boss, kart and event modules. They call the player through the main executable but do not contain it. |
| GameCube `main.dol` (GSNE8P) | Where the GameCube player code lives. It is not part of the Steam install, and none was found on this machine. |
| roeming/SA2B at 874a480 | No player source. `config/GSNE8P/symbols.txt` names a few helpers (GetPlayerRunningSpeed at 0x800380AC, the playerpwp/playertwp arrays). |

A `main.dol` dumped from a GameCube disc you own would make the native
routines executable under Unicorn, the same way the stage and boss work is
verified. The controller's public surface (`SpeedPhysics`, `SpeedPlayer`,
`Input`, `Terrain`, `step`) is the seam where a verified port would replace
the rules below.

## What is native

| Input | Source |
| --- | --- |
| Tuning | PhysicsArray row 0 (Sonic) or 1 (Shadow) via `player_physics::read`: jump2_timer 60, max_x_spd 3, jmp_y_spd 1.66, jog/run/rush/dash speeds 0.46/1.39/2.3/5.09, jmp_addit 0.076, run_accel 0.05, air_accel 0.031, slow_down -0.06, run_break -0.18, air_break -0.17, air_resist -0.008, air_resist_y -0.01, lim_h_spd/lim_v_spd 16, rad 4, height 10, weight 0.08, center_height 5.4 |
| Terrain | The mounted `objLandTable0013` Basic collision, as in the Iron Gate session |
| Start | The single-player start row (0, 300, 340) |
| Grind rails | The 12 City Escape paths whose callback is 0x4980C0 (PC_SA2/STG13.ini path list at RVA 0xCDB35C) |
| Death zones | PC_SA2/STG13.ini table at RVA 0xCDC678 |
| Springs | `spring::StageSpring` (the C-verified callback), as in Iron Gate |
| Bush blocks (GREEN_B) | Basic collision model at RVA 0xCAA350, placed by SET position and Y angle |
| Goal ring | `goal_ring::initialize` and `Contact` with the stg13D.rel program; only the regular goal is live |
| Rings | o_ring_M1 pickup within 14 of the collision centre |

## What is adapted

Controller (each rule names the parameter it uses):

- **Ground.** Speed is a scalar along the facing, projected onto the ground
  plane. With the stick held it accelerates by `run_accel` (half above
  `rush_speed`) up to `max_x_spd` times the stick. Above that, only slopes and
  objects add speed. Drag is `air_resist` times speed. Gravity along the slope
  is `weight` times the slope's sine. Holding against the motion above
  `run_speed` brakes by `run_break`. Without the stick, `slow_down` stops the
  body, and slopes gentler than `slow_down` hold it. The turn rate falls from
  0.35 to 0.07 radians a frame as speed rises.
- **Jump.** Leaves along the ground normal at `jmp_y_spd` and adds
  `jmp_addit` while held, for up to `jump2_timer` frames while rising. Air
  drag is `air_resist` horizontally and `air_resist_y` vertically, and gravity
  is `weight`, capped at `lim_v_spd`. A held jump peaks at about 71 units.
- **Air.** Steering adds `air_accel` without exceeding the larger of the old
  speed and `max_x_spd`; holding against the motion brakes by `air_break`.
- **Spin (B while moving).** A ball keeps its speed: drag, a tenth of
  `slow_down`, and slopes. It ends below half `jog_speed`.
- **Spin dash (B held while standing).** Charges for 45 frames and releases
  at up to `dash_speed`.
- **Homing attack (A in the air after a jump).** Targets the nearest live
  enemy, item box or wooden container within 220 units, in front (cone cosine
  0.35) and at most 60 above. It flies at `dash_speed` for up to 60 frames and
  bounces up at 1.2 times `jmp_y_spd`. With no target it air-dashes forward at
  `dash_speed` or faster.
- **Rails.** A body falling or level within 7 units horizontally of a rail
  line, from 2 below to 9 above it, is captured. It keeps its momentum along
  the rail; half of gravity acts along the rail, plus drag. It leaves at either
  end or with A (a jump off the rail), and cannot be recaptured for 20 frames.
- **Collision.** Vertical ground probes and sphere push-out over the native
  triangles. Unlike Iron Gate, a wall only pushes from its front side. City
  Escape needs this: the board-section ramp ends in a 20-unit back-facing lip
  at z 9100 that would otherwise stop a runner dead.

Objects:

| Object | Rule |
| --- | --- |
| KASOKU (82 dash panels) | Contact within 12 + radius horizontally and 12 vertically. Sets the facing to local +Z turned by the Y angle, the speed to Scale.x, and locks the stick for Scale.y frames. 30-frame cooldown. |
| RING_LINEAR / RING_CIRCLE | Scale.z rings, Scale.x apart along local +Z / on a circle of radius Scale.y |
| ITEMBOXAIR, CONTWOOD | Homing targets, destroyed by any attack. Items are not granted. |
| E AI, E KUMI, E GOLD | Stationary homing targets 8 above their placement; 100 points. Touching one outside ball form costs every ring, or a miss with none. |
| CONTIRON, CONTCHAO | Solid 20-unit cubes, as in Iron Gate |
| SAVEPOINT | 30-unit horizontal contact |

Not run (listed in the headless report): the snowboard and BOARDCOL (the
opening is run on foot), CCUBE and CCYL (their sizes do not match a solid
reading: a CCUBE at the board ramp would bury the landing), 3SPRING,
TJUMPDAI trick ramps, the building wall-run path (callback 0x497B50), the GUN
truck chase, switches, item effects, enemy behaviour, sounds and the camera.

## Run it headless

From `asset-tools/crates/sa2-play`:

```powershell
cargo run --release --no-default-features -- $game $pcSa2Config --city-escape out.json [MAX_FRAMES [CHECKPOINT]]
```

`speed_autopilot` plans over the live collision (`play_nav` with opt-in hops
of 30 units across up to 3 cells), adds spring and rail links by simulating
the controller, and then only produces stick and button input. When the goal
is unreachable it drives to the nearest reachable point and stops.

| Check (`tests/city_escape_play.rs`, `--ignored`) | Result on the pinned Steam build |
| --- | --- |
| Inventory | Sonic's row, 12 rails, 3 checkpoints, 82 dash panels, 1 live goal at (3955, -22139, 16129.8) |
| Opening run | From the start down the board hill on foot, over the ramp, rail 0, two more rails and the dash panels to (318, -6565, 10050) in 88.7 s: about 10,000 units, 15 rings, 5 dash panels, no deaths |

**Not reached:** the rest of the stage, from that point on. The next part
of the route appears to be the uphill rail from (292, -6557, 9905) to
(292, -6457, 9765), which leads to the first SAVEPOINT at (319, -6470, 9639).
Under these rules, a grinder starting at running speed stalls on it, so no
planner link exists. This could be the stand-in's rail physics (a
native-matched rail would settle it) or a route this session doesn't model
(the nearby HAMMER objects are not run).
