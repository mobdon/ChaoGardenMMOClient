# Finalhazard player handoff and stage frame

These request paths run against the pinned `Boss_Last2D.rel` (module 88, SHA-256
`6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f`):

    sa2-assets finalhazard-placement-clock Boss_Last2D.rel .local/out.json REQUEST.json

The request's `path` selects the routine. Engine imports are not ported: each one is reported with
its call site, target and the arguments the stage code sets, and its result is a request input.

| Path | Section 1 code | What it reproduces |
| --- | --- | --- |
| `switch_camera` | 0xf5cc–0xf6a8, 0xf724–0xf7fc | Six-word camera request when the handoff enters state 2 or 3 |
| `switch_replay` | 0xf414–0xff34 per frame | Stage player task: states 0 (ring drain, handoff trigger), 1–3 (handoff) and 4 (ring-out debris) |
| `voice_gate` | 0xeb38 | Whether a stage voice may play |
| `arena_clamp` | 0xee8c, 0x40cc | Per-frame arena bounds for both players |
| `timed_voice` | 0xf070 | Timed boss voice lines |
| `super_player` | 0xad8c | One update of a super-form player (control flow) |
| `super_flight` | 0xe370 | Super-form flight thrust, speed bands and turning |
| `super_collision` | 0xc598 | Per-frame collision sphere placement for a super form |
| `super_hover` | 0xe9f0 | Super-form hover: the released-stick part of flight |
| `super_free_move` | 0xdc90 | Action 58: stick-driven free movement |
| `super_control` | 0xbad0 | Super-form hit reaction and action selection |
| `super_charge_draw` | 0xc67c | Action-67 charge effect draw call |
| `super_draw` | 0xd2dc | Super-form player draw routine |
| `super_effects` | 0xd60c | Super-form effects: spin, aura, fade and boost trail |
| `stage_setup` | 0xa078 | Stage setup: lights, hit handler, object hooks, both super forms, boss and timer objects |
| `super_construct` | 0xa87c, 0xab04 | Super-form constructors (characters 9 and 10): task and player block, engine set-up calls, effect block, task handlers, timer object; reports the engine table words read |
| `stage_teardown` | 0xa2a0 | Stage teardown: timer and stage objects, ring drop hit handler, light lists, both player tasks |
| `stage_object` | 0xa390 | Stage object set-up: boss set-up 0x414 when module word 5:0x382b8 is set, handlers 0xa3f4, 0xa560, 0xa758 |
| `boss_construct` | 0x414 | Boss object construction: task (update 0x1bb8, handlers 0x1e04 and 0x638), work block, entity start and scale, collision flag, child object 0x8528; returns the task or 0 |
| `boss_update` | 0x1bb8 | Boss task update: action mirror, pause gates (0x801cbd88 + 0x37 / + 0x39), stage change handling, action routine from the jump table at 5:0xfdc (actions 0..13), child follow 0x1fc8, then 0x744 and 0x920 |
| `boss_helper` | 0x9d4, 0x9f4, 0xa40, 0xa88, 0xc60, 0xc98, 0xd04, 0xd40, 0xdc4 | Boss state helpers: boss task (6:0x0), child, state, stunned check, state timer start and clear, pattern table 5:0x514 step, action set |
| `boss_action_zero` | 0xe2c | Boss action 0: motion start (step 0), sound and voice inside the motion window 4:0xe0..4:0xe8, then ring burst 0x18ed0 (players capped at 3), action 2 and the next sound when the motion ends; clears stage half-word 0xa |
| (action 1) | 0x10a8 | Boss action 1 is an empty routine (a lone `blr`) |
| `boss_action_two` | 0xff4 | Boss action 2: 60-frame wait (entity half-word 6), child 0x86b8, then the next pattern entry (0xd40) as the action |
| `boss_action_three` | 0x10ac | Boss action 3: frees the stage handles and starts motion 2; resets lights, child (0x866c), camera and both players; in the window 4:0xc4..4:0xf4 plays sound and voice; at the motion end clears entity flag 4, child 0x8818, motion 1, action 1, stage byte 4 and the handoff start 0xec04 |
| `boss_attack` | 0x12e8 | Boss actions 5, 6, 7: voice roll 0xaf0 each frame; spawn attack kind 0/1/2 (0x62a0) into state word 0x14, 60-frame wind-up, launch (0x63e8), wait for the attack to report 1 (0x6410), free it and take the next pattern entry |
| `boss_beam` | 0x1440 | Boss actions 8, 9, 10: voice roll 0xaf0; beam parameter block (bytes 0/1, 1/1 or 1/2 by action, work vectors 6:0x164 and 6:0x170, the child) passed to 0x3e58, beam in state word 0x18, waits of 60 and 600 frames, fire 0x3fc0, wait for 0x403c to report 1, free, next pattern entry |
| `boss_combo` | 0x1640 | Boss actions 11, 12: beam (block 1/2) plus attack kind 2, 60-frame wind-up, attack launch; action 11 then fires the beam once the attack reports 1 and waits for the beam before the next pattern entry; any other action hands the attack to 0x6460 and runs 0x86b8 on the child every 600 frames (step 4) |
| `boss_defeat` | 0x1844 | Boss action 13 (defeat): frees the stage handles, motion 3, lights, child reset, clears entity flag 4; rumble every 45 frames before 4:0xf8, three sound and voice windows (4:0xfc..0x100, 0x104..0x108 on task + 1, 0x10c..0x110), explosion 0x92c0 every 30 frames and flash 0x9518 every 10 frames before 4:0x114 (native magic-number division); at the motion end the result clock (0x800c3974 → 0x800c3518) and story advance 0x8001c78c with the stage byte |
| `ring_drop` | 0x19470 | Hit ring drop: defeat at zero rings, else scatter up to 20 rings |

