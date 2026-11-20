# Metal Harbor (story level 10): stage-specific code

Code reference: GameCube disc `stg10D.rel` (module 10, SHA-256
`5de85d7f…19ac`) paired with GSNE8P main.dol. The Steam
`resource/gd_PC/stg10D.rel` (module 9, SHA-256 `46898f02…6c53`) has
byte-identical sections, so `metal_harbor::MetalHarbor::read` takes either;
engine calls are always reported at GSNE8P addresses. All offsets below are
stg10D section-1 (code) offsets.

## Which routines are stage-specific

Each of the 990 functions of the decomp's `config/GSNE8P/stg10D/symbols.txt`
was hashed over relocation- and branch-masked code (the method of
[STAGE-OBJECTS.md](STAGE-OBJECTS.md)) and looked up in every other REL with a
symbol list. 880 appear in ten or more other modules (shared stage objects,
the shared library, Tobitiri, rings, springs and so on) and are owned
elsewhere. The other 110 (plus the weak MSL `sqrtf`) are Metal Harbor's own:
81 are shared only with `stg57D` (the stage's two-player version), the rest
with a few other stages (stg06, stg25, stg30, stg40, stg59; one with
boss_fdog, three with the stg12/15/20/51 group), 4 with no other module.

## Ported (this slice: the stage program and its stage task)

`_prolog` (+0xA640) passes the program "STG10" (.data+0x1599BC) to
LoadSubprogram; its three entries and the stage task the setup creates:

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0xA66C | 0x3F0 | setup | `setup`: 34 or 35 ordered engine/module calls, hook slots 0x801A2A18, trail table 0x801E77D8, task callbacks |
| +0xAA5C | 0x120 | teardown | `teardown` |
| +0xACD4 | 0x54 | program exec | `exec` |
| +0xAB7C | 0x158 | speed probe | none: reads player 0's speed, computes a value and drops it; the oracle checks it writes nothing outside its frame |
| +0xAD28 | 0x16C | stage task exec | `task_exec` (sky mode flags, .data+0x117C, BGM restart) |
| +0xAE94 | 0x14 | stage task destructor | none: reads its work pointer into its own frame only |
| +0xAEA8 | 0x184 | stage task display | `display` (camera latch at .bss+0xDC, sky draw calls) |
| +0xB02C | 0x120 | stage task second display | `display2` |
| +0x13DC | 0x120 | five-entry drift list (.bss+0, count .bss+0x78) | `drift` |
| +0xA3B0 | 0xE0 | texture override install (.data+0x154404 into 0x801E5D28) | `install` |
| +0xA490 | 0x1B0 | player reset and override restore | `restore` |
| +0x106A8 | 0x13C | follower task spawn (exec +0x107E4) | `spawn` |

Every constant is read from the module through the relocation of the
instruction that loads it (rodata 0x348–0x380, 0x68; data 0x1170, 0x117C,
0x1588D8, 0x154404 …), never typed in. The bss/data locations are checked the
same way.

## Verification

`tests/metal_harbor_ppc.py` loads GSNE8P main.dol into the `dol_ppc` harness
with stg10D.rel relocated (`stage_rel_ppc`), stubs every main.dol routine the
module branches to (asserting each reported address is a real import of the
GC module) and every module helper outside the routine under test, and runs
the REL's own code on 965 randomized cases. `tests/metal_harbor.rs` replays
each with `metal_harbor::replay` and compares the ordered calls and every
written location bit for bit, for the GC module and, with `SA2_GAME_PATH`,
the Steam one:

    SA2_MAIN_DOL=… SA2_GC_REL_DIR=… SA2_PPC_PYTHON=<python with unicorn> \
    SA2_GAME_PATH=<Steam SA2> cargo test --release --test metal_harbor -- --ignored

Without those files the test is ignored (and skips when forced).

## Not ported yet (98 routines)

Left for the next Metal Harbor slices, as `module` calls where this slice
reaches them: the sky model draw +0x1594 and the stage objects +0x0–+0x9CC0,
+0xA250, +0xB14C–+0xB3FC, +0xF9D4, the follower exec +0x107E4 (with its
callbacks +0x10A34/+0x10A38), +0x12958, +0x158B4 and +0x18570. Helpers this
slice calls that are shared library routines (+0xB500 light loader, +0x10C98,
+0x10D34, +0x15298, +0x158E0, +0x1582C, +0x3261C, +0x326A0) belong to their
shared owners.

Fails closed instead of reading past the work: a drift count above five, a
negative follower player index, and a reset player whose index names no
present player (the original reads through it).
