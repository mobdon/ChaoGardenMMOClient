# Shared enemy library (`enemy_lib`)

Native ports of SA2B's enemy code from the GameCube disc (GSNE8P): main.dol's
shared enemy helpers and the enemy logic each stage REL carries. Offsets in
stg09D.rel's code section are the same in the PC and GameCube files (the
section bytes match; only the relocations differ), so the GameCube REL's
relocations name main.dol addresses directly.

## Modules

| Module | Native code |
| --- | --- |
| `engine` | main.dol: work set-up 0x80018D28, switch groups 0x80018C88 / 0x80018C10 / 0x80018B88, `DiffAngle`, `AdjustAngle`, `njRoundOff`, `njFraction`, `rand`, approach 0x8002AE5C, notice tests 0x800187C4 / 0x800188B0 / 0x8001897C, nearest player 0x80037C84, face a player 0x800176AC, leash 0x80017918, home offset 0x80015B18, `sqrtf` (main.dol's and the REL's), `njScalor`, `njUnitVector`, `njDistanceP2P` |
| `motion` | the enemy motion sequencer 0x80015C40 / 0x80015BA8 and its REL table rows |
| `e_ai` | stg09D.rel E AI: row callback 0x51034 and the E EMI-family routines 0x4E8EC..0x51034 |
| `e_gold` | stg09D.rel E GOLD: row callback 0x4A024 and 0x494AC..0x49DB4, plus main.dol's view test fn_80018A4C (`engine::player_in_view`) |
| `e_kumi` | stg09D.rel E KUMI: row callback 0x4C408 and 0x4A3A8..0x4C01C, its eight kinds |
| `e_namie` | stg09D.rel E NAMIE: row callback 0x52A1C and 0x51398..0x52980 (its child segments, 0x53F0C, are not run) |
| `ground` | main.dol fn_80017C00: the probe point fn_800186B0, the ceiling push, ground settling 0x800184E4 and walls fn_80017D4C |
| `land_probe` (shared) | adds fn_800CFE6C (`six_way_probe`) and GetShadowPosXYZ / fn_80066E1C (`shadow_xyz`) beside Prison Lane's fn_800D05D0 and GetShadowPos |
| `host` | `EnemyHost`, the calls the enemy code makes to code not ported here |

## What the host stands for

Each `EnemyHost` method is one main.dol routine or REL child-task creator:
collision hit queries (CCL), culling (`CheckRangeOut`), the six-ray land probe (`EnemyHost::shadow`,
which a host answers with `land_probe::shadow_xyz` on its land collision),
sounds, score, debris
(fn_80016D10), the player reactions, and the REL's bullet, beam, jet and
effect child tasks. `Event::Child` records the creator's REL offset and the
vectors it is given, so each child can be ported later on its own.

`Session` (play_session) runs every Prison Lane E AI, E GOLD, E KUMI and E NAMIE each
frame, before the doors read the group counts; each joins its group (or not)
in its own set-up. Its host does not have the player's collision or culling, so enemies are
never touched by or bump into the player and are never culled; they collide
with the ground through `Session::shadow_probe`, which the host supplies from
its land collision (without it the probe finds nothing, as GetShadowPosXYZ
does with an empty selection); the game flags
at 0x801CC168 keep their main.dol initial values (all 0) except +0x37 (the
session's `effects_suppressed`) and the frame counter +0x7C
(`Session::game_frames`, advanced by fn_80062B38 in the game). `destroy_enemy` on
an E AI sets the task's collision-hit flag (4); the enemy's own kill path then
runs on its next frame (mode 10, score, drops, and it leaves its switch group).
`rand` starts at main.dol's initial `next` (1); a host that knows the game's
state on entering the stage sets `Session::rand`.

The SET loader (main.dol 0x80065E54) copies the rotation halfwords with `lha`,
so task angles are sign-extended.

## Verification

`tests/land_probe_xyz.rs` / `land_probe_xyz_ppc.py` compare `six_way_probe`
with fn_800CFE6C on 1,500 synthetic collision nodes. The enemy replays run
fn_80017C00 natively against a scripted GetShadowPosXYZ answer, so the probe
point, ceiling, ground and wall responses are checked bit for bit.
`shadow_xyz` itself (the selection and merge around fn_800CFE6C) is ported
from the disassembly but not yet run against the game.

The tests are `#[ignore]`d and need `SA2_MAIN_DOL` (GSNE8P main.dol),
`SA2_STAGE_REL` (GameCube stg09D.rel from the same disc) and
`SA2_PPC_PYTHON` (a Python with Unicorn). None of these files are committed.

* `tests/enemy_lib.rs` / `enemy_lib_ppc.py`: every `engine` helper on random
  inputs and 12,000 motion steps over E AI's table, bit for bit.
* `tests/e_ai.rs` / `e_ai_ppc.py`: stg09D.rel's own 0x51034 (and, with
  `SA2_ENEMY=gold` / `kumi` / `namie`, E GOLD's 0x4A024, E KUMI's 0x4C408
  and E NAMIE's 0x52A1C) run frame
  by frame
  under the `dol_ppc` harness with the game around it scripted (80 runs over
  all kinds, `SA2_E_AI_SEED` / `SA2_E_AI_RUNS`). The task, the enemy work,
  the collision radius and attribute, the group counts, rand and every host
  call match bit for bit.

`tests/stage_rel_ppc.py` loads a GameCube stage REL into the harness with its
relocations and Gekko's `frsqrte`.

## Not ported yet

E NAMIE's child segments (0x53F0C / 0x53BB8: until they run, kinds 0 to 2
wait in state 2 for the flag a segment sets); the other REL child tasks
(shots 0x44BA0, beams 0x45638, kind 4 shots 0x4358C, jets 0x43ED0, effects);
debris and drops.