## Handoff camera

The packet is: stage scalar (section 4 + 0x51c for state 2, + 0x558 for state 3), `0x8000 - yaw`,
a resource word, the incoming player's position pointer (0x801e7368 entry + 0x14), 0x51, 0. The
yaw is word 0xc of that entry; the character is byte 1 of the 0x801e7328 entry. Resource words
come from 0x801e5928: 0x364 (character 9) or 0x39c entering state 2, 0x354 (character 9) or 0x37c
entering state 3. Pass `table_words` in ascending offset order. Above camera mode 13 the stage
reads the mode again and passes that value to 0x800c3518 before submitting to 0x800cb438.

## Handoff replay

`switch_replay` takes `player`, `switch_state` (1–3), `timer`, `countdown`, `visits`, the three
engine pointer tables `player1` (0x801e7368), `player2` (0x801e7328) and `player_tasks`
(0x801e7348), `table_words`, an optional `line`, and `frames`. Each frame supplies the poll result,
both characters and yaws, the camera mode and second lookup, and engine global word 0x7c.

The slot that is active when a frame begins receives the state 2 entry writes: action 0x42 and
0x176 = 0x6b. Those writes reach the outgoing player, because the swap happens first. Entering
state 3 writes action 0x42, 0x176 = 0x64 and 0x17c = 0x57 to the incoming player. The camera
always targets the incoming player. After the state 3 camera, the voice gate decides the cue
(0x871–0x874 by player and visit count) and the 20-frame countdown. State 3 returns to normal
dispatch after 180 frames; a frame after that is rejected.

Each frame begins with the engine call 0x8001ccbc, given as `game_state`. When it returns 7, the work block is reset: player, state, timer, reason, switch count, visits and voice line go to 0 and the ring pool to 50. Then 0x80021a14 runs.

State 0 is the normal fight, and its frames need `stage`. With no hold active (module word
section 6 + 0x664, globals 0x42/0x43), the ring clock (work + 0xa) counts up. Every 61st frame it
drains a ring. Before frame 18000 the active player loses one ring and the other holds the rest of
the pool (work + 0xc); from 18000 on, both are emptied. Fifteen and four rings play warnings
0x869/0x867 and 0x86a/0x868 through the voice gate. At zero rings the active player is defeated:
the engine is told through 0x8003b914, and the task enters state 4. The handoff starts (0xec04)
once the active player passes Z −50 (player 0) or +50 (player 1). That needs no boss hold flag 4,
switch reason other than 1, and neither slot posing (0x6d on slot 0, 0x74 on slot 1). A pair of
engine sound calls ordered by the active player ends the frame. Each engine ring-count read
(0x80062a24) comes from the stage block's `ring_reads`, in call order. The ring-add calls are
reported, and their effect stays in the engine. No ring arithmetic is modeled.

Engine results and engine state are always request inputs. A path that needs one that is missing
fails; no default value is supplied.

