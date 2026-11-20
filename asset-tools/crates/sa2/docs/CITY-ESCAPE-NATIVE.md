# City Escape native inputs, and the missing Sonic controller

`city_escape_native::CityEscape::read` collects City Escape's gameplay inputs
(Hero story, level 13) from the installed Steam data. It applies no adapted
rules, and it contains no player controller.

## Why there is no Sonic controller

The player controller is engine code, and none of it is readable from the
install:

| Source | What it holds |
| --- | --- |
| Steam `sonic2app.exe` | The PC player routines, for example PGetAcceleration at 0x45B610, PGetAccelerationAir at 0x45D770 and PGetFriction at 0x45F4D0, per the mod-loader declarations. The `.text` section is SteamStub-protected: a `.bind` section is present, and the bytes at those addresses have about 7.96 bits of entropy. This project does not decrypt or unpack it. |
| `resource/gd_PC/DLL/Win32/Data_DLL.dll` | Data. Its `.text` section is 0x8AA bytes and holds no player code. |
| `resource/gd_PC/*.rel` | GameCube stage, boss, kart and event modules. They call the player through the main executable but do not contain it. |
| GameCube `main.dol` (GSNE8P) | The GameCube player code. It is not part of the Steam install, and none was found on this machine. |
| roeming/SA2B at 874a480 | No player source. `config/GSNE8P/symbols.txt` names a few helpers (GetPlayerRunningSpeed at 0x800380AC, the playerpwp/playertwp arrays). |

A `main.dol` dumped from a GameCube disc you own would make the routines
executable under Unicorn, as the stage and boss modules are, so a Rust port
could be checked against them. Until then, no controller is provided for
speed stages.

## What is read

| Input | Source |
| --- | --- |
| Physics | PhysicsArray row 0 (Sonic) or 1 (Shadow), all 33 fields (`player_physics::read`) |
| Start | Single-player start row: (0, 300, 340), yaw 0x4000 as stored |
| Terrain | Mounted `objLandTable0013` Basic collision |
| Grind rails | The 12 paths whose callback is 0x4980C0 in the PC_SA2/STG13.ini path list at RVA 0xCDB35C: 147 points, with rotation words and lengths as stored |
| Death zones | PC_SA2/STG13.ini table at RVA 0xCDC678 |
| Springs | `spring::StageSpring` tasks for the 10 SPRA/SPRB placements (the C-verified callback) |
| Bush blocks | GREEN_B Basic collision at RVA 0xCAA350, placed by SET position and Y angle (18 placements) |
| Goal | The stg13D.rel goal-ring program and each GOALRING's native initialization route. Of the two placements, only the Rotation.x 0 one reaches contact. |
| Everything else | Raw SET rows (position, rotation words, Scale) by object name, including 82 KASOKU dash panels and 3 SAVEPOINTs. Their behaviour is not interpreted. |

`tests/city_escape_native.rs` checks these values on the pinned Steam build
(run with `--ignored`, `SA2_GAME_PATH` and `SA2_PC_CONFIG`).
