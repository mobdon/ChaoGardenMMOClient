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
with one Gekko feature added:

- Each result rounds once to single precision.
- Fused multiply-adds round to double, then to single.
- `frsqrte` and `ps_rsqrte` return Gekko's table estimate
  (`tests/gekko_float.py`, transcribed from Dolphin by the Finalhazard work).
  Every `frsqrte` that Unicorn runs natively is patched to it too.
- `fres` is still exact.
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
| Shared ground movement | fn_8004C734 (0xF9C), called by 7 character mains; callees fn_8004BA74, fn_8004B568, fn_80038F40, fn_80047714, fn_800473B4, fn_80047CBC, fn_80048150, fn_8004E560 | Next |
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