State 4 (0xf8cc) follows a ring-out. On every fourth frame it builds a debris burst from seven
engine random values (scaled by 1/32768) and five trigonometric results. Those values give an
offset of (−0.2 − 0.1u, 0.08 − 0.04u, −0.05 + 0.1u), a speed of 4 + 3u and angles 16384 − 32768u
and 65536u. The stage passes the direction and the defeated player's 0x801e7328 word 0x230 vector to
0x80114ba0, whose effect is not modeled. It then calls the local spawn 0x1425c with a size of
1 + 0.5u. Every state 4 frame then writes 0.2 and 0.01 to words 0xc and 0x10 of that player's
0x801e7348 entry. Such frames need a `debris` block (`randoms`, `trig`, `origin`). The ring transfer inside 0xec04 needs
switch reason 1, which blocks this trigger; only the boss's own call reaches it.

A frame without `stage` reports the voice gate and the tail's two local calls as call sites only;
its `cue_result` stands in for the gate. With a `stage` block (`channels`, `present`, `positions`,
`distances`, `global_42`, `global_43`, `task_0c`) the replay runs all three and nests their engine
events under each call. It also carries the voice-line index from frame to frame.

## Voice gate, arena bounds and voice lines

The gate scans two engine voice channels (stride 0x1c at 0x803ad3d0). States 5, 0 and −1 are idle.
A busy category-3 channel is stopped through 0x8001162c; any other busy channel returns −1. A null
table returns 0.

Unless the switch state is 4, the arena routine pulls a player whose engine-reported XZ distance
exceeds 1500 back to radius 1499 around the stage center. It then caps X at 250 and Y to −500..300
and submits the position through 0x80039754. The constants are read from the module's relocated
data. The stage square root at 0x40cc is ported with the stage's three double-precision Newton steps.
Its starting estimate comes from the shared `msl_math::gekko_frsqrte`. That uses the Gekko
hardware `frsqrte` table when the git-ignored file `.local/research/gekko-estimate-tables.json`
is present (override `SA2_GEKKO_TABLES`), and the exact reciprocal square root otherwise. The
table comes from Dolphin's GPL source and is not in the repository. Unicorn computes `frsqrte`
exactly, so with the file present the tests replace that instruction through `tests/gekko_tables.py`.
Across 40,009 inputs the routine then matches the Rust port bit for bit, and the exact estimate
would have changed none of the results. Without the file, `finalhazard_stage_sqrt_ppc.py` skips,
and the other comparisons use the exact estimate on both sides.

When global byte 0x42 or 0x43 is 1, the voice-line routine releases the stage task's held object
instead. Otherwise, with no countdown and on every fourth frame, line index 0–9 selects a
jump-table case (section 5 + 0x40008). The case holds a frame threshold (1800 … 17400) and the
voice IDs for each player. A busy category-3 channel or a held object defers the line. Otherwise
the local voice-task helper 0x18ed0 is called; it stays unported.

## Super-form player update

The constructors 0xa87c and 0xab04 build the two super-form players. They write character 9 and
character 10 to the 0x37c-byte player block, and install 0xad8c as the update. `super_player`
reproduces one call to it. It takes the object addresses, the entity action and flags, the
collision flags, the player block fields it reads, an optional effect block (player word 0x36c),
and engine results in call order (`lengths`, `randoms`, `trig`, `approach`, `defeat_query`).

Every frame sets the collision object's flags. Boost actions 61–64 and 67 clear bit 0x10 of inner
word 0x34; any other action sets it. When the player's bit in global byte 0x37 is set, the update
runs the engine follow and tail calls only. Otherwise it counts the 6-frame cycle at player byte
0x35c. If the 0x64 vector is longer than player float 0xec, it spawns a spark (except in actions
0, 15, 53, 57 and 59). It then dispatches the action through the 68-entry table at section 5 +
0x3fd50:

| Action | Behavior |
| --- | --- |
| 0 → 59 | Flight; hover handler 0xe9f0 while posing 99, else 0x80048228 and movement 0xe370 |
| 60 | Movement, then 0xe1ac |
| 61–64 | Player float 0x68 approaches (4, 0.1), (−4, 0.1), (2, 0.4) or (−2, 0.4) through 0x8002abf4, then movement |
| 67 | 0xc67c |
| 15, 53 | Engine movement calls; 53 also holds through 0xe9f0 |
| 57 | Ring-out. After 40 frames: pose 104 and 15 debris pieces |
| 58 | 0xdc90 |

