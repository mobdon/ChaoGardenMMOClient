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
| fn_8002334C static pass: land-table entries near the players | `land_select::select_static` | tests/land_select.rs, 300 cases on the relocated GC stg11D.rel |
| fn_8002334C in full, with the dynamic land objects movers register | `land_select::select_frame` | tests/land_nodes.rs, 40 frames with synthetic movers and tasks |
| fn_80023808: each query's re-pick from the frame's dynamic and static subsets | `land_select::Selection::reselect` | same, 160 queries |
| fn_800D1020: the broadphase, candidate triangles near a moving sphere (njScalor 0x801151F4 included) | `land_nodes::LandNodes::query` | same, 160 queries including a full 0xC0 candidate list |
| fn_800CEE08: a moving sphere against one object's candidate triangles (with PSMTXInverse, njUnitVector, and #426's acosf) | `land_contact`, `LandNodes::collide` | same, every query's objects with candidates in both modes, over 300 hits |
| fn_800D0904 with fn_800D13A0, fn_800D14C8, fn_800D17E0, fn_800D2298, fn_800D23A8, fn_800D24B4, fn_800D251C and Ninja's table rotations, njTranslateEx and fn_8011610C: the land collision node list and its bucketed triangles | `land_nodes::LandNodes::update` | tests/land_nodes.rs, 40 frames on stg11D.rel plus synthetic object trees, triangle, quad and flipped strip sets, flag 0x08000000 and a full triangle pool |
| Walker collision helpers: fn_800428F8 (sphere offset), fn_800420C0 (surface physics), fn_800414F0 / fn_80041984 (floor and wall hit sorting), fn_80040ACC (wall slide), fn_80042694 (contact tilt), plane helpers 0x8011B6C8 / 0x8011B7B8 / 0x8011BDB4, sqrtf 0x8002F15C, fn_80027C2C, DiffAngle / SubAngle / AdjustAngle | `walker_collide` | tests/walker_collide.rs, 3,000 to 4,000 random cases each |
| fn_8003EAFC (ground pass), fn_80043ED4 (floor pass), fn_80043600 (re-land), fn_800424C8 (riding dynamic land), fn_80042E0C / fn_800407A0, fn_80042B4C / fn_80040540, fn_80041C78, fn_80040F48, fn_80041814 | `walker_ground`, `walker_collide` | tests/walker_ground.rs, 400 randomized Iron Gate cases per pass |
| 0x80045880: the collision core, with the CCL contact reactions and fn_8004614C (push-plane slide); fn_8004F11C's interaction flags and the CCL data come in as `CclView` | `walker_ground::collision_core` | same, 400 cases |
| 0x80046DE4: add `acc` to `spd` with the stop and brake rules, rotate back to world space, clamp, drop components within +/-0.02 without stick input | `walker_frame::player_world_speed` | same, 1,500 cases (fn_8004E560 stubbed) |

Set `SA2_MAIN_DOL` and `SA2_GSNE8P_DOL` to the extracted main.dol, and set
`SA2_PPC_PYTHON` to a Python with Unicorn. Then run
`cargo test --test msl_math --test walker_frame -- --ignored`. The land test
also needs `SA2_STG11_REL` (the GameCube stg11D.rel); `tests/rel_load.py`
loads and relocates it into the harness.

## Ownership

This port owns the walker exec (0x800B3A8C) and its mode handlers,
0x80046DE4, and 0x80045880 with its collision core. The Sonic port (#426)
owns 0x8004C734's tree, including fn_8004E560 (which 0x80046DE4 calls),
GetStageNum, VectorAngle and njInnerProduct. `walker_frame` inlines
njInnerProduct only until #426 publishes it. The Prison Lane port owns trig
(`ninja_trig`). #426 owns the matrix ops (`ninja_matrix`).

## Next: the walker exec

The collision core and everything under it are ported. Next is the walker
exec (0x800B3A8C) and its mode handlers, which call 0x80046DE4, 0x8003E668
and 0x80045880, then plugging the native walker into #424's session.

## Land collision pipeline (found)

1. The current LandTable is at r13-0x79F0. fn_8002334C (0x4BC) picks the
   entries near the player tasks (`playertwp`, 0x801E7768). It writes 12-byte
   records {flags, NJS_OBJECT*, ...} to r13-0x79F4, with the count at
   r13-0x79A4 (at most 0xC0).
2. fn_800D0904 (0x638) turns that list into collision nodes (ported, see
   `land_nodes`). A quirk it keeps: when the rebuild object at r13-0x7750
   is listed, its old node survives next to the new one. The nodes are
   0x40 bytes each, from a pool set up by fn_800D2560 (free list at
   r13-0x773C, active list at r13-0x7738). Each node keeps its object's
   transform and bounding sphere. A removed entry goes back through
   fn_800D24B4.
3. fn_800D1020 is the broadphase (ported). It first calls fn_80023808,
   which rewrites the selection list for the query's sphere, then searches
   the listed nodes. It walks the sorted lists for a query
   sphere and fills a candidate list of up to 0xC0 entries (count at
   r13-0x7758).
4. fn_800CEE08 (0x1064) tests the candidates' triangles (ported, see
   `land_contact`). Its callers run fn_800D1020 once and then this routine
   for each listed object.

The emulator check needs the GameCube stg11D.rel loaded and relocated, with
r13-0x7A00 pointing at its `objLandTable0011`. The installed module and the
disc's copy are byte-identical (SHA-1 8fbe506bcbbe6f012d2bb8240edb5f6c8cb6ece0).

## Gekko estimates

`frsqrte` (everywhere) and `fres` (njScalor, PSMTXInverse) are Gekko table
estimates. The tables are GPL-derived, so they live in a local, git-ignored
file read by `msl_math::gekko_tables()` and `tests/gekko_tables.py`
(`.local/research/gekko-estimate-tables.json`, or `SA2_GEKKO_TABLES`).
Without it the port uses exact `1/sqrt(x)` and `1/x`, and the main.dol
checks skip. Create it with
`python asset-tools/crates/sa2/tests/gekko_tables.py --extract-from-git fc5e0a3`.
