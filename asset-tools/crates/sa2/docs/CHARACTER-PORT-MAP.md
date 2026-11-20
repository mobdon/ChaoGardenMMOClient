# Playable character controllers: port map (GameCube GSNE8P)

All addresses are GSNE8P main.dol (SHA-1 e4c50f32242a3cd36cdc8ae776661601a52130a2).
Found with the local disassembly index (`ppcmap.py callers/brief/tree`). Nothing here comes
from the Steam build. The fourteen character loaders are the callers of the shared player-work
setup fn_8004F838 ([WALKER-PORT-MAP.md](WALKER-PORT-MAP.md)).

## Who loads whom

fn_8001DEA8 creates each player's character. It reads the character word for player 1
(0x803AD828) and player 2 (0x803AD82C), then calls one loader per value. Amy, Metal Sonic,
Tikal and Chaos have no value of their own. Each is the alternate loader of a main character.
The main loader calls it, then returns, when 0x801CC186 (0x801CC168+0x1E) is non-zero and
0x801CC168+0x55+player is non-zero (the battle-mode alternate costume).

| Word | Loader | Alternate loader | Char byte (+1) | PhysicsArray row (+3) | Exec (task +0x10) | Work size |
| --- | --- | --- | --- | --- | --- | --- |
| 0 Sonic | 0x800874C0 | 0x800871C4 (Amy, row 8) | 0 | 0 / 8 | 0x80083E44 | 0x3A0 |
| 1 Shadow | 0x80086E3C | 0x80086BA4 (Metal Sonic, row 0xC) | 1 | 1 / 0xC | 0x80083E44 | 0x3A0 |
| 4 Knuckles | 0x800A2360 | 0x800A1C9C (Tikal, row 0xF) | 4 | 4 / 0xF | 0x8009F8A0 | 0x420 |
| 5 Rouge | 0x800A1F00 | 0x800A1AA8 (Chaos, row 0x10) | 5 | 5 / 0x10 | 0x8009F8A0 | 0x420 |
| 2 Tails | 0x800A9930 | none | 2 | 2 | 0x800A8654 | 0x3E0 |
| 3 Eggman | 0x800D92E4 | none | 3 | 3 | 0x800D82A0 | 0x36C |
| 6 / 7 walkers | 0x800B5AC0 / 0x800B5D2C | 0x800B58D4 / 0x800B56E8 | 6 / 7 | 6, 7 / 0xD, 0xE | 0x800B3A8C | 0x454 |

So there are two playable families in scope. The alternates are not Sonic-tree variants
across the board: **Tikal and Chaos run the Knuckles tree** with Knuckles' or Rouge's
character byte.

- **Speed family** (Sonic, Shadow, Amy, Metal Sonic). The exec is fn_80083E44 (0x2D60 bytes).
  Its action machine is fn_8007E89C (0x55A8). Every loader installs the same callbacks:
  +0x14 0x8007BFF4 display, +0x18 0x8007E800, +0x1C 0x8007E304. The +0x2C callback is
  0x8007BEB4 for Sonic and Amy and 0x8007BC7C for Shadow and Metal. There is also an attached
  task 0x80073330 with 0x800734E8 / 0x80073324.
- **Hunter family** (Knuckles, Rouge, Tikal, Chaos). The exec is fn_8009F8A0 (0x2208). Its action
  machine is fn_8009B8FC (0x3FA4). +0x18 0x8009B81C is shared. The
  display (+0x14) is per character: Knuckles 0x80099BA0, Rouge 0x80098280, Tikal 0x80099150,
  Chaos 0x800975D4 (with +0x1C 0x80096F64). +0x2C is 0x8009A738 for Rouge and 0x8009ADFC for
  the other three.

### Character-specific code inside the shared execs

Both execs branch on the character byte (+1) and the PhysicsArray row (+3) in their own body.
Those branches are the per-character deltas:

- Hunter exec: +3 at 0x8009FB2C, 0x800A00D0, 0x800A0838, 0x800A150C, 0x800A1678, 0x800A1708
  and 0x800A18E8; +1 at 0x800A0420, 0x800A05E4, 0x800A069C, 0x800A1358, 0x800A1478 and
  0x800A15BC. Action machine: 13 sites, from 0x8009BB48 to 0x8009F818.
- Speed exec: +3 at 0x800841BC, 0x8008453C, 0x80084664, 0x80084968, 0x8008536C, 0x80085D58,
  0x80085FFC, 0x800867DC and 0x800868E8; +1 at 0x80084B44. Action machine: 20+ sites, from
  0x8007E910 on.
- The hunter display helper fn_800945A8 picks its scale by row (4, 5, 0xF, 0x11).

