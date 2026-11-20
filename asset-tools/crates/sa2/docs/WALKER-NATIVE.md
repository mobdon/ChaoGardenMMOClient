# Native walker movement (GSNE8P main.dol)

This ports Eggman's and Tails' walker movement from the owner's GameCube SA2
Battle `main.dol` (SHA-1 e4c50f32242a3cd36cdc8ae776661601a52130a2). Each
routine is checked bit for bit against main.dol under Unicorn, using #426's
`tests/dol_ppc.py` harness. Entry points and the call graph are in
WALKER-PORT-MAP.md on #424.

## Ported

| Routine | Rust | Check |
| --- | --- | --- |
| MSL atanf 0x80110E64, atan2f 0x80111168, asinf 0x80111058, _inv_sqrtf 0x801110B4 | `msl_math` | tests/msl_math.rs, 1,941 cases |
| fn_8005071C: floor normal to tilt angles | `msl_math::normal_angles` | same |
| fn_8003E668: world velocity into the player frame, `spd` and `dotp` | `walker_frame::player_speed_from_world` | tests/walker_frame.rs, 1,500 cases |
| fn_80046950: speed clamp to `lim_h_spd` / `lim_v_spd` | `walker_frame::clamp_speed` | same, 600 cases |
| 0x80046DE4: add `acc` to `spd` with the stop and brake rules, rotate back to world space, clamp, drop components within ±0.02 without stick input | `walker_frame::player_world_speed` | same, 1,500 cases (fn_8004E560 stubbed) |

Set `SA2_MAIN_DOL` and `SA2_GSNE8P_DOL` to the extracted main.dol, and set
`SA2_PPC_PYTHON` to a Python with Unicorn. Then run
`cargo test --test msl_math --test walker_frame -- --ignored`.

## Ownership

This port owns the walker exec (0x800B3A8C) and its mode handlers,
0x80046DE4, and 0x80045880 with its collision core. The Sonic port (#426)
owns 0x8004C734's tree, including fn_8004E560 (which 0x80046DE4 calls),
GetStageNum, VectorAngle and njInnerProduct. `walker_frame` inlines
njInnerProduct only until #426 publishes it. The Prison Lane port owns trig
(`ninja_trig`). #426 owns the matrix ops (`ninja_matrix`).

## Next: the collision core under 0x80045880

0x80045880 sets the walker against the stage. It calls fn_8003EAFC (0xC64
bytes) and fn_80043ED4 (0x19AC bytes), which reach the PhysicsArray readers
fn_80040ACC, fn_80040F48, fn_80041C78 and fn_800420C0. They also reach
fn_80042E0C, fn_80043600 and fn_800414F0/80041984/80042B4C/80042694/
800428F8/800424C8, and the land-collision search fn_800CEE08 (0x1064) and
fn_800D1020 (0x380). The search works on global collision lists addressed
from r13 (for example -0x7738, -0x7754, -0x7758 and -0x77A8). Checking it
needs the stage's land collision loaded into the emulated memory, as the
game lays it out. The port goes bottom-up from fn_800D1020 and fn_800CEE08.

## Land collision pipeline (found)

1. The current LandTable is at r13-0x7A00. fn_8002334C (0x4BC) picks the
   entries near the player tasks (`playertwp`, 0x801E7768). It writes 12-byte
   records {flags, NJS_OBJECT*, ...} to r13-0x79F4, with the count at
   r13-0x79A4 (at most 0xC0).
2. fn_800D0904 (0x638) turns that list into collision nodes. The nodes are
   0x40 bytes each, from a pool set up by fn_800D2560 (free list at
   r13-0x773C, active list at r13-0x7738). Each node keeps its object's
   transform and bounding sphere. A removed entry goes back through
   fn_800D24B4.
3. fn_800D1020 is the broadphase. It walks the sorted lists for a query
   sphere and fills a candidate list of up to 0xC0 entries (count at
   r13-0x7758).
4. fn_800CEE08 (0x1064) tests the candidates' triangles.

The emulator check needs the GameCube stg11D.rel loaded and relocated, with
r13-0x7A00 pointing at its `objLandTable0011`. The installed module and the
disc's copy are byte-identical (SHA-1 8fbe506b…).
