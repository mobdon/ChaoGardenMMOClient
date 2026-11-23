# Native garden doorway clicks

The browser tags the existing dark doorway recesses in the three original
garden GLBs as bounded return-to-hub click targets. It submits the existing
Rust `{type:"action",action:"hub"}` intent. Rust remains responsible for the
transition, spawn and canonical player state. No destination, progression,
terrain, save, API or original game engine is added by this adapter.

The native source geometry, vertex colors and textures remain unchanged.
The recesses were identified in local renders of the original landtables and
measured from their dark vertex-colored triangles after the exact transforms
in `src/world-layout.js`. These are **adapted click regions**; original retail
trigger placement, collision volumes and transition code remain untraced.

| Garden | Original GLB primitive | Canonical world recess bounds (x / y / z) | Current authoritative reachability |
| --- | --- | --- | --- |
| Neutral | `0007_object_0180555C_primitive0`; duplicate `0024_object_017FE994_primitive0` | `[-11.9132,-9.8232] / [0.0406,3.6206] / [-12.2833,-11.3995]` | Actual browser walking and native recess click returned a disposable Chao to the Rust hub. |
| Hero | `0022_object_0175FA24_primitive0` | `[14.0326,16.2452] / [-3.4,0.37] / [32.679,34.8912]` | Outside the current ±22 terrain field. It can be clicked when visible; a complete walk-up route requires a separate terrain/authority change. |
| Dark | `0028_object_0171BA54_primitive1` | `[10.8088,15.8049] / [-0.1,2.5] / [7.4675,9.2987]` | Inside the current terrain domain; browser walk/click acceptance remains separate. |

The large cliff primitive is not itself an unrestricted button. A hit must
match an enabled original mesh, the current zone and the measured recess
bounds (with a 0.002-unit rounding margin). The adapter checks that loaded
geometry contains the expected recess before tagging it. Missing, renamed,
disabled or displaced source geometry retains the procedural return portal.
After successful tagging the procedural arch is hidden. The existing Home
HUD action remains available in every garden, including Hero.

Diagnostics on the game canvas are `data-native-garden-exit`,
`data-native-garden-exit-meshes` and `data-native-garden-exit-reachable`.
They reset on zone construction and current-load failure. Reachability here
means the recorded doorway lies inside the existing authoritative terrain
domain; it does not certify complete terrain collision or a walk-through
transition. The adapter never triggers from client distance alone.

Run `node --test tests/garden-exits.test.js` and
`node scripts/verify-native-garden-exits.mjs`. The latter loads the actual
ignored GLBs with Babylon, applies the production layout, ray-picks original
dark recess triangles and checks outside-point rejection. It writes source
hashes, transforms, exact picked points and limits to
`.local/reviews/native-garden-exits.json`. Unit and geometry checks do not
replace a real browser click followed by a Rust snapshot round trip.

November 18 browser check: a disposable Chao on port 5176 walked through the
actual Neutral Garden to `(-10.429, 0.061, -7.716)`, recentered the follow camera
and clicked the visible dark cave recess. The next Rust snapshot/UI confirmed
Main Hub at `(0,6)` and the welcome-back notice. The fixture used the frozen
Rust executable and copied native terrain, with no live credentials/saves.
Local screenshots: `.local/screenshots/native-garden-exit-before.png` and
`native-garden-exit-return.png`. Two incidental garden rings affected only this
disposable Chao. Dark's browser approach stopped at a terrain obstruction near
`(7.245,6.960)`; its full walk/click acceptance remains open. No full-garden
collision or native automatic walk-through claim follows from these checks.

The original landtables are extracted from the local PC `Data_DLL.dll` using
official SA Tools 786 `Chao_DLL.ini` mappings for `objLandTableNeut`,
`objLandTableHero` and `objLandTableDark`; conversion provenance remains in
`.local/imports/conversion-report.json`. Native lobby/Kindergarten doors,
Black Market rooms, learning and original stock rules are separate work.
