# Shared stage objects (`stage_objects`)

Every GameCube stage REL of SA2B (GSNE8P) carries its own copy of the generic
object code: object list rows 0 to 23 and the effect, message and HUD tasks
they start. `src/stage_objects/` ports that code once for all stages. The
reference copy is **stg42D.rel**; every routine is named by its section-1
offset there, and each module's `ROUTINES` table records its length and the
SHA-256 of its relocation-masked bytes.

## Why one port serves every stage

`masked_text` zeroes every relocated field of a REL's code (ADDR32,
ADDR16_LO/HI/HA, REL24). `branch_masked_text` also zeroes the displacement of
every `b`/`bl`: a REL resolves its calls within section 1 when it is linked,
without a relocation, so a routine that calls a module-local helper (fabsf at
+0x84, sqrtf at +0x74CC, the stage's effect and land helpers) has a different
displacement in each stage. `tests/stage_objects_shared.rs` checks each
recorded hash against stg42D and finds the same branch-masked bytes in the
other 65 stage and boss RELs: 155 routine entries, each found in at least one
other REL, most in 46 to 65. Routines split around local calls are listed as
pieces (the `bl` word itself is left out).

The masked hash proves the code is the same, not that the constants it loads
are: `tests/so_effects.rs` (`routine_constants_are_shared_across_stage_rels`)
also compares the section-4/5 words the effect routines address in every
copy. Stage data (CollisionData rows, item tables, task names, model
offsets) is passed in by the stage, read from its own REL, never assumed to
be at stg42D's offsets.

## Coverage

| Rows | Object | Module | Ported (stg42D section 1) |
|---|---|---|---|
| 0 | LIGHT SW | `light_modmod` | set-up 0x13C78, player test 0x13D7C, exec 0x14030; light setter fn_80025028 |
| 1 | MODMOD | `light_modmod` | set-up 0x20310, exec 0x2039C, display 0x203C4 (its draw call is `li r3,0` on GC) |
| 2 | RING | `ring` | Ring 0x196E8, o_ring_0, o_ring_8 (magnet), RingEnd, CreateRingObject/Task, RingModelDest/Exec/Disp/Disp2, group link/unlink |
| 3, 4 | RINGLINE, RINGCIRC | `ring` (reused) | `ring_group.rs` (stg13D), masked-identical |
| 5, 6 | ITEMBOX, ITEMBOXAIR | `itembox` | exec, init, grow, wait, break, dest, the 11 item functions each, displays |
| 8, 9 | CONTWOOD, CONTIRON | `containers` | set-up, delete, exec (fall, break, pieces, item drop) |
| 10, 13 | SKULL, PSKULL | `skull` | load, set-up, exec, rider lookup, shared display 0x1DA40 |
| 11, 12 | UDREEL, STOPLSD | `udreel_stoplsd` | UDREEL load/exec/displays, shadow helper 0x746C; STOPLSD load/exec |
| 14 | SCHBOX | `schbox` | set-up, exec, SearchBoxMan, PauseHint (destructor shared with HINTBOX) |
| 15 | HINTBOX | `hintbox` | set-up, exec, destructor |
| 17-21 | SPHERE, CCYL, CCUBE, CWALL, CCIRCLE | `colli_prim` | the five execs and the size helper 0xBE94 |
| 22 | MSGER | `messenger` | create, spawner, destroy, collision child, message task, trail and shadow displays |
| 23 | BIG THE CAT | `big_the_cat` | 0x1B7FC (on GC only the range check) |
| - | itemBoxManager (HUD queue) | `itembox::queue` | callback 0x9FB8, spawn, update + 3 modes, dest, display layout |
| - | Tobitiri (scattered ring) | `ring` | update 0x19FF0; setup 0x19D70 reused from `finalhazard` ring_drop |
| - | trail plugin (table 0x801E77D8) | `messenger::trail` | alloc 0x635C, steps 0x6478/0x6938, dispatch 0x6E50, free 0x6EA4, drop-oldest, clip |
| - | effects | `effects` | 10 particle creators, their execs, SpSparkExec manager |
| - | Omochao (MSGER mode 2) | `messenger::omochao` | exec 0x156CC, display 0x17468 (logic; draws are events), follow 0x14DEC, steer 0x14AC8, turn limit 0x1451C, placement 0x14608/0x148A4, hints 0x14344/0x14484, holder 0x6300, force query 0x7558 |
| - | movable objects (moving floors) | `dyn_land` | probe 0x378C, ceiling 0x38AC, ground 0x3924, walls 0x3B68, physics step 0x491C, free/create 0x49D0/0x49F8, set-up 0x4B44, release 0x4BA8, update 0x4BE0; main.dol fn_80012EA4 and ObjectMovableInitialize 0x800DE884 |
| - | player contact (main.dol) | `player_contact` | 30 routines: CCL hit searches and hit lists, IsThisTaskPlayer, the container attack checks fn_800368A0/fn_8003699C, SetVelocityP, SetInputP, fn_80039EB0, fn_800399BC, fn_8003A54C, On/OffControl3D, AddScore, AddNumRing, AddMechHP, 1-up fn_800641B8, the item effects fn_800373B8/fn_800374E0/fn_800375FC/fn_80037718, the vibration request fn_8002FB2C |

Rows 7 (ORI) and 16 (EFFOBJ0) are ported by the Prison Lane work
(`ori.rs`, `effect_emitter.rs`) and are not repeated here.

The stage setup (stg42D +0x2E84..+0x2F1C) only stores addresses into
main.dol's hook slots, each when it is still empty: the Tobitiri setup
0x19D70 at 0x801A2A18, the item callback 0x9FB8 at 0x801A2A20, and the
trail routines into the table at 0x801E77D8 (+4 alloc, +8 step, +0xC free).
These are the GSNE8P values from the disc REL's own relocations. The Steam
install's `resource/gd_PC/*.rel` are a different build linked against
another main.dol (its main.dol data imports sit about 0x400 lower, e.g.
0x801A2638, 0x801A2640, 0x801E73D8) and must not be paired with GSNE8P's.

## Engine boundary

main.dol routines the objects call are hooks on each module's host trait,
named after their GSNE8P address (CheckRangeOut, CCL_*, task creation and
freeing, SET state, sound, score, rings, rendering, player state). Math the
objects use runs on the crate's existing native ports: `ninja_trig`,
`ninja_matrix`, `msl_math`, `enemy_lib::engine`, `player_range`,
`land_contact::invert`, `land_nodes::translate`, `color_blend`,
`solid_box::fall_step`, `damage_ring::gravity_face`.

The player-contact routines the objects call are native in
`player_contact` (checked against main.dol itself, every byte they write).
Each object host can answer its contact hooks with them; the module lists
which function answers which host method. PSKULL and Omochao run their
movable body through `dyn_land`.

## Verification

The tests are `#[ignore]`d and need local files from the user's own disc,
never committed:

* `SA2_MAIN_DOL`: GSNE8P main.dol
* `SA2_GC_REL_DIR`: the folder of GameCube stage RELs (stg42D.rel and the rest)
* `SA2_PPC_PYTHON` (or `python` on PATH): a Python with Unicorn

Each oracle (`tests/stage_objects_ppc.py`, `tests/so_*_ppc.py`, sharing
`tests/so_harness.py`) loads main.dol into the `dol_ppc` harness, loads and
relocates stg42D.rel, stubs only the engine routines above (recording them
as events) and runs the REL's own code on randomized cases. The Rust tests
replay every case and compare events and state bit for bit.

    cargo test --release --test stage_objects_shared --test stage_objects \
      --test so_bigcat --test so_containers --test so_effects --test so_hintbox \
      --test so_itembox --test so_messenger --test so_ring --test so_skull \
      --test so_dynland --test so_omochao --test so_contact -- --ignored

## Not ported

* Display bodies that only render: HINTBOX 0x11DF0 (and 0x11608, 0x11D64),
  SCHBOX 0x1CB18/0x1CCD4/0x1CF3C/0x1BF34, CONTWOOD/CONTIRON 0xF380, 0xF42C,
  0xFF28, 0x10034, spark display 0xDF88, the item icon quads 0x9C08/0x9D08,
  the explosion sphere 0x7684. They are hooks.
* ORI's helpers 0x193EC/0x1949C and its rocket table 0x1B824/0x1B878.
* Engine calls the contact routines make that are not player contact: sound,
  voice and jingles, the barrier and invincibility effect tasks
  (fn_800BE308/fn_800BE374/fn_800BE408), task creation and freeing, and the
  2P ring rank fn_80063E34 (needs the unported fn_800736F0/fn_8001DDA8).
* Omochao's message window (fn_800E00E8/fn_800E018C/fn_800E0118), player
  bone position fn_80038008, the 0x1DFE0 trail effect (the only existing
  port, `duel_debris`, targets another main.dol) and its draw calls.
* The walls routine's fallbacks for a zero unit-vector estimate (0x3DD4..,
  0x40C0.., 0x43AC.., 0x4694.., 0x47B4..0x4854) are ported but no oracle
  case reaches them; they appear unreachable.
* Paths where the original reads through a null pointer or overruns a work
  block (a 21st queued item, failed allocations): the port stops or skips
  instead; the oracles do not exercise them.
