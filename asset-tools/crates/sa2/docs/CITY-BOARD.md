# City Escape opening: board, animation and ground

The installed Steam stage has four physical boards. The normal solid SET places one at index 103; hard mode uses index 97; the two-player solid SET uses indices 244 and 245. The other two SET layers have none. All four resolve to object 77, `BOARD`, at PC VA 0x5EBC50. Object 59, `BOARDCOL`, is the separate trigger that requests a player-mode change.

**Reproduce on Windows.** Build the crate and use a new output directory inside the checkout's ignored `.local`. Run from the crate directory, with the checkout-local Rust toolchain or Cargo on PATH:

```powershell
cargo build --locked
$sa2 = (Resolve-Path target/debug/sa2-assets.exe).Path
$game = 'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2'
$gd = Join-Path $game 'resource/gd_PC'
$exe = Join-Path $game 'sonic2app.exe'
$rel = Join-Path $gd 'stg13D.rel'
$dll = Join-Path $gd 'DLL/Win32/Data_DLL.dll'
$out = (New-Item -ItemType Directory -Path (Join-Path (Resolve-Path '../../..') ('.local/city-board/' + [guid]::NewGuid()))).FullName
& $sa2 --source-root $game city-board-glb $exe "$out/street.glb" street "$gd/objtex_stg13.prs"
& $sa2 --source-root $game city-board-pose $dll "$out/pose.json" $exe street 121 3.25
& $sa2 --source-root $game city-spawn-ground $rel "$out/start.json" $exe 10000
& $sa2 --source-root $game player-physics $exe "$out/physics.json"
```

The GLB contains the original rest geometry and base-mip textures. Model variants are `street` (single-player), `shadow`, `amy`, `metal`, and `sonic` (two-player default). `city-board-model EXE OUTPUT.sa2mdl VARIANT` also extracts the native `cast-shadow` and `modifier` resources. Those require separate rendering passes. [Model and pixel comparisons](BOARD-MODEL.md).

The pose tool resolves the source model and samples the requested action's stage-default motion in board-local coordinates. `city-board-pose` accepts either stage module: addresses in the DLL are RVAs and addresses in the REL are relative to section 5. A live engine can override a slot. This tool selects defaults explicitly. [Motion evidence](BOARD-MOTION.md). `city-board-resources stg13D.rel OUTPUT.json` follows the actual paired instruction relocations in the installed board selector and resolves its texture lists by name. Raw zero words with relocations are pointers; a null word without a relocation is a null pointer.

**What updates on a tick.** A successful bind selects the player slot from the low bit of SET X rotation. On rider actions 0x79 through 0x90 inclusive the board copies position, speed, pitch and roll, and sets yaw to 0x8000 minus rider yaw. Motion mode above 2 caches the action and animation frame for blending. A missing rider leaves the board unchanged. Leaving a board action while attached enters mode 2 and requests the external motion routine followed by the range check. The replay stops at that boundary; it does not fabricate the result of either routine.

The following observations exercise SET index 103. Coordinates here are sample inputs, not a recorded trajectory. `tick` and `draw` select the SET's rider slot; `two_player` is the observed engine flag. Missing vs null motion rows are distinct. [Native draw and shadow decisions](BOARD-DRAW.md).

```json
{"frames":[
  {"op":"bind","allocation_ok":true},
  {"op":"tick","players":[{"position":[0,300,340],"angles":[0,0,0],"speed":[0,0,0],"action":121,"motion_mode":3,"frame":0},null]},
  {"op":"draw","two_player":false,"players":[{"character":0,"action":121,"motion_mode":3,"frame":0},null],"motions":[{"index":145,"pointer":0}]},
  {"op":"destroy"}
]}
```

Save this JSON under the private output directory, then run:

```powershell
& $sa2 --source-root $game city-replay "$gd/set0013_s.bin" "$out/replay.json" $exe 103 "$out/request.json"
```

The callback source is [roeming/SA2B, o_ce_board.c at 874a480](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/src/stg13_cityescape/o_ce_board.c). The C reference covers 52 trace rows including allocation and cache branches. An independent test also executes the original PowerPC update at section 1 + 0x3B4 in the installed REL, comparing 44 updates bit for bit. Both checks supply rider observations and stand-ins for the two detached calls. The REL ships with Steam; it has a different hash from the GSNE8P reconstruction's expected disc image. Neither test executes the Windows player controller.

**Ground and progression.** `city-collision` and `city-raycast` decode the original City Escape terrain directly from either pinned module. On the single-player spawn at (0,300,340), the downward ray passes 513.4246520996094 native units before reaching static triangle geometry. This geometric result does not set the player's grounded flag. [Independent terrain comparisons](CITY-LAND.md) · [PC player defaults](PLAYER-PHYSICS.md) · [Remaining gameplay acceptance trace](PLAYABLE-GAPS.md).

Run `verify-city-board.ps1` with the pinned files and separate local conversions described in the linked pages to check the C and instruction references, motions, geometry, pixels, terrain, physical constants and CLI manifests. No reference game, model, texture, save or generated output is included in Git.
