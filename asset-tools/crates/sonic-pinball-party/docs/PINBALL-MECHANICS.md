# Pinball mechanics are a separate port

The asset catalog and native mechanics port have separate evidence. Rust source work now translates bounded ball/flipper/collision and frame routines; [PORT-STATUS.md](PORT-STATUS.md) records the current limits. The full game remains incomplete. The 23 visual descriptor rows are not 23 verified playable table definitions, and a board raster does not establish collision geometry or scoring.

| Subsystem | Evidence needed before a mechanics port | Current status |
| --- | --- | --- |
| Table identity and progression | Native selection dispatcher, row/stage mapping, transitions, unlocks and table-specific modes | Visual descriptor indices preserved; semantic mapping unknown. |
| Ball/flipper simulation | Units, integration order, fixed-point precision, timestep, gravity, impulses, restitution, flipper swept contacts and bounds | Native reset/integrator/flippers/velocity-limit source ports and bounded parity exist; complete launch/lifecycle/table integration remains unfinished. |
| Collision and objects | Native shapes/materials, ramps, bumpers, slingshots, gates, sensors and trigger dispatch | Original map/material/flipper probing and response source ports exist. Full object behavior and retail playthrough parity remain separate. |
| Scoring and objectives | Native event/state graph, score arithmetic, multipliers, combos, missions, jackpots and boss rules | Region/layer trigger routers and action queues are translated; native score HUD formats the existing u64 value. Complete award/objective rules remain unported. Audio/debug labels do not establish rules. |
| Ball lifecycle | Plunger, tilt/nudge, multiball, drains, bonus count, extra balls and game-over ordering | Native directional nudge, table lifecycle state machines, launch velocities, nested reset and multiball setup have scoped parity. Dependencies and full playthroughs remain incomplete; no accumulated tilt penalty is inferred. |
| Party/minigames | Native table/mode dispatch, input timing, multiplayer ownership, fail/win/reward paths | Unknown. Chao Ball has separate Garden candidate resource evidence. |
| Save/transfer ordering | Exact main save structure/checksum, atomicity, progression updates and Garden transfer transactions | Main save unknown; bounded Garden record reader only. |
| Audio/animation synchronization | Event-to-song mapping, sequence timing, sprite script scheduling and pause/resume behavior | Original main-game song routing, player lifecycle, command leaves and queued message animation are translated. Complete note/mixer timing and audible/gameplay synchronization remain unverified. |

Future mechanics work should pin each native routine/data span and distinguish verified original behavior from adaptations. Acceptance requires deterministic input traces and state/output parity for success, failure, boundary and corruption cases. No mechanics constants are inferred from Sonic Advance. MMO rules and authority must be documented separately; a valid native save checksum grants no authority to award progression.
