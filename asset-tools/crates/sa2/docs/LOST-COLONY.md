# Lost Colony (story level 27): stage-specific code

Code reference: GameCube disc `stg27D.rel` (module 27, SHA-256
`a41ae2d3…144f`) paired with GSNE8P main.dol. The Steam
`resource/gd_PC/stg27D.rel` (module 26, SHA-256 `ec0ab22a…b344`) has
byte-identical sections (every section compared), so
`lost_colony::LostColony::read` takes either and every value ported here is
the same in both. Engine calls are reported at GSNE8P addresses. Offsets are
stg27D section-1 (code) offsets.

## Which routines are stage-specific

`lists/stg27D.tsv` (relocation- and branch-masked hashes of the decomp's 942
functions, looked up in every other REL) leaves 69 functions found in fewer
than ten other modules. Most are shared only with `stg55D` (the stage's
two-player version); 13 appear in no other module.

## Ported (this slice: the stage program, its stage task and the light tasks)

`_prolog` (+0x632C) passes the program "STG27" (.data+0x672BC) to
LoadSubprogram.

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0x6358 | 0x368 | setup | `setup`: 37 ordered calls with a stage task; .bss+0x8C8, hooks 0x801A2A18 +0/+8/+0x28, trail table 0x801E77D8, spot-light hook 0x803ADC30 = +0x6AE0 |
| +0x66C0 | 0x114 | teardown | `teardown` |
| +0x6DC0 | 0x234 | program exec | `exec` (light level easing, fog blend request, target decay with .bss+0x90 bias and .bss+0x94C count) |
| +0x12E18 | 0x38 | fog blend request | `blend` (also applied inside `exec`) |
| +0x7184 | 0x20 | stage task first exec | `task_start` |
| +0x6FF4 | 0x190 | stage task exec | `task_exec` (per-player mode 3 latch, three glowing models of .data+0x673CC) |
| +0x71A4 | 0x84 | stage task destructor | `task_dest` |
| +0x7228 | 0x3F4 | stage task display | `display` (GX light 2 at the player, land table .data+0x522F0) |
| +0x761C | 0x318 | stage task second display | `display2` (glowing table .data+0x5316C, 13-frame texture table .data+0x52380) |
| +0x6AE0 | 0x2E0 | spot-light hook | `hook` |
| +0x67D4 | 0xD4 | light task exec | `light_exec` (shared with stg11/55/58; none ported before) |
| +0x68A8 | 0x7C | light task destructor | `light_dest` |
| +0x6924 | 0x1BC | light task display | `light_display` |

Every constant is read from the module through the relocation of the
instruction that loads it (rodata 0x2A8–0x32C; data 0x673A8, 0x673BC–0x673C8,
0x673E4, 0x673E8; land tables 0x522F0, 0x5316C, 0x52380; texture list
0x4B46C), never typed in. `njSin` uses `ninja_trig::nj_sin` (GSNE8P
0x80112398's table). Values the engine computes are inputs where the code
branches or computes on them (view depths, distances, transformed vectors)
and names (`"v"`, `"p"`, `"cam"`, `"ret 0x800334b0"`) where they only pass
through to another engine call.

## Verification

`tests/lost_colony.rs` runs every routine on the disc REL (`SA2_GC_REL_DIR`)
and the Steam REL (`SA2_GAME_PATH`), checks decoded values (0.38 easing, 0.96
decay, 0.8 count weight, slot routines, the light-bit mask) and that both RELs
give identical results. It skips without the files. No PowerPC oracle (by
request: fastest decent plan).

## Not ported yet (56 routines)

The stage's own objects, left as later slices: +0x60–+0x6088 (44 routines,
mostly with stg55), +0x7954–+0x86E8 (8 routines with stg22/24/55),
+0x8DAC, +0xBB58, +0xBE6C, +0x23E0. Helpers this slice calls that are shared
library code (10+ RELs) are reported as `module` calls and belong to their
owners: +0x9394, +0x6154, +0x61C8, +0x2F8E4, +0x2F860, +0x24840, +0x247EC,
+0xE660, +0xE664, +0x12E50, +0x127FC, +0x12D90, +0x48BF0, +0xC44C,
+0x3513C (DamegeRingScatter), +0x11DE4, +0xE668, +0xCF28, +0xDA1C, +0xDA70.

Fails closed instead of reading past the work: a light slot byte outside
0..32 (.bss+0x10 holds 32 slots), a negative land-table count, and a depth
list whose length differs from the table's record count.