Every dispatch case ends with three engine calls: 0x80046b7c, 0x80045618 and 0x8003e400. The
effect block then follows the travel since the last frame. For character 9 the scale is 0.25,
with thresholds 0.8 and 0.4; for character 10 it is 0.2, with 0.64 and 0.4. When 0x8004fda0
reports a defeat, the action becomes 58. 0xc67c (action 67) draws the charge effect on even frames (`super_charge_draw`). It uses the
player's pose entry, the engine motion table 0x801e6288 and the model table 0x801e5928. Action 58 (0xdc90) reads the player's 52-byte controller
entry at 0x801e5038. When the stick is outside a ±14 dead zone it moves the entity 5 units: up or
down by the stick's Y sign while button bit 0x400 is held, and otherwise across X and Z along the
stick direction relative to the camera yaw. The angle is scaled by 65536/2π. 0xe370 (flight), 0xe9f0 (hover) and 0xc598
(collision placement) are ported. 0xe1ac, called in action 60, has no effect for any input: its
pose check needs pose 99 and 109 at once, so it always returns (checked under emulation). 0xc598 clears entity rotation words 8 and 0x10 and data2 words
0x18 and 0x20 unless the action is 67. It then places the collision sphere (inner words 8..0x10,
copied to 0x38..0x40) at the position plus the engine-rotated offset (0, player float 0x140, 0).

## Flight movement

`super_flight` reproduces 0xe370, the movement handler that flight (59), action 60 and the boosts
call. It starts from the acceleration in data2 words 0xc..0x14. While the stick is released it
adds forward thrust from the speed (player float 0x64) beyond threshold 0xe8, and applies drag
below 0. While the stick is held the thrust above 0xd0 is scaled by 1.7. Player floats 0x68 and
0x6c add vertical and side thrust.

With the stick held, the stick magnitude is shaped by four speed bands (player floats 0xd0, 0xe4,
0xe8, 0xec): a floor of 0.5, cut-offs at 0.4 × 0xe4 and 0.5 × (2 × 0xe8), and scales 0.6, 0.7, 0.9,
0.3 and 0.4. The result is multiplied by player float 0xfc. When the 0x70 velocity is non-zero,
the stick angle is corrected toward the travel direction. Engine angle checks above 4096 and 8192
then select one of three engine turn routines, which get the angle in data2 word 0x1c. The
magnitude is stored to player float 0x3c, and the acceleration goes to player words 0x58..0x60.

The stick query, vector and matrix services and the turn routines are engine code. Their results
are request inputs: `held`, `stick_angle`, `stick_magnitude`, `v70_length`, `rotated`, `dots`,
`cross`, `turn` and `angle_delta`.

## Action selection

`super_control` reproduces 0xbad0. The super-form update calls it while the player's pause bit is
clear. First it handles hits. Player halfword 0x10 bit 0x8000 sets entity flag 0x8000 and clears
the hit timer (entity halfword 6). A running timer counts down. Otherwise the engine hit check runs,
and a fresh hit plays the character's hit sound (0x300b or 0x301d), enters stun action 15 with pose
21, and sets the speed to minus player float 0xe8.

The action table at section 5 + 0x3fe60 then moves between flight (59), steering (60), the four
boosts (61–64, pose 90/92/94/96, with player float 0x68 pushed past ±2.7), the charge action (67,
120 frames), landing (12 → 1), stun recovery (15 → 59), and the returns from actions 53, 54, 58 and
66. Inputs are the player's bytes in the four engine button tables 0x803ad498, 0x803ad494,
0x803ad488 and 0x803ad484. The stick and landing queries are request inputs.

The three local checks are ported with it:
- **0xdf44** leaves the current action when entity flag 0x1000 is set. The previous action
  (entity byte 1) picks the exit: 9 → 18, 15 → 59, 33 → 53, 36 → 59, 37 → 57 with a voice line.
- **0xe120** settles into flight (pose 87) when the stick is released at zero speed.
- **0xe200** launches toward the boss when it is unshielded and within 200 units. It sets action
  67 and pose 89, with a launch vector of 10 × the normalized difference. It returns the engine
  transform call's result, as the native code does.

The engine vector services that 0xe200 uses write results in place; those results are request
inputs (`charge`).

## Super-form draw

