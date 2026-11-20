# Engine map of main.dol (E0, November 20, 2026)

Milestone E0 of [ENGINE-CORE-PLAN.md](ENGINE-CORE-PLAN.md), issue
[#440](https://github.com/mobdon/ChaoGardenMMOClient/issues/440). Read-only
trace of the engine core in main.dol GSNE8P (SHA-1
`e4c50f32242a3cd36cdc8ae776661601a52130a2`) and, for the Iron Gate stage
module, stg11D.rel from the same disc (SHA-1
`8fbe506bcbbe6f012d2bb8240edb5f6c8cb6ece0`).

How it was traced: a local static decoder (linear PowerPC decode, `bl`
targets, `lis`/`addi` and `r13`/`r2` constant tracking, function bounds from
the reconstruction's `config/GSNE8P/symbols.txt`, REL relocations applied for
stg11D). Nothing was run and the game was not launched. Every address below is
a main.dol or stg11D address that the decoder read; names are symbols.txt names
or, where symbols.txt only has `fn_XXXXXXXX`, a role written in words. Anything
not established from the bytes is marked **unresolved** or **inferred**.
"G" below means the game-state block `lbl_801CC168` (0x801CC168).

## 0. Two address spaces (read this first)

The crate records many engine addresses from the RELs in the Steam install's
`resource/gd_PC`. Those RELs are linked against a **different main.dol build**
than GSNE8P. Aligning the module-0 relocations of Steam `stg11D.rel` / `stg42D.rel`
with the GSNE8P disc RELs (same relocation counts, monotone order; 372 code
pairs in stg11D) gives these offsets from Steam-REL addresses to GSNE8P:

| Steam-REL range (code) | Add to get GSNE8P |
| --- | ---: |
| 0x80005000–0x8001ADD3 | +0x0 |
| 0x8001ADD4–0x80022BC3 | +0x84 |
| 0x80022BC4–0x80027A7F | +0x248 |
| 0x80027A80–0x800DDCAB | +0x268 |
| 0x800DDCAC–0x800ECD77 | +0x2DC |
| from 0x800ECD78 | +0x348 (one outlier pair at +0x1558) |

Each boundary is the first aligned pair with the new offset, so an address
between two pairs on either side of a boundary has to be checked by hand.
Data: 0x801CBD88 (Steam) = 0x801CC168 (GSNE8P, +0x3E0); 0x801E4FF8 (Steam) =
0x801E53F8 (GSNE8P, +0x400); 0x801E51D8 = 0x801E55D8.

Consequences for the plan and the crate:

| Plan / crate address | Build | GSNE8P | What it is in GSNE8P |
| --- | --- | --- | --- |
| 0x80070724 | Steam | 0x8007098C | `FreeTaskC` |
| 0x80070790 | Steam | 0x800709F8 | `CreateChildTask` |
| 0x80071878 | Steam | 0x80071AE0 | `FreeTask` |
| 0x80071888 | Steam | 0x80071AF0 | `DestroyTask` |
| 0x80071BA4 | Steam | 0x80071E0C | `CreateElementalTask` |
| 0x80071EB0 / 0x80071FB0 / 0x80072008 | Steam | 0x80072118 / 0x80072218 / 0x80072270 | motion-table helpers (section 1.6) |
| 0x800725B8 | Steam | 0x80072820 | motion helper, 16 bl callers |
| 0x80072820 | Steam | 0x80072A88 | inside fn_800729F0 (no entry) |
| 0x801CBD88 | Steam | 0x801CC168 | G |
| 0x801E4FF8 ("camera block") | Steam | 0x801E53F8 | **per-player stick table**, not the camera (section 5) |
| 0x80065E54 | GSNE8P | same | an instruction inside the SET spawner fn_80065C40 (`lha` of the rotation), not a function entry |
| 0x800232A4, 0x8001DD78, 0x8000F8B8, 0x8001A13C, 0x8004F838, 0x800B56E8, 0x800B3A8C, 0x800AF558, 0x8004E560, 0x801E53F8, 0x801CC1E4 | GSNE8P | same | as named below |

So the "task module 0x80070790–0x80072820" of the plan is GSNE8P
**0x800706B0–0x80072080** (task code) followed by a motion-table cluster.

## 1. Task module

### 1.1 Task header (0x50 bytes, `GetTask` 0x80071D04 allocates with `MAlloc(0x50)`)

| Off | Field | Set by | Read by |
| --- | --- | --- | --- |
| +0x00 | next (circular list) | GetTask tail insert, CreateChildTask | walkers, DestroyTask |
| +0x04 | prev | same | DestroyTask |
| +0x08 | parent | CreateChildTask | DestroyTask (unlinks from parent's child ring) |
| +0x0C | first child (circular child ring) | CreateChildTask | walkers recurse into it; DestroyTask, FreeTaskC destroy children |
| +0x10 | exec | GetTask (`no_op` 0x80071F58 if null); FreeTask stores `DestroyTask` | exec pass |
| +0x14 | display | callers | display pass, first group |
| +0x18 | destructor | callers | DestroyTask calls it first |
| +0x1C | display, third group | callers | display pass (fn_80071858) |
| +0x20 | display, depth-sorted group | callers | display pass (fn_800706B0 / fn_80071718) |
| +0x24 | display, fourth group | callers | display pass (fn_800715D8) |
| +0x28 | display, fifth group | callers | display pass (fn_80071498) |
| +0x2C | shadow/pre-pass display | callers | fn_80071358 from fn_80070F5C (before the main pass) |
| +0x30 | SET object entry pointer | SET spawner | DestroyTask clears bit 0 of its halfword +0x2 |
| +0x34 | twp, `syCalloc(1, 0x30)` if im bit 0x2 | CreateElementalTask | CreateChildTask copies twp +0x08..+0x1C from the parent; DestroyTask frees; twp +0x2C non-null → `fn_800067F8(task)` first (collision release, **inferred**) |
| +0x38 | `syCalloc(1, 0x40)` if im bit 0x1 | CreateElementalTask | freed by DestroyTask |
| +0x3C | `syCalloc(2, 0x1C)` if im bit 0x4 | CreateElementalTask | freed by DestroyTask |
| +0x40 | `syCalloc(1, 0x10)` if im bit 0x8; **player tasks store their player work here** (0x454 bytes in the walker loaders) | CreateElementalTask, loaders | freed by DestroyTask |
| +0x44 | name pointer | GetTask (4th argument) | — |
| +0x48 | copy of the previous +0x44 word of the block (read before +0x44 is written) | GetTask | — |
| +0x4C | 0 | GetTask | — |

Notes from the bytes:
- `CreateElementalTask(im r3, level r4, exec r5, name r6)` 0x80071E0C:
  GetTask, then the four `syCalloc`s in the order +0x34, +0x3C, +0x38, +0x40;
  any failure calls DestroyTask and returns 0. The im mask is tested as a
  16-bit value.
- `CreateChildTask(im r3, exec r4, parent r5)` 0x800709F8: calls
  `CreateElementalTask(im, 7, exec, 0)` (level 7 is never linked into a list),
  appends to the parent's child ring, sets +0x08 = parent, and copies twp words
  +0x08, +0x0C, +0x10, +0x14, +0x18, +0x1C from the parent's twp when both
  exist.
- `FreeTask` 0x80071AE0 only stores `DestroyTask` into +0x10; the task is
  destroyed by the next exec pass.
- `DestroyTask` 0x80071AF0 order: current-callback global = +0x18, call
  +0x18(task); zero +0x10..+0x2C; destroy every child (loop on +0x0C); unlink
  from next/prev; if it has a parent and is the parent's first child, the
  parent's +0x0C becomes next (or 0 if alone); otherwise, if it is the head of
  one of the 8 lists, that list head becomes next (or 0); release twp collision,
  free +0x34, +0x3C, +0x38, +0x40; clear the SET entry bit; zero links; `syFree`.
  The branch that compares +0x10 with `FreeTaskC` reads +0x10 after it was
  zeroed, so it is never taken.
- `FreeTaskC` 0x8007098C (used as an exec): when it is the task's exec and the
  task has no children, exec becomes `DestroyTask`; when called with another
  exec, it destroys every child.
- Current callback global `makethread` 0x803ADA48 (r13−0x7878): every walker
  and DestroyTask store the callback they are about to call, then 0.

### 1.2 Lists and levels

- `btp` 0x801FCEB8: 8 list heads (levels 0–7), cleared by `clearBtp`
  0x80071F2C (called from the session init fn_80021828).
- `GetTask` links levels 0–6 at the tail of `btp[level]`; level 7 is created
  unlinked (child tasks).
- Level argument of `CreateElementalTask` (`li r4` before each `bl`, main.dol
  + stg11D): 0 ×52, 1 ×37, 2 ×25, 3 ×87, 4 ×2, 5 ×19, 7 ×1, 2 sites
  unresolved. No site passes 6, and no pass walks level 6 (below).
- im argument: 0 ×94, 2 ×77, 7 ×14, 8 ×34, 10 ×3 (main.dol + stg11D).

### 1.3 Every function in the cluster

Callers are `bl` sites in main.dol; "stg11D" counts REL24 relocations from
stg11D.rel into that function.

| Address | Size | Name / role | Callers | Task fields |
| --- | ---: | --- | ---: | --- |
| 0x800706B0 | 0x228 | depth-sorted +0x20 pass for one level: counts tasks with +0x20 and twp, `njCalcPoint` of twp+0x14, sorts by view-space Z (bidirectional bubble), calls +0x20, then fn_80071680 on children | 2 | R +0x00, +0x0C, +0x20, +0x34 |
| 0x800708D8 | 0xB4 | `LoopTaskC`: exec of a child ring, recursive | 2 | R +0x00, +0x0C, +0x10 |
| 0x8007098C | 0x6C | `FreeTaskC` | 0 (+12 stg11D) | R/W +0x10, R +0x0C |
| 0x800709F8 | 0xC4 | `CreateChildTask` | 29 (+26) | W +0x00, +0x04, +0x08, parent +0x0C, twp |
| 0x80070ABC | 0x74 | destroy every task of levels 0–7 | 1 (fn_8002176C) | — |
| 0x80070B30 | 0x78 | destroy every task of levels 2–5 | 7 | — |
| 0x80070BA8 | 0x78 | display frame, several viewports: fn_80117D38, fn_80117D74, fn_80117D14, fn_80070F5C, then fn_80070C54 per viewport while G+0x22 stays 0 (count in r3), GXFlush | 5 | — |
| 0x80070C20 | 0x34 | display frame, one viewport: same without the loop | 9 | — |
| 0x80070C54 | 0x308 | **display pass** (section 2) | 2 | — |
| 0x80070F5C | 0x2B8 | GX pre-pass: fog off, copy-texture setup, fn_80071358 (+0x2C displayers), GX state restore | 2 | — |
| 0x80071214 | 0x3C | **exec pass**: fn_80071A40 for levels 0..5 | 3 | — |
| 0x80071250 | 0x70 | destroy every task of one level | 0 | — |
| 0x800712C0 / 0x80071358 | 0x98 / 0xA8 | +0x2C walk: children / level | 2 / 1 | R +0x2C |
| 0x80071400 / 0x80071498 | 0x98 / 0xA8 | +0x28 walk | 2 / 3 | R +0x28 |
| 0x80071540 / 0x800715D8 | 0x98 / 0xA8 | +0x24 walk | 2 / 3 | R +0x24 |
| 0x80071680 / 0x80071718 | 0x98 / 0xA8 | +0x20 walk (unsorted) | 3 / 1 | R +0x20 |
| 0x800717C0 / 0x80071858 | 0x98 / 0xA8 | +0x1C walk | 2 / 3 | R +0x1C |
| 0x80071900 / 0x80071998 | 0x98 / 0xA8 | +0x14 walk | 2 / 3 | R +0x14 |
| 0x80071A40 | 0xA0 | exec walk of one level (+0x10), children through LoopTaskC | 2 | R +0x10 |
| 0x80071AE0 | 0x10 | `FreeTask` | 175 (+66) | W +0x10 |
| 0x80071AF0 | 0x214 | `DestroyTask` | 129 (+47) | all |
| 0x80071D04 | 0x108 | `GetTask` (level, exec, name) | 1 | all |
| 0x80071E0C | 0x120 | `CreateElementalTask` | 161 (+64) | +0x34..+0x40 |
| 0x80071F2C | 0x2C | `clearBtp` | 1 | — |
| 0x80071F58 | 0x4 | `no_op` (default exec) | 0 | — |
| 0x80071F5C, 0x80071FA8, 0x80072030 | 0x4C, 0x88, 0x50 | buffers at 0x803ADA50..0x803ADA5C (alloc in boot fn_800D2834, free at exit in `main`, fill in fn_80071FA8); role **unresolved** | 1, 2, 1 | — |

Walker detail: the exec walk (0x80071A40) re-reads `btp[level]` after each
task, so a task that destroys the list head does not break the walk; it saves
`next` before calling. The display walks save `next` but do not re-read the
head. Children are walked after their parent.

### 1.4 What follows the cluster

0x80072080–0x80073324 is a separate cluster that builds motion/animation
tables (0x80072820 has 16 `bl` callers and 17 stg11D imports; fn_800729F0 and
fn_80072998 are called around stage init/exit). It is not part of the task
system. Its role is **unresolved** beyond that.

## 2. Per-frame passes

### 2.1 Exec

`fn_80071214` 0x80071214: `fn_80071A40(level)` for level = 0, 1, 2, 3, 4, 5.
Level 6 is never executed. Callers: the in-stage tick fn_8001E45C, and mode
cases of fn_8001F368 (0x8001F63C, 0x8001F678).

### 2.2 Display (fn_80070C54 0x80070C54), in order

1. `fn_8012297C(3)`; +0x14 of level 0; `fn_8012297C(1)`; +0x14 of level 1;
   `fn_8012297C(3)`; **[gate]** +0x14 of levels 2..5.
2. `fn_8012297C(2)`; +0x20 of level 0 depth-sorted (0x800706B0); +0x20 of
   level 1 unsorted (0x80071718); then if `fn_80032428()` is non-zero:
   **[gate]** for levels 2..5, each task with +0x20 and twp is handed to
   `fn_80032170(task, 0, f1)` and `fn_80030EA8()` follows; otherwise
   **[gate]** depth-sorted +0x20 for levels 2..5.
3. +0x1C of levels 0, 1, **[gate]** 2..5.
4. `fn_800648C8()`.
5. +0x24 of levels 0, 1, **[gate]** 2..5.
6. +0x28 of levels 0, 1, **[gate]** 2..5.
7. `fn_8012297C(3)`.

The +0x2C group runs earlier, from fn_80070F5C. `fn_8012297C` is a render-state
selector whose meaning is **unresolved**.

**[gate]**: levels 2..5 are skipped when (G+0x3D == 1 and the camera index
0x803ADAD0 == 0) or (G+0x3D == 2 and 0x803ADAD0 == 1) or G+0x3D == 3. The
camera task writes 0x803ADAD0 (section 6), so this is a per-viewport
filter; what G+0x3D values mean is **unresolved**.

### 2.3 Pause and time-stop

- Pause is a game mode, not a flag inside the passes. In mode 0x10 (play) the
  frame returns a request when a player pressed Start (word +0x10 of the pad
  record, see section 5) and G+0x16, +0x39, +0x42 are clear; mode then becomes
  **0x11** (0x8001FA74) with `SE_Call(0x18002)`. Mode 0x11 runs
  `fn_8001E12C`: pad copy, display (0x80070BA8 / 0x80070C20), pause overlay —
  **no exec pass and no frame counter**.
- G+0x49 non-zero (written by fn_8005132C, fn_80051474, fn_800519E8, cleared
  by fn_8001ED50 / fn_80051C64): fn_8001E45C runs `fn_8001E89C` instead, which
  executes only level 0 (`fn_80071A40(0)`) and the exec of
  `playertp[G+0x49 − 1]`, then displays everything.
- G+0x37 (0x801CC19F; Steam 0x801CBD88+0x37): `fn_80062B38` returns at once
  when it is set, so the frame counter and the stage clock stop. The exec pass
  does not test it; 21 task routines read it themselves (e.g. effect emitters,
  enemies). Writers: fn_80076B0C (called from fn_80078D94) and fn_80076CC8
  (no `bl` caller), both in the Sonic-family player code. Calling it
  "time-stop" is **inferred** from that.

## 3. Master loop

```
__start → main 0x800D3AD0
  fn_800D2834 (boot: heaps, pads, video; fn_80022F08 sets the VI post-retrace callback)
  fn_800E33B4, fn_80022EF0 ×2
  G+0x80 = 0, G+0x13 = 0, fn_80119EE4(fn_8001A720, 0), fn_800E10DC, fn_800DA358, fn_80117D14
  loop while 0x803ADBAC == 0:
    fn_80035B38, fn_80014CD8
    fn_800D2DD4            ← top-level sequence (returns 1 → reset request)
    fn_80035B68
    G+0x78 (0x801CC1E0) += ticks 0x803AD8AC;  [0x803AD860]+0x14 += ticks
    fn_80035A7C, fn_80114B28, fn_800E30CC, fn_80117DF8
    call *0x803AD548       ← initial value fn_80022EF0 = vsync wait
  shutdown: … GXDrawDone, VISetBlack, VIFlush, VIWaitForRetrace, OSResetSystem
```

- **vsync**: `fn_80022F08(interval, ticks)` 0x80022F08 stores the interval
  in 0x803AD8B0 and 0x803AD8A8, ticks in 0x803AD8AC, and installs
  `fn_80022F5C` with `VISetPostRetraceCallback`. Each retrace,
  fn_80022F5C decrements 0x803AD8A8 and reads the pads (section 5).
  `fn_80022EF0` spins while 0x803AD8A8 > 0, then reloads it from 0x803AD8B0.
- **Top-level sequence** fn_800D2DD4: switch on G+0x80 (0x801CC1E8), 26 cases,
  table 0x801CC258. Case 4 (0x800D2E98): `fn_800729F0(0)`, `fn_80021828`
  (session init), → case 5. Case 5 (0x800D2ED4): `fn_8001F368` (stage
  frame); on −2: `fn_8002176C` (stage exit, destroys all tasks via
  0x80070ABC), → case 2. Cases 7/8: load and run a non-stage REL by name
  (e.g. "EVENT" at 0x803B052C) through the subprogram table (section 4.2).
- **Stage modes** fn_8001F368: switch on the halfword 0x803AD81A, 34 cases,
  table 0x80195060. Relevant cases: 0–3 load (case 2/3 call
  `fn_8001EB80` then `fn_8001EAE0`), **7** start (frame + `fn_80064848`;
  when that returns non-zero → mode 0x10), **0x10** play (frame, then exit and
  pause checks on 0x803AD81C), **0x11** pause (display only), 0x13–0x15 exits.
- **In-stage frame** fn_8001E45C 0x8001E45C:
  1. G+0x49 ≠ 0 → fn_8001E89C (2.3), return.
  2. Compute ticks for this draw into 0x803AD8AC (float math on G+0xDC/+0xE0/
     +0xE4 with `njRoundUp`; +1 every 5th frame when G+0xF == 2). The exact
     formula is **unresolved** here (it is float code).
  3. Per tick, while 0x803AD8A4 < 0x803AD8AC: `fn_800226E0` (pad copy, section
     5); `fn_800648C4` only when mode == 0x10 (0x8001E600: compare with 0x10,
     branch past the call when not equal; the pause frame fn_8001E12C does the
     same with 0x11); `fn_80071214` (exec 0..5); `fn_80006B00`; `fn_800D0904`;
     `fn_80062B38` (frame counter); game-over checks on the player tasks;
     counter++; `fn_80022EE0` between ticks.
  4. `fn_80035B68`; display: G+0x20 ≠ 0 → `fn_80070BA8([0x803ADAD4])` +
     `fn_80014CD8`, else `fn_80070C20`; `njPopMatrixEx`; `fn_80061CBC`; fades
     and overlays; `fn_80035B94`; `fn_80057524`.
- **Frame counter** G+0x7C (0x801CC1E4): incremented only by `fn_80062B38`
  0x80062B38 (one `bl` caller, fn_8001E45C), skipped while G+0x37 is set.
  Reset to 0 by fn_8001DDD8, fn_80064278, fn_80064484. The same function
  advances the stage clock: G+0x1D frames (0..59), G+0x1C seconds, G+0x1B
  minutes, capped at 99:59:59, copied to [0x803AD860]+0..+2, unless G+0x1A ==
  1 or G+0x39 == 1.
- Elapsed-tick total G+0x78 (0x801CC1E0) is the one `main` adds to.

## 4. Stage load and unload (level 11, Iron Gate)

Stage number: `GetStageNum` 0x8001DD78 returns the halfword 0x803AD820.

### 4.1 Order

1. Top-level case 4 → `fn_80021828` (session init): mode 0x803AD81A = 3 → 0,
   `fn_80064428`, `fn_8006445C`, `fn_80064470`, `fn_80064414`, `fn_800DDA3C`,
   `fn_80027C20`, `fn_8002309C`, display flags, **`clearBtp`**,
   `fn_80071FA8`, 4 × `fn_800DEA3C`, `fn_80072820`, `srand`, 4 ×
   `fn_800C4458` (camera task, section 6), `fn_8001A8A8`, `fn_800C3C8C`,
   `fn_800503C8`, **`fn_8001DEA8` (load players, 4.3)**, `fn_80066488`,
   `fn_80024B2C`, `fn_80018E5C`, `fn_80012CC0`, `fn_80067D58`, `fn_8006CA48`,
   `fn_80019CCC`, `fn_800DCBC8`, `fn_800DB604`.
2. Mode 2/3 → `fn_8001EB80` (stage load): 0x803AD824 = stage, fog table
   (`njGenerateFogTable3`, `njSetFogTable`), character swaps for stages 0x5A,
   3 and 0x3C..0x46 (destroy player 0, set 0x803AD828, reload players), clear
   0x801E5870 and 0x801E5630, `fn_80067FF4`, **`fn_8001A3BC` → `fn_8001A5B0`:
   read `stg11D.rel`, `OSLink`, call module prolog (header +0x34)**,
   **`fn_80019E18`: call the subprogram's init**, `fn_8001EDFC`, G+0x16 = 1,
   `fn_8001DDD8` (frame counter reset), `fn_800C4370` (camera reset),
   `fn_80026490`. Then `fn_8001EAE0` (calls `fn_80022F08`, the vsync
   interval) and mode 7.

### 4.2 stg11D.rel

- `_prolog` (stg11D .text+0xA408): `LoadSubprogram(&table)` 0x8001A01C
  stores the table pointer in `ActiveSubprg` 0x803AD804.
- Subprogram table (stg11D .data+0x5D850): name "STG11", then +0x0C init
  fn_11_A434, +0x10 unload fn_11_A76C, +0x14 per-frame fn_11_A874.
  Engine side: `fn_80019E18` calls +0x0C, `fn_80019D78` calls +0x10 then
  the module epilog (header +0x38) then `OSUnlink`, `fn_80019E44` calls
  +0x14 (used by the non-stage sequence cases).
- **Init fn_11_A434, in order** (main.dol callees by address):
  1. `fn_800324E4(0x80, …)` — **unresolved** role
  2. `fn_800245EC(…)` — landtable/stage file load: builds a `.prs` name,
     `fn_8001A088`, **`fn_8001A13C` (lights)**, allocates the land buffers
     (0x803AD8EC, 0x803AD8F0), sets G+0x3B = 1
  3. `fn_80072270`, `fn_80072218` — motion-table cluster
  4. `fn_80065048(0, 0x800)` — SET file load: `set%04d` with the stage
     number and `%s_s.bin` / `%s_u.bin` (also `_2p_`, `_hd_` variants)
     through `fn_8001A024`; the result goes to stg11D .bss+0x24
  5. `fn_8006638C(set table, set data)` — SET task:
     `CreateElementalTask(0, 2, fn_80066274, …)`, destructor fn_80066314;
     its exec calls the SET spawner `fn_80065C40` (contains 0x80065E54) and
     fn_80065EC8
  6. `fn_8001A8A8`, `fn_80033710`, `fn_800C3CD8(f1, f2)` (camera module,
     role **unresolved**), `fn_8011E154`, `fn_8011E150`
  7. 4 × `fn_8002506C`, 2 × `fn_80025028`, `fn_80025048`, `fn_8002503C`
     (land-table registration family; exact roles **unresolved**)
  8. fn_11_C2F0, `BGM_SetVolume(0)`, `StgPlayMusic`
  9. `CreateElementalTask(2, 1, fn_11_AE18, …)` with display fn_11_AF34 and
     destructor fn_11_B3BC; 0x803AD718 = 1
  10. `fn_8003BCC4(death zones)` — task "KillPlayerFallingDownStageP",
      level 0, destructor fn_8003C144
  11. fn_11_A230; installs stage callbacks into 0x801A2A18 table
      (`DamegeRingScatter` +0x0, fn_11_148D4 +0x8, fn_11_11618 +0x28) when
      empty
  12. fn_11_152EC ×2, fn_11_B484, fn_11_330DC, fn_11_28038,
      `fn_8006D500` (lights again via fn_8001A13C)
  13. 0x801E77D8 table: fn_800E014C, fn_11_FED8, fn_11_109CC, fn_11_10A20,
      rest zero
  14. fn_11_11610, fn_11_16DE0(0, 0), 0x803ADC30 = fn_11_ABA0
- `CCL_Init` 0x8000F8B8 (the plan's InitCollision) is not called by the stage
  init itself: it is called per task (80 stg11D sites, 25 main.dol sites,
  including every player loader).
- `fn_800232A4` (12 stg11D imports, no main.dol `bl` caller) is called by
  stage object code inside stg11D, not by init. Its role (dynamic land-object
  registration in the crate's notes) was not re-traced here: **unresolved**
  in this pass.
- **Unload** fn_11_A76C: fn_11_11614, fn_11_27FE4, fn_11_33058,
  `fn_8002455C`, fn_11_B440, fn_11_15880, fog off, clears its 0x801A2A18
  callbacks, `fn_800336DC`, `fn_80072118`, `fn_80066364`, `fn_8001A2B0`,
  `fn_800245B8`, fn_11_A2A4. Stage exit `fn_8002176C` destroys all tasks.

### 4.3 Players

`fn_8001DEA8` 0x8001DEA8 (load players): for player 0 always, player 1 only
when G+0x1E ≠ 0, switch on the character word (0x803AD828 for player 0,
0x803AD82C for player 1):

| Char | Loader |
| ---: | --- |
| 0 | 0x800874C0 |
| 1 | 0x80086E3C |
| 2 | 0x800A9930 |
| 3 | 0x800D92E4 |
| 4 | 0x800A2360 |
| 5 | 0x800A1F00 |
| 6 | 0x800B5AC0 |
| 7 | 0x800B5D2C |

then `fn_8001D74C(player)` (start position).

## 5. Input

```
VI post-retrace: fn_80022F5C → fn_80119A04
  memmove 0x80357F60 → 0x80357F30 (previous PADStatus ×4, 12 bytes each)
  PADRead(0x80357F60) 0x800F4780; PADClamp 0x800F3A00
  per port (record 0x80357E60 + 0x34·port), when PADStatus.err == 0:
    +0x04 = 0x001F07FE
    +0x08 held     = fn_80119BD8(buttons)      (table 0x801D51A4, count 0x803AD6F0)
    +0x0C = ~held
    +0x10 pressed  = fn_80119BD8(cur & (prev ^ cur))
    +0x14 released = fn_80119BD8(prev & (prev ^ cur))
    +0x18, +0x1A   = PADStatus +7, +6 (analog triggers)
    +0x1C, +0x1E   = fn_80119C4C(stick x, y)
    +0x20, +0x22   = PADStatus +4, +5 (C stick)
  error → PADReset path.
Per tick: fn_800226E0 (and fn_8002239C)
  0x801E55D8[i] = fn_80119B84(port map) → &0x80357E60[port], or the
                  idle record 0x801950E8 when disconnected
  G+0x3E == 1 (input on): copy record 0 to 0x801E5438 when G+0x3F == 1 and
                  record 1 to 0x801E546C when G+0x40 == 1; every other of
                  the 8 copies is neutralised (+0x2C = old held, held 0,
                  +0xC = ~0, pressed/released 0, sticks and triggers 0).
                  G+0x3E != 1: all 8 are neutralised.
  fn_80022048 → for i in 0..8 (sticks only for i < 2): dead zone ±14,
                  angle = BAMS(atan2f(y, x)) (axis cases 0/0x4000/0x8000/
                  0xC000) minus the camera angle [*(0x80175378 + 4i) + 0x10]
                  when *0x803ADB04 is set, magnitude = min(|v|³ / 125000, 1)
                  (|v| by frsqrte + 3 Newton steps); 0x801E53B8 + 8·i always,
                  0x801E53F8 + 8·i only when G+0x3E == 1 and G+0x3F+i == 1;
                  0x803AD874[i] = 1 when the magnitude crosses 0.5 upward
  then fn_80022BC4, fn_80021E68 (demo record/playback when G+0x00 is 1/2,
                  writes the copies and 0x801E53F8), fn_80021D0C,
                  fn_80022A94(0..3): not ported yet
```

`GetAnalog` 0x8004E560 reads 0x801E53F8 (92 callers). The pause check in
fn_8001E45C reads word +0x10 (pressed) of `*0x801E55D8[i]`.

## 6. Camera

- `fn_800C4458` 0x800C4458: if 0x803ADAF8 is empty,
  `CreateElementalTask(2, 0, fn_800C3CF0, …)` (im 2, level 0; corrected in E4)
  → 0x803ADAF8, destructor
  fn_800C3D90; then fn_800C34C4, fn_800C5914, fn_800C6F70, fn_800C91C4;
  0x803ADAD4 = viewport count argument; 0x803ADAD0 = 0; camera work at
  0x801FF424 initialised (0x31C7, 0x31C7, 0x4000); fn_800C36A4, fn_800C3A94,
  fn_800C3C38, fn_800C76F4. `fn_800C4370` wraps it (10 callers).
- First exec fn_800C3CF0 installs exec **fn_800C3DA0**, display
  **fn_800C3FCC**, +0x1C fn_800C3FAC, destructor fn_800C3D90, then runs the
  exec once.
- Exec fn_800C3DA0, per viewport (0x803ADAD0 counts up): view matrix
  0x801FF3E4 (0x40 bytes) rebuilt, mode updates (fn_800C4964, fn_800C5730,
  fn_800C7590, fn_800C14AC, fn_800C73A0, fn_800C7288, fn_800C7464,
  fn_800C6B38, fn_800C7358, fn_800C4038), result copied to the block at
  `*0x803ADB04`, then `fn_800C4278` → `njSetCameraAngle`, `njSetCamera`
  0x8011242C, fn_80024CB8.
- Display fn_800C3FCC: per viewport, 0x803ADAD0 = index, `fn_800C4278`
  (re-applies that viewport's Ninja camera before the display pass draws it).
- Which mode routine is the "default player camera" for Iron Gate is
  **unresolved**; 0x801E4FF8 is not a camera block in GSNE8P (section 0).

## 7. Draw path

- Walker display 0x800AF558 calls the Ninja matrix stack (`njPushMatrixEx`,
  `njTranslateEx`, `njRotateX/Y/Z`, `njSetMatrix`, …), `njSetTexture`,
  `OnControl3D`/`OffControl3D`, fn_80024CB8, fn_80024E68, fn_80025028,
  fn_8002B2F8..fn_8002B35C, and the model draws fn_8011DF44 (58 callers),
  fn_8011E214 (68 callers) and fn_8011C3A0.
- `fn_8011DF44` → fn_8011D554 → **fn_8011DFC0(object, draw)**: object-tree
  walk (eval flags at +0x0, model +0x4, child/sibling recursion,
  `njTranslateEx`, `njRotateEx`/`njQuaternionEx2`, fn_8011610C) calling
  `draw(model)` = **fn_8011D484** for each model.
- fn_8011D484 → fn_8011D414, fn_8012192C, reaching (static, depth 8): GXBegin,
  GXClearVtxDesc, GXSetVtxDesc, GXSetVtxAttrFmt, GXInitTexObj, GXLoadTexObj,
  GXLoadTexObjPreLoaded, GXSetAlphaCompare, GXSetBlendMode, GXSetChanCtrl,
  GXSetChanMatColor, GXSetNumChans, GXSetNumTevStages, GXSetNumTexGens,
  GXSetTevAlphaIn, GXSetTevAlphaOp, GXSetTevColorIn, GXSetTevColorOp,
  GXSetTevOp, GXSetTevOrder, GXSetTexCoordGen2, GXSetZCompLoc, GXSetZMode,
  and the internal __GXSendFlushPrim, __GXSetGenMode, __GXSetMatrixIndex,
  __GXSetSUTexRegs, __GXSetVAT, __GXSetVCD, __GXUpdateBPMask,
  __GXXfVtxSpecs.
- The walker display also reaches GXLoadPosMtxImm, GXLoadNrmMtxImm,
  GXLoadTexMtxImm, GXSetChanAmbColor and the GX light calls
  (GXInitLightPos/Dir/Color/Attn, GXLoadLightObjImm).
- fn_8011E214 → fn_801203E0 (motion draw) calls through the pointer
  0x803AE048, set at run time by fn_80113B08 (from fn_8011C3A0), and through
  `[0x803AD6B0]+0xC`. Their targets are **unresolved** statically.
- Landtable display `LandManagerDisp` 0x8002437C (and `LandManagerDispSort`
  0x800242F4) → fn_80023BA0, fn_800240CC → `njCnkCacheDrawModel`,
  `gjDrawModel`, reaching GXCallDisplayList, GXSetArray, GXSetFog,
  GXLoadTexObj and the same TEV/vertex set as above. `GXLoadTexObj` calls
  through `[gx]+0x410` (0x803AD640).

## 8. The Chao Walker and 0x8004F838

`fn_8004F838(player, task)` 0x8004F838: allocates 0x84 bytes at
player work +0x90, records the player in the tables 0x801E5C48 + 0x1AE0
(`playerpwp`, = 0x801E7728), +0x1B00, +0x1B20 (`playertwp` 0x801E7768),
+0x1B40 (`playertp` 0x801E7788), and copies **PhysicsArray row = player
work byte +0x3** from 0x80196578 + row·0x84 (0x84 bytes) into player work
+0xC0. Player work is the task's +0x40 word.

The 14 callers (task name = the `CreateElementalTask` name argument; char =
work byte +0x1; row = work byte +0x3):

| Loader | Name | Char | Row | Exec |
| --- | --- | ---: | ---: | --- |
| 0x800874C0 | SonicTheHedgehog | 0 | 0 | 0x80083E44 |
| 0x800871C4 | SonicTheHedgehog | 0 | 8 | 0x80083E44 |
| 0x80086E3C | SonicTheHedgehog | 1 | 1 | 0x80083E44 |
| 0x80086BA4 | SonicTheHedgehog | 1 | 0xC | 0x80083E44 |
| 0x800A9930 | MilesTailsPrower | 2 | 2 | 0x800A8654 |
| 0x800D92E4 | Eggman | 3 | 3 | 0x800D82A0 |
| 0x800A2360 | KnucklesTheEchidna | 4 | 4 | 0x8009F8A0 |
| 0x800A1C9C | KnucklesTheEchidna | 4 | 0xF | 0x8009F8A0 |
| 0x800A1F00 | KnucklesTheEchidna | 5 | 5 | 0x8009F8A0 |
| 0x800A1AA8 | KnucklesTheEchidna | 5 | 0x10 | 0x8009F8A0 |
| 0x800B5AC0 | EggWalker | 6 | 6 | **0x800B3A8C** |
| 0x800B58D4 | EggWalker | 6 | **0xD** | **0x800B3A8C** |
| 0x800B5D2C | EggWalker | 7 | 7 | **0x800B3A8C** |
| 0x800B56E8 | EggWalker | 7 | **0xE** | **0x800B3A8C** |

All four walker loaders do `CreateElementalTask(7, 1, 0x800B3A8C, "EggWalker")`,
`syCalloc(1, 0x454)` player work, `CCL_Init`, and install destructor
0x800B245C and +0x2C 0x800B0D7C. Their displays differ:

| Loader | Row | +0x14 display | +0x1C display |
| --- | ---: | --- | --- |
| 0x800B5AC0 | 6 | 0x800AE574 | 0x800AE2E0 |
| 0x800B58D4 | 0xD | 0x800B0324 | 0x800AFFC0 |
| 0x800B5D2C | 7 | 0x800B15B8 | 0x800B110C |
| 0x800B56E8 | 0xE | 0x800AF558 | 0x800AF2E8 |

How the rows 0xD/0xE are reached (bytes of 0x800B5AC0 / 0x800B5D2C): the
char-6 and char-7 loaders first test **G+0x1E ≠ 0 and the byte G+0x55+player
≠ 0**; when both hold they call 0x800B58D4 (row 0xD) or 0x800B56E8 (row 0xE)
instead and return. `fn_8001DEA8` only loads player 1 when G+0x1E ≠ 0.

So, from the bytes:
- **The Phase 2 premise holds**: the alternate walker rows 0xD and 0xE are
  loaded by 0x800B58D4 and 0x800B56E8, and both install the walker exec
  0x800B3A8C through `CreateElementalTask`.
- **Correction to WALKER-PORT-MAP.md**: 0x800B56E8 is not the Mech Eggman
  loader. It is the char-7 alternate (row 0xE) with display 0x800AF558. The
  Mech Eggman loader is 0x800B5D2C (row 7, display 0x800B15B8). The Mech Tails
  pair is 0x800B5AC0 (row 6) / 0x800B58D4 (row 0xD).
- Reaching the walker rows needs G+0x1E set and the per-player byte at
  G+0x55. In the path traced here, G+0x1E is also the gate for loading a second
  player. A one-player story Iron Gate (character 7) therefore loads row 7.
- Naming row 0xD "Chao Walker" and row 0xE "Dark Chao Walker" (and rows 8, 0xC,
  0xF, 0x10 as Amy, Metal Sonic, Tikal, Chaos) is **inferred** from the
  char/row pairing. main.dol has no name string for those rows; the four
  walker loaders all pass "EggWalker".

## 8a. Chao Walker selection path

Traced with a store scan over main.dol and all 77 GC RELs that have
reconstruction symbols, including indexed stores (`add rX, G, i` then
`stb …, 0x55(rX)`). advertiseD.rel (SHA-1
`1764d34512136037e914daa688b0fc2c605398fe`) and titleD.rel
(`68ff4296329274c250e5dab409927012bbe3774b`) match the GSNE8P build list.
REL addresses are given as section offsets (`fn_91_XXXX` is advertiseD .text,
`fn_1_XXXX` is titleD .text).

**Writers of the per-player bytes G+0x55 / G+0x56** (0x801CC1BD / 0x801CC1BE):
none in main.dol. Only two modules write them:

- advertiseD `fn_91_201D8(player r3, selection r4)` (size 0x1C8): stores
  `selection & 0x7F` in its own table (.bss+0x63790 + 0x160 + player), then:
  - selection 0..7: `fn_8001DDC0(player, selection)` (character word
    0x803AD828 / 0x803AD82C) and G+0x55+player = 0; G+0x53+player = 1 when a
    menu table entry (+0x45) is 1.
  - selection 8..16 through the jump table at advertiseD .data+0x9C530, each
    setting G+0x55+player = **1**:

    | Selection | Character | Loader row (section 8) |
    | ---: | ---: | --- |
    | 8 | 0 | 0x800871C4, row 8 |
    | 12 | 1 | 0x80086BA4, row 0xC |
    | **13** | **6** | **0x800B58D4, row 0xD** |
    | **14** | **7** | **0x800B56E8, row 0xE** |
    | 15 | 4 | 0x800A1C9C, row 0xF |
    | 16 | 5 | 0x800A1AA8, row 0x10 |
    | 9, 10, 11 | — | no write (returns) |

  Called by advertiseD `fn_91_1DFAC` (+0x1BB0, +0x1BBC) and `fn_91_203F4`
  (+0x1C0, +0x1CC, +0x534, +0x548): two calls each, one per player.
- titleD `fn_1_2620` (size 0x1840): the title screen's attract demos
  ("2P 3POINTS MATCH" / "2P 1POINT MATCH" strings in titleD .data). It sets
  G+0x1E = 1 or 2, the next sequence state G+0x84 = 4, and G+0x55/G+0x56 = 0,
  or = 1 for both players when the chosen demo row's first word is 6, 0 or 4
  (.data+0x1360, 16-byte rows; G+0x53 is cleared alongside).

**Writers of G+0x1E** (0x801CC186): main.dol only clears it (`fn_80064988`
+0xE0, which also clears G+0x00, G+0x20 and sets G+0x80; 15 callers in
fn_80064CD8). titleD `fn_1_2620` / `fn_1_4674` / `fn_1_4AB0` and advertiseD
write 0, 1 or 2. Non-zero values are written in advertiseD `fn_91_0`,
`fn_91_1DFAC` (+0x17E0 = 2, +0x19C8 = 1, +0x1D68 = 1, +0x1D7C = 2),
`fn_91_203F4` (+0x284 = 1, +0x298 = 2, +0x588 = 2), `fn_91_24B00` (+0x1540 = 1),
`fn_91_2D0D0` (+0x57C = 1), `fn_91_2D8D0` (+0x380 = 1), `fn_91_3024C` (+0x1B0 =
1), `fn_91_30434` (+0xB8 = 2). The functions that call `fn_91_201D8` for both
players are among them. (The value at each site is the `li` just before
the store; a site reached from two branches may store either.)

**What it means for a one-player Iron Gate start:**

- main.dol's `fn_8001DEA8` loads player 1 whenever G+0x1E ≠ 0, and the
  walker loaders 0x800B5AC0 / 0x800B5D2C take the alternate row only when
  G+0x1E ≠ 0 **and** G+0x55+player ≠ 0.
- The only code that sets G+0x55+player is the two-player character select
  (advertiseD `fn_91_201D8`, selections 13/14) and the title-screen 2P demos.
  Every path found that sets it also sets G+0x1E to 1 or 2.
- So **the game never starts a one-player stage with a walker row**: with
  G+0x1E = 0 (one player) the loader ignores G+0x55. For the game itself to
  load row 0xD/0xE in Iron Gate it needs G+0x1E ≠ 0, which also loads a second
  player through `fn_8001DEA8` (with 0x803AD82C as its character).
- **Unresolved:** what G+0x1E = 1 versus 2 select (both values appear in the
  2P menus and demos), and the menu names of the advertiseD functions above
  (advertiseD's strings were not mapped). Whether any other module writes
  these bytes through a pointer the scan cannot follow (a store through a
  register loaded from memory) is not excluded.

## 8b. One-player via the native select route

Question: can the game's own values (only what advertiseD/titleD write) give
player 1 = selection 13 or 14 (row 0xD / 0xE walker) with **no** player 2 in
stage 11?

**1. What decides whether player 2 is loaded** (`fn_8001DEA8`, loop over
player r31 = 0, 1):

| Address | Condition |
| --- | --- |
| 0x8001DF7C | player index == 1 (player 0 is always loaded) |
| 0x8001DF84 | G+0x1E (0x801CC186) ≠ 0, tested as a signed byte against zero only |
| 0x8001DF90–0x8001E02C | dispatch on the character word 0x803AD82C: each of 0, 1, 4, 5, 2, 3, 7, 6 calls its loader; any other value calls no loader |
| 0x8001E034 | `fn_8001D74C(1)` (start position) runs whenever the first two pass, whether or not a loader ran |

Nothing else is read: no player-count byte, no "none" character value,
G+0x55+1 is not tested here (only inside the char-6/7 loaders).
0x803AD82C lies in .sbss (zero-initialised, so character 0 until written).
Every constant written to it is 0 or 1 (titleD `fn_8001DDC0(1, 0/1)`). The
other writes go through `fn_8001DDC0(1, rX)` with a register value (advertiseD
7 sites, titleD 2 sites); the select writer `fn_91_201D8` only produces 0..7
or leaves the word unchanged (selections 9–11). No write of a value outside
0..7 was found; the register-valued sites are **not bounded statically**
(unresolved).

**2. Other readers of G+0x1E.** In main.dol, 163 reads: 156 are zero tests,
3 use the byte as a value (fn_800101E0 +0x58, fn_800107EC +0x7C, fn_80069118
+0x3C) and 4 compare it with 2 (fn_8001C810 +0xC0, fn_8001EFA8 +0x48,
fn_8005F230 +0x104, fn_80061EF4 +0x2C). What 1 versus 2 selects is
**unresolved**. The walker loaders read it only as zero/non-zero:
0x800B5AC0 / 0x800B5D2C at entry (alternate-row gate) and again at
0x800B5BC0 / 0x800B5E2C, where G+0x1E ≠ 0 and G+0x53+player == 1 pick a
different pair of model/motion tables and set player-work byte +0x2 = 1.
0x800B58D4 / 0x800B56E8 do not read G+0x1E.

Session init `fn_80021828` (0x800218AC–0x80021980): with G+0x1E ≠ 0 it sets
G+0x23 = 0 and **G+0x20 = 1 (split screen) when either player's character is 6
or 7**, or when G+0x1F is 1 or 4. Otherwise G+0x20 = 0 (stage 0x47 is left
as it is). With G+0x1E == 0, G+0x20 = 0. G+0x20 then selects the
multi-viewport display (`fn_80070BA8`) and the camera task's viewport count
(`fn_800C4458(4/2/1)` at 0x80021AF0–0x80021B0C). So a native two-player walker
game runs split screen.

**3. Stage 11 under G+0x1E ≠ 0.** The init fn_11_A434 does not read G+0x1E,
so nothing refuses to load. 13 zero tests exist in stg11D's object and display
code: fn_11_14D28 (4, around fn_800148FC / fn_80014CD8 / fn_8001475C),
fn_11_2497C, fn_11_2549C, fn_11_2633C (each next to a stage-number check and
`IsThisTaskPlayer`), fn_11_3DCFC (before `SetNoRevive`), fn_11_3DF60 (2,
around fog and fn_800618B4), fn_11_3E0F0, fn_11_47040 (2). What each branch
changes is **unresolved**.

**4. Conclusion.** No natively reachable state was found that gives player 1
the row-0xD/0xE walker with no player 2:
- the alternate row needs G+0x1E ≠ 0 (0x800B5AC0 / 0x800B5D2C);
- G+0x1E ≠ 0 always runs player 2's dispatch, and player 2's character word
  is 0 by default with every write found in 0..7, so a player-2 task is
  loaded;
- with a walker character it also switches to split screen (G+0x20 = 1).

A one-player Iron Gate as the walker therefore needs a port-side choice
(calling 0x800B58D4 or 0x800B56E8 for player 0 with G+0x1E = 0). Main.dol
never does this. The one gap is the register-valued writes to 0x803AD82C,
which this static pass does not bound.

## 9. Unresolved, for the next milestones

- fn_8012297C (render-state selector in the display pass) and G+0x3D values.
- The float formula that sets ticks per draw (0x803AD8AC) in fn_8001E45C.
- fn_800324E4, fn_80033710, fn_800C3CD8, and the fn_8002503C..fn_8002506C
  family in the stage init.
- fn_800232A4's role (only stg11D object code calls it).
- Which camera mode routine Iron Gate uses by default, and where the
  per-viewport camera block at `*0x803ADB04` comes from.
- Draw callbacks behind 0x803AE048 and `[0x803AD6B0]+0xC`.
- What G+0x1E = 1 versus 2 means (section 8a).
- Buffers 0x803ADA50..0x803ADA5C (fn_80071F5C, fn_80071FA8, fn_80072030).
