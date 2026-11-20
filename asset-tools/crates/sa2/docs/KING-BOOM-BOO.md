# King Boom Boo (story level 63): stage-specific code

Level 63 is King Boom Boo: the native Hero sequence and
[STAGE-LAND-PARITY.md](STAGE-LAND-PARITY.md) map 63 to `boss_bigbogyD.rel`
(64 is Egg Golem, `Boss_GolemD.rel`).

Code reference: GameCube disc `boss_bigbogyD.rel` (module 85, SHA-256
`73ba8086…c130`) read against GSNE8P main.dol symbols. The player's Steam
`resource/gd_PC/boss_bigbogyD.rel` (module 84, SHA-256 `ab634339…362f`) is a
different build, and `king_boom_boo::KingBoomBoo::read` takes either:

- Section 1 (code) is identical instruction for instruction up to +0x197A0.
  There a routine shared by 70 RELs is 0x2F4 bytes shorter on Steam, so every
  later code offset is 0x2F4 lower. Module-local calls and code pointers are
  reported in the GameCube layout (`code_offset`).
- Section 4 (.rodata) differs only past +0xE00 and section 5 (.data) only past
  +0x42B18. Nothing ported here reads those ranges.
- Constants are always read from the file passed in, so the Steam values are
  the ones used for Steam files. For everything ported here, the two builds
  give the same output. The unit test runs both files and compares them.

Engine calls are reported at GSNE8P addresses. Each call is checked against the
module's import relocation at its call site. For the GameCube file, the
relocation must name that address.

## Which routines are stage-specific

`lists/boss_bigbogyD.tsv` (functions whose relocation- and branch-masked bytes
appear in fewer than 10 other RELs) has 230 rows, 0x12D74 bytes. 204 rows
(+0x0 to +0x110EC) are this module's own code: 183 appear in no other module, the rest are shared with
`stg28D` (the SANDGLASS object) or other bosses. The 26 tail rows (+0x375E0 to
+0x58068) are the boss group's shared library, used by up to nine other
boss/duel RELs.

## Ported (stage program, fence, boss spawn)

`_prolog` (+0xEBF0) passes the program "BIGBOGY " (.data+0x3E118) to
LoadSubprogram:

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0xEC1C | 0x2A0 | setup | `setup`: 29 ordered engine/module calls, hook slots 0x801A2A18 (+0, +4), trail table 0x801E77D8, set-file handle .bss+0x90 |
| +0xEEBC | 0xE8 | teardown | `teardown` |
| +0xEFA4 | 0x88 | program exec | `exec`: spawns the boss at (0, -40, 0) with the arena centre .rodata+0xA50 = (200, -40, 0), then installs the shared exec +0xF124 |
| +0xF02C | 0xF8 | arena fence (called by +0xF124) | `fence`: pulls player 0 back onto the radius-260 (.rodata+0xA7C) circle around the centre in XZ |
| +0x584 | 0x3FC | boss spawn | `spawn`: task (exec +0x7498), 0xE4-byte work, motion, gravity -0.02, collision (.rodata+0x2A0, 11 shapes), helper tasks, 256x256 texture buffer |

Strings the setup loads, as read from .data: `set_b_BigBogy_s.bin`,
`set_b_BigBogy_u.bin`, `modeltex_bossbogy`, `efftex_bossbogy`,
`titletex_bossbogy`, `boss_02a.adx` and `mh0063`.

The spawn reproduces the original's distance at mwp+0xC4: it subtracts
centre.z from position.x (not position.z). The fence's length and unit vector
are computed in single precision, the way njScalor and njUnitVector work. They
are not checked bit for bit against the engine.

## Not ported yet

- The boss AI, display and collision: +0x0 to +0xDBEC, including exec +0x7498,
  display +0x77BC and +0x8010, and destructor +0x980.
- The DOOR object: +0xDBF0 to +0xEB94, with its four-mode table at
  .data+0x3C634.
- The SANDGLASS object: +0xF2B4 to +0x110EC, shared with `stg28D`.

In the ported routines, these appear as `module` calls. Helpers this slice
reaches that belong to shared owners: +0x2C6B4 and +0x2C630 (message bank,
61-62 RELs), +0x13E08, +0x148FC and +0x14950 (trail plugins, 68 RELs),
DamegeRingScatter +0x32240 and +0x1C078 (hook routines, 64-65 RELs), and the
boss-group library +0x562BC and +0x58068.
