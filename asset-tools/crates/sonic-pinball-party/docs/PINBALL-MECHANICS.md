# Pinball mechanics are a separate port

This asset PR does not execute or translate the native game engine. The 23 visual descriptor rows are not 23 verified playable table definitions. A rendered board establishes tile/map/palette pairing; it does not establish collision geometry, scoring or simulation parity.

| Subsystem | Evidence needed before a mechanics port | Current status |
| --- | --- | --- |
| Table identity and progression | Native selection dispatcher, row/stage mapping, transitions, unlocks and table-specific modes | Visual descriptor indices preserved; semantic mapping unknown. |
| Ball/flipper simulation | Units, integration order, fixed-point precision, timestep, gravity, impulses, restitution, flipper swept contacts and bounds | Unknown. Do not derive collisions from rendered pixels. |
| Collision and objects | Native shapes/materials, ramps, bumpers, slingshots, gates, sensors and trigger dispatch | Unknown. Tilemap data is presentation evidence only. |
| Scoring and objectives | Native event/state graph, score arithmetic, multipliers, combos, missions, jackpots and boss rules | Unknown. Audio/debug labels do not establish rules. |
| Ball lifecycle | Plunger, tilt/nudge, multiball, drains, bonus count, extra balls and game-over ordering | Unknown. |
| Party/minigames | Native table/mode dispatch, input timing, multiplayer ownership, fail/win/reward paths | Unknown. Chao Ball has separate Garden candidate resource evidence. |
| Save/transfer ordering | Exact main save structure/checksum, atomicity, progression updates and Garden transfer transactions | Main save unknown; bounded Garden record reader only. |
| Audio/animation synchronization | Event-to-song mapping, sequence timing, sprite script scheduling and pause/resume behavior | Header/tile primitives available; timing unknown. |

Future mechanics work should pin each native routine/data span and distinguish verified original behavior from adaptations. Acceptance requires deterministic input traces and state/output parity for success, failure, boundary and corruption cases. No mechanics constants are inferred from Sonic Advance. MMO rules and authority must be documented separately; a valid native save checksum grants no authority to award progression.
