# Knuckles vs Rouge (stage 33): stage-specific code

Code reference: GameCube disc `stg33D.rel` (module 33, SHA-256
`0de8a3a7…9585`) paired with GSNE8P main.dol. The Steam
`resource/gd_PC/stg33D.rel` (module 32, SHA-256 `9f2d0961…9942`) has
byte-identical sections, the same module-local relocations and the same
main-program import sites, so `knuckles_rouge::KnucklesRouge::read` takes
either and nothing had to choose between them. Engine calls are reported at
GSNE8P addresses; every one is checked against the relocation of its `bl`
(the address itself on the GC module, the relocation kind on Steam). All
offsets below are stg33D section-1 (code) offsets.

## Which routines are stage-specific

Of the 488 functions in the decomp's `stg33D/symbols.txt`, 47 match fewer
than ten other RELs after relocation and branch masking (the method of
[STAGE-OBJECTS.md](STAGE-OBJECTS.md)):

- 24 with no other module, 3 shared only with stg32D (LIGHTEFF), 1 with
  stg20D (+0x108) and +0x421C (stg31/32/43/44);
- +0x1A9D8/+0x1AB0C (ITEMBOXBAL GEN), shared with stg15/20/29/39/45/53/55/56;
- 16 routines +0x2D46C…+0x2E708: the boss title card group shared with
  stg42D +0x20DD4…+0x22070 (and stg19/20/29, the Golem, Last and Bogy
  bosses). They are already ported from stg42D in `duel_title` and
  `duel_display`; stage 33 code reaching them is reported as `module` calls.

## Ported (this slice)

`_prolog` (+0x38E0) passes "STG33" (.data+0x15370) to LoadSubprogram.

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0x390C | 0x320 | setup | `setup`: 29–33 ordered calls, hooks 0x801A2A18 +0/+8/+0x20, trail table 0x801E77D8, stage task callbacks, playertwp[1]+2 |
| +0x3C2C | 0xFC | teardown | `teardown` |
| +0x3D28 | 0x4 | program exec | `blr`: no effect |
| +0x3D2C | 0x154 | stage task exec | `task_exec`: texture table .data+0x15528, colour lerp, 13-frame counter into .data+0x15236 |
| +0x3E80 | 0x4 | stage task destructor | `blr`: no effect |
| +0x3E84 | 0x398 | stage task display | `display`: sky, three spinning models, model list, last model; skipped per split-screen mode |
| +0x421C | 0x124 | model-list draw | `model_list` (descriptor .data+0x8AB4: 8 entries) |
| +0x3354 | 0x128 | rival director spawn | `director_spawn` |
| +0x3664 | 0x27C | sparkle spawn | `sparkle_spawn` |
| +0x35E0 | 0x60 | sparkle exec | `sparkle_exec` |
| +0x347C | 0x164 | sparkle display | `sparkle_display` |
| +0x4340 | 0x70 | LIGHTEFF loader (row 37) | `light_load` |
| +0x4590 | 0x188 | LIGHTEFF exec | `light_exec` |
| +0x43B4 | 0x1DC | LIGHTEFF display | `light_display` |
| +0x1AB0C | 0x30 | ITEMBOXBAL GEN loader (row 14) | `balloon_load` |
| +0x1A9D8 | 0x130 | ITEMBOXBAL GEN exec | `balloon_exec` (content table .data+0x21F50) |

Constants are read through the relocation of the instruction that loads
them (rodata 0x0, 0x10, 0x4C–0xC4, 0xC8–0xF0, 0x868–0x870; data 0x15528,
0x15550–0x15584, 0x8AB4, 0x21F50 …). The task-exec and display data words
are read from the module's initialised .data; nothing else in stg33D writes
them. Engine results (rand, njSin, fn_800334B0, njCalcPoint depth, task and
memory allocation) are inputs.

## Not ported yet

- The rival director, task exec +0xCE8 (0x261C bytes, jump table
  .data+0x3B7C) with its display +0x40 and helpers +0x0, +0x108 (same masked
  bytes as stg20D +0x0, reached from stage20's arena bounds), +0x264,
  +0x3C0, +0x440, +0x4F0, +0x57C, +0x5C4, +0x734, +0x7EC, +0x80C, +0x988,
  +0xA08, +0xB6C, +0xC48: 16 routines, the next stage-33 slice.
- Shared code reached here is owned elsewhere and reported as `module`
  calls: +0x1B0B8, +0xBD0C, +0xBD94 (fog loader), +0x1B13C, +0xAD60,
  +0xA8C8, +0x20C5C (DamegeRingScatter), +0x741C/+0x7F10/+0x7F64 (trail),
  +0x2CB08, +0x6C10, +0x19D08 (ITEMBOXBALLOON), +0x3644 (parent
  destructor, 47 other RELs).

Fails closed on a negative model-list count (the original would loop about
2^32 times) and on a balloon content index outside .data.

## Tests

`cargo test --release --lib knuckles_rouge`. With `SA2_GC_REL_DIR` (the
disc `rel/` folder) and `SA2_GAME_PATH` (the Steam install) the tests also
read both RELs, compare their sections, and run every routine on fixed
inputs; without them only the rejection test runs.
