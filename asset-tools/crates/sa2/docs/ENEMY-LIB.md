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
| `host` | `EnemyHost`, the calls the enemy code makes to code not ported here |

## What the host stands for

Each `EnemyHost` method is one main.dol routine or REL child-task creator:
collision hit queries (CCL), culling (`CheckRangeOut`), ground and wall
collision (fn_80017C00, land collision), sounds, score, debris
(fn_80016D10), the player reactions, and the REL's bullet, beam, jet and
effect child tasks. `Event::Child` records the creator's REL offset and the
vectors it is given, so each child can be ported later on its own.

`Session` (play_session) runs every Prison Lane E AI each frame, before the
doors read the group counts. Its host does not have the player's collision,
culling or land collision, so enemies are never touched by or bump into the
player, are never culled, and do not collide with the ground; the game flags
at 0x801CC168 keep their main.dol initial values (all 0). `destroy_enemy` on
an E AI sets the task's collision-hit flag (4); the enemy's own kill path then
runs on its next frame (mode 10, score, drops, and it leaves its switch group).
`rand` starts at main.dol's initial `next` (1); a host that knows the game's
state on entering the stage sets `Session::rand`.

The SET loader (main.dol 0x80065E54) copies the rotation halfwords with `lha`,
so task angles are sign-extended.

## Verification

Both tests are `#[ignore]`d and need `SA2_MAIN_DOL` (GSNE8P main.dol),
`SA2_STAGE_REL` (GameCube stg09D.rel from the same disc) and
`SA2_PPC_PYTHON` (a Python with Unicorn). None of these files are committed.

* `tests/enemy_lib.rs` / `enemy_lib_ppc.py`: every `engine` helper on random
  inputs and 12,000 motion steps over E AI's table, bit for bit.
* `tests/e_ai.rs` / `e_ai_ppc.py`: stg09D.rel's own 0x51034 (and, with
  `SA2_ENEMY=gold`, E GOLD's 0x4A024) run frame by frame
  under the `dol_ppc` harness with the game around it scripted (80 runs over
  all kinds, `SA2_E_AI_SEED` / `SA2_E_AI_RUNS`). The task, the enemy work,
  the collision radius and attribute, the group counts, rand and every host
  call match bit for bit.

`tests/stage_rel_ppc.py` loads a GameCube stage REL into the harness with its
relocations and Gekko's `frsqrte`.

## Not ported yet

E KUMI (0x4C408), E NAMIE (0x52A1C); E GOLD in `Session` (it is built but
not run there: E GOLD's set-up calls fn_80018D28, not the counting
fn_80018C88, so it never joins a switch group, while `prison_doors::enemy_counts`
still counts it); the REL child tasks
(shots 0x44BA0, beams 0x45638, kind 4 shots 0x4358C, jets 0x43ED0, effects);
fn_80017C00's ground response (it needs the land collision search); debris
and drops.