`super_draw` reproduces 0xd2dc. It draws a player only when it is the stage task's active player,
or while global byte 0x42 is 1. It sets the engine draw state, plus extra state unless the
player's byte at 0x803ad524 is 1 or 5. It stores the player block in module word section 6 +
0x4b0 and installs callback 0xc77c (character 9) or 0xccf8 (character 10). It places the model at
the entity's position, rotation (yaw as 0x8000 minus the entity's) and scale.

With an effect block, the effect's texture word is swapped into the character's engine object for
the duration of the draw. The current pose is then drawn with its motion and model; mode 2 uses
player word 0x19c as the model. Action 54 uses the player's 0xbc model with the engine scale at
0x803ad544. The draw state is restored at the end. Module and code addresses in arguments are
written as "88:SECTION+OFFSET". The callbacks 0xc77c and 0xccf8 are not ported.

`super_effects` reproduces 0xd60c, the second draw routine. Character 10 in poses 8–10 draws four
spinning models, alternating engine models by frame parity. Pose 100 draws an aura whose texture
frame (module halfword section 5 + 0x3c9b6) follows player byte 0x35c. Otherwise, after frame
14400, a fade sets 40 module material colours to grey (frame − 14400) × 255 / 18000 + 128. The 40
materials are the entries whose source colour in the module data has alpha 0xff. While boosting
(entity flag 0x400), a trail is drawn and turned by the boost pose (90/92/94/96).

## Hit ring drop

Stage setup (0xa078, 0xa2a0) installs 0x19470 as the handler the engine calls when a player is
hit. `ring_drop` reproduces it. With zero rings the player is defeated through 0x8003b914 and
0x80069ee0, except characters 6 and 7. Otherwise every ring is taken (0x80063584) and up to 20
ring objects (update 0x196f0, not yet ported) are spawned at the player's position. Their headings
are spread evenly: (start + i × 360 / n) × 65536 / 360, from a random start of 360 × random / 32768
degrees. Then the drop sound 0x8014 plays.

## Verification

With a Python environment that has Unicorn 2, run each test as
`python TEST SA2_ASSETS_EXE Boss_Last2D.rel` from the worktree. The tests execute the installed
instructions with relocated data and compare them against the Rust output:

| Test | Cases |
| --- | --- |
| `tests/finalhazard_switch_camera_ppc.py` | 336 camera packets: both states, both slots, characters 9/10/−1, yaw wraparound, modes −1/13/14/20 |
| `tests/finalhazard_handoff_replay_ppc.py` | 400 replays, 39,182 frames; every call, argument and non-stack store in order. Stage calls and the handoff start run natively in 240 replays, 160 of them starting in state 0 or out of range. Every reachable branch must be reached |
| `tests/finalhazard_super_flight_ppc.py` | 3,000 flight updates across all speed bands, both stick states, every turn routine; engine pointer outputs written into emulated memory |
| `tests/finalhazard_super_collision_ppc.py` | 500 collision placements |
| `tests/finalhazard_super_hover_ppc.py` | 800 hover updates; 0xe1ac stores nothing for poses 99, 109, 116 and others |
| `tests/finalhazard_super_free_move_ppc.py` | 1,500 action-58 updates around the dead zone, both button states |
| `tests/finalhazard_super_control_ppc.py` | 4,000 action-selection updates across every dispatched action, hit states and button tables |
| `tests/finalhazard_super_charge_draw_ppc.py` | 500 charge-effect draws with filled engine tables |
| `tests/finalhazard_super_draw_ppc.py` | 800 draws: active and inactive players, both characters, effect swap, action 54, pose modes |
| `tests/finalhazard_super_effects_ppc.py` | 1,500 effect draws across spin, aura, fade (frame boundaries) and trail poses |
| `tests/finalhazard_ring_drop_ppc.py` | 1,500 ring drops: zero, capped and negative counts, characters 6/7, failed allocations |
| `tests/finalhazard_stage_sqrt_ppc.py` | 40,009 stage square roots on the Gekko estimate, bit for bit (skips without the local table file) |
| `tests/finalhazard_stage_setup_ppc.py` | 24 stage setups: hit handler present or not, either player task present, module word 0x484 |
| `tests/finalhazard_super_construct_ppc.py` | 72 constructions: both constructors, both slots, task or player allocation failing, global byte 0x801cbd88, module word 0x4ac |
| `tests/finalhazard_stage_teardown_ppc.py` | 64 teardowns (module words 0x484 and 0x480, hit handler absent, foreign, ring drop or another module address, either player task) and 4 object set-ups |
| `tests/finalhazard_boss_construct_ppc.py` | 240 constructions: each allocation failing, module byte 5:0x50c (signed extremes), collision flags, stage byte |
| `tests/finalhazard_boss_update_ppc.py` | 600 seeded updates; memory read again after a call is changed at that point, so each request value is checked at its own read |
| `tests/finalhazard_boss_helper_ppc.py` | 4160 helper calls with the nested helpers run natively: boss, data and state present or not, action, flags, state word 0xc, pattern rows and columns |
| `tests/finalhazard_boss_action_zero_ppc.py` | 525 frames: steps 0, 1 and out of range, motion frames around both window edges, motion end, player counts |
| `tests/finalhazard_boss_actions_ppc.py` | 120 action 2 frames and 2160 action 3 frames: steps, timers, patterns, stage handles, motion flags and window edges, motion end, entity flags |
| `tests/finalhazard_boss_attack_ppc.py` | 960 frames: steps, actions 5..7, timers, attack states, attack freed or already gone, patterns |
| `tests/finalhazard_boss_beam_ppc.py` | 1152 frames, with the beam parameter block read off the stack: steps, actions 8..10, timers, beam states, beam freed or gone, patterns |
| `tests/finalhazard_boss_combo_ppc.py` | 1008 frames: steps 0..5, timers, action after launch (11, 12, other), attack and beam states, objects freed or gone, step-4 timer |
| `tests/finalhazard_boss_defeat_ppc.py` | 1500 seeded frames: motion frames around every window edge, frame counters at and around the 45/30/10 periods and near 2^32, motion end |
| `tests/finalhazard_super_player_ppc.py` | 1,500 updates across every dispatched action, both characters, effect block present or absent; every call, argument and non-stack store |
| `tests/finalhazard_stage_frame_ppc.py` | 197 voice-gate, 600 arena (270 radial clamps) and 3,000 voice-line cases |

