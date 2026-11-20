# Sonic's movement, ported from main.dol

The player controller is engine code. The Steam executable's copy is
SteamStub-protected, which this project does not get around, so the port
works from the GameCube build instead: `main.dol` of SA2 Battle (GSNE8P),
taken from the user's own disc. GameCube discs are not encrypted. The file
is never committed; tests read it from `SA2_MAIN_DOL`.

## Getting main.dol

With a Dolphin RVZ image of your own disc:

```powershell
python -I tests/extract_main_dol.py "D:\path\to\disc.rvz" .local\research\sa2b-gsne8p\main.dol
```

The expected SHA-1 is e4c50f32242a3cd36cdc8ae776661601a52130a2, the build
the roeming/SA2B decompilation targets. Its `symbols.txt` names or numbers
every function, which is how the routines below are addressed. Dolphin's
"Extract System Data" gives the same file.

## How code is checked

`tests/dol_ppc.py` loads main.dol into Unicorn at its real addresses. It
runs the parts of start-up the player code depends on: the static
constructors, the Ninja sine table and the Ninja matrix stack. It can then
call any function with chosen registers and memory. Unicorn has no
paired-single unit, so functions that use paired singles (the PSMTX matrix
library and the Ninja wrappers around it) are interpreted in Python
instruction by instruction.

The arithmetic is IEEE single precision, as on the PC build and in QEMU,
with Gekko's estimate instructions added when their tables are present:

- Each result rounds once to single precision.
- Fused multiply-adds round to double, then to single.
- With the local Gekko estimate tables (`tests/gekko_tables.py`,
  `.local/research/gekko-estimate-tables.json`; transcribed from Dolphin's
  GPL source and kept out of git), `frsqrte`, `ps_rsqrte` and `fres` return
  Gekko's table estimates everywhere. Without them they are exact.
- `fctiwz` gives 0x80000000 for NaN and out-of-range values.

Gekko's 25-bit multiplier operand is not modeled. Rust ports follow these
rules, so the check is "the same instructions, on this machine model".

## Map