## The hunter exec (fn_8009F8A0)

Arguments are the task (`r3`), with `twp` = task+0x34, `mwp` = task+0x38 and `pwp` = task+0x40.

1. For player 2 (pwp+0 non-zero) in stages 0x2C and 0x32, it swaps the gravity vector
   0x801E5624 for 0x801E5618, saving the old one at 0x801FEAC8. It also swaps
   0x803AD90C..0x803AD918. All of these are restored at exit.
2. It runs the input step fn_80056450.
3. In actions 0x37 and 0x39 it calls the action machine only when twp+1 isn't 0x24. When
   twp+1 is 0x24, it clears flag bits 0x3C, masks pwp+0x10 to 0x3FFC and calls fn_8005523C.
   Every other action calls the action machine fn_8009B8FC.
4. It counts frames at pwp+0x3A2 while flag bit 0x100 is set.
5. It runs the per-action body through the 0x58-entry jump table at 0x801C2AD4. The typical
   ground case calls fn_80048490 → **fn_8004C734** → fn_80046DE4 → fn_80045880 →
   fn_8003E668, then fn_80054D04.

It calls 59 other routines. Hunter-module routines among them: fn_8009B8FC, fn_800936AC,
fn_80090C80, fn_8009B5CC, fn_8008BEDC, fn_800A26A0, fn_8009B464, fn_80096CA8, fn_800914F8,
fn_8009B708 and fn_80092C00.

The action machine fn_8009B8FC has 22 internal callees and dispatches through its jump table at
0x801C2974. Its helpers sit in 0x80090AA8..0x80096CA8.

## What it depends on, and who owns it

| Routine | Role | Owner / state |
| --- | --- | --- |
| fn_8004C734 + tree (0x8004BA74, 0x8004B568, 0x80038F40, 0x80047714, 0x800473B4, 0x80047CBC, 0x80048150) | shared ground move | Sonic thread: `player_accel::ground_move`, PR #481 (merged into this branch, not into main yet) |
| fn_80046DE4, fn_80045880, fn_8003E668 | movement run, frame | walker thread; fn_8003E668 is on main (`walker_frame`) |
| fn_8003D8D0 + tree (0x8003CA00, 0x8003C908, 0x8003D720, 0x8003D678, 0x8002B54C, 0x8002B6C0) | motion step | walker thread, PR #456 (merged into this branch, not into main yet) |
| fn_80067600, fn_8006779C | land probes | walker thread |
| fn_8004E560 | analog input | on main (`player_analog`) |
| fn_8003E9E0 / fn_8003EA6C | player-frame rotate | on main (`ninja_matrix`) |
| njUnitVector, njScalor | Ninja vectors | `ninja_vector` (PR #481, with PowerPC's NaN), which `enemy_lib` now calls |
| PhysicsArray rows | parameters | on main (`player_physics`) |

## Already in the crate

The data readers come from #198, #201, #365 and #366: `player_physics` (all 17 PhysicsArray
rows), `character_animations`, `character_motions` and the pose samplers. None of them runs a
controller. No branch, stash or worktree has native code for either exec. The client's dev
roster (uncommitted on codex/tiny-chao-phone) and the server's `character_tech.rs` are MMO
adaptations built from Steam tables. They are not ports.

## Hunter port status (`src/hunter.rs`, `tests/hunter_dol.rs`)

| main.dol | Rust | Check |
| --- | --- | --- |
| fn_80091454 (action machine, 9 call sites) | `hunter::drop_check` | 3,000 cases bit-exact |
| fn_80093270 (action machine) | `hunter::climb_request` | 3,000 cases bit-exact |
| fn_8009B708 (exec) | `hunter::collision_center` | 1,500 cases bit-exact |
| fn_8009F8A0 (the exec, all 0x58 cases and the tail) | `hunter_exec::hunter_exec` | ported from the listing; unit tests for dispatch order, the climb results, the player-2 swap, Rouge's motion switch and the push-out bound (not Unicorn) |

