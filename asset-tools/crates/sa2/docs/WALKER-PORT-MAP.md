# Walker movement port map (GameCube GSNE8P)

The walker's movement and collision response are main-program code. The Steam
executable's code section is encrypted on disk, so the port works from the
owner's GameCube Sonic Adventure 2 Battle image instead. A local read-only RVZ
reader extracted `main.dol`, and its SHA-1 e4c50f32242a3cd36cdc8ae776661601a52130a2
matches the reference build of the roeming/SA2B reconstruction. The disc's
`stg11D.rel` (SHA-1 8fbe506bcbbe6f012d2bb8240edb5f6c8cb6ece0) also matches the
reference. Extracted files stay in ignored local storage; Git holds only
addresses, names and Rust.

Function names are from the reconstruction's `config/GSNE8P/symbols.txt`;
`fn_XXXXXXXX` entries are not yet named upstream.

## Entry points found

| Address | Role | Evidence |
| --- | --- | --- |
| 0x80196578 | PhysicsArray, 0x84-byte rows | Rows match the Steam table; Mech Tails at 0x80196890 (slot 6), Mech Eggman at 0x80196914 (slot 7) |
| 0x8004F838 | Shared player-work setup | Allocates player work and copies the PhysicsArray row; called by all 14 character loaders |
| 0x800B56E8 | Mech Eggman loader | Character byte 7, 0x454-byte player work, `CreateElementalTask(7, 1, 0x800B3A8C)` |
| 0x800B5AC0 | Mech Tails loader | Character byte 6 |
| 0x800B3A8C | Mech Eggman exec (task +0x10) | 0x1BA4 bytes; the per-frame update |
| 0x800AF558 | Mech Eggman display (task +0x14) | Matrix and model calls only |
| 0x800B245C, 0x800AF2E8, 0x800B0D7C, 0x800B10CC | Further task callbacks (+0x18, +0x1C, +0x2C, +0x28) | Installed by the loader |

## Size of the port

Not counting Ninja, GX, OS and sound libraries, the exec reaches 135
main-program functions within two calls (about 23,000 instructions) and 296
within six (about 38,000). The largest shared routines it calls directly are
0x8004C734 (0xF9C bytes), 0x80045880 (0x8CC), 0x80056450 (0x8E4),
0x80046DE4 (0x43C), 0x8003E668 (0x378) and 0x8004E560 (0x3C4). The exec calls
the run 0x8004C734 / 0x80046DE4 / 0x80045880 / 0x8003E668 repeatedly from its
mode handlers, which makes it the first port target.

## Port order

1. Name the exec's mode switch and the repeated movement run above. Port each
   routine to Rust and check it instruction for instruction against the
   PowerPC under Unicorn, as the Finalhazard work does.
2. Port the landtable collision those routines call, against the native
   geometry `play_world` already holds.
3. Feed the native player state into `Session::step` as its `PlayerSample`.

## Shared facts for the port

- Small-data bases set in `__start` (0x8000325C): r2 = 0x803B6520 (`.sdata2`
  constants), r13 = 0x803B52C0.
- The player work layout matches SAMT's `playerwk` (include/samt/sonic/player.h
  in the reconstruction). For example: `dotp` +0x34, `spd` +0x64,
  `floor_normal` +0x7C, `attr` +0x88, `last_attr` +0x8C, and the copied
  PhysicsArray row `p` at +0xC0, so `run_accel` is +0xFC and `weight` +0x138.
  The walker work (`walkerwk`, figure/ewalker.h) extends it to 0x454 bytes.
- Ownership across threads: the Sonic controller port (#426) owns the shared
  ground-movement routine 0x8004C734 and its callee tree, plus the
  paired-single Unicorn DOL harness. The Prison Lane port owns the trig table
  and njSin (0x80112050), the Ninja rotate calls and the CCL form-3 box rule.
  This port owns the walker exec, its mode handlers, 0x80046DE4, 0x80045880
  and 0x8003E668, collision response, lock-on and the laser.

## 0x8003E668: world velocity into the player frame

Arguments are r3 = task work (rotation at +8, +0xC, +0x10), r4 = a world
vector followed by three angles at +0x18..+0x20, and r5 = player work. The
routine rotates the vector into the player's frame by Y, then -X, then -Z,
using the floor normal's angles (via 0x8005071C) instead of the task rotation
when the task is grounded (`flag & 3`) and `attr` bit 0x100 is set. It then
does the following:

- zeroes the vertical component when grounded;
- when grounded and `last_attr` bits 0xC are clear, zeroes x if |x| < 0.01
  and z if |z| <= 0.9 * `run_accel`; when airborne, zeroes x and z at or
  below 0.001;
- stores the result in `spd`, and masks all six angles to 16 bits;
- sets `dotp` to -dot(global vector at 0x801E5624, `floor_normal`) when
  grounded, otherwise 1.0.

On stage 0x5A it also clears `spd.x` below a slope-dependent threshold built
from `weight` (0.1, 1.2 and 0.77 from `.sdata2`). Constants read from
`.sdata2`: 0.0, 0.01, 0.9, 0.001, 1.0, 0.1, 1.2, 0.77, plus 0.5 and 3.0 for
the inline reciprocal square root.
