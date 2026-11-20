# Dry Lagoon (story level 18): stage-specific code

Code reference: GameCube disc `stg18D.rel` (module 18, SHA-256
`6dc9427c…04fc`) paired with GSNE8P main.dol. The player's Steam
`resource/gd_PC/stg18D.rel` (module 17, SHA-256 `edca61dc…fc00`) is accepted
too. All offsets below are GC section-1 (code) offsets; engine calls are
reported at GSNE8P addresses.

## Steam and disc differ here

The data sections (rodata, data, bss size) are byte-identical. The code
section is not: Steam's is 0x4C bytes shorter. Comparing the two with branch
displacements masked leaves exactly two deletions:

- setup GC +0xDA88..+0xDAAC (0x24 bytes): installing fn_18_2B7A0 into the
  empty hook slot 0x801A2A18+0x14;
- teardown GC +0xDC3C..+0xDC64 (0x28 bytes): clearing that slot again.

Steam is followed: `dry_lagoon::setup` never fills slot +0x14 and `teardown`
never clears it, whichever copy is read. Reads of the Steam module map GC
offsets across the two cuts (`STEAM_CUTS`).

## Which routines are stage-specific

`stage-port-tools/lists/stg18D.tsv` (relocation- and branch-masked hashes
against every other REL) lists 141 functions shared with fewer than ten other
modules. Most are shared only with `stg46D` (the two-player Dry Lagoon).

## Ported (this slice: the stage program, its tasks and the water helpers)

`_prolog` (+0xD748) passes the program "STG18" (.data+0xD223C) to
LoadSubprogram.

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0xD774 | 0x46C | setup | `setup`: BgClip and BgExec tasks, lights, music (`d_lagoon.adx` / `t9_rouge.adx` by the 2-player byte), hook slots, trail table 0x801E77D8, water task |
| +0xDBE0 | 0x208 | teardown | `teardown` |
| +0xDDE8 | 0xC4 | program exec | `exec`: Bg2Exec at the rodata 0x464 position, then exec = +0xDEAC |
| +0xDEAC | 0x20 | program exec, later frames | `exec_after` |
| +0xE704 | 0x24 | BgClip exec | `bg_clip` |
| +0xDECC | 0x1BC | BgExec exec (ambient sound, 90-frame random extra sound) | `ambient` |
| +0xE088 | 0x5E8 | BgExec display (water switch, three animated water layer blocks) | `display` |
| +0xE670, +0xE6E4 | 0x20 each | BgExec / Bg2Exec destructors | `destructor` |
| +0xE690 | 0x4 | Bg2Exec exec | none: returns at once |
| +0xE694 | 0x50 | Bg2Exec display | `bg2_display` |
| +0x98C0 | 0x17C | camera region | `camera_region` |
| +0x9A3C | 0x110 | store region | `store_region` (n other than 0 or 1 writes through an unset register in the original: refused) |
| +0x9B4C | 0xAC | water height by region | `water_level` |
| +0x9D48 | 0x170 | water surface in view | `surface_visible` (tanf and njCalcPoint are engine calls; their results are inputs) |
| +0xA80 | 0x190 | water task spawn | `water_spawn` |
| +0x17128 | 0x2C | watched-object reset (shared with stg07/08/10/24/38/57) | `watch` |

Every constant is read through the relocation of the instruction that loads
it (rodata 0x278–0x2AC, 0x450–0x4B4; data 0xD1EC8–0xD20C4 texture-animation
tables, 0xD22D4–0xD22E0), and immediates (the 90-frame period, sound ids,
allocation sizes) from their instructions. `display` fails closed where the
original stores through a missing water task.

## Verification

`tests/dry_lagoon.rs` runs every routine on the GC REL and the Steam REL
(`SA2_GC_REL_DIR`, `SA2_GAME_PATH`); each engine call is checked against the
GC module's own relocation (a wrong address fails), and both copies must give
identical results. No PowerPC oracle (by plan).

## Not ported yet (124 routines)

Reported as `module` calls where this slice reaches them: the water above /
below handlers +0xC028 and +0xBAA0, the water task's exec +0xC10 and its
callbacks +0xE8C/+0xF80, +0x75C/+0x894, and the stage's own objects: KAME
(+0x5AA4…+0x7F4C, the turtle), WIND/SIDEWIND, PLANT2/3, POOL, WARP, CANOE,
PALM, LEAF, FLOWER, CHIN/CHIN2/CHIN3, TSUBO, MINITSUBO, FISH, BUTTERFLY,
STAKE, SHELF, DLCONT and EFLENSF0. Shared helpers called here (+0xE728 light
loader, +0xD570, +0xD5E4, +0x12C9C, +0x12D38, +0x16B0C, +0x170A0, +0x17154,
+0x24928, +0x24944, +0x28E4C, +0x28EA0, +0x3428C, +0x34208, +0x4718C, +0x11358)
belong to their shared owners.