`hunter_exec` keeps twp, mwp and pwp in their native layout (`native_work::Work`). It calls
each shared routine by address through `HunterWorld::step`: the ground move fn_8004C734
(`player_accel::ground_move`, PR #481), the walker steps (`walker_move`, PR #456), the land
probe fn_80067600, and the hunter routines not ported yet (the action machine fn_8009B8FC,
fn_800936AC, fn_80092C00 and others). A runtime binds the addresses to their ports.
Sounds, rumble and effects are `walker_exec::EngineCall`s. The player history is
`walker_act::history_back`, and `sqrtf` is `player_turn::inline_sqrtf`.

Still to port in the hunter module: the action machine fn_8009B8FC (0x3FA4), its helpers,
the climb/glide routines fn_800936AC, fn_80090C80 and fn_80092C00, and the runtime adapter
that binds `HunterWorld::step` to the Rust ports.

### Hunter module inventory (0x8008D888..0x800A29AC)

The module holds 79 functions, 0x15124 bytes. Sizes and caller counts come from main.dol.
"Callers 0" means a callback reached through a pointer.

| Address | Size | Callers |
| --- | --- | --- |
| 0x8008D888 | 0x174 | 4 |
| 0x8008D9FC | 0xBC | 0 |
| 0x8008DAB8 | 0x310 | 0 |
| 0x8008DDC8 | 0x2C0 | 2 |
| 0x8008E088 | 0x424 | 10 |
| 0x8008E4AC | 0x1094 | 0 |
| 0x8008F540 | 0x35C | 1 |
| 0x8008F89C | 0xB34 | 1 |
| 0x800903D0 | 0x6D8 | 1 |
| 0x80090AA8 | 0x1D8 | 5 |
| 0x80090C80 | 0x2D8 | 2 |
| 0x80090F58 | 0x4FC | 1 |
| 0x80091454 | 0xA4 | 9 |
| 0x800914F8 | 0x128 | 2 |
| 0x80091620 | 0x10C | 1 |
| 0x8009172C | 0x164 | 1 |
| 0x80091890 | 0x108 | 1 |
| 0x80091998 | 0x224 | 1 |
| 0x80091BBC | 0x7C | 0 |
| 0x80091C38 | 0x12C | 0 |
| 0x80091D64 | 0x204 | 0 |
| 0x80091F68 | 0x198 | 1 |
| 0x80092100 | 0x30 | 0 |
| 0x80092130 | 0x4 | 0 |
| 0x80092134 | 0x34C | 0 |
| 0x80092480 | 0x1B4 | 1 |
| 0x80092634 | 0x150 | 0 |
| 0x80092784 | 0x58 | 0 |
| 0x800927DC | 0x170 | 1 |
| 0x8009294C | 0x74 | 0 |
| 0x800929C0 | 0x144 | 0 |
| 0x80092B04 | 0xFC | 0 |
| 0x80092C00 | 0x670 | 1 |
| 0x80093270 | 0xA8 | 1 |
| 0x80093318 | 0x174 | 1 |
| 0x8009348C | 0x220 | 1 |
| 0x800936AC | 0x2D4 | 1 |
| 0x80093980 | 0x140 | 2 |
| 0x80093AC0 | 0x108 | 1 |
| 0x80093BC8 | 0x7A0 | 2 |
| 0x80094368 | 0x240 | 1 |
| 0x800945A8 | 0x218 | 4 |
| 0x800947C0 | 0x348 | 1 |
| 0x80094B08 | 0x470 | 2 |
| 0x80094F78 | 0x240 | 1 |
| 0x800951B8 | 0x17C | 1 |
| 0x80095334 | 0x784 | 21 |
| 0x80095AB8 | 0x5D4 | 14 |
| 0x8009608C | 0x658 | 50 |
| 0x800966E4 | 0x178 | 1 |
| 0x8009685C | 0x1F8 | 23 |
| 0x80096A54 | 0x254 | 5 |
| 0x80096CA8 | 0x2BC | 2 |
| 0x80096F64 | 0x670 | 0 |
| 0x800975D4 | 0x6AC | 0 |
| 0x80097C80 | 0x43C | 0 |
| 0x800980BC | 0x1C4 | 1 |
| 0x80098280 | 0x844 | 0 |
| 0x80098AC4 | 0x4F0 | 0 |
| 0x80098FB4 | 0x19C | 1 |
| 0x80099150 | 0x5D4 | 0 |
| 0x80099724 | 0x47C | 0 |
| 0x80099BA0 | 0x71C | 0 |
| 0x8009A2BC | 0x47C | 0 |
| 0x8009A738 | 0x688 | 0 |
| 0x8009ADC0 | 0x3C | 0 |
| 0x8009ADFC | 0x668 | 0 |
| 0x8009B464 | 0x168 | 1 |
| 0x8009B5CC | 0x13C | 1 |
| 0x8009B708 | 0x114 | 1 |
| 0x8009B81C | 0xE0 | 0 |
| 0x8009B8FC | 0x3FA4 | 1 |
| 0x8009F8A0 | 0x2208 | 0 |
| 0x800A1AA8 | 0x1F4 | 1 |
| 0x800A1C9C | 0x264 | 1 |
| 0x800A1F00 | 0x460 | 3 |
| 0x800A2360 | 0x340 | 3 |
| 0x800A26A0 | 0x30C | 9 |
| 0x800A29AC | 0xA4 | 0 |
