# Sonic vs Shadow 1 (stage 19): stage-specific code

Code reference: GameCube disc `stg19D.rel` (module 19, SHA-256
`cf88d26b…1afd`) paired with GSNE8P main.dol. The Steam
`resource/gd_PC/stg19D.rel` (module 18, SHA-256 `ce44ab18…88fa`) has
byte-identical sections (all six compared), so `stage_code::sonic_vs_shadow1::read`
takes either and nothing differs between them. Engine calls are reported at
GSNE8P addresses. All offsets are stg19D section-1 (code) offsets.

`stage_code` (src/stage_code/mod.rs) is the shared reader for these ports:
constants and pointers come from the relocation of the instruction that
loads them, and on the GC copy every reported engine address is checked
against the import relocation of the `bl` or `lis`/`addi` that names it
(the audit of every 0x80xxxxxx literal against GSNE8P). Engine results a
routine uses are inputs (`returns`, consumed in call order).

## Which routines are stage-specific

Of the 397 functions in the decomp's `config/GSNE8P/stg19D/symbols.txt`, 70
have masked bytes in fewer than ten other RELs:

- 10 are stage 19's own (no other module, or the stg20D/stg29D program exec,
  or the stg42D facing check): ported here.
- 44 (+0x145C–+0x5418) are the forest stage objects shared with stg03D
  (Green Forest), 24 of them also with stg04D: TOBOKU, TOBOKU_FAR, JP,
  JP_FAR, KARAMI, ROLTUTAT, SHOUKOU, SHOUKOUK, JO, JO_FAR, TESTWOOD and SAKU
  with their helpers. They are not ported in this slice (see below).
- 16 (+0x232E8–+0x2499C) are the boss gauge/title-card group shared with
  stg42D and seven boss modules: already ported from stg42D as `duel_title`,
  `duel_gauge` and `duel_display` (same masked bytes; stg19D offset =
  stg42D offset + 0x2514). They are reported as `module` calls here.

## Ported

| Offset | Size | Routine | Port |
| --- | --- | --- | --- |
| +0xE78 | 0x2FC | setup (program "STG19" at .data+0x6B9C, entry 1) | `setup` |
| +0x1174 | 0xF8 | teardown (entry 2) | `teardown` |
| +0x126C | 0x100 | program exec (entry 3, also stg20D/stg29D) | `exec` |
| +0x136C | 0x4C | stage task exec (texture id cycle 0..13) | `task_exec` |
| +0x13B8 | 0xA4 | stage task display (sky models at height 920) | `task_display` |
| +0xD30 | 0x11C | director task create ("ControlShadow") | `director_create` |
| +0x38C | 0x954 | director exec (nine-entry jump table .data+0x218) | `director_exec` |
| +0x0 | 0x138 | arena clamp (centre .data+0x100, radius .data+0x10C) | inside `director_exec` |
| +0x1C4 | 0x130 | catch-up test | inside `director_exec` |
| +0x2F4 | 0x98 | facing test (= stg42D +0x334) | inside `director_exec` |

Module helpers they call that are shared code are `module` calls: the inline
`sqrtf` +0x138 (61 RELs; its result is an input), the light loader +0xD204,
+0x94E0/+0x957C, +0xD17C, +0x16708, +0x1678C, the trail callbacks
+0x7FE8/+0x8ADC/+0x8B30, the hook routines DamegeRingScatter +0x1C284,
+0xC1D0 and +0x95A4, the message effect +0x1AB34 (stg42D +0x18620), the
gauge creator +0x22984 (stg42D +0x20470) and the title card +0x24584
(stg42D +0x22070). The director's two name-card displays +0xCE0/+0xD08 are
shared (73 RELs).

## Left out

- The forest objects (44 routines above): they belong with stg03D, Green
  Forest, the next stage on the list; its PR ports them once for both modules.
- Stack bytes the original leaves unset (the exec's request block +0x12/+0x13)
  are not reported; the original passes whatever was on the stack.
- The value `DestroyTask` leaves in the director task's child word is engine
  state: it is an input (`returns["task+0xc"]`).

## Dependencies (unported engine or other owners)

fn_8001CD40, fn_8001DDA0/DDA8, fn_80022E24/E64, fn_800399BC (set player
position), fn_8003A54C, fn_800C7AAC/800C8544/800C3BDC/800C77A8 (camera
requests), fn_80072820 (texture load), CreateElementalTask, syCalloc,
FreeTask, DestroyTask, rand, atan2f, DiffAngle, njDistanceP2P, njSin/njCos,
the fog and ninja draw calls, StgPlayMusic, BGM_SetVolume.

## Verification

`tests/sonic_vs_shadow1.rs` runs every routine on the GC REL
(`SA2_GC_REL_DIR`) and the Steam REL (`SA2_GAME_PATH`): setup/teardown hooks
and lights, the exec's two modes, the texture cycle, the display position,
director mode 0/1/7 transitions, the restart reset and the arena pull-back.
Without the files they skip. No emulator oracle (by request).