| Area | main.dol | Status |
| --- | --- | --- |
| Physics parameters | PhysicsArray 0x80196578, stride 0x84, copied into player work at +0xC0 (layout matches the PC `playerwk`) | Read (`player_physics`) |
| MSL `sinf`/`cosf` | 0x801115A8 / 0x80111414, with `__sinit_trigf_c` | **Ported** (`ninja_trig::msl_sinf`, `msl_cosf`, from Prison Lane's #425) |
| Ninja sine table | njSinCosTbl 0x802D7758, built by fn_801181A0 | **Ported** (`ninja_trig::nj_sin`, `nj_cos`, `table_be_bytes`) |
| Ninja matrix stack | fn_80119C74 (0xE0000000, 0x30 bytes each), njPush/PopMatrix, njRotateX/Y/Z (PSMTXRotRad + PSMTXConcat), njCalcVector (PSMTXMultVecSR), njCalcPoint (PSMTXMultVec) | **Ported** (`ninja_matrix`) |
| Player-frame vectors | fn_8003E9E0 (rotate by Z, X, -Y), fn_8003EA6C (rotate by Y, -X, -Z) | **Ported** (`ninja_matrix::rotate_by_angles`, `unrotate_by_angles`) |
| Analog input | fn_8004E560 (`GetAnalog` on PC): stick angle and strength, confusion remap, steering on kind-0x11 collision objects; with njInnerProduct, njOuterProduct, MSL acosf and VectorAngle (0x8002B7C4) | **Ported** (`player_analog`, `ninja_vector`) |
| Turning | SubAngle 0x8002B904, DiffAngle 0x8002B8E0, AdjustAngle 0x8002B918; fn_800473B4 (turn toward a yaw, faster turns slow down above dash_speed, velocity carried round); fn_80047714 (smoothed turn with stick-dependent velocity blend) | **Ported** (`player_turn`) |
| Facing downhill | njScalor 0x801151F4 and njUnitVector 0x80115D24 (Gekko frsqrte/fres estimates); fn_80047CBC and fn_80048150 aim the motion yaw down slopes from gravity in the player's frame | **Ported** (`ninja_vector::scalor`, `unit_vector`, `player_slope`) |
| Path following (loops, rails) | fn_80038F40 with MSL floorf 0x80111240, fn_8002B770 (wrap), fn_80027D50, fn_8011B51C (point-line), fn_800CE5E8 (point-segment), fn_8005C460 (nearest point), fn_8005CDCC (segment record), fn_8005C910 (sample); reuses Iron Gate's plane_distance (fn_8011B6C8) and njDistanceP2P/sqrtf | **Ported** (`player_path`) |
| Ground acceleration | fn_8004B568 (stick-relative) and fn_8004BA74 (camera-relative steering, with njRotate 0x80115218 = PSMTXRotAxisRad + PSVECNormalize) | **Ported** (`player_accel`) |
| Shared ground movement | fn_8004C734 (0xF9C), called by 7 character mains; hands over to fn_8004BA74 (surface 0x100000) or fn_8004B568 (surface 0x100) | **Ported** (`player_accel::ground_move`) |
| Sonic | main fn_80083E44, action state machine fn_8007E89C; loaders fn_80086BA4, fn_80086E3C, fn_800871C4, fn_800874C0 | Planned |

## Ported so far

- **Ninja trigonometry** lives in `ninja_trig` (from the Prison Lane work,
  #425). `msl_sinf` and `msl_cosf` follow MSL's compiled `sinf`/`cosf`,
  including its four-term reduction constants, which a static constructor
  fills at start-up. `nj_sin`/`nj_cos` read the 65,536-entry njSinCosTbl,
  which is built from `sinf/cosf(pi_f32 * 2k / 65536)` (pi_f32 = 0x40490FD8,
  as stored), the way fn_801181A0 does. MSL's sine is not correctly rounded,
  so a libm-based table would differ in about half its entries.
  `tests/ninja_trig_dol.rs` (ignored; needs `SA2_MAIN_DOL` and
  `SA2_PPC_PYTHON`) checks the whole table byte-for-byte against the one
  the game builds. It also checks 3,014 sinf/cosf inputs, including zeros,
  infinities, NaN and huge values.

- **Ninja matrices** (`ninja_matrix`): `PSMTXConcat`, `PSMTXMultVecSR`
  (`njCalcVector`), `PSMTXMultVec` (`njCalcPoint`), `PSMTXRotTrig` and
  `njRotateX/Y/Z`, the matrix stack (`njPushMatrixEx`, `njPopMatrixEx`,
  `njUnitMatrix`, the push-unit helper) and the two player-frame helpers
  fn_8003E9E0 and fn_8003EA6C. The paired-single lanes and fused
  multiply-adds follow the compiled order, down to the signs of zeros.
  `tests/ninja_matrix_dol.rs` compares 400 random concatenations, 400
  vector/point transforms, 600 rotations (random angles, including
  out-of-range ones) and 600 player-frame round trips against main.dol.
  All of them match bit for bit.

- **Analog input** (`player_analog::get_analog`, fn_8004E560) and the
  vector helpers it uses (`ninja_vector`: njInnerProduct, njOuterProduct,
  MSL `acosf` and `VectorAngle`). `acosf` builds on Iron Gate's `msl_math`
  (`atanf`, `_inv_sqrtf`). This branch carries a byte-identical copy of
  that module, so the two branches merge cleanly. The routine's engine
  inputs (per-player stick table, frame counter, stage number, raw stick Y,
  gravity) are parameters. The collision lookup fn_800060EC(player, 0x11)
  is a boundary: the caller passes the touched object's angles.
  `tests/player_analog_dol.rs` matches 1,500 vector-product and `acosf`
  cases, 600 `VectorAngle` cases and 1,500 fn_8004E560 cases against
  main.dol bit for bit.

- **Turning** (`player_turn`): the 16-bit angle helpers and fn_800473B4.
  fn_800473B4 rotates the local velocity into world space, turns the yaw
  toward the target by 0x100 per frame. Above `dash_speed` the step is
  `256 - 256 * sqrt((speed - dash) / 16)`, with the compiler's inline
  frsqrte-and-Newton square root. It then rotates back and blends the result
  into the velocity (0.99/0.01 when the player work's +0x34 exceeds 0.4,
  otherwise half and half). `tests/player_turn_dol.rs` matches 3,000
  angle-helper cases and 1,500 turns against main.dol. fn_80047714
  (`turn_smoothly`) steps by 0x800 beyond 0x2000 away and otherwise by a quarter
  or an eighth of the gap. It keeps 10% of the old velocity off the ground,
  and on the ground 5% without stick input or 99%/50% with it, scaled by
  the player work's +0x40 below 1. It is checked on 1,500 more turns.

- **Facing downhill** (`player_slope`): fn_80047CBC and fn_80048150 rotate
  gravity into the player's frame and, on slopes, aim the motion work's yaw
  down them. The aim steps by 4096|z|*speed/jog_speed or 2048|z| in the
  strong version, and 3072|z| or 1536|z| in the soft one. The strong one
  then turns the yaw by up to 0x2000; the soft one hands over to
  fn_80047714. Both skip when the motion velocity (njScalor, Gekko's
  estimate) is slower than jog_speed or points steeply along gravity.
  `tests/player_slope_dol.rs` matches 1,500 njScalor/njUnitVector cases and
  2,400 slope turns. This includes PowerPC's positive default NaN for a zero
  vector.

- **Path following** (`player_path`): fn_80038F40 runs the player along the
  player work's path (+0xB8). It needs at least 1.2x dash_speed while the
  stick is held, finds the nearest point and path distance, and advances
  that distance by the speed in whichever direction the player faces. It
  then aims the motion yaw at a point 9 units ahead projected onto the floor
  plane, and turns the task yaw by a step chosen from how parallel the two
  path crosses are. The path helpers cover open and looped type-1 (angled)
  and type-2 (vectored) paths. Type 2's wrap reads its first record at +8,
  as the original does. `tests/player_path_dol.rs` checks 2,000
  floorf/wrap cases, 1,500 point-line and point-segment cases, 500 random
  paths (nearest point, every segment record, 2,000 samples) and 1,500
  path-follow frames against main.dol, bit for bit. Where degenerate
  geometry yields NaN, it is PowerPC's positive default NaN.

- **Ground acceleration** (`player_accel`): fn_8004B568 adds gravity times
  weight and air resistance, turns toward the stick (on the spot when
  standing, fn_800473B4 or fn_80047714 when moving), and adds the stick's
  push (run_accel, 0.4x at run speed, 0.8x on a sharp turn) or the
  slow-down. A friction limit (lim_frict) holds a standing player still.
  fn_8004BA74 is the camera-relative version. It turns the raw stick about
  the player's own gravity axis (player work +0x7C) from the camera's
  side, eases the aim downhill by 512|z|, and scales the push by speed band
  (max_x_spd, jog, run, rush). It follows the path when status bit 0x2000
  is set, and applies the surface grip (grd_frict, grd_frict_z) when the
  surface flag 4 is set. The camera position (camera work +0x18 via
  0x80175378) and the raw stick are inputs. `tests/player_accel_dol.rs`
  matches 3,000 frames of each and 800 njRotate cases against main.dol,
  bit for bit. To run PSMTXRotAxisRad, the harness now lets a paired-single
  function call plain code (MSL sinf/cosf): Unicorn runs the callee and
  hands back to the interpreter.

- **Shared ground movement** (`player_accel::ground_move`, fn_8004C734).
  Surface flag 0x100000 hands over to fn_8004BA74 and 0x100 to fn_8004B568.
  Otherwise it adds gravity times weight in the player's frame, with more
  on steep ground: all of it on a banked wall at speed, 5x, 0.8x or 0.4x
  extra weight below up -0.4, -0.3, -0.1, and 4.225x, 2x or 1.4x more
  sideways pull on walls and slants. After air resistance it turns toward
  the stick: on the spot when standing, then fn_80047714 or fn_800473B4
  by speed and turn size, the strong downhill turn (fn_80047CBC) on surface
  0x80000, and the path (fn_80038F40) on status 0x2000. Pushing into a wall
  (player work +0x70) bends the stick along it: the plane intersection of
  the wall and the player's gravity plane (Iron Gate's fn_8011B7B8). The
  walkers (characters 6 and 7) turn with fn_800473B4 throughout, or always
  when their object's +0x35C byte is 1. The same friction steps as
  fn_8004BA74 finish the frame, damped 10x when the player is tipped and
  slow. `tests/player_accel_dol.rs` matches 6,000 frames against main.dol,
  bit for bit.
