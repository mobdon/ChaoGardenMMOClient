# Finalhazard player handoff and stage frame

These request paths run against the pinned `Boss_Last2D.rel` (module 88, SHA-256
`6e694afcb14f1ab41e31983083055721e70f672a20e1d535df3523287413b64f`):

    sa2-assets finalhazard-placement-clock Boss_Last2D.rel .local/out.json REQUEST.json

The request's `path` selects the routine. Engine imports are not ported: each one is reported with
its call site, target and the arguments the stage code sets, and its result is a request input.

| Path | Section 1 code | What it reproduces |
| --- | --- | --- |
| `switch_camera` | 0xf5cc–0xf6a8, 0xf724–0xf7fc | Six-word camera request when the handoff enters state 2 or 3 |
| `switch_replay` | 0xf458–0xff34 per frame | Handoff states 1–3 through the shared tail's timer store |
| `voice_gate` | 0xeb38 | Whether a stage voice may play |
| `arena_clamp` | 0xee8c, 0x40cc | Per-frame arena bounds for both players |
| `timed_voice` | 0xf070 | Timed boss voice lines |

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
data. The stage square root at 0x40cc is ported with an exact reciprocal-root estimate (as Unicorn
computes `frsqrte`), followed by the stage's three double-precision Newton steps. Hardware
estimates can differ only in rare final roundings. That case has not been measured.

When global byte 0x42 or 0x43 is 1, the voice-line routine releases the stage task's held object
instead. Otherwise, with no countdown and on every fourth frame, line index 0–9 selects a
jump-table case (section 5 + 0x40008). The case holds a frame threshold (1800 … 17400) and the
voice IDs for each player. A busy category-3 channel or a held object defers the line. Otherwise
the local voice-task helper 0x18ed0 is called; it stays unported.

## Verification

With a Python environment that has Unicorn 2, run each test as
`python TEST SA2_ASSETS_EXE Boss_Last2D.rel` from the worktree. The tests execute the installed
instructions with relocated data and compare them against the Rust output:

| Test | Cases |
| --- | --- |
| `tests/finalhazard_switch_camera_ppc.py` | 336 camera packets: both states, both slots, characters 9/10/−1, yaw wraparound, modes −1/13/14/20 |
| `tests/finalhazard_handoff_replay_ppc.py` | 240 replays, 15,296 frames; every call, argument and non-stack store in order. Stage calls run natively in 80 replays. Every branch must be reached |
| `tests/finalhazard_stage_frame_ppc.py` | 197 voice-gate, 600 arena (270 radial clamps) and 3,000 voice-line cases |

Deliberate one-line faults were caught by each test. These are offline comparisons with the
installed instructions. The game was not run.

## Open

The fight is not playable. Engine distance, position setting, voice-task creation, camera and
player services are observed, not ported. Live collision, player movement, animation-completion
signals and story advancement remain open. Most of the module's section 1 has not been ported.