Deliberate one-line faults were caught by each test. These are offline comparisons with the
installed instructions. The game was not run.

## Module map (unported parts)

These links were found from relocations and direct calls in section 1. They show where code is
reached from, not what each object does.

| Section 1 offset | Reached from | Notes |
| --- | --- | --- |
| 0xa04c, 0xa074 | REL prolog and epilog | Module entry points |
| 0xa078, 0xa2a0, 0xa390 (ported) | Stage setup | Call 0xa87c and 0xab04 and install 0x19470 (exec 0x196f0, about 3.8 KB) |
| 0xa87c, 0xab04 (ported) | 0xa078 | Super-form player constructors (characters 9 and 10). Each installs the update 0xad8c and its handlers (ported), the draw routine 0xd2dc (ported), 0xd60c, and the stage player task (0xff7c → 0xf3f8, replayed) |
| 0x18b5c | Mode 2 of the helper 0x18ed0 | Allocates 0xdc bytes and installs 0x15f7c (about 7.5 KB), 0x17d18, 0x188f0, 0x18dbc and 0x191ac. Not reached in this module (see below) |
| 0x1844, 0x7f58 | 0x1bb8, 0x8328 | Encounter phase code covered by earlier request paths |

The helper 0x18ed0 switches on its seventh argument: mode 0 at 0x18f1c, mode 1 at 0x18fc8, mode 2
at 0x1907c, nothing for 3 and above. Only mode 2 creates the 0x18b5c object. Its only two callers
(0xf90 and 0xf3dc) both pass mode 1. A reachability pass over section 1 starts from the REL prolog
and epilog and the code pointers in module data that name function starts (jump-table entries
excluded). It follows direct calls and code relocations, leaving out the unused modes of 0x18ed0.
That pass finds 28 functions (20,604 bytes, 17% of section 1) unreachable, including 0x18b5c,
0x15f7c, 0x17d18 and 0x1569c. Of the 98,532 reachable bytes, the Rust ports reference functions
covering about 36%. The largest reachable unported routines are 0x196f0 (installed by stage setup
through 0x19470), 0x1105c, 0x120d4 and 0x1b384.

## Open

The fight is not playable. Engine distance, position setting, voice-task creation, camera and
player services are observed, not ported. Live collision, player movement, animation-completion
signals and story advancement remain open. Most of the module's section 1 has not been ported.
